//! The evaluator: turning IR descriptions into concrete game objects and numbers.
//!
//! Every IR type that *describes* something rather than stating it outright resolves
//! here — [`mtg_ir::Selector`] to a set of objects or players,
//! [`mtg_ir::Value`] to a number, [`mtg_ir::trigger::Condition`] to a boolean,
//! [`mtg_ir::ObjectFilter`] to a predicate. Effect resolution, target legality,
//! the layer system and the footprint analysis all call into it, which makes this
//! the busiest module in the engine.
//!
//! # It cannot ask questions
//!
//! Evaluation is pure and synchronous, so it has no way to prompt. Anything
//! requiring a player decision — `Selector::ChosenBy`, `Value::ChosenByController` —
//! must already have its answer recorded in [`Ctx::bindings`], put there at
//! announcement or by an earlier resolution step. If it is missing, evaluation fails
//! with [`EvalError::UnresolvedChoice`] rather than guessing, and the caller is
//! responsible for having gathered choices first.
//!
//! That is a deliberate division: gathering choices is a stateful, interactive
//! process that belongs to the resolver, and mixing it into evaluation would make
//! every characteristic lookup potentially interactive.
//!
//! # The circularity, and how it is broken
//!
//! Filters ask about characteristics ("each creature with power 2 or less"), and
//! characteristics are computed by the layer system, which itself needs to evaluate
//! selectors to know which objects an effect applies to. That is a genuine cycle in
//! the rules, and CR 613.6 resolves it: a continuous effect's affected set is
//! determined using the characteristics produced by the layers *below* the one being
//! applied.
//!
//! So characteristics reach the evaluator through the [`CharacteristicsSource`]
//! trait, with two implementations — [`PrintedChars`], used from inside a layer pass,
//! and [`ComputedChars`], used everywhere else. The layer system passes the
//! restricted one, and the cycle cannot close.

use std::collections::BTreeMap;

use mtg_core::{
    CardType, Characteristics, ColorSet, CounterKind, ObjectId, PlayerId, Target, Zone, ZoneRef,
};
use mtg_ir::{
    ObjectFilter, Selector, Value, ability::Keyword, selector::Binding, trigger::Condition,
};

use crate::{layers::PrintedCards, state::GameState};

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum EvalError {
    /// A selector or value needed a player's decision that was never recorded.
    UnresolvedChoice,
    /// An effect referred to a binding no earlier step established.
    MissingBinding(Binding),
    /// A target slot index the ability does not have.
    MissingTarget(u8),
    /// An object referenced after it ceased to exist. Legitimate for
    /// last-known-information lookups, an error anywhere else.
    NoSuchObject(ObjectId),
    /// Card data for an object was not loaded.
    UnknownCard(ObjectId),
}

pub type Eval<T> = Result<T, EvalError>;

/// Where characteristics come from. See the module docs on CR 613.6.
pub trait CharacteristicsSource {
    fn characteristics(&self, state: &GameState, id: ObjectId) -> Option<Characteristics>;
}

/// Printed characteristics only, ignoring continuous effects.
///
/// Used from inside a layer pass, where consulting computed characteristics would
/// close the cycle. Also the right answer for objects not on the battlefield, which
/// most continuous effects do not reach anyway.
pub struct PrintedChars<'c>(pub &'c dyn PrintedCards);

impl CharacteristicsSource for PrintedChars<'_> {
    fn characteristics(&self, state: &GameState, id: ObjectId) -> Option<Characteristics> {
        let obj = state.objects.get(&id)?;
        if obj.face_down {
            Some(Characteristics::face_down())
        } else if self.0.layout(obj.card) == mtg_ir::Layout::Split
            && obj.zone.zone != Zone::Stack
            && obj.cast_context.is_none()
        {
            self.0.split_characteristics(obj.card)
        } else {
            Some(self.0.face(obj.card, obj.face)?.printed_characteristics())
        }
    }
}

/// Full characteristics, with every continuous effect applied.
pub struct ComputedChars<'c>(pub &'c dyn PrintedCards);

impl CharacteristicsSource for ComputedChars<'_> {
    fn characteristics(&self, state: &GameState, id: ObjectId) -> Option<Characteristics> {
        if state.objects.contains_key(&id) {
            return crate::layers::compute(state, self.0, id);
        }
        // CR 608.2h / 113.7a — an object that has left the battlefield is evaluated as it
        // last existed there: "destroy target creature, you gain life equal to its power".
        // Its last-known copy is put back into a scratch copy of the state, so continuous
        // effects that applied to it still do.
        let last = state.last_known.get(&id)?;
        let mut then = state.clone();
        then.objects.insert(id, last.clone());
        crate::layers::compute(&then, self.0, id)
    }
}

/// Everything evaluation needs that is not the game state itself.
pub struct Ctx<'a> {
    pub state: &'a GameState,
    pub cards: &'a dyn PrintedCards,
    pub chars: &'a dyn CharacteristicsSource,
    /// The ability's source, for `Selector::SelfSource` (CR 113.7).
    pub source: ObjectId,
    /// The ability's controller, for `Selector::You`. Fixed when the ability
    /// triggered or was activated, not read live (CR 603.3a).
    pub controller: PlayerId,
    /// Targets chosen on announcement.
    pub targets: &'a [Target],
    /// Per target, whether it is still legal (CR 608.2b).
    ///
    /// Empty means "all legal", which is the common case. A target that has become
    /// illegal resolves to nothing rather than being an error: the spell does as much
    /// as it still can, so an effect naming an illegal target simply does nothing to
    /// it.
    pub target_legal: &'a [bool],
    /// The value announced for `{X}`.
    pub x: u32,
    /// Slots filled by earlier steps, and by the triggering event.
    pub bindings: &'a BTreeMap<Binding, Vec<Target>>,
}

impl Ctx<'_> {
    /// Whether the target in a given slot is still legal.
    pub fn target_is_legal(&self, index: u8) -> bool {
        self.target_legal
            .get(index as usize)
            .copied()
            .unwrap_or(true)
    }

    fn characteristics(&self, id: ObjectId) -> Eval<Characteristics> {
        self.chars
            .characteristics(self.state, id)
            .ok_or(EvalError::UnknownCard(id))
    }

    /// Whether an object has a keyword, looked up through its *current* ability
    /// list so that granted and removed keywords are respected.
    pub fn has_keyword(&self, id: ObjectId, kw: Keyword) -> Eval<bool> {
        let obj = self
            .state
            .objects
            .get(&id)
            .ok_or(EvalError::NoSuchObject(id))?;
        let face = self
            .cards
            .face(obj.card, obj.face)
            .ok_or(EvalError::UnknownCard(id))?;
        let ch = self.characteristics(id)?;
        if ch.granted_keywords.contains(&kw) {
            return Ok(true);
        }
        let combined = self.cards.layout(obj.card) == mtg_ir::Layout::Split
            && obj.zone.zone != Zone::Stack
            && obj.cast_context.is_none();
        let right = combined.then(|| self.cards.face(obj.card, 1)).flatten();
        Ok(ch.abilities.iter().any(|aid| {
            let ability = if combined && usize::from(aid.0) >= face.abilities.len() {
                right.and_then(|right| {
                    right
                        .abilities
                        .iter()
                        .find(|a| usize::from(a.id.0) + face.abilities.len() == usize::from(aid.0))
                })
            } else {
                face.abilities.iter().find(|a| a.id == *aid)
            };
            ability.is_some_and(|a| matches!(&a.kind, mtg_ir::AbilityKind::Keyword(k) if *k == kw))
        }))
    }
}

/// Resolve a selector to concrete objects.
///
/// Selectors that denote players resolve to an empty object set rather than an
/// error, because many selectors legitimately denote either and callers ask for
/// both.
pub fn objects(ctx: &Ctx, sel: &Selector) -> Eval<Vec<ObjectId>> {
    match sel {
        Selector::SelfSource => Ok(vec![ctx.source]),

        Selector::You
        | Selector::Opponents
        | Selector::EachPlayer
        | Selector::ActivePlayer
        | Selector::DefendingPlayer
        | Selector::EnchantedPlayer
        | Selector::Player(_)
        | Selector::ControllerOf(_)
        | Selector::OwnerOf(_) => Ok(Vec::new()),

        Selector::Target { index } => {
            if !ctx.target_is_legal(*index) {
                return Ok(Vec::new());
            }
            match ctx.targets.get(*index as usize) {
                Some(Target::Object(o)) => Ok(vec![*o]),
                Some(Target::Player(_)) => Ok(Vec::new()),
                None => Err(EvalError::MissingTarget(*index)),
            }
        }

        Selector::Bound(b) => Ok(bound(ctx, *b)?
            .iter()
            .filter_map(|t| match t {
                Target::Object(o) => Some(*o),
                Target::Player(_) => None,
            })
            .collect()),

        Selector::All { zone, filter } => {
            let mut out = Vec::new();
            for id in in_zone(ctx.state, *zone) {
                if matches(ctx, filter, id)? {
                    out.push(id);
                }
            }
            Ok(out)
        }

        // The choice must already have been made and recorded.
        Selector::ChosenBy { .. } => Err(EvalError::UnresolvedChoice),

        Selector::TopOfLibrary { player, count } => {
            let n = value(ctx, count)?.max(0) as usize;
            let mut out = Vec::new();
            for p in players(ctx, player)? {
                let lib = ctx.state.objects_in(ZoneRef::of(Zone::Library, p));
                out.extend(lib.into_iter().take(n));
            }
            Ok(out)
        }

        Selector::Union(parts) => {
            let mut out = Vec::new();
            for p in parts {
                for id in objects(ctx, p)? {
                    if !out.contains(&id) {
                        out.push(id);
                    }
                }
            }
            Ok(out)
        }

        Selector::Except(base, minus) => {
            let drop = objects(ctx, minus)?;
            Ok(objects(ctx, base)?
                .into_iter()
                .filter(|o| !drop.contains(o))
                .collect())
        }
    }
}

/// Resolve a selector to players.
/// A spell or ability with exactly one chosen target (an empty "up to one" slot is none).
pub(crate) fn single_target(obj: &crate::state::GameObject) -> bool {
    obj.cast_context.as_ref().is_some_and(|c| {
        (0..c.targets.len())
            .filter(|i| !c.empty_slots.contains(&(*i as u8)))
            .count()
            == 1
    })
}

pub fn players(ctx: &Ctx, sel: &Selector) -> Eval<Vec<PlayerId>> {
    match sel {
        Selector::You => Ok(vec![ctx.controller]),
        Selector::ActivePlayer => Ok(vec![ctx.state.active_player]),
        Selector::Player(p) => Ok(vec![*p]),
        Selector::DefendingPlayer => Ok(crate::combat::defending_player(ctx.state, ctx.source)
            .into_iter()
            .collect()),
        Selector::EnchantedPlayer => Ok(ctx
            .state
            .objects
            .get(&ctx.source)
            .or_else(|| ctx.state.last_known.get(&ctx.source))
            .and_then(|o| o.attached_player)
            .into_iter()
            .collect()),
        // Players who have left the game are no one's opponents (CR 800.4a).
        Selector::Opponents => Ok(ctx
            .state
            .apnap()
            .into_iter()
            .filter(|p| *p != ctx.controller)
            .collect()),
        // APNAP order, because effects that act on "each player" do so in that
        // order and any choices they cause are made in it (CR 101.4).
        Selector::EachPlayer => Ok(ctx.state.apnap()),

        Selector::Target { index } => {
            if !ctx.target_is_legal(*index) {
                return Ok(Vec::new());
            }
            match ctx.targets.get(*index as usize) {
                Some(Target::Player(p)) => Ok(vec![*p]),
                Some(Target::Object(_)) => Ok(Vec::new()),
                None => Err(EvalError::MissingTarget(*index)),
            }
        }

        Selector::Bound(b) => Ok(bound(ctx, *b)?
            .iter()
            .filter_map(|t| match t {
                Target::Player(p) => Some(*p),
                Target::Object(_) => None,
            })
            .collect()),

        // An object-denoting selector used where a player is wanted means the
        // controllers of those objects — "its controller".
        Selector::SelfSource | Selector::All { .. } | Selector::TopOfLibrary { .. } => {
            let mut out = Vec::new();
            for id in objects(ctx, sel)? {
                if let Some(p) = crate::layers::controller(ctx.state, id)
                    && !out.contains(&p)
                {
                    out.push(p);
                }
            }
            Ok(out)
        }

        Selector::ChosenBy { .. } => Err(EvalError::UnresolvedChoice),

        Selector::ControllerOf(inner) | Selector::OwnerOf(inner) => {
            let owner = matches!(sel, Selector::OwnerOf(_));
            let mut out = Vec::new();
            for id in objects(ctx, inner)? {
                let live = ctx.state.objects.get(&id);
                let who = if owner {
                    live.or_else(|| ctx.state.last_known.get(&id))
                        .map(|o| o.owner)
                } else {
                    crate::layers::controller(ctx.state, id)
                        .or_else(|| ctx.state.last_known.get(&id).map(|o| o.controller))
                };
                if let Some(p) = who
                    && !out.contains(&p)
                {
                    out.push(p);
                }
            }
            Ok(out)
        }

        Selector::Union(parts) => {
            let mut out = Vec::new();
            for p in parts {
                for id in players(ctx, p)? {
                    if !out.contains(&id) {
                        out.push(id);
                    }
                }
            }
            Ok(out)
        }

        Selector::Except(base, minus) => {
            let drop = players(ctx, minus)?;
            Ok(players(ctx, base)?
                .into_iter()
                .filter(|p| !drop.contains(p))
                .collect())
        }
    }
}

fn bound(ctx: &Ctx, b: Binding) -> Eval<Vec<Target>> {
    ctx.bindings
        .get(&b)
        .cloned()
        .ok_or(EvalError::MissingBinding(b))
}

/// Every object in a zone, across all players for per-player zones.
fn in_zone(state: &GameState, zone: Zone) -> Vec<ObjectId> {
    if zone.is_shared() {
        state.objects_in(ZoneRef::shared(zone))
    } else {
        state
            .turn_order
            .iter()
            .flat_map(|p| state.objects_in(ZoneRef::of(zone, *p)))
            .collect()
    }
}

/// Evaluate a dynamic number.
pub fn value(ctx: &Ctx, v: &Value) -> Eval<i32> {
    Ok(match v {
        Value::Fixed(n) => *n,
        Value::X => ctx.x as i32,
        // A triggered ability has no X of its own; its X slot carries the event's amount
        // (see `detect::event_amount`).
        Value::EventAmount => ctx.x as i32,
        Value::Count(sel) => {
            let objs = objects(ctx, sel)?.len();
            // A selector denoting players counts players.
            if objs == 0 {
                players(ctx, sel)?.len() as i32
            } else {
                objs as i32
            }
        }
        Value::LifeTotal(sel) => players(ctx, sel)?
            .first()
            .map(|p| ctx.state.player(*p).life)
            .unwrap_or(0),
        Value::Counters(sel, kind) => objects(ctx, sel)?
            .iter()
            .map(|id| {
                // Last-known information for an object that has left (CR 608.2h).
                ctx.state
                    .objects
                    .get(id)
                    .or_else(|| ctx.state.last_known.get(id))
                    .and_then(|o| o.counters.get(kind).copied())
                    .unwrap_or(0)
            })
            .sum(),
        Value::Power(sel) => sum_characteristic(ctx, sel, |c| c.power)?,
        Value::Toughness(sel) => sum_characteristic(ctx, sel, |c| c.toughness)?,
        Value::GreatestPower(sel)
        | Value::GreatestToughness(sel)
        | Value::GreatestManaValue(sel) => {
            let mut most = 0;
            for id in objects(ctx, sel)? {
                let c = ctx.characteristics(id)?;
                let n = match v {
                    Value::GreatestPower(_) => c.power.unwrap_or(0),
                    Value::GreatestToughness(_) => c.toughness.unwrap_or(0),
                    _ => c.mana_cost.mana_value() as i32,
                };
                most = most.max(n);
            }
            most
        }
        Value::LeastToughness(sel) => {
            let mut least = None::<i32>;
            for id in objects(ctx, sel)? {
                let t = ctx.characteristics(id)?.toughness.unwrap_or(0);
                least = Some(least.map_or(t, |l| l.min(t)));
            }
            least.unwrap_or(0)
        }
        Value::CardTypesAmong(sel) => {
            let mut types = Vec::new();
            for id in objects(ctx, sel)? {
                for t in &ctx.characteristics(id)?.card_types {
                    if !types.contains(t) {
                        types.push(*t);
                    }
                }
            }
            types.len() as i32
        }
        Value::CardsDrawnThisTurn(who) => players(ctx, who)?
            .iter()
            .map(|p| ctx.state.draws_this_turn.get(p).copied().unwrap_or(0) as i32)
            .sum(),
        Value::SpellsCastThisTurn(who) => players(ctx, who)?
            .iter()
            .map(|p| ctx.state.spells_by_player.get(p).copied().unwrap_or(0) as i32)
            .sum(),
        Value::If {
            cond,
            then,
            otherwise,
        } => {
            if condition(ctx, cond)? {
                value(ctx, then)?
            } else {
                value(ctx, otherwise)?
            }
        }
        Value::MostSpellsCastThisTurn(who) => players(ctx, who)?
            .iter()
            .map(|p| ctx.state.spells_by_player.get(p).copied().unwrap_or(0) as i32)
            .max()
            .unwrap_or(0),
        Value::BasicLandTypesAmong(sel) => {
            let mut seen = [false; 5];
            for id in objects(ctx, sel)? {
                for s in &ctx.characteristics(id)?.subtypes {
                    if let Some(i) = ["Plains", "Island", "Swamp", "Mountain", "Forest"]
                        .iter()
                        .position(|n| ctx.cards.subtype_name(*s) == Some(*n))
                    {
                        seen[i] = true;
                    }
                }
            }
            seen.iter().filter(|s| **s).count() as i32
        }
        Value::PartySize(sel) => {
            const ROLES: [&str; 4] = ["Cleric", "Rogue", "Warrior", "Wizard"];
            // Which roles each creature could fill (a changeling has every creature type).
            let mut fills: Vec<[bool; 4]> = Vec::new();
            for id in objects(ctx, sel)? {
                let subtypes = &ctx.characteristics(id)?.subtypes;
                let roles = ROLES.map(|role| {
                    subtypes
                        .iter()
                        .any(|s| ctx.cards.subtype_name(*s) == Some(role))
                });
                if roles.iter().any(|r| *r) {
                    fills.push(roles);
                }
            }
            party(&fills, 0, &mut vec![false; fills.len()])
        }
        Value::ColorsSpent | Value::ManaSpentOfColor(_) => {
            let spent = ctx
                .state
                .objects
                .get(&ctx.source)
                .map(|o| o.mana_spent.clone())
                .unwrap_or_default();
            match v {
                Value::ManaSpentOfColor(c) => spent
                    .iter()
                    .filter(|(x, _)| *x == Some(*c))
                    .map(|(_, n)| i32::from(*n))
                    .sum(),
                _ => spent.iter().filter(|(c, n)| c.is_some() && *n > 0).count() as i32,
            }
        }
        Value::TimesKicked => ctx.state.objects.get(&ctx.source).map_or(0, |o| {
            o.cast_context
                .as_ref()
                .map_or(o.kicks, |c| c.kicks.max(o.kicks)) as i32
        }),
        Value::OpponentsAttacked => {
            let mut players: Vec<PlayerId> = ctx
                .state
                .combat
                .attackers
                .values()
                .filter_map(|t| match t {
                    Target::Player(p) if *p != ctx.controller => Some(*p),
                    _ => None,
                })
                .collect();
            players.sort();
            players.dedup();
            players.len() as i32
        }
        Value::CastX => ctx
            .state
            .objects
            .get(&ctx.source)
            .or_else(|| ctx.state.last_known.get(&ctx.source))
            .map_or(0, |o| o.cast_x as i32),
        Value::Half { value: v, up } => {
            let v = value(ctx, v)?.max(0);
            if *up { (v + 1) / 2 } else { v / 2 }
        }
        Value::DiedThisTurn(filter) => {
            // Each as it last existed on the battlefield (CR 608.2h): put back into a
            // scratch copy of the state for the filter to read.
            let mut then = ctx.state.clone();
            for id in &ctx.state.died_this_turn {
                if let Some(o) = ctx.state.last_known.get(id) {
                    then.objects.insert(*id, o.clone());
                }
            }
            let c = Ctx {
                state: &then,
                cards: ctx.cards,
                chars: ctx.chars,
                source: ctx.source,
                controller: ctx.controller,
                targets: ctx.targets,
                target_legal: ctx.target_legal,
                x: ctx.x,
                bindings: ctx.bindings,
            };
            let mut n = 0;
            for id in &ctx.state.died_this_turn {
                if matches(&c, filter, *id).unwrap_or(false) {
                    n += 1;
                }
            }
            n
        }
        // Replaced by the number rolled before anything reads it (`Effect::RollDie`).
        Value::RollResult => 0,
        Value::LifeGainedThisTurn(who) => players(ctx, who)?
            .iter()
            .map(|p| ctx.state.life_gained_amount.get(p).copied().unwrap_or(0))
            .sum(),
        Value::StartingLife => ctx.state.player(ctx.controller).starting_life,
        Value::DistinctPowers(sel) => {
            let mut powers = std::collections::BTreeSet::new();
            for id in objects(ctx, sel)? {
                if let Some(p) = ctx.characteristics(id)?.power {
                    powers.insert(p);
                }
            }
            powers.len() as i32
        }
        Value::Speed => i32::from(ctx.state.player(ctx.controller).speed.unwrap_or(0)),
        Value::ColorsAmong(sel) => {
            let mut all = mtg_core::ColorSet::COLORLESS;
            for id in objects(ctx, sel)? {
                all = all.union(ctx.characteristics(id)?.colors);
            }
            all.count() as i32
        }
        Value::Devotion(colors) => {
            use mtg_core::ManaSymbol as M;
            let mut n = 0;
            for id in ctx.state.battlefield() {
                if crate::layers::controller(ctx.state, id) != Some(ctx.controller) {
                    continue;
                }
                let Ok(ch) = ctx.characteristics(id) else {
                    continue;
                };
                for s in &ch.mana_cost.symbols {
                    let counts = match *s {
                        M::Colored(c) | M::MonoHybrid(_, c) | M::Phyrexian(c) => {
                            colors.contains(&c)
                        }
                        M::Hybrid(a, b) => colors.contains(&a) || colors.contains(&b),
                        _ => false,
                    };
                    n += i32::from(counts);
                }
            }
            n
        }
        Value::SpellsCastBefore => ctx
            .state
            .objects
            .get(&ctx.source)
            .and_then(|o| o.cast_context.as_ref())
            .map_or(0, |c| c.cast_index as i32),
        Value::ManaValue(sel) => {
            let mut total = 0;
            for id in objects(ctx, sel)? {
                total += ctx.characteristics(id)?.mana_cost.mana_value() as i32;
            }
            total
        }
        Value::Sum(vs) => {
            let mut t = 0;
            for v in vs {
                t += value(ctx, v)?;
            }
            t
        }
        Value::Product(vs) => {
            let mut t = 1;
            for v in vs {
                t *= value(ctx, v)?;
            }
            t
        }
        Value::Negate(v) => -value(ctx, v)?,
        Value::Max(a, b) => value(ctx, a)?.max(value(ctx, b)?),
        Value::Min(a, b) => value(ctx, a)?.min(value(ctx, b)?),
        Value::ChosenByController { .. } => return Err(EvalError::UnresolvedChoice),
    })
}

fn sum_characteristic(
    ctx: &Ctx,
    sel: &Selector,
    pick: fn(&Characteristics) -> Option<i32>,
) -> Eval<i32> {
    let mut total = 0;
    for id in objects(ctx, sel)? {
        total += pick(&ctx.characteristics(id)?).unwrap_or(0);
    }
    Ok(total)
}

/// Evaluate a condition.
pub fn condition(ctx: &Ctx, c: &Condition) -> Eval<bool> {
    Ok(match c {
        Condition::Always => true,
        Condition::YourTurn => ctx.state.active_player == ctx.controller,
        Condition::NoSpellsLastTurn => ctx
            .state
            .spells_by_player_last_turn
            .values()
            .all(|n| *n == 0),
        Condition::PlayerCastTwoLastTurn => ctx
            .state
            .spells_by_player_last_turn
            .values()
            .any(|n| *n >= 2),
        Condition::YouAreMonarch => ctx.state.monarch == Some(ctx.controller),
        Condition::Saddled => crate::layers::effects(ctx.state, ctx.cards)
            .iter()
            .any(|e| {
                matches!(
                    e.modification,
                    mtg_ir::effect::Modification::Restriction(mtg_ir::effect::Restriction::Saddled)
                ) && crate::layers::applies(ctx.state, ctx.cards, e, ctx.source)
            }),
        Condition::MaxSpeed => ctx.state.player(ctx.controller).speed == Some(4),
        Condition::HasCityBlessing => ctx.state.player(ctx.controller).city_blessing,
        Condition::CreatureDiedThisTurn => ctx.state.died_this_turn.iter().any(|id| {
            ctx.state.last_known.get(id).is_some_and(|o| {
                !o.face_down
                    && ctx.cards.face(o.card, o.face).is_some_and(|f| {
                        f.printed_characteristics()
                            .has_type(mtg_core::CardType::Creature)
                    })
            }) || ctx.state.last_known.get(id).is_some_and(|o| o.face_down)
        }),
        // A permanent remembers where it was cast from; a spell still on the stack knows it
        // from its cast context.
        Condition::WasCast(from) => ctx
            .state
            .objects
            .get(&ctx.source)
            .and_then(|o| {
                o.cast_from
                    .or_else(|| o.cast_context.as_ref().and_then(|c| c.cast_from))
            })
            .is_some_and(|z| from.is_none_or(|f| f == z)),
        Condition::ClassLevelAtLeast(n) => ctx
            .state
            .objects
            .get(&ctx.source)
            .is_some_and(|o| o.class_level.max(1) >= *n),
        Condition::Monstrous => ctx
            .state
            .objects
            .get(&ctx.source)
            .is_some_and(|o| o.monstrous),
        Condition::YouAttackedThisTurn => ctx.state.attacked_this_turn.contains(&ctx.controller),
        Condition::Renowned => ctx
            .state
            .objects
            .get(&ctx.source)
            .is_some_and(|o| o.renowned),
        Condition::CastFor(alt) => ctx.state.objects.get(&ctx.source).is_some_and(|o| {
            o.cast_for == Some(*alt)
                || o.cast_context.as_ref().and_then(|c| c.alt_cost) == Some(*alt)
        }),
        Condition::YouAreAttacked => ctx.state.combat.attackers.values().any(|t| match t {
            mtg_core::Target::Player(p) => *p == ctx.controller,
            mtg_core::Target::Object(o) => {
                crate::layers::controller(ctx.state, *o) == Some(ctx.controller)
            }
        }),
        Condition::YouGainedLifeThisTurn => {
            ctx.state.gained_life_this_turn.contains(&ctx.controller)
        }
        Condition::OpponentLostLifeThisTurn => ctx
            .state
            .lost_life_this_turn
            .iter()
            .any(|p| *p != ctx.controller),
        Condition::OpponentDamagedThisTurn => ctx
            .state
            .damaged_this_turn
            .iter()
            .any(|p| *p != ctx.controller),
        Condition::ControlledSinceLastUpkeep => {
            let since = ctx
                .state
                .upkeeps
                .get(&ctx.controller)
                .map_or(mtg_core::Timestamp::ZERO, |(last, _)| *last);
            ctx.state
                .objects
                .get(&ctx.source)
                .is_some_and(|o| o.timestamp > since)
        }
        Condition::Kicked => ctx
            .state
            .objects
            .get(&ctx.source)
            .is_some_and(|o| o.kicked || o.cast_context.as_ref().is_some_and(|c| c.kicked)),
        Condition::DuringStep(s) => ctx.state.step == *s,
        Condition::TargetsMatching(f) => {
            let Some(cc) = ctx
                .state
                .objects
                .get(&ctx.source)
                .and_then(|o| o.cast_context.as_ref())
            else {
                return Ok(false);
            };
            let mut any = false;
            for (i, t) in cc.targets.iter().enumerate() {
                // Placeholders for slots left empty are not targets.
                if !cc.empty_slots.contains(&(i as u8))
                    && let Target::Object(id) = t
                    && matches(ctx, f, *id)?
                {
                    any = true;
                    break;
                }
            }
            any
        }
        Condition::CountAtLeast { what, at_least } => {
            value(ctx, &Value::Count(Box::new(what.clone())))? >= value(ctx, at_least)?
        }
        Condition::CountAtMost { what, at_most } => {
            value(ctx, &Value::Count(Box::new(what.clone())))? <= value(ctx, at_most)?
        }
        Condition::ValueAtLeast { lhs, rhs } => value(ctx, lhs)? >= value(ctx, rhs)?,
        Condition::ValueEquals { lhs, rhs } => value(ctx, lhs)? == value(ctx, rhs)?,
        Condition::Exists(sel) => !objects(ctx, sel)?.is_empty() || !players(ctx, sel)?.is_empty(),
        Condition::Not(inner) => !condition(ctx, inner)?,
        Condition::And(cs) => {
            for c in cs {
                if !condition(ctx, c)? {
                    return Ok(false);
                }
            }
            true
        }
        Condition::Or(cs) => {
            for c in cs {
                if condition(ctx, c)? {
                    return Ok(true);
                }
            }
            false
        }
    })
}

/// Test an object against a filter.
pub fn matches(ctx: &Ctx, filter: &ObjectFilter, id: ObjectId) -> Eval<bool> {
    let obj = ctx
        .state
        .objects
        .get(&id)
        .ok_or(EvalError::NoSuchObject(id))?;

    Ok(match filter {
        ObjectFilter::Any => true,
        ObjectFilter::IsSelf => id == ctx.source,
        ObjectFilter::InBinding(slot) => bound(ctx, *slot)?.contains(&Target::Object(id)),

        ObjectFilter::HasType(t) => ctx.characteristics(id)?.has_type(*t),
        ObjectFilter::HasSubtype(s) => ctx
            .characteristics(id)?
            .has_subtype(*s, |s| ctx.cards.subtype_name(s)),
        ObjectFilter::HasSupertype(s) => ctx.characteristics(id)?.supertypes.contains(s),
        ObjectFilter::HasColor(c) => ctx.characteristics(id)?.colors.contains(*c),
        ObjectFilter::Colorless => ctx.characteristics(id)?.colors == ColorSet::COLORLESS,
        ObjectFilter::Multicolored => ctx.characteristics(id)?.colors.count() >= 2,
        ObjectFilter::Token => ctx.state.objects.get(&id).is_some_and(|o| o.is_token),
        ObjectFilter::AttachedToSource => ctx
            .state
            .objects
            .get(&id)
            .is_some_and(|o| o.attached_to == Some(ctx.source)),
        ObjectFilter::Named(name) => ctx
            .cards
            .face(obj.card, obj.face)
            .is_some_and(|face| face.name.to_lowercase().replace('−', "-") == *name),
        ObjectFilter::NamedLikeSource => {
            let name = |id: ObjectId| {
                ctx.state
                    .objects
                    .get(&id)
                    .or_else(|| ctx.state.last_known.get(&id))
                    .and_then(|o| ctx.cards.face(o.card, o.face))
                    .map(|f| f.name.clone())
            };
            name(id).is_some_and(|n| Some(n) == name(ctx.source))
        }

        ObjectFilter::Tapped(want) => obj.tapped == *want,
        ObjectFilter::AttackingOrBlocking => {
            ctx.state.combat.is_attacking(id) || ctx.state.combat.is_blocking(id)
        }

        ObjectFilter::Attacking => ctx.state.combat.is_attacking(id),
        ObjectFilter::AttackingAlone => {
            ctx.state.combat.attackers.len() == 1 && ctx.state.combat.is_attacking(id)
        }
        ObjectFilter::Blocking => ctx.state.combat.is_blocking(id),
        ObjectFilter::BlockingSource => ctx
            .state
            .combat
            .blocks
            .get(&ctx.source)
            .is_some_and(|bs| bs.contains(&id)),
        ObjectFilter::AttackedThisTurn => ctx.state.attacked_creatures.contains(&id),
        ObjectFilter::DealtDamageBySelfThisTurn => {
            ctx.state.damaged_by_this_turn.contains(&(ctx.source, id))
        }
        ObjectFilter::AttachedToSelf => ctx
            .state
            .objects
            .get(&ctx.source)
            .is_some_and(|s| s.attached_to == Some(id)),
        ObjectFilter::HasKeyword(k) => ctx.has_keyword(id, *k)?,
        ObjectFilter::ToughnessAtMost(v) => {
            ctx.characteristics(id)?.toughness.unwrap_or(0) <= value(ctx, v)?
        }
        ObjectFilter::ManaValueAtLeast(v) => {
            ctx.characteristics(id)?.mana_cost.mana_value() as i32 >= value(ctx, v)?
        }

        ObjectFilter::ControlledBy(sel) => {
            let who = players(ctx, sel)?;
            crate::layers::controller(ctx.state, id).is_some_and(|p| who.contains(&p))
        }
        ObjectFilter::OwnedBy(sel) => players(ctx, sel)?.contains(&obj.owner),
        ObjectFilter::SharesColorWith(sel) => {
            let mine = ctx.characteristics(id)?.colors;
            let mut shares = false;
            for other in objects(ctx, sel)? {
                shares |= !mine
                    .intersect(ctx.characteristics(other)?.colors)
                    .is_colorless();
            }
            shares
        }

        ObjectFilter::SharesCreatureTypeWith(sel) => {
            let is_type = |s: mtg_core::Subtype| {
                ctx.cards
                    .subtype_name(s)
                    .is_some_and(mtg_core::is_creature_type)
            };
            let mine = ctx.characteristics(id)?;
            let mut shares = false;
            for other in objects(ctx, sel)? {
                let theirs = ctx.characteristics(other)?;
                // A changeling is every creature type: it shares one with anything that
                // has a creature type, including another changeling.
                let any = |c: &mtg_core::Characteristics| {
                    c.every_creature_type || c.subtypes.iter().any(|s| is_type(*s))
                };
                shares |= (mine.every_creature_type && any(&theirs))
                    || (theirs.every_creature_type && any(&mine))
                    || mine
                        .subtypes
                        .iter()
                        .any(|s| is_type(*s) && theirs.subtypes.contains(s));
            }
            shares
        }

        ObjectFilter::PowerAtMost(v) => {
            ctx.characteristics(id)?.power.unwrap_or(0) <= value(ctx, v)?
        }
        ObjectFilter::PowerAtLeast(v) => {
            ctx.characteristics(id)?.power.unwrap_or(0) >= value(ctx, v)?
        }
        ObjectFilter::ManaValueAtMost(v) => {
            ctx.characteristics(id)?.mana_cost.mana_value() as i32 <= value(ctx, v)?
        }

        ObjectFilter::HasCounter(kind) => obj.counters.get(kind).copied().unwrap_or(0) > 0,
        ObjectFilter::HasAnyCounter => obj.counters.values().any(|n| *n > 0),
        ObjectFilter::DealtDamageThisTurn => {
            ctx.state.damaged_by_this_turn.iter().any(|(_, o)| *o == id)
        }
        // Summoning sickness is set on entry and cleared at the start of its
        // controller's turn, so it is exactly "entered this turn" for a creature.
        ObjectFilter::EnteredThisTurn => obj.entered_turn == Some(ctx.state.turn),
        ObjectFilter::HasChosenSubtype => {
            match ctx
                .state
                .objects
                .get(&ctx.source)
                .and_then(|s| s.chosen_subtype)
            {
                Some(s) => ctx
                    .characteristics(id)?
                    .has_subtype(s, |s| ctx.cards.subtype_name(s)),
                None => false,
            }
        }
        ObjectFilter::HasChosenColor => {
            match ctx
                .state
                .objects
                .get(&ctx.source)
                .and_then(|s| s.chosen_color)
            {
                Some(c) => ctx.characteristics(id)?.colors.contains(c),
                None => false,
            }
        }

        // CR 115.4: protection, shroud, hexproof and ward all bear on whether an
        // object can be chosen as a target. Ward does not prevent targeting, only
        // taxes it, so it is excluded here and handled at cost calculation.
        ObjectFilter::Targetable => {
            !obj.phased_out
                && !ctx.has_keyword(id, Keyword::Shroud)?
                && !(ctx.has_keyword(id, Keyword::Hexproof)?
                    && crate::layers::controller(ctx.state, id) != Some(ctx.controller))
                && !protected_from(ctx.state, ctx.cards, id, ctx.source)
        }

        ObjectFilter::CastFromZone(zone) => {
            obj.cast_from
                .or_else(|| obj.cast_context.as_ref().and_then(|c| c.cast_from))
                == Some(*zone)
        }
        ObjectFilter::Kicked => obj.cast_context.as_ref().is_some_and(|c| c.kicked),
        ObjectFilter::IsCommander => ctx.state.commander.is_commander(obj.owner, obj.card),
        ObjectFilter::InZone(zone) => obj.zone.zone == *zone,
        ObjectFilter::SingleTarget => single_target(obj),
        ObjectFilter::HasXInCost => ctx
            .characteristics(id)?
            .mana_cost
            .symbols
            .contains(&mtg_core::ManaSymbol::Variable),
        // CR 112.1 — a spell is a *card* on the stack. An ability on the stack is not
        // one, which is why this cannot just be "is in the stack zone".
        ObjectFilter::IsSpell => {
            obj.zone.zone == Zone::Stack
                && obj
                    .cast_context
                    .as_ref()
                    .is_none_or(|c| c.ability.is_none())
        }
        ObjectFilter::IsAbility => {
            obj.zone.zone == Zone::Stack
                && obj
                    .cast_context
                    .as_ref()
                    .is_some_and(|c| c.ability.is_some())
        }

        ObjectFilter::Not(inner) => !matches(ctx, inner, id)?,
        // Its chosen targets, leaving out empty-slot placeholders.
        ObjectFilter::TargetsObject(inner) => {
            let Some(cc) = obj.cast_context.as_ref() else {
                return Ok(false);
            };
            let mut any = false;
            for (i, t) in cc.targets.iter().enumerate() {
                if let Target::Object(t) = t
                    && !cc.empty_slots.contains(&(i as u8))
                    && matches(ctx, inner, *t)?
                {
                    any = true;
                    break;
                }
            }
            any
        }
        ObjectFilter::And(fs) => {
            for f in fs {
                if !matches(ctx, f, id)? {
                    return Ok(false);
                }
            }
            true
        }
        ObjectFilter::Or(fs) => {
            for f in fs {
                if matches(ctx, f, id)? {
                    return Ok(true);
                }
            }
            false
        }
    })
}

/// Whether an object is a creature. Frequent enough to be worth naming.
pub fn is_creature(ctx: &Ctx, id: ObjectId) -> Eval<bool> {
    Ok(ctx.characteristics(id)?.has_type(CardType::Creature))
}

/// Counters that modify power and toughness, netted out (CR 613.7d).
pub fn net_pt_counters(state: &GameState, id: ObjectId) -> i32 {
    let Some(o) = state.objects.get(&id) else {
        return 0;
    };
    o.counters
        .get(&CounterKind::PlusOnePlusOne)
        .copied()
        .unwrap_or(0)
        - o.counters
            .get(&CounterKind::MinusOneMinusOne)
            .copied()
            .unwrap_or(0)
}

/// CR 702.16 — whether `protected` has protection from `source`: a printed "protection
/// from …" whose quality `source` has. Evaluated from the protected object's side, so
/// "protection from creatures" reads the source's types.
pub fn protected_from(
    state: &GameState,
    cards: &dyn PrintedCards,
    protected: ObjectId,
    source: ObjectId,
) -> bool {
    let Some(obj) = state.objects.get(&protected) else {
        return false;
    };
    if !state.objects.contains_key(&source) {
        return false;
    }
    let chars = ComputedChars(cards);
    // Granted protection, "gains protection from red until end of turn", judged from the
    // protected object's point of view like printed protection.
    let granted = crate::layers::effects(state, cards).into_iter().any(|e| {
        let mtg_ir::effect::Modification::Restriction(mtg_ir::effect::Restriction::Protection {
            from,
            chosen_color: false,
            spares_source,
        }) = &e.modification
        else {
            return false;
        };
        !(*spares_source && e.source == source)
            && crate::layers::applies(state, cards, &e, protected)
            && {
                let ctx = Ctx {
                    state,
                    cards,
                    chars: &chars,
                    source: protected,
                    controller: obj.controller,
                    targets: &[],
                    target_legal: &[],
                    x: 0,
                    bindings: crate::empty_bindings(),
                };
                matches(&ctx, from, source).unwrap_or(false)
            }
    });
    if granted {
        return true;
    }
    let Some(face) = cards.face(obj.card, obj.face) else {
        return false;
    };
    let ctx = Ctx {
        state,
        cards,
        chars: &chars,
        source: protected,
        controller: obj.controller,
        targets: &[],
        target_legal: &[],
        x: 0,
        bindings: crate::empty_bindings(),
    };
    let Ok(ch) = ctx.characteristics(protected) else {
        return false;
    };
    face.abilities.iter().any(|a| {
        ch.abilities.contains(&a.id)
            && matches!(&a.kind, mtg_ir::AbilityKind::Protection { from }
                if state.objects.contains_key(&source)
                    && matches(&ctx, from, source).unwrap_or(false))
    })
}

/// The most roles from `role` on that distinct creatures can fill, each creature filling at
/// most one (CR 700.8).
fn party(fills: &[[bool; 4]], role: usize, used: &mut Vec<bool>) -> i32 {
    if role == 4 {
        return 0;
    }
    let mut best = party(fills, role + 1, used);
    for (i, roles) in fills.iter().enumerate() {
        if roles[role] && !used[i] {
            used[i] = true;
            best = best.max(1 + party(fills, role + 1, used));
            used[i] = false;
            if best == (4 - role) as i32 {
                break;
            }
        }
    }
    best
}
