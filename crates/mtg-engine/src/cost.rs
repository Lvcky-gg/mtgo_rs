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
    if let Some(cc) = obj.cast_context.as_ref().filter(|c| c.kicked)
        && let Some(k) = kicker(face)
    {
        // A multikicker is paid once for each time it was announced (CR 702.33c).
        for _ in 0..cc.kicks.max(1) {
            base.symbols.extend(k.symbols.iter().cloned());
        }
    }
    // Additional casting costs paid in mana: "pay {3}", alone or as the chosen option.
    if let Some(extra) = additional_cast_cost(face) {
        base.symbols.extend(extra.mana.symbols);
    }
    if let Some(i) = obj.cast_context.as_ref().and_then(|c| c.cost_choice)
        && let Some((_, option)) = cost_choice_options(face).and_then(|o| o.get(usize::from(i)))
    {
        base.symbols.extend(option.mana.symbols.iter().cloned());
    }
    // Strive: more for each target beyond the first (CR 702.103a).
    if let Some(per) = strive(face)
        && let Some(cc) = obj.cast_context.as_ref()
    {
        for _ in 1..target_count(cc) {
            base.symbols.extend(per.symbols.iter().cloned());
        }
    }
    // "… as though it had flash if you pay {2} more": owed when cast at a time a sorcery
    // couldn't be — as it was cast, or, before then, now.
    if let Some(extra) = flash_surcharge(face) {
        let flashed = match obj.cast_context.as_ref() {
            Some(cc) if obj.zone.zone == mtg_core::Zone::Stack => cc.flashed,
            _ => !sorcery_timing(state, controller),
        };
        if flashed {
            base.symbols.extend(extra.symbols.iter().cloned());
        }
    }
    // Spree: each chosen mode's own cost (CR 702.172a).
    if let Some(costs) = spree(face)
        && let Some(cc) = obj.cast_context.as_ref()
    {
        for m in &cc.modes {
            if let Some(c) = costs.get(usize::from(*m)) {
                base.symbols.extend(c.symbols.iter().cloned());
            }
        }
    }
    // Escalate: more for each mode beyond the first (CR 702.120a).
    if let Some(per) = escalate(face)
        && let Some(cc) = obj.cast_context.as_ref()
    {
        for _ in 1..cc.modes.len() {
            base.symbols.extend(per.symbols.iter().cloned());
        }
    }
    let mut cost = modify(state, cards, object, controller, base);
    // Emerge: less by the sacrificed creature's mana value (CR 702.119a).
    if let Some(cc) = obj.cast_context.as_ref()
        && cc.emerge_reduction > 0
    {
        cost = apply_delta(cost, 0, i64::from(cc.emerge_reduction));
    }
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

/// "This spell costs {2} less to cast if it targets a tapped creature": what a target has
/// to match for the spell's own cost to change.
pub fn target_discount(
    state: &GameState,
    cards: &dyn PrintedCards,
    object: ObjectId,
) -> Option<mtg_ir::ObjectFilter> {
    let obj = state.objects.get(&object).filter(|o| !o.face_down)?;
    cards
        .face(obj.card, obj.face)?
        .abilities
        .iter()
        .find_map(|a| match &a.kind {
            mtg_ir::AbilityKind::Static {
                what: mtg_ir::Selector::SelfSource,
                modification: Modification::Restriction(Restriction::CostModifier { .. }),
                condition: Some(mtg_ir::trigger::Condition::TargetsMatching(f)),
            } => Some(f.clone()),
            _ => None,
        })
}

/// What casting `object` would cost with `target` among its targets — for offering a spell
/// that only its target-dependent discount makes affordable.
pub fn cost_with_target(
    state: &GameState,
    cards: &dyn PrintedCards,
    object: ObjectId,
    controller: PlayerId,
    target: mtg_core::Target,
) -> Option<ManaCost> {
    let mut state = state.clone();
    let cc = state
        .objects
        .get_mut(&object)?
        .cast_context
        .get_or_insert_with(Default::default);
    cc.targets = vec![target];
    cc.empty_slots.clear();
    total_cost(&state, cards, object, controller)
}

/// Adjust the generic portion of a cost, leaving coloured symbols untouched.
pub(crate) fn apply_delta(cost: ManaCost, increase: i64, reduction: i64) -> ManaCost {
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

/// Whether a "can't cast" restriction stops `who` casting this spell now: "your opponents
/// can't cast creature spells", "each player can't cast more than one spell each turn".
pub fn cast_forbidden(
    state: &GameState,
    cards: &dyn PrintedCards,
    object: ObjectId,
    who: PlayerId,
) -> bool {
    let chars = ComputedChars(cards);
    crate::layers::effects(state, cards).iter().any(|e| {
        let mtg_ir::effect::Modification::Restriction(mtg_ir::effect::Restriction::CantCast {
            who: players,
            spells,
            beyond,
        }) = &e.modification
        else {
            return false;
        };
        let Some(controller) = e
            .controller
            .or_else(|| crate::layers::controller(state, e.source))
        else {
            return false;
        };
        let ctx = Ctx {
            state,
            cards,
            chars: &chars,
            source: e.source,
            controller,
            targets: &[],
            target_legal: &[],
            x: 0,
            bindings: crate::empty_bindings(),
        };
        eval::players(&ctx, players)
            .unwrap_or_default()
            .contains(&who)
            && eval::matches(&ctx, spells, object).unwrap_or(false)
            && beyond.is_none_or(|n| {
                state.spells_by_player.get(&who).copied().unwrap_or(0) >= u32::from(n)
            })
    })
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
    // "Activate only if you control a Swamp" (CR 602.5b): checked as it is activated.
    if !conditions_hold(state, cards, source, who, &cost.timing) {
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
        // CR 701.43c — a permanent can be exerted even if it is already exerted.
        A::Exert => true,
        // "Remove X storage counters", "remove any number of …": zero will do.
        A::RemoveCounters {
            what: Selector::SelfSource,
            amount: Value::X,
            ..
        } => true,
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
    if let A::TapUntapped {
        filter,
        count: Value::Fixed(n),
    } = part
    {
        let from = state
            .battlefield()
            .into_iter()
            .filter(|id| crate::layers::controller(state, *id) == Some(who))
            .filter(|id| state.objects.get(id).is_some_and(|o| !o.tapped))
            .filter(|id| eval::matches(&ctx, filter, *id).unwrap_or(false))
            .collect();
        return Some((from, (*n).max(0) as u32));
    }
    let (zone, filter, n) = match part {
        A::Reveal {
            filter,
            count: Value::Fixed(n),
        } => (mtg_core::ZoneRef::of(mtg_core::Zone::Hand, who), filter, *n),
        A::ReturnToHand {
            filter,
            count: Value::Fixed(n),
        } => (
            mtg_core::ZoneRef::shared(mtg_core::Zone::Battlefield),
            filter,
            *n,
        ),
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
        // "Exile a blue card from your hand" (an alternative cost).
        A::ExileFrom {
            zone: mtg_core::Zone::Hand,
            filter,
            count: Value::Fixed(n),
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

/// Everything a spell on the stack costs besides mana: its additional casting cost, what
/// any paid kicker, what casting it from a graveyard adds (retrace's land, escape's
/// cards), and non-mana parts of an alternative cost ("pay 1 life and exile a blue card").
pub fn spell_extra_cost(
    state: &GameState,
    cards: &dyn PrintedCards,
    object: ObjectId,
) -> Option<mtg_ir::Cost> {
    let o = state.objects.get(&object).filter(|o| !o.face_down)?;
    let face = cards.face(o.card, o.face)?;
    let cc = o.cast_context.as_ref();
    let mut cost = additional_cast_cost(face);
    let mut extend = |more: Vec<mtg_ir::AdditionalCost>| {
        if !more.is_empty() {
            cost.get_or_insert_with(mtg_ir::Cost::free)
                .additional
                .extend(more);
        }
    };
    if let Some(i) = cc.and_then(|c| c.cost_choice)
        && let Some((_, option)) = cost_choice_options(face).and_then(|o| o.get(usize::from(i)))
    {
        extend(option.additional.clone());
    }
    if let Some(cc) = cc.filter(|c| c.kicked)
        && let Some(kicker) = kicker_cost(face)
    {
        for _ in 0..cc.kicks.max(1) {
            extend(kicker.additional.clone());
        }
    }
    if cc.and_then(|c| c.cast_from) == Some(mtg_core::Zone::Graveyard) {
        extend(cast_from_additional(face, mtg_core::Zone::Graveyard).additional);
    }
    if let Some(alt @ (mtg_ir::ability::AltCost::Pay | mtg_ir::ability::AltCost::Emerge)) =
        cc.and_then(|c| c.alt_cost)
    {
        extend(
            face.abilities
                .iter()
                .find_map(|a| match &a.kind {
                    mtg_ir::AbilityKind::AlternativeCost { cost, kind, .. } if *kind == alt => {
                        Some(cost.additional.clone())
                    }
                    _ => None,
                })
                .unwrap_or_default(),
        );
    }
    cost
}

/// "As an additional cost to cast this spell, sacrifice a creature or pay {3}.": the
/// options, each with how it reads.
pub fn cost_choice_options(face: &mtg_ir::CardFace) -> Option<&[(Box<str>, mtg_ir::Cost)]> {
    face.abilities.iter().find_map(|a| match &a.kind {
        mtg_ir::AbilityKind::AdditionalCastCostChoice { options } => Some(options.as_slice()),
        _ => None,
    })
}

/// Which options of an "A or B" additional cost `who` could pay for `object` now, with
/// the rest of its cost.
pub fn payable_cost_choices(
    state: &GameState,
    cards: &dyn PrintedCards,
    object: ObjectId,
    who: PlayerId,
) -> Vec<u8> {
    let Some(obj) = state.objects.get(&object) else {
        return Vec::new();
    };
    let Some(options) = cards.face(obj.card, obj.face).and_then(cost_choice_options) else {
        return Vec::new();
    };
    let Some(total) = total_cost(state, cards, object, who) else {
        return Vec::new();
    };
    (0..options.len() as u8)
        .filter(|i| {
            let option = &options[usize::from(*i)].1;
            let mut with = total.clone();
            with.symbols.extend(option.mana.symbols.iter().cloned());
            additional_payable(state, cards, object, who, option)
                && crate::mana::plan_spell(state, cards, who, &with, 0, object).is_some()
        })
        .collect()
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

/// A face's strive cost, paid once for each target beyond the first (CR 702.103).
pub fn strive(face: &mtg_ir::CardFace) -> Option<&ManaCost> {
    face.abilities.iter().find_map(|a| match &a.kind {
        mtg_ir::AbilityKind::Strive { per_target } => Some(per_target),
        _ => None,
    })
}

/// A face's flash surcharge: what casting it as though it had flash costs more.
pub fn flash_surcharge(face: &mtg_ir::CardFace) -> Option<&ManaCost> {
    face.abilities.iter().find_map(|a| match &a.kind {
        mtg_ir::AbilityKind::FlashSurcharge { cost } => Some(cost),
        _ => None,
    })
}

/// Whether `who` could cast a sorcery now (CR 307.1): their main phase, stack empty.
pub fn sorcery_timing(state: &GameState, who: PlayerId) -> bool {
    state.active_player == who
        && state.step.is_main_phase()
        && state
            .objects_in(mtg_core::ZoneRef::shared(mtg_core::Zone::Stack))
            .is_empty()
}

/// A face's spree costs, one per mode (CR 702.172).
pub fn spree(face: &mtg_ir::CardFace) -> Option<&[ManaCost]> {
    face.abilities.iter().find_map(|a| match &a.kind {
        mtg_ir::AbilityKind::Spree { costs } => Some(costs.as_slice()),
        _ => None,
    })
}

/// A face's escalate cost, paid once for each mode beyond the first (CR 702.120).
pub fn escalate(face: &mtg_ir::CardFace) -> Option<&ManaCost> {
    face.abilities.iter().find_map(|a| match &a.kind {
        mtg_ir::AbilityKind::Escalate { per_mode } => Some(per_mode),
        _ => None,
    })
}

/// How many distinct things a spell targets, leaving out empty-slot placeholders.
pub fn target_count(cc: &crate::state::CastContext) -> usize {
    let mut seen: Vec<&mtg_core::Target> = Vec::new();
    for (i, t) in cc.targets.iter().enumerate() {
        if !cc.empty_slots.contains(&(i as u8)) && !seen.contains(&t) {
            seen.push(t);
        }
    }
    seen.len()
}

/// Whether a face's kicker may be paid any number of times (multikicker, replicate).
pub fn is_multikicker(face: &mtg_ir::CardFace) -> bool {
    face.abilities
        .iter()
        .any(|a| matches!(a.kind, mtg_ir::AbilityKind::Kicker { multi: true, .. }))
}

/// A face's kicker mana cost, if it has one the engine can pay: mana, and parts chosen as
/// it is paid ("kicker—sacrifice an artifact or creature", casualty), or fixed life.
pub fn kicker(face: &mtg_ir::CardFace) -> Option<ManaCost> {
    kicker_cost(face).map(|c| c.mana)
}

/// A face's whole kicker cost, with chosen non-mana parts and fixed life payments.
pub fn kicker_cost(face: &mtg_ir::CardFace) -> Option<mtg_ir::Cost> {
    use mtg_ir::AdditionalCost as A;
    face.abilities.iter().find_map(|a| match &a.kind {
        mtg_ir::AbilityKind::Kicker { cost, .. }
            if cost.timing.is_empty()
                && cost.additional.iter().all(|p| {
                    matches!(
                        p,
                        A::Sacrifice { .. }
                            | A::TapUntapped { .. }
                            | A::Discard { .. }
                            | A::PayLife {
                                amount: mtg_ir::Value::Fixed(0..)
                            }
                    )
                }) =>
        {
            Some(cost.clone())
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

/// Whether every condition holds for `source` controlled by `who` — an alternative cost's
/// "if an opponent lost life this turn" (spectacle).
pub fn conditions_hold(
    state: &GameState,
    cards: &dyn PrintedCards,
    source: ObjectId,
    who: PlayerId,
    conditions: &[mtg_ir::trigger::Condition],
) -> bool {
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
    conditions
        .iter()
        .all(|c| eval::condition(&ctx, c).unwrap_or(false))
}

/// Emerge: whether `object` could be cast for its emerge cost by sacrificing `creature`
/// (CR 702.119a) — the cost less that creature's mana value, paid without it.
pub fn emerge_affordable(
    state: &GameState,
    cards: &dyn PrintedCards,
    object: ObjectId,
    who: PlayerId,
    creature: ObjectId,
) -> bool {
    use crate::eval::CharacteristicsSource;
    let chars = ComputedChars(cards);
    let Some(mv) = chars
        .characteristics(state, creature)
        .map(|c| c.mana_cost.mana_value())
    else {
        return false;
    };
    let mut trial = state.clone();
    trial.objects.remove(&creature);
    let Some(obj) = trial.objects.get_mut(&object) else {
        return false;
    };
    let cost = if obj.zone.zone == mtg_core::Zone::Stack {
        obj.cast_context
            .get_or_insert_with(Default::default)
            .emerge_reduction = mv;
        total_cost(&trial, cards, object, who)
    } else {
        // From the hand, before it is cast: the printed emerge cost.
        cards.face(obj.card, obj.face).and_then(|f| {
            f.abilities.iter().find_map(|a| match &a.kind {
                mtg_ir::AbilityKind::AlternativeCost {
                    cost,
                    kind: mtg_ir::ability::AltCost::Emerge,
                    ..
                } => Some(apply_delta(cost.mana.clone(), 0, i64::from(mv))),
                _ => None,
            })
        })
    };
    cost.is_some_and(|c| crate::mana::plan_spell(&trial, cards, who, &c, 0, object).is_some())
}
