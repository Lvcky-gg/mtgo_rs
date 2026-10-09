//! The layer system (CR 613).
//!
//! Every question of the form "what is this permanent actually like right now?"
//! goes through here. The answer is computed from scratch each time, by applying
//! every live continuous effect in a fixed order:
//!
//! | Layer | What it changes |
//! |-------|-----------------|
//! | 1 | copy effects |
//! | 2 | control-changing |
//! | 3 | text-changing |
//! | 4 | type-changing |
//! | 5 | color-changing |
//! | 6 | ability adding and removing |
//! | 7a | characteristic-defining power/toughness |
//! | 7b | effects that set power/toughness to a specific value |
//! | 7c | effects that modify power/toughness (`+N/+N`) |
//! | 7d | counters |
//! | 7e | switching power and toughness |
//!
//! Within a layer, effects apply in timestamp order (CR 613.7), except where one
//! effect *depends* on another, in which case the dependency applies first
//! (CR 613.8).
//!
//! Why this is a separate pass rather than something the effects do directly:
//! continuous effects are not events. An anthem does not "add +1/+1 once"; it
//! makes a statement that is continuously true, and whether it applies to a given
//! creature can change without anything happening. The only correct model is to
//! recompute, which is why [`state::GameObject`] stores no computed values.
//!
//! Recomputing on every read would be too slow for a UI that redraws the board
//! every frame, so [`CharacteristicsCache`] memoises against
//! `GameState::generation`. The cache is invalidated wholesale on any mutation
//! rather than selectively, because working out what a mutation *could* have
//! changed is exactly the reasoning this design exists to avoid.

use std::collections::BTreeMap;

use mtg_core::{Characteristics, ObjectId, PlayerId, Timestamp};
use mtg_ir::Selector;

use crate::eval::CharacteristicsSource;

use crate::state::{AffectedSet, ContinuousEffect, GameState};

/// Layer numbers, times ten so sublayers fit as 70..=79.
pub mod layer {
    pub const COPY: u8 = 10;
    pub const CONTROL: u8 = 20;
    pub const TEXT: u8 = 30;
    pub const TYPE: u8 = 40;
    pub const COLOR: u8 = 50;
    pub const ABILITY: u8 = 60;
    pub const PT_CDA: u8 = 70;
    pub const PT_SET: u8 = 71;
    pub const PT_MODIFY: u8 = 72;
    pub const PT_COUNTERS: u8 = 73;
    pub const PT_SWITCH: u8 = 74;
}

/// Memoised characteristics, valid only for one `GameState::generation`.
#[derive(Default, Debug)]
pub struct CharacteristicsCache {
    generation: Timestamp,
    entries: BTreeMap<ObjectId, Characteristics>,
}

impl CharacteristicsCache {
    /// Look up an object's current characteristics, recomputing if the state has
    /// moved on since the cache was filled.
    pub fn get<'c>(
        &'c mut self,
        state: &GameState,
        printed: &dyn PrintedCards,
        id: ObjectId,
    ) -> Option<&'c Characteristics> {
        if self.generation != state.generation {
            self.entries.clear();
            self.generation = state.generation;
        }
        match self.entries.entry(id) {
            std::collections::btree_map::Entry::Occupied(e) => Some(e.into_mut()),
            std::collections::btree_map::Entry::Vacant(e) => {
                Some(e.insert(compute(state, printed, id)?))
            }
        }
    }

    pub fn invalidate(&mut self) {
        self.entries.clear();
    }
}

/// Re-exported so existing engine code and tests keep one import path. The trait itself
/// lives in `mtg-ir`, next to the card representation it describes.
pub use mtg_ir::PrintedCards;

/// Apply every continuous effect to one object, in layer order.
///
/// Sublayer order inside layer 7 is where this is easy to get wrong, so it is
/// explicit: characteristic-defining values, then effects that *set* power and
/// toughness, then effects that *modify* them, then counters, and only then the
/// switch sublayer. Counters are not continuous effects — they are object state —
/// but they apply at a defined point in that sequence, so they are slotted in
/// rather than tacked on at the end. Applying them after a switch effect would
/// produce visibly wrong numbers.
pub fn compute(
    state: &GameState,
    printed: &dyn PrintedCards,
    id: ObjectId,
) -> Option<Characteristics> {
    let obj = state.objects.get(&id)?;
    let mut ch = if obj.face_down {
        Characteristics::face_down()
    } else if printed.layout(obj.card) == mtg_ir::Layout::Split
        && obj.zone.zone != mtg_core::Zone::Stack
        && obj.cast_context.is_none()
    {
        printed.split_characteristics(obj.card)?
    } else {
        let mut ch = printed.face(obj.card, obj.face)?.printed_characteristics();
        // CR 712.8e — a transformed permanent's mana value is its front face's. Its back
        // has no mana cost of its own, so the front's stands in.
        if obj.face != 0
            && printed.layout(obj.card) == mtg_ir::Layout::Transforming
            && let Some(front) = printed.face(obj.card, 0)
        {
            ch.mana_cost = front.mana_cost.clone();
        }
        ch
    };
    // CR 702.103b — a bestowed Aura is an Aura enchantment, and no creature.
    if obj.bestowed() {
        ch.card_types.retain(|t| *t != mtg_core::CardType::Creature);
        if !ch.card_types.contains(&mtg_core::CardType::Enchantment) {
            ch.card_types.push(mtg_core::CardType::Enchantment);
        }
        ch.subtypes.retain(|s| {
            !printed
                .subtype_name(*s)
                .is_some_and(mtg_core::is_creature_type)
        });
        ch.every_creature_type = false;
        if let Some(aura) = printed.subtype_named("Aura") {
            ch.subtypes.push(aura);
        }
        ch.power = None;
        ch.toughness = None;
    }

    // Created once, on the stack, and threaded through: every selector and value
    // inside a continuous effect is evaluated against printed characteristics.
    let chars = crate::eval::PrintedChars(printed);
    let effects = ordered_effects(state, printed, id);
    // Whether a "creatures that are …" effect reaches this object is decided as its layer
    // comes up, from what the object has become so far: a creature that became blue in
    // layer 5 gets "blue creatures get +1/+1" in layer 7 (CR 613.1). Other objects are
    // still read as printed.
    let reaches = |effect: &ContinuousEffect, ch: &Characteristics| match effect.affected {
        AffectedSet::Fixed(_) => true,
        AffectedSet::Dynamic(_) => {
            let so_far = SoFar {
                id,
                ch,
                printed: &chars,
            };
            affects(state, printed, &so_far, effect, id)
        }
    };

    // Layers 1 through 7c.
    for effect in effects.iter().filter(|e| e.layer <= layer::PT_MODIFY) {
        if reaches(effect, &ch) {
            apply_one(&mut ch, effect, state, printed, &chars, obj);
        }
    }

    // Layer 7d — counters.
    let net = crate::eval::net_pt_counters(state, id);
    if net != 0 {
        if let Some(p) = ch.power.as_mut() {
            *p += net;
        }
        if let Some(t) = ch.toughness.as_mut() {
            *t += net;
        }
    }

    // Layer 7e — switching, which must see the post-counter values.
    for effect in effects.iter().filter(|e| e.layer >= layer::PT_SWITCH) {
        if reaches(effect, &ch) {
            apply_one(&mut ch, effect, state, printed, &chars, obj);
        }
    }

    Some(ch)
}

/// The object being computed as it stands partway through the layers, and every other
/// object as printed.
struct SoFar<'a> {
    id: ObjectId,
    ch: &'a Characteristics,
    printed: &'a crate::eval::PrintedChars<'a>,
}

impl CharacteristicsSource for SoFar<'_> {
    fn characteristics(&self, state: &GameState, id: ObjectId) -> Option<Characteristics> {
        if id == self.id {
            Some(self.ch.clone())
        } else {
            self.printed.characteristics(state, id)
        }
    }
}

/// Continuous effects that may affect `id`, sorted into application order. Fixed sets
/// are settled here; dynamic ones are decided layer by layer in [`compute`].
///
/// Sorting is by `(layer, timestamp)`, then dependency-corrected. Dependency
/// handling (CR 613.8) is the subtle part: effect A depends on B when applying B
/// would change what A applies to, or what A does. The correct algorithm is to
/// repeatedly pick, among the effects not yet applied in this layer, one that
/// nothing else in that set depends on.
fn ordered_effects(
    state: &GameState,
    printed: &dyn PrintedCards,
    id: ObjectId,
) -> Vec<ContinuousEffect> {
    let mut applicable: Vec<ContinuousEffect> = effects(state, printed)
        .into_iter()
        .filter(|e| match &e.affected {
            AffectedSet::Fixed(ids) => ids.contains(&id),
            AffectedSet::Dynamic(_) => true,
        })
        .collect();

    applicable.sort_by_key(|e| (e.layer, e.timestamp));
    dependency_sort(&mut applicable);
    applicable
}

/// Reorder within each layer so that dependencies apply first (CR 613.8).
///
/// Left as a stable no-op beyond timestamp order for now: dependency loops are
/// rare, and CR 613.8b says that when effects depend on each other in a loop,
/// timestamp order wins anyway. Implementing this properly needs
/// `would_change_application`, which is a genuine piece of work and is tracked as
/// its own task rather than faked here.
fn dependency_sort(_effects: &mut [ContinuousEffect]) {}

/// Every continuous effect in force.
///
/// Two kinds: those created by resolving spells and abilities, which live in
/// `GameState::continuous`, and those generated by the **static abilities** of
/// permanents on the battlefield (CR 604.2, CR 611.3). The second kind is derived here
/// on every read rather than stored, because a static ability's effect exists exactly
/// while its source is on the battlefield — storing it would mean keeping a copy in
/// step with every zone change, which is the bookkeeping this module avoids.
///
/// A static effect takes its source's timestamp (CR 613.7a) and re-evaluates its
/// affected set on every pass.
///
/// A permanent that loses all its abilities (layer 6) has no static abilities, so they
/// generate no effects — not even in layer 7, which comes later (CR 613.1f). Which
/// permanents lose them is read from printed characteristics, like every affected set.
/// The ability that removes them keeps applying, even to its own source (CR 613.6).
/// Not modelled: removal decided by characteristics another effect changed (CR 613.8).
pub fn effects(state: &GameState, printed: &dyn PrintedCards) -> Vec<ContinuousEffect> {
    let mut out = state.continuous.clone();
    let mut statics = Vec::new();
    let spells = state
        .objects_in(mtg_core::ZoneRef::shared(mtg_core::Zone::Stack))
        .into_iter()
        .filter(|id| {
            state
                .objects
                .get(id)
                .is_some_and(|o| o.cast_context.as_ref().is_none_or(|c| c.ability.is_none()))
        })
        .map(|id| (id, true));
    for (id, on_stack) in state
        .battlefield()
        .into_iter()
        .map(|id| (id, false))
        .chain(spells)
    {
        let Some(obj) = state.objects.get(&id) else {
            continue;
        };
        // A face-down permanent has no abilities (CR 708.2).
        if obj.phased_out || obj.face_down {
            continue;
        }
        let Some(face) = printed.face(obj.card, obj.face) else {
            continue;
        };
        for ability in &face.abilities {
            let mtg_ir::AbilityKind::Static {
                what,
                modification,
                condition,
            } = &ability.kind
            else {
                continue;
            };
            // A spell's static abilities work only on the spell itself ("this spell
            // can't be countered"); the rest wait until it is a permanent.
            if on_stack && *what != Selector::SelfSource {
                continue;
            }
            // "As long as …": evaluated from the source's point of view, against printed
            // characteristics like any affected set (CR 613.6), which also keeps this out
            // of the layer computation it is part of.
            if let Some(cond) = condition {
                let chars = crate::eval::PrintedChars(printed);
                let ctx = crate::eval::Ctx {
                    state,
                    cards: printed,
                    chars: &chars,
                    source: id,
                    controller: obj.controller,
                    targets: &[],
                    target_legal: &[],
                    x: 0,
                    bindings: crate::empty_bindings(),
                };
                if !crate::eval::condition(&ctx, cond).unwrap_or(false) {
                    continue;
                }
            }
            statics.push(ContinuousEffect {
                id,
                source: id,
                affected: AffectedSet::Dynamic(what.clone()),
                modification: modification.clone(),
                duration: mtg_ir::effect::Duration::WhileSourcePresent,
                timestamp: obj.timestamp,
                layer: crate::resolve::layer_of(modification),
                ability: Some(ability.id),
                controller: None,
            });
        }
    }
    let silencers: Vec<ContinuousEffect> = out
        .iter()
        .chain(&statics)
        .filter(|e| {
            matches!(
                e.modification,
                mtg_ir::effect::Modification::LoseAllAbilities
            )
        })
        .cloned()
        .collect();
    if !silencers.is_empty() {
        let chars = crate::eval::PrintedChars(printed);
        statics.retain(|e| {
            !silencers.iter().any(|s| {
                !(s.source == e.source && s.ability == e.ability)
                    && affects(state, printed, &chars, s, e.source)
            })
        });
    }
    out.extend(statics);
    out
}

/// Whether a restriction matching `want` applies to an object.
pub fn restricted(
    state: &GameState,
    printed: &dyn PrintedCards,
    id: ObjectId,
    want: impl Fn(&mtg_ir::effect::Restriction) -> bool,
) -> bool {
    effects(state, printed).iter().any(|e| {
        matches!(&e.modification, mtg_ir::effect::Modification::Restriction(r) if want(r))
            && applies(state, printed, e, id)
    })
}

/// Whether a continuous effect applies to an object, for callers outside the layer
/// system (combat restrictions, "can't be countered").
pub fn applies(
    state: &GameState,
    printed: &dyn PrintedCards,
    effect: &ContinuousEffect,
    id: ObjectId,
) -> bool {
    let chars = crate::eval::PrintedChars(printed);
    affects(state, printed, &chars, effect, id)
}

/// Whether a continuous effect applies to an object.
///
/// Dynamic sets are evaluated against **printed** characteristics
/// ([`crate::eval::PrintedChars`]) rather than computed ones. That is CR 613.6:
/// an effect's affected set is determined using the characteristics produced by
/// the layers below it. It is also the only thing keeping this out of infinite
/// recursion, since computing characteristics is what called us.
fn affects(
    state: &GameState,
    printed: &dyn PrintedCards,
    chars: &dyn CharacteristicsSource,
    effect: &ContinuousEffect,
    id: ObjectId,
) -> bool {
    match &effect.affected {
        // Frozen at resolution: "target creature gets +1/+1 until end of turn"
        // keeps affecting that creature even if it stops matching the description.
        AffectedSet::Fixed(ids) => ids.contains(&id),
        AffectedSet::Dynamic(sel) => {
            let ctx = effect_ctx(state, printed, chars, effect);
            crate::eval::objects(&ctx, sel).is_ok_and(|ids| ids.contains(&id))
        }
    }
}

/// Build an evaluation context for a continuous effect's own selectors and values.
///
/// The controller is read from the source's *stored* controller rather than its
/// computed one. Using the computed controller would re-enter the layer system
/// through layer 2 while it is already running.
fn effect_ctx<'a>(
    state: &'a GameState,
    printed: &'a dyn PrintedCards,
    chars: &'a dyn CharacteristicsSource,
    effect: &ContinuousEffect,
) -> crate::eval::Ctx<'a> {
    static NO_TARGETS: &[mtg_core::Target] = &[];
    let controller = state
        .objects
        .get(&effect.source)
        .map(|o| o.controller)
        .unwrap_or(state.active_player);
    crate::eval::Ctx {
        state,
        cards: printed,
        chars,
        source: effect.source,
        controller,
        targets: NO_TARGETS,
        target_legal: &[],
        x: 0,
        bindings: crate::empty_bindings(),
    }
}

fn apply_one(
    ch: &mut Characteristics,
    effect: &ContinuousEffect,
    state: &GameState,
    printed: &dyn PrintedCards,
    chars: &dyn CharacteristicsSource,
    obj: &crate::state::GameObject,
) {
    use mtg_ir::effect::Modification as M;

    // Values inside a modification are evaluated in the source's own context. A
    // value that cannot be evaluated contributes nothing rather than poisoning the
    // whole characteristic computation.
    let eval_value = |v: &mtg_ir::Value| -> i32 {
        crate::eval::value(&effect_ctx(state, printed, chars, effect), v).unwrap_or(0)
    };

    match &effect.modification {
        M::AddTypes(ts) => {
            for t in ts {
                if !ch.card_types.contains(t) {
                    ch.card_types.push(*t);
                }
            }
        }
        M::RemoveTypes(ts) => ch.card_types.retain(|t| !ts.contains(t)),
        M::RemoveSupertype(s) => ch.supertypes.retain(|x| x != s),
        M::NoManaCost => ch.mana_cost = mtg_core::ManaCost::default(),
        M::AddSubtypes(ss) => {
            for s in ss {
                if !ch.subtypes.contains(s) {
                    ch.subtypes.push(*s);
                }
            }
        }
        M::SetTypes(ts) => ch.card_types = ts.clone(),
        M::SetCreatureTypes(ss) => {
            ch.subtypes.retain(|s| {
                !printed
                    .subtype_name(*s)
                    .is_some_and(mtg_core::is_creature_type)
            });
            ch.every_creature_type = false;
            for s in ss {
                if !ch.subtypes.contains(s) {
                    ch.subtypes.push(*s);
                }
            }
        }
        // Replaced by the choice as the effect begins (`resolve`); nothing until then.
        M::BecomesChosen(_) => {}
        M::SetColors(cs) => {
            ch.colors = cs.iter().fold(mtg_core::ColorSet::COLORLESS, |a, c| {
                a.union(mtg_core::ColorSet::single(*c))
            });
        }
        M::AddColors(cs) => {
            for c in cs {
                ch.colors = ch.colors.union(mtg_core::ColorSet::single(*c));
            }
        }
        M::LoseAllAbilities => {
            ch.abilities.clear();
            ch.granted_keywords.clear();
            ch.granted_abilities.clear();
        }
        M::LoseKeyword(k) => {
            ch.granted_keywords.retain(|g| g != k);
            if let Some(face) = printed.face(obj.card, obj.face) {
                ch.abilities.retain(|id| {
                    !face.abilities.iter().any(|a| {
                        a.id == *id && matches!(a.kind, mtg_ir::AbilityKind::Keyword(x) if x == *k)
                    })
                });
            }
        }
        // A granted ability's id cannot index the object's printed ability list, so a
        // non-keyword grant is recorded by where its text lives (see
        // `crate::abilities`), and a keyword by name.
        M::GrantAbility(a) => {
            if !matches!(a.kind, mtg_ir::AbilityKind::Keyword(_)) {
                ch.granted_abilities.push(mtg_core::GrantedAbility {
                    effect: effect.id,
                    static_ability: effect.ability,
                });
            } else if let mtg_ir::AbilityKind::Keyword(k) = a.kind
                && !ch.granted_keywords.contains(&k)
            {
                ch.granted_keywords.push(k);
                if k == mtg_core::Keyword::Changeling {
                    ch.every_creature_type = true;
                }
            }
        }
        M::SwitchPowerToughness => core::mem::swap(&mut ch.power, &mut ch.toughness),

        // Layer 7a/7b — set base values, replacing whatever was there.
        M::SetBasePower(power) => ch.power = Some(eval_value(power)),
        M::SetBasePowerToughness { power, toughness } => {
            ch.power = Some(eval_value(power));
            ch.toughness = Some(eval_value(toughness));
        }
        // Layer 7c — the ordinary pump. Applies only to objects that have power
        // and toughness at all; a +1/+1 effect on a non-creature does nothing
        // rather than inventing values.
        M::ModifyPowerToughness { power, toughness } => {
            let (dp, dt) = (eval_value(power), eval_value(toughness));
            if let Some(p) = ch.power.as_mut() {
                *p += dp;
            }
            if let Some(t) = ch.toughness.as_mut() {
                *t += dt;
            }
        }

        // Copy and text-changing effects rewrite the whole printed object, which
        // needs the copiable-values machinery of CR 707. Not yet implemented.
        M::CopyOf(_) | M::ChangeText { .. } => {}
        // Layer 2, handled by `controller` rather than here, since it changes no
        // characteristic.
        M::Control(_) => {}
        // Restrictions are consulted when validating actions, not when computing
        // characteristics.
        M::Restriction(_) => {}
    }
}

/// The *computed* controller of an object, which layer 2 can change. Every rules
/// check that asks "whose is this?" must use this, not `GameObject::controller`.
///
/// Among several control-changing effects the latest timestamp wins (CR 613.7).
pub fn controller(state: &GameState, id: ObjectId) -> Option<PlayerId> {
    control(state, id).map(|(who, _)| who)
}

/// Whether the object has summoning sickness (CR 302.6): its controller has not
/// controlled it continuously since their most recent turn began. A permanent's own
/// flag covers entering; a control-changing effect counts from when it began, against
/// the start of the new controller's most recent turn.
pub fn summoning_sick(state: &GameState, id: ObjectId) -> bool {
    let Some(obj) = state.objects.get(&id) else {
        return false;
    };
    match control(state, id) {
        Some((who, Some(since))) => {
            let turn_began = state
                .upkeeps
                .get(&who)
                .map_or(Timestamp::ZERO, |(_, latest)| *latest);
            obj.summoning_sick || since > turn_began
        }
        _ => obj.summoning_sick,
    }
}

/// The computed controller, and when the control-changing effect that decides it began
/// (`None` when no effect applies and the stored controller stands).
fn control(state: &GameState, id: ObjectId) -> Option<(PlayerId, Option<Timestamp>)> {
    let obj = state.objects.get(&id)?;
    let mut who = obj.controller;
    let mut best: Option<Timestamp> = None;

    for e in &state.continuous {
        if e.layer != layer::CONTROL {
            continue;
        }
        let mtg_ir::effect::Modification::Control(sel) = &e.modification else {
            continue;
        };
        // Only `Fixed` sets are consulted here. A dynamic control-changing set
        // would need characteristics, and characteristics need the controller —
        // the one cycle CR 613.6 does not break for us. Control effects in
        // practice name their object.
        let AffectedSet::Fixed(ids) = &e.affected else {
            continue;
        };
        if !ids.contains(&id) {
            continue;
        }
        if best.is_some_and(|b| e.timestamp <= b) {
            continue;
        }
        // Resolve the new controller from the effect's own perspective: who created it,
        // or else whoever controls its source.
        if let Selector::You = sel
            && let Some(by) = e
                .controller
                .or_else(|| state.objects.get(&e.source).map(|s| s.controller))
        {
            best = Some(e.timestamp);
            who = by;
        }
    }

    // "You control enchanted creature": an Aura's static ability, which names its object
    // through attachment — a fact, not a characteristic, so it needs no layer pass. The
    // Aura's own stored controller is used, for the same reason as above.
    if let Some(host) = state
        .battlefield()
        .into_iter()
        .filter_map(|a| state.objects.get(&a))
        .filter(|a| a.attached_to == Some(id) && !a.phased_out)
        .filter(|a| a.controls_host)
        .max_by_key(|a| a.timestamp)
        && best.is_none_or(|b| host.timestamp > b)
    {
        best = Some(host.timestamp);
        who = host.controller;
    }

    Some((who, best))
}
