//! Static read/write analysis over effect trees.
//!
//! This module exists to answer one question:
//!
//! > Two triggered abilities controlled by the same player are about to go on the
//! > stack. The rules say that player chooses the order (CR 603.3b). Can the order
//! > possibly matter?
//!
//! If it cannot, the engine orders them itself and never interrupts. If it can,
//! the engine asks — and, because the analysis names the *specific* resource both
//! abilities touch, it can say why the order matters instead of presenting two
//! indistinguishable list items.
//!
//! The analysis is a conservative approximation. [`Resource::Unanalysable`]
//! swallows anything not understood, and an unanalysable footprint conflicts with
//! everything, so the failure mode is "asks unnecessarily" rather than "silently
//! picks the wrong order". That direction is not negotiable: a wrong auto-order
//! is a rules violation, whereas a redundant prompt is only friction.

use std::collections::BTreeSet;

use mtg_core::{CounterKind, ObjectId, PlayerId, Zone};
use serde::{Deserialize, Serialize};

use crate::{
    effect::{Effect, Modification},
    selector::Selector,
    value::Value,
};

/// A named piece of game state an effect may touch.
///
/// Granularity is deliberately object-level rather than field-level. Two effects
/// that both touch the same permanent are treated as conflicting even if one
/// taps it and the other adds a counter, because a third effect could make those
/// interact (a counter could grant an ability that cares about tapping). Finer
/// granularity would need whole-program reasoning about the layer system.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
pub enum Resource {
    /// Mutable per-object state and computed characteristics of one object.
    Object(ObjectId),
    /// Every object matching a description the analysis could not narrow to
    /// concrete ids. Conflicts with any `Object` and with any other `ObjectClass`.
    ObjectClass,
    Life(PlayerId),
    /// A player's mana pool. Separate from life because mana abilities touch it
    /// constantly and would otherwise make every trigger look like it conflicts.
    ManaPool(PlayerId),
    /// The contents or order of one player's zone.
    ZoneOf(PlayerId, Zone),
    SharedZone(Zone),
    Counters(ObjectId, CounterKind),
    /// The set of live continuous effects, i.e. the layer system's input.
    ContinuousEffects,
    Stack,
    TurnState,
    /// Analysis gave up. Conflicts with everything, including itself.
    Unanalysable,
}

impl Resource {
    fn is_wildcard(&self) -> bool {
        matches!(self, Resource::Unanalysable | Resource::ObjectClass)
    }
}

/// What an effect reads and what it writes.
#[derive(Clone, Default, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Footprint {
    pub reads: BTreeSet<Resource>,
    pub writes: BTreeSet<Resource>,
    /// Resources only ever increased, by a known non-negative amount — gaining life.
    ///
    /// Kept apart from `writes` because increases commute with each other: gaining 1 and
    /// then 2 life ends where gaining 2 and then 1 does, and nothing between them can
    /// differ in a way the rules check (life going *up* never makes a player lose).
    /// An increase still conflicts with anything that reads, writes or decreases the
    /// same resource.
    #[serde(default)]
    pub gains: BTreeSet<Resource>,
    /// Resources only ever decreased, by a known non-negative amount — losing life.
    ///
    /// Decreases commute with each other too: if the first takes a player to 0 they lose
    /// before the second resolves, and in the other order they lose after it — a lost game
    /// either way. A decrease mixed with an increase does *not* commute, because
    /// state-based actions between them can end the game in one order and not the other.
    #[serde(default)]
    pub losses: BTreeSet<Resource>,
    /// The effect asks its controller a question during resolution. Two effects
    /// that both prompt are never auto-ordered, because the player's answer to
    /// the first can reasonably depend on having seen the second resolve.
    pub prompts: bool,
}

impl Footprint {
    pub fn unanalysable() -> Self {
        Self {
            reads: BTreeSet::from([Resource::Unanalysable]),
            writes: BTreeSet::from([Resource::Unanalysable]),
            prompts: true,
            ..Default::default()
        }
    }

    fn reading(r: Resource) -> Self {
        Self {
            reads: BTreeSet::from([r]),
            ..Default::default()
        }
    }

    fn writing(r: Resource) -> Self {
        Self {
            writes: BTreeSet::from([r]),
            ..Default::default()
        }
    }

    pub fn merge(&mut self, other: Footprint) {
        self.reads.extend(other.reads);
        self.writes.extend(other.writes);
        self.gains.extend(other.gains);
        self.losses.extend(other.losses);
        self.prompts |= other.prompts;
    }

    fn with(mut self, other: Footprint) -> Self {
        self.merge(other);
        self
    }

    /// True when no execution order of the two effects can produce different
    /// results — the standard read/write commutativity test, plus the prompt rule.
    ///
    /// `a` and `b` commute when neither writes anything the other reads or
    /// writes. Read/read overlap is fine: two effects that both count your
    /// creatures without changing anything cannot observe each other. Increases
    /// commute with increases and decreases with decreases (see `gains`), but a
    /// change of either kind conflicts with a read or write of the same resource.
    pub fn commutes_with(&self, other: &Footprint) -> bool {
        if self.prompts && other.prompts {
            return false;
        }
        if self.has_wildcard() || other.has_wildcard() {
            return false;
        }
        let (mine, theirs) = (self.changed(), other.changed());
        !intersects(&mine, &other.reads)
            && !intersects(&theirs, &self.reads)
            && !intersects(&self.writes, &theirs)
            && !intersects(&other.writes, &mine)
            && !intersects(&self.gains, &other.losses)
            && !intersects(&self.losses, &other.gains)
    }

    /// Everything this changes in any way: written, increased or decreased.
    fn changed(&self) -> BTreeSet<Resource> {
        self.writes
            .iter()
            .chain(&self.gains)
            .chain(&self.losses)
            .cloned()
            .collect()
    }

    fn has_wildcard(&self) -> bool {
        self.reads
            .iter()
            .chain(&self.writes)
            .any(Resource::is_wildcard)
    }

    /// The resources responsible for a conflict, for the UI's explanation of why
    /// an ordering prompt is being shown.
    pub fn conflicts_with(&self, other: &Footprint) -> Vec<Resource> {
        let mut out: BTreeSet<Resource> = BTreeSet::new();
        let (mine, theirs) = (self.changed(), other.changed());
        for w in &mine {
            let clash = other.reads.contains(w)
                || other.writes.contains(w)
                || self.writes.contains(w) && theirs.contains(w)
                || self.gains.contains(w) && other.losses.contains(w)
                || self.losses.contains(w) && other.gains.contains(w);
            if clash || w.is_wildcard() {
                out.insert(w.clone());
            }
        }
        for w in &theirs {
            if self.reads.contains(w) || w.is_wildcard() {
                out.insert(w.clone());
            }
        }
        out.into_iter().collect()
    }
}

fn mtg_ir_is_ambiguous(o: &crate::effect::ManaOutput) -> bool {
    o.is_ambiguous()
}

fn intersects(a: &BTreeSet<Resource>, b: &BTreeSet<Resource>) -> bool {
    a.intersection(b).next().is_some()
}

/// Resolves selectors to concrete ids. Implemented by the engine, which is the
/// only thing that knows the game state; the analysis itself stays pure.
pub trait SelectorResolver {
    /// Concrete objects the selector matches right now, or `None` when the
    /// selector cannot be pinned down (it depends on a choice not yet made, or on
    /// state the other effect might change).
    fn resolve_objects(&self, sel: &Selector) -> Option<Vec<ObjectId>>;
    /// Players the selector matches, or `None` if indeterminate.
    fn resolve_players(&self, sel: &Selector) -> Option<Vec<PlayerId>>;
}

/// Derive the footprint of an effect tree.
///
/// Called once per pending trigger, after targets are chosen, so selectors are as
/// concrete as they will ever be.
pub fn analyse(effect: &Effect, r: &dyn SelectorResolver) -> Footprint {
    match effect {
        // Global prevention can interact with any combat-damage event.
        Effect::PreventAllCombatDamage => Footprint::unanalysable(),
        Effect::PreventDamage { .. } | Effect::PreventDamageShield { .. } => {
            Footprint::unanalysable()
        }
        Effect::Nothing => Footprint::default(),
        Effect::GainClassLevel { .. } => objects_write(&Selector::SelfSource, r),
        Effect::BecomeMonarch { .. }
        | Effect::Dig { .. }
        | Effect::ExtraTurn { .. }
        | Effect::PutAttacking { .. }
        | Effect::SpendOnly { .. }
        | Effect::ExileIfDiesThisTurn { .. }
        | Effect::Reflexive { .. } => Footprint::unanalysable(),
        Effect::GrantPlay { what, .. } | Effect::GrantCastLater { what } => objects_write(what, r),

        Effect::Sequence(items) => items
            .iter()
            .fold(Footprint::default(), |acc, e| acc.with(analyse(e, r))),

        Effect::If {
            cond,
            then,
            otherwise,
        } => {
            // Both arms are possible, so the footprint is their union.
            let mut f = condition_footprint(cond, r);
            f.merge(analyse(then, r));
            f.merge(analyse(otherwise, r));
            f
        }

        // A modal effect's mode is chosen at announcement, so by the time this
        // runs the mode is known — but the IR does not carry the choice, so be
        // conservative and union every mode.
        Effect::Modal { modes, .. } => modes.iter().fold(
            Footprint {
                prompts: true,
                ..Default::default()
            },
            |acc, (_, e)| acc.with(analyse(e, r)),
        ),

        Effect::Let { what, body, .. } => selector_read(what, r).with(analyse(body, r)),
        Effect::ForEach { what, body } => selector_read(what, r).with(analyse(body, r)),
        Effect::Repeat { times, body } => value_footprint(times, r).with(analyse(body, r)),

        // An optional effect prompts by construction.
        Effect::May {
            then, otherwise, ..
        } => {
            let f = Footprint {
                prompts: true,
                ..Default::default()
            }
            .with(analyse(then, r));
            match otherwise {
                Some(o) => f.with(analyse(o, r)),
                None => f,
            }
        }
        Effect::UnlessPays { otherwise, .. } => {
            Footprint::unanalysable().with(analyse(otherwise, r))
        }
        Effect::MayPay { then, .. } => {
            // Paying a cost can touch anything the cost names; treat it as a prompt
            // plus an unanalysable write until costs get their own analysis.
            Footprint::unanalysable().with(analyse(then, r))
        }

        // Looks at hidden cards and asks: never auto-ordered.
        Effect::ReorderLibraryTop { .. } => Footprint::unanalysable(),
        // Poison can end the game (CR 704.5c): never auto-ordered.
        Effect::GivePoison { .. } => Footprint::unanalysable(),
        // Energy is read only by costs and counts; nothing orders against it.
        Effect::GainEnergy { who, amount } => {
            selector_read(who, r).with(value_footprint(amount, r))
        }
        Effect::GainLife { who, amount } => {
            life_change(who, amount, r, true).with(value_footprint(amount, r))
        }
        Effect::LoseLife { who, amount } => {
            life_change(who, amount, r, false).with(value_footprint(amount, r))
        }

        Effect::AddMana { who, produces } => {
            let mut f = players_write(who, r, Resource::ManaPool);
            // A source with a choice of colours asks its controller which to make.
            f.prompts = produces.iter().any(mtg_ir_is_ambiguous);
            f
        }

        Effect::Draw { who, count } => {
            zones_write(who, r, &[Zone::Library, Zone::Hand]).with(value_footprint(count, r))
        }
        Effect::Discard { who, count, .. } => {
            let mut f = zones_write(who, r, &[Zone::Hand, Zone::Graveyard]);
            f.merge(value_footprint(count, r));
            // Which card to discard is the player's choice.
            f.prompts = true;
            f
        }

        Effect::MoveZone { what, to, .. } => {
            let mut f = objects_write(what, r);
            f.writes.insert(Resource::SharedZone(*to));
            f.writes.insert(Resource::Stack);
            f
        }

        Effect::Shuffle { who } => zones_write(who, r, &[Zone::Library]),
        Effect::LookAndSort {
            who,
            count,
            keep_zone,
            other_zone,
        } => {
            let mut f = zones_write(who, r, &[*keep_zone, *other_zone, Zone::Library]);
            f.merge(value_footprint(count, r));
            f.prompts = true;
            f
        }

        Effect::Destroy { what }
        | Effect::Regenerate { what }
        | Effect::Tap { what }
        | Effect::Untap { what } => objects_write(what, r),
        Effect::Sacrifice { what, .. } => {
            let mut f = objects_write(what, r);
            f.prompts = true;
            f
        }
        Effect::DealDamage { source, to, amount } => objects_read(source, r)
            .with(objects_write(to, r))
            .with(value_footprint(amount, r)),

        Effect::DealDamageDivided { source, shares } => {
            shares.iter().fold(objects_read(source, r), |f, to| {
                f.with(objects_write(to, r))
            })
        }

        Effect::AddCounters { what, kind, amount }
        | Effect::RemoveCounters { what, kind, amount } => {
            let mut f = objects_write(what, r);
            if let Some(ids) = r.resolve_objects(what) {
                for id in ids {
                    f.writes.insert(Resource::Counters(id, *kind));
                }
            }
            f.with(value_footprint(amount, r))
        }

        Effect::Attach { what, to } => objects_write(what, r).with(objects_write(to, r)),

        Effect::CreateToken {
            count, controller, ..
        } => {
            let mut f = Footprint::writing(Resource::SharedZone(Zone::Battlefield));
            f.merge(value_footprint(count, r));
            f.merge(selector_read(controller, r));
            f
        }

        Effect::ExileIfLeaves { what }
        | Effect::Transform { what }
        | Effect::BecomeRenowned { what }
        | Effect::BecomeMonstrous { what } => objects_write(what, r),
        Effect::ExileUntilSourceLeaves { what } => {
            objects_write(what, r).with(Footprint::writing(Resource::SharedZone(Zone::Exile)))
        }
        // Reveals hidden information and may ask a question.
        Effect::Explore { .. }
        | Effect::ExileReturnTransformed { .. }
        | Effect::Connive { .. }
        | Effect::EnterAttacking { .. }
        | Effect::Cascade
        | Effect::Proliferate
        | Effect::RevealHandChoose { .. }
        | Effect::Madness { .. } => Footprint::unanalysable(),
        Effect::CreateTokenCopy {
            of,
            count,
            controller,
        } => {
            let mut f = Footprint::writing(Resource::SharedZone(Zone::Battlefield));
            f.merge(selector_read(of, r));
            f.merge(value_footprint(count, r));
            f.merge(selector_read(controller, r));
            f
        }

        Effect::GainControl { what, who, .. } => objects_write(what, r)
            .with(selector_read(who, r))
            .with(Footprint::writing(Resource::ContinuousEffects)),

        Effect::CounterSpell { what, .. } | Effect::CopySpell { what, .. } => {
            objects_write(what, r).with(Footprint::writing(Resource::Stack))
        }
        // Asks another player whether to pay, so it always prompts.
        Effect::CounterUnlessPays { what, .. } => {
            let mut f = objects_write(what, r).with(Footprint::writing(Resource::Stack));
            f.prompts = true;
            f
        }
        Effect::CastWithoutPaying { .. } => Footprint::unanalysable(),
        Effect::Fight { a, b } => objects_write(a, r).with(objects_write(b, r)),
        // Only schedules something for later; nothing now is read or written.
        Effect::Delayed { .. } => Footprint::default(),

        Effect::Continuous {
            what, modification, ..
        } => {
            let mut f = objects_write(what, r);
            f.writes.insert(Resource::ContinuousEffects);
            f.merge(modification_footprint(modification, r));
            f
        }

        Effect::Native { .. } => Footprint::unanalysable(),
    }
}

fn modification_footprint(m: &Modification, r: &dyn SelectorResolver) -> Footprint {
    match m {
        Modification::CopyOf(s) | Modification::Control(s) => selector_read(s, r),
        Modification::SetBasePower(v) => value_footprint(v, r),
        Modification::SetBasePowerToughness { power, toughness }
        | Modification::ModifyPowerToughness { power, toughness } => {
            value_footprint(power, r).with(value_footprint(toughness, r))
        }
        _ => Footprint::default(),
    }
}

fn condition_footprint(c: &crate::trigger::Condition, r: &dyn SelectorResolver) -> Footprint {
    use crate::trigger::Condition as C;
    match c {
        C::Always
        | C::YourTurn
        | C::DuringStep(_)
        | C::Kicked
        | C::ControlledSinceLastUpkeep
        | C::OpponentDamagedThisTurn
        | C::YouAreAttacked
        | C::OpponentLostLifeThisTurn
        | C::CastFor(_)
        | C::Renowned
        | C::YouAttackedThisTurn
        | C::Monstrous
        | C::ClassLevelAtLeast(_)
        | C::WasCast(_)
        | C::CreatureDiedThisTurn
        | C::YouAreMonarch
        | C::MaxSpeed
        | C::Saddled
        | C::NoSpellsLastTurn
        | C::PlayerCastTwoLastTurn => Footprint::default(),
        // Only read while the spell's cost is determined (CR 601.2f).
        C::TargetsMatching(_) => Footprint::default(),
        C::CountAtLeast { what, at_least } => {
            selector_read(what, r).with(value_footprint(at_least, r))
        }
        C::CountAtMost { what, at_most } => {
            selector_read(what, r).with(value_footprint(at_most, r))
        }
        C::ValueAtLeast { lhs, rhs } | C::ValueEquals { lhs, rhs } => {
            value_footprint(lhs, r).with(value_footprint(rhs, r))
        }
        C::Exists(s) => selector_read(s, r),
        C::Not(inner) => condition_footprint(inner, r),
        C::And(cs) | C::Or(cs) => cs.iter().fold(Footprint::default(), |acc, c| {
            acc.with(condition_footprint(c, r))
        }),
    }
}

fn value_footprint(v: &Value, r: &dyn SelectorResolver) -> Footprint {
    match v {
        Value::Fixed(_) => Footprint::default(),
        // X is fixed at announcement, so reading it observes nothing mutable.
        Value::X | Value::EventAmount | Value::ColorsSpent | Value::ManaSpentOfColor(_) => {
            Footprint::default()
        }
        // Fixed as the spell was cast.
        Value::SpellsCastBefore => Footprint::default(),
        Value::Count(s)
        | Value::ManaValue(s)
        | Value::CardTypesAmong(s)
        | Value::PartySize(s)
        | Value::BasicLandTypesAmong(s)
        | Value::SpellsCastThisTurn(s) => selector_read(s, r),
        Value::Power(s) | Value::Toughness(s) | Value::LeastToughness(s) => objects_read(s, r),
        Value::LifeTotal(s) => players_read(s, r, Resource::Life),
        Value::Counters(s, kind) => {
            let mut f = objects_read(s, r);
            if let Some(ids) = r.resolve_objects(s) {
                for id in ids {
                    f.reads.insert(Resource::Counters(id, *kind));
                }
            }
            f
        }
        Value::Sum(vs) | Value::Product(vs) => vs.iter().fold(Footprint::default(), |acc, v| {
            acc.with(value_footprint(v, r))
        }),
        Value::Negate(v) => value_footprint(v, r),
        Value::Max(a, b) | Value::Min(a, b) => value_footprint(a, r).with(value_footprint(b, r)),
        Value::ChosenByController { min, max } => {
            let mut f = value_footprint(min, r).with(value_footprint(max, r));
            f.prompts = true;
            f
        }
    }
}

/// A selector used only to look something up. Objects *and* players, since a
/// selector can denote either.
fn selector_read(s: &Selector, r: &dyn SelectorResolver) -> Footprint {
    let mut f = objects_read(s, r);
    f.merge(players_read(s, r, Resource::Life));
    f
}

fn objects_read(s: &Selector, r: &dyn SelectorResolver) -> Footprint {
    match r.resolve_objects(s) {
        Some(ids) => Footprint {
            reads: ids.into_iter().map(Resource::Object).collect(),
            prompts: selector_prompts(s),
            ..Default::default()
        },
        None => Footprint::reading(Resource::ObjectClass),
    }
}

fn objects_write(s: &Selector, r: &dyn SelectorResolver) -> Footprint {
    match r.resolve_objects(s) {
        Some(ids) => Footprint {
            writes: ids.into_iter().map(Resource::Object).collect(),
            prompts: selector_prompts(s),
            ..Default::default()
        },
        None => Footprint::writing(Resource::ObjectClass),
    }
}

fn players_read(s: &Selector, r: &dyn SelectorResolver, f: fn(PlayerId) -> Resource) -> Footprint {
    match r.resolve_players(s) {
        Some(ps) => Footprint {
            reads: ps.into_iter().map(f).collect(),
            ..Default::default()
        },
        None => Footprint::reading(Resource::Unanalysable),
    }
}

fn players_write(s: &Selector, r: &dyn SelectorResolver, f: fn(PlayerId) -> Resource) -> Footprint {
    match r.resolve_players(s) {
        Some(ps) => Footprint {
            writes: ps.into_iter().map(f).collect(),
            ..Default::default()
        },
        None => Footprint::writing(Resource::Unanalysable),
    }
}

/// A life gain (`up`) or loss. Only an amount known not to be negative counts as a pure
/// increase or decrease; anything else (a power that could be negative, say) is an
/// ordinary write, which conflicts with every other change to that life total.
fn life_change(who: &Selector, amount: &Value, r: &dyn SelectorResolver, up: bool) -> Footprint {
    let signed = match amount {
        Value::Fixed(n) if *n >= 0 => Some(up),
        // A negative gain is a loss, and the reverse.
        Value::Fixed(_) => Some(!up),
        // X is announced as a non-negative number.
        Value::X | Value::EventAmount | Value::ColorsSpent | Value::ManaSpentOfColor(_) => Some(up),
        _ => None,
    };
    let mut f = players_write(who, r, Resource::Life);
    match signed {
        Some(true) => f.gains = std::mem::take(&mut f.writes),
        Some(false) => f.losses = std::mem::take(&mut f.writes),
        None => {}
    }
    f
}

fn zones_write(s: &Selector, r: &dyn SelectorResolver, zones: &[Zone]) -> Footprint {
    match r.resolve_players(s) {
        Some(ps) => {
            let mut w = BTreeSet::new();
            for p in ps {
                for z in zones {
                    w.insert(Resource::ZoneOf(p, *z));
                }
            }
            Footprint {
                writes: w,
                ..Default::default()
            }
        }
        None => Footprint::writing(Resource::Unanalysable),
    }
}

/// Selectors that make the player choose during resolution.
fn selector_prompts(s: &Selector) -> bool {
    matches!(s, Selector::ChosenBy { .. })
}
