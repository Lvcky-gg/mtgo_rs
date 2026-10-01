//! Determining what a spell actually costs (CR 601.2f).
//!
//! The printed mana cost is the starting point, not the answer. Effects that make
//! spells cost more or less apply in a fixed order, and the order matters:
//!
//! 1. Cost **increases** apply first.
//! 2. Cost **reductions** apply second.
//! 3. The result is floored — a reduction cannot take a cost below zero, and it can
//!    never reduce the coloured part of a cost.
//!
//! Doing reductions before increases would let a large reduction zero out a cost
//! that a later increase should have kept payable, which is both wrong and
//! exploitable.

use mtg_core::{CardId, ManaCost, ManaSymbol, ObjectId, PlayerId};
use mtg_ir::effect::{Modification, Restriction};

use crate::{
    eval::{self, ComputedChars, Ctx},
    layers::PrintedCards,
    state::GameState,
};

/// The cost to cast `object`, after every applicable modifier.
pub fn total_cost(
    state: &GameState,
    cards: &dyn PrintedCards,
    object: ObjectId,
    controller: PlayerId,
) -> Option<ManaCost> {
    let obj = state.objects.get(&object)?;
    let face = cards.face(obj.card, obj.face)?;
    // An alternative cost the card is being cast for (flashback) replaces the printed one;
    // a card still in the graveyard is costed as it would be cast from there.
    let base = match (&obj.cast_context, obj.zone.zone) {
        (Some(cc), _) if cc.cost_override.is_some() => cc.cost_override.clone()?,
        (_, mtg_core::Zone::Graveyard) => graveyard_cast(cards, obj.card, obj.face)?.0,
        // Foretold or plotted: its later cost (CR 702.143a, 702.170d).
        (_, mtg_core::Zone::Exile) if obj.cast_later.is_some() => obj
            .cast_later
            .as_ref()
            .and_then(|(_, _, cost, _)| cost.clone())
            .unwrap_or_default(),
        _ => face.mana_cost.clone(),
    };
    // A kicked spell costs its kicker too (CR 601.2f: additional costs are part of the
    // total cost).
    let mut base = base;
    if obj.cast_context.as_ref().is_some_and(|c| c.kicked)
        && let Some(k) = kicker(face)
    {
        base.symbols.extend(k.symbols);
    }
    let cost = modify(state, cards, object, controller, base);
    // CR 903.8: commander tax. Owed by a commander still in the command zone (what it would
    // cost to cast now) or by one on the stack (what was fixed when it was cast).
    let tax = if obj.zone.zone == mtg_core::Zone::Command
        && state.commander.is_commander(obj.owner, obj.card)
    {
        state.commander.tax(obj.owner)
    } else {
        state.commander.tax_due.get(&object).copied().unwrap_or(0)
    };
    Some(apply_delta(cost, i64::from(tax), 0))
}

/// Apply cost modifiers to a base cost.
pub fn modify(
    state: &GameState,
    cards: &dyn PrintedCards,
    object: ObjectId,
    controller: PlayerId,
    base: ManaCost,
) -> ManaCost {
    let chars = ComputedChars(cards);
    let (mut increase, mut reduction) = (0i64, 0i64);

    // Static abilities ("creature spells you cast cost {1} less") as well as effects.
    for effect in &crate::layers::effects(state, cards) {
        let Modification::Restriction(Restriction::CostModifier { what, delta }) =
            &effect.modification
        else {
            continue;
        };

        let source_controller = state
            .objects
            .get(&effect.source)
            .map(|o| o.controller)
            .unwrap_or(controller);
        let ctx = Ctx {
            state,
            cards,
            chars: &chars,
            source: effect.source,
            controller: source_controller,
            targets: &[],
            target_legal: &[],
            x: 0,
            bindings: crate::empty_bindings(),
        };

        // Does the modifier apply to this spell?
        if !eval::matches(&ctx, what, object).unwrap_or(false) {
            continue;
        }
        match eval::value(&ctx, delta).unwrap_or(0) {
            d if d > 0 => increase += i64::from(d),
            d if d < 0 => reduction += i64::from(-d),
            _ => {}
        }
    }

    // "This spell costs {1} less to cast for each artifact you control" and affinity work
    // wherever the card is being cast from (CR 601.2f). On the stack they are already
    // among the effects above.
    if let Some(obj) = state.objects.get(&object)
        && obj.zone.zone != mtg_core::Zone::Stack
        && let Some(face) = cards.face(obj.card, obj.face)
    {
        let ctx = Ctx {
            state,
            cards,
            chars: &chars,
            source: object,
            controller,
            targets: &[],
            target_legal: &[],
            x: 0,
            bindings: crate::empty_bindings(),
        };
        for ability in &face.abilities {
            let mtg_ir::AbilityKind::Static {
                what: mtg_ir::Selector::SelfSource,
                modification: Modification::Restriction(Restriction::CostModifier { what, delta }),
                condition,
            } = &ability.kind
            else {
                continue;
            };
            if condition
                .as_ref()
                .is_some_and(|c| !eval::condition(&ctx, c).unwrap_or(false))
                || !eval::matches(&ctx, what, object).unwrap_or(false)
            {
                continue;
            }
            match eval::value(&ctx, delta).unwrap_or(0) {
                d if d > 0 => increase += i64::from(d),
                d if d < 0 => reduction += i64::from(-d),
                _ => {}
            }
        }
    }

    apply_delta(base, increase, reduction)
}

/// Adjust the generic portion of a cost, leaving coloured symbols untouched.
fn apply_delta(cost: ManaCost, increase: i64, reduction: i64) -> ManaCost {
    let generic: i64 = cost
        .symbols
        .iter()
        .filter_map(|s| match s {
            ManaSymbol::Generic(n) => Some(i64::from(*n)),
            _ => None,
        })
        .sum();

    // Increases first, then reductions, then floored at zero (CR 601.2f).
    let adjusted = (generic + increase - reduction).max(0);

    let mut symbols: Vec<ManaSymbol> = cost
        .symbols
        .into_iter()
        .filter(|s| !matches!(s, ManaSymbol::Generic(_)))
        .collect();
    if adjusted > 0 {
        symbols.insert(0, ManaSymbol::Generic(adjusted.min(u8::MAX as i64) as u8));
    }
    ManaCost { symbols }
}

/// Whether a card can be cast from a zone at this moment, ignoring cost.
///
/// Timing only: the cost question is [`crate::mana::can_pay`]. Kept separate
/// because the two fail for different reasons and a player is owed the right one —
/// "it isn't your main phase" and "you can't afford it" are different problems.
pub fn timing_allows(
    state: &GameState,
    cards: &dyn PrintedCards,
    object: ObjectId,
    controller: PlayerId,
) -> bool {
    let Some(obj) = state.objects.get(&object) else {
        return false;
    };
    let Some(ch) = crate::layers::compute(state, cards, object) else {
        return false;
    };
    let _ = obj;

    // Lands are played as a special action, not cast (CR 305.1).
    if ch.has_type(mtg_core::CardType::Land) {
        return false;
    }

    let stack_empty = state
        .objects_in(mtg_core::ZoneRef::shared(mtg_core::Zone::Stack))
        .is_empty();
    let sorcery_time =
        state.active_player == controller && state.step.is_main_phase() && stack_empty;

    // Instants and anything with flash may be cast whenever its controller has
    // priority; everything else needs sorcery timing (CR 302.6, CR 307.5).
    let instant_speed = ch.has_type(mtg_core::CardType::Instant)
        || has_flash(state, cards, object)
        || flash_permitted(state, cards, object, controller);

    (instant_speed || sorcery_time) && cast_conditions_met(state, cards, object, controller)
}

/// "You may cast creature spells as though they had flash" from a permanent its caster
/// controls, or the card's own "you may cast this spell as though it had flash if …".
pub(crate) fn flash_permitted(
    state: &GameState,
    cards: &dyn PrintedCards,
    object: ObjectId,
    controller: PlayerId,
) -> bool {
    let chars = ComputedChars(cards);
    let ctx_for = |source: ObjectId| Ctx {
        state,
        cards,
        chars: &chars,
        source,
        controller,
        targets: &[],
        target_legal: &[],
        x: 0,
        bindings: crate::empty_bindings(),
    };
    let granted = crate::layers::effects(state, cards).iter().any(|e| {
        matches!(&e.modification, Modification::Restriction(Restriction::FlashFor(f))
            if crate::layers::controller(state, e.source) == Some(controller)
                && eval::matches(&ctx_for(e.source), f, object).unwrap_or(false))
    });
    if granted {
        return true;
    }
    let Some(obj) = state.objects.get(&object) else {
        return false;
    };
    let Some(face) = cards.face(obj.card, obj.face) else {
        return false;
    };
    face.abilities.iter().any(|a| match &a.kind {
        mtg_ir::AbilityKind::Static {
            what: mtg_ir::Selector::SelfSource,
            modification: Modification::Restriction(Restriction::FlashFor(_)),
            condition,
        } => condition
            .as_ref()
            .is_none_or(|c| eval::condition(&ctx_for(object), c).unwrap_or(false)),
        _ => false,
    })
}

/// Whether the spell's own "cast this spell only …" conditions hold right now.
pub fn cast_conditions_met(
    state: &GameState,
    cards: &dyn PrintedCards,
    object: ObjectId,
    controller: PlayerId,
) -> bool {
    let Some(obj) = state.objects.get(&object) else {
        return false;
    };
    if obj.face_down {
        return true;
    }
    let Some(face) = cards.face(obj.card, obj.face) else {
        return true;
    };
    let chars = ComputedChars(cards);
    let ctx = Ctx {
        state,
        cards,
        chars: &chars,
        source: object,
        controller,
        targets: &[],
        target_legal: &[],
        x: 0,
        bindings: crate::empty_bindings(),
    };
    face.abilities.iter().all(|a| match &a.kind {
        mtg_ir::AbilityKind::CastOnlyIf { condition } => {
            eval::condition(&ctx, condition).unwrap_or(false)
        }
        _ => true,
    })
}

fn has_flash(state: &GameState, cards: &dyn PrintedCards, object: ObjectId) -> bool {
    let chars = ComputedChars(cards);
    let Some(obj) = state.objects.get(&object) else {
        return false;
    };
    let ctx = Ctx {
        state,
        cards,
        chars: &chars,
        source: object,
        controller: obj.controller,
        targets: &[],
        target_legal: &[],
        x: 0,
        bindings: crate::empty_bindings(),
    };
    ctx.has_keyword(object, mtg_ir::ability::Keyword::Flash)
        .unwrap_or(false)
}

/// The printed cost of a card, for display.
pub fn printed_cost(cards: &dyn PrintedCards, card: CardId) -> Option<ManaCost> {
    Some(cards.face(card, 0)?.mana_cost.clone())
}

// ---- activated ability costs (CR 602.2b, CR 601.2g-h) ----------------------

/// The cost of one activated ability of a permanent, as printed.
pub fn ability_cost(
    state: &GameState,
    cards: &dyn PrintedCards,
    source: ObjectId,
    ability: mtg_core::AbilityId,
) -> Option<mtg_ir::Cost> {
    match &crate::abilities::find(state, cards, source, ability)?.kind {
        mtg_ir::AbilityKind::Activated { cost, .. } => Some(cost.clone()),
        _ => None,
    }
}

/// CR 302.6: a creature's ability with {T} or {Q} in its cost can't be activated unless
/// its controller has controlled it continuously since their most recent turn began —
/// unless it has haste. Noncreature permanents are not affected.
pub fn tap_symbol_allowed(state: &GameState, cards: &dyn PrintedCards, source: ObjectId) -> bool {
    let Some(obj) = state.objects.get(&source) else {
        return false;
    };
    let _ = obj;
    if !crate::layers::summoning_sick(state, source) {
        return true;
    }
    let Some(ch) = crate::layers::compute(state, cards, source) else {
        return false;
    };
    if !ch.has_type(mtg_core::CardType::Creature) {
        return true;
    }
    let chars = ComputedChars(cards);
    let ctx = Ctx {
        state,
        cards,
        chars: &chars,
        source,
        controller: obj.controller,
        targets: &[],
        target_legal: &[],
        x: 0,
        bindings: crate::empty_bindings(),
    };
    ctx.has_keyword(source, mtg_ir::ability::Keyword::Haste)
        .unwrap_or(false)
}

/// Whether the non-mana part of an activated ability's cost can be paid right now.
///
/// Only the parts the engine knows how to pay are accepted. An ability carrying any other
/// cost is not offered at all: a cost silently treated as free is worse than an ability
/// that is honestly unavailable.
pub fn additional_payable(
    state: &GameState,
    cards: &dyn PrintedCards,
    source: ObjectId,
    who: PlayerId,
    cost: &mtg_ir::Cost,
) -> bool {
    use mtg_ir::{AdditionalCost as A, Selector, Value};
    let Some(obj) = state.objects.get(&source) else {
        return false;
    };
    if !cost.timing.is_empty() {
        return false;
    }
    cost.additional.iter().all(|c| match c {
        // A marker, not a payment.
        A::Cycling => true,
        A::Tap {
            what: Selector::SelfSource,
        } => !obj.tapped && tap_symbol_allowed(state, cards, source),
        A::Untap {
            what: Selector::SelfSource,
        } => obj.tapped && tap_symbol_allowed(state, cards, source),
        A::Sacrifice {
            what: Selector::SelfSource,
            count: Value::Fixed(1),
        } => true,
        // CR 119.4: life can be paid only if the total is at least the amount.
        A::PayLife {
            amount: Value::Fixed(n),
        } => state.player(who).life >= *n,
        A::PayEnergy {
            amount: Value::Fixed(n),
        } => i64::from(state.player(who).energy) >= i64::from(*n),
        A::RemoveCounters {
            what: Selector::SelfSource,
            kind,
            amount: Value::Fixed(n),
        } => obj.counters.get(kind).copied().unwrap_or(0) >= *n,
        A::Loyalty { delta } => {
            *delta >= 0
                || i64::from(
                    obj.counters
                        .get(&mtg_core::CounterKind::Loyalty)
                        .copied()
                        .unwrap_or(0),
                ) >= -i64::from(*delta)
        }
        // "Exile this card from your graveyard".
        A::ExileFrom {
            zone: mtg_core::Zone::Graveyard,
            filter: mtg_ir::ObjectFilter::IsSelf,
            ..
        } => obj.zone.zone == mtg_core::Zone::Graveyard,
        // "Discard this card" — cycling and friends, from the hand.
        A::Discard {
            count: Value::Fixed(1),
            filter: mtg_ir::ObjectFilter::IsSelf,
            at_random: false,
        } => obj.zone.zone == mtg_core::Zone::Hand,
        // Crew: the untapped creatures available must have enough power between them.
        A::TapCreaturesWithPower { .. } => cost_candidates(state, cards, source, who, c)
            .is_some_and(|(from, n)| total_power(state, cards, &from) >= n as i32),
        // "Sacrifice a creature", "discard a card": enough to choose from.
        part => cost_candidates(state, cards, source, who, part)
            .is_some_and(|(from, n)| from.len() as u32 >= n),
    })
}

/// For a cost part that is a choice — "sacrifice a creature", "sacrifice another
/// artifact", "discard a card" — what may be chosen and how many. `None` for any other
/// part.
pub fn cost_candidates(
    state: &GameState,
    cards: &dyn PrintedCards,
    source: ObjectId,
    who: PlayerId,
    part: &mtg_ir::AdditionalCost,
) -> Option<(Vec<ObjectId>, u32)> {
    use mtg_ir::{AdditionalCost as A, Selector, Value};
    let chars = ComputedChars(cards);
    let ctx = Ctx {
        state,
        cards,
        chars: &chars,
        source,
        controller: who,
        targets: &[],
        target_legal: &[],
        x: 0,
        bindings: crate::empty_bindings(),
    };
    // Ninjutsu: an attacking creature you control that blockers have been declared
    // against and that no creature blocks.
    if let A::ReturnUnblockedAttacker = part {
        let blockers_declared = matches!(
            state.step,
            mtg_core::Step::DeclareBlockers
                | mtg_core::Step::FirstStrikeCombatDamage
                | mtg_core::Step::CombatDamage
                | mtg_core::Step::EndCombat
        );
        let from = state
            .combat
            .attackers
            .keys()
            .copied()
            .filter(|_| blockers_declared)
            .filter(|id| !state.combat.was_blocked.contains(id))
            .filter(|id| crate::layers::controller(state, *id) == Some(who))
            .collect();
        return Some((from, 1));
    }
    if let A::TapCreaturesWithPower {
        power: Value::Fixed(n),
    } = part
    {
        // Untapped creatures you control other than the one being crewed.
        let from = state
            .battlefield()
            .into_iter()
            .filter(|id| *id != source)
            .filter(|id| crate::layers::controller(state, *id) == Some(who))
            .filter(|id| state.objects.get(id).is_some_and(|o| !o.tapped))
            .filter(|id| {
                crate::layers::compute(state, cards, *id)
                    .is_some_and(|c| c.has_type(mtg_core::CardType::Creature))
            })
            .collect();
        return Some((from, (*n).max(0) as u32));
    }
    let (zone, filter, n) = match part {
        A::Sacrifice {
            what:
                Selector::All {
                    zone: mtg_core::Zone::Battlefield,
                    filter,
                },
            count: Value::Fixed(n),
        } => (
            mtg_core::ZoneRef::shared(mtg_core::Zone::Battlefield),
            filter,
            *n,
        ),
        A::Discard {
            count: Value::Fixed(n),
            filter,
            at_random: false,
        } if *filter != mtg_ir::ObjectFilter::IsSelf => {
            (mtg_core::ZoneRef::of(mtg_core::Zone::Hand, who), filter, *n)
        }
        // "Exile three other cards from your graveyard" (escape).
        A::ExileFrom {
            zone: mtg_core::Zone::Graveyard,
            filter,
            count: Value::Fixed(n),
        } if *filter != mtg_ir::ObjectFilter::IsSelf => (
            mtg_core::ZoneRef::of(mtg_core::Zone::Graveyard, who),
            filter,
            *n,
        ),
        _ => return None,
    };
    let from = state
        .objects_in(zone)
        .into_iter()
        // A card being cast is not in the hand any more, and never pays for itself by
        // being discarded.
        .filter(|id| *id != source || zone.zone == mtg_core::Zone::Battlefield)
        .filter(|id| {
            zone.zone != mtg_core::Zone::Battlefield
                || crate::layers::controller(state, *id) == Some(who)
        })
        .filter(|id| eval::matches(&ctx, filter, *id).unwrap_or(false))
        .collect();
    Some((from, n.max(0) as u32))
}

/// A spell's additional casting cost ("As an additional cost to cast this spell, …").
pub fn additional_cast_cost(face: &mtg_ir::CardFace) -> Option<mtg_ir::Cost> {
    face.abilities.iter().find_map(|a| match &a.kind {
        mtg_ir::AbilityKind::AdditionalCastCost { cost } => Some(cost.clone()),
        _ => None,
    })
}

/// A face's way to be cast from `zone`, if it has one: the mana cost, and whether it is
/// exiled afterwards. Additional costs (retrace's discarded land, escape's exiled cards)
/// are [`cast_from_additional`].
pub fn cast_from(face: &mtg_ir::CardFace, zone: mtg_core::Zone) -> Option<(ManaCost, bool)> {
    face.abilities.iter().find_map(|a| match &a.kind {
        mtg_ir::AbilityKind::Aftermath if zone == mtg_core::Zone::Graveyard => {
            Some((face.mana_cost.clone(), true))
        }
        mtg_ir::AbilityKind::CastFrom {
            zone: z,
            cost,
            exile,
            transformed: false,
        } if *z == zone && cost.timing.is_empty() => Some((cost.mana.clone(), *exile)),
        _ => None,
    })
}

/// How a card's face may be cast from its owner's graveyard ([`cast_from`]), including a
/// transforming card's back face cast with disturb (CR 702.146a): the cost is on the front.
pub fn graveyard_cast(
    cards: &dyn PrintedCards,
    card: CardId,
    face: u8,
) -> Option<(ManaCost, bool)> {
    let zone = mtg_core::Zone::Graveyard;
    if let Some(found) = cards.face(card, face).and_then(|f| cast_from(f, zone)) {
        return Some(found);
    }
    if face != 1 || cards.layout(card) != mtg_ir::Layout::Transforming {
        return None;
    }
    cards
        .face(card, 0)?
        .abilities
        .iter()
        .find_map(|a| match &a.kind {
            mtg_ir::AbilityKind::CastFrom {
                zone: z,
                cost,
                exile,
                transformed: true,
            } if *z == zone => Some((cost.mana.clone(), *exile)),
            _ => None,
        })
}

/// The additional costs of casting a face from `zone` ([`cast_from`]), as a cost.
pub fn cast_from_additional(face: &mtg_ir::CardFace, zone: mtg_core::Zone) -> mtg_ir::Cost {
    let additional = face
        .abilities
        .iter()
        .find_map(|a| match &a.kind {
            mtg_ir::AbilityKind::CastFrom { zone: z, cost, .. } if *z == zone => {
                Some(cost.additional.clone())
            }
            _ => None,
        })
        .unwrap_or_default();
    mtg_ir::Cost {
        additional,
        ..mtg_ir::Cost::free()
    }
}

/// Whether a face's kicker is buyback (CR 702.27): paid, it returns to hand on resolution.
pub fn is_buyback(face: &mtg_ir::CardFace) -> bool {
    face.abilities
        .iter()
        .any(|a| matches!(a.kind, mtg_ir::AbilityKind::Kicker { buyback: true, .. }))
}

/// Whether a face's kicker is entwine (CR 702.42): paid, every mode is chosen.
pub fn is_entwine(face: &mtg_ir::CardFace) -> bool {
    face.abilities
        .iter()
        .any(|a| matches!(a.kind, mtg_ir::AbilityKind::Kicker { entwine: true, .. }))
}

/// A face's kicker mana cost, if it has one.
pub fn kicker(face: &mtg_ir::CardFace) -> Option<ManaCost> {
    face.abilities.iter().find_map(|a| match &a.kind {
        mtg_ir::AbilityKind::Kicker { cost, .. } if cost.additional.is_empty() => {
            Some(cost.mana.clone())
        }
        _ => None,
    })
}

/// The total power of some creatures, for crew.
pub fn total_power(state: &GameState, cards: &dyn PrintedCards, ids: &[ObjectId]) -> i32 {
    ids.iter()
        .filter_map(|id| crate::layers::compute(state, cards, *id).and_then(|c| c.power))
        .map(|p| p.max(0))
        .sum()
}
