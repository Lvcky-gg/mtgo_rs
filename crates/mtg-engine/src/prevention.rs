//! Damage prevention (CR 615).
//!
//! Every way damage gets dealt — a resolving spell or ability, a fight, combat — asks
//! [`prevent`] how much of it survives before building its damage event. Prevented damage
//! is never dealt (CR 615.6), so nothing downstream sees it: no lifelink, no counters, no
//! damage triggers.
//!
//! Four kinds of prevention are consulted:
//! - protection from the damage source’s qualities;
//! - global Fog-style prevention and whole-recipient shields (`GameState::prevent_*`);
//! - shields created by resolving spells and abilities (`GameState::damage_shields`),
//!   which may be limited to a source, to combat damage, or to the next N damage;
//! - static abilities of permanents ([`Restriction::PreventDamage`]).
//!
//! When several could apply, the affected player would choose the order (CR 616.1).
//! Prevention that stops everything is applied first here, so a counting shield is
//! never spent on damage something else would have prevented anyway — the order a
//! player would pick.

use mtg_core::{CounterKind, DamageShield, Event, ObjectId, Target};
use mtg_ir::PrintedCards;
use mtg_ir::effect::{Modification, Restriction};

use crate::state::GameState;

/// How much of `amount` damage from `source` to `to` is still dealt.
///
/// Counting shields that prevent some of it are spent by pushing
/// [`Event::DamageShieldUsed`] onto `events`. Damage dealt simultaneously is prevented
/// against one state, so what `events` already spends is taken into account: one
/// shield is not spent twice on the same batch.
pub fn prevent(
    state: &GameState,
    cards: &dyn PrintedCards,
    source: ObjectId,
    to: Target,
    amount: u32,
    combat: bool,
    events: &mut Vec<Event>,
) -> u32 {
    if amount == 0 {
        return 0;
    }
    // CR 615.12: prevention cannot reduce this damage, but applicable effects
    // still perform their additional actions. Finite damage shields are not spent.
    if state.damage_unpreventable {
        if let Target::Object(object) = to {
            if crate::layers::restricted(state, cards, object, |r| {
                matches!(r, Restriction::PreventDamageRemoveCounter)
            }) {
                remove_counter(state, object, CounterKind::PlusOnePlusOne, events);
            }
            remove_counter(state, object, CounterKind::Shield, events);
        }
        return amount;
    }
    if (combat && state.prevent_combat_damage)
        || state.prevent_damage_to.contains(&to)
        || static_prevents(state, cards, source, to, combat)
        || matches!(to, Target::Object(object)
            if crate::eval::protected_from(state, cards, object, source))
        || matches!(to, Target::Player(p) if crate::targeting::player_protected(state, cards, p))
    {
        return 0;
    }
    // "Prevent that damage. Remove a +1/+1 counter from this creature": like a shield
    // counter, but it keeps working with no counters left.
    if let Target::Object(o) = to
        && crate::layers::restricted(state, cards, o, |r| {
            matches!(r, Restriction::PreventDamageRemoveCounter)
        })
    {
        let spent = events.iter().any(|e| {
            matches!(e, Event::CountersChanged { object, kind: CounterKind::PlusOnePlusOne, delta: -1 }
                if *object == o)
        });
        let has = state
            .objects
            .get(&o)
            .and_then(|obj| obj.counters.get(&CounterKind::PlusOnePlusOne))
            .is_some_and(|n| *n > 0);
        if !spent && has {
            events.push(Event::CountersChanged {
                object: o,
                kind: CounterKind::PlusOnePlusOne,
                delta: -1,
            });
        }
        return 0;
    }
    // CR 122.1c: damage to a permanent with a shield counter removes a counter instead.
    // Damage dealt at once by several sources costs one counter, not one each.
    if let Target::Object(o) = to
        && state.objects.get(&o).is_some_and(|obj| {
            obj.counters
                .get(&CounterKind::Shield)
                .is_some_and(|n| *n > 0)
        })
    {
        let spent = events.iter().any(|e| {
            matches!(e, Event::CountersChanged { object, kind: CounterKind::Shield, delta: -1 }
                if *object == o)
        });
        if !spent {
            events.push(Event::CountersChanged {
                object: o,
                kind: CounterKind::Shield,
                delta: -1,
            });
        }
        return 0;
    }
    let covers = |s: &&DamageShield| {
        s.to.is_none_or(|t| t == to)
            && s.by.is_none_or(|b| b == source)
            && (combat || !s.combat_only)
    };
    if state
        .damage_shields
        .iter()
        .filter(covers)
        .any(|s| s.remaining.is_none())
    {
        return 0;
    }
    let mut left = amount;
    for s in state.damage_shields.iter().filter(covers) {
        if left == 0 {
            break;
        }
        let already: u32 = events
            .iter()
            .map(|e| match e {
                Event::DamageShieldUsed { shield, amount } if *shield == s.id => *amount,
                _ => 0,
            })
            .sum();
        let used = s.remaining.unwrap_or(0).saturating_sub(already).min(left);
        if used > 0 {
            left -= used;
            events.push(Event::DamageShieldUsed {
                shield: s.id,
                amount: used,
            });
        }
    }
    left
}

/// Apply a counter-removal side effect once to this simultaneous damage batch.
fn remove_counter(state: &GameState, object: ObjectId, kind: CounterKind, events: &mut Vec<Event>) {
    let already = events.iter().any(|event| {
        matches!(event,
        Event::CountersChanged { object: id, kind: counter, delta: -1 }
            if *id == object && *counter == kind)
    });
    let has_counter = state
        .objects
        .get(&object)
        .and_then(|object| object.counters.get(&kind))
        .is_some_and(|count| *count > 0);
    if has_counter && !already {
        events.push(Event::CountersChanged {
            object,
            kind,
            delta: -1,
        });
    }
}

/// Whether a static ability of a permanent prevents this damage.
fn static_prevents(
    state: &GameState,
    cards: &dyn PrintedCards,
    source: ObjectId,
    to: Target,
    combat: bool,
) -> bool {
    crate::layers::effects(state, cards).iter().any(|e| {
        let Modification::Restriction(Restriction::PreventDamage {
            dealt_to,
            dealt_by,
            combat_only,
            from,
        }) = &e.modification
        else {
            return false;
        };
        if *combat_only && !combat {
            return false;
        }
        let by = *dealt_by && crate::layers::applies(state, cards, e, source);
        let to = *dealt_to
            && matches!(to, Target::Object(o) if crate::layers::applies(state, cards, e, o))
            && source_matches(state, cards, e.source, from, source);
        by || to
    })
}

/// Test the damage source against a static prevention's filter ("by creatures"), from
/// the point of view of the permanent with the ability.
fn source_matches(
    state: &GameState,
    cards: &dyn PrintedCards,
    ability_source: ObjectId,
    filter: &mtg_ir::ObjectFilter,
    damage_source: ObjectId,
) -> bool {
    if matches!(filter, mtg_ir::ObjectFilter::Any) {
        return true;
    }
    let chars = crate::eval::ComputedChars(cards);
    let ctx = crate::eval::Ctx {
        state,
        cards,
        chars: &chars,
        source: ability_source,
        controller: crate::layers::controller(state, ability_source).unwrap_or(state.active_player),
        targets: &[],
        target_legal: &[],
        x: 0,
        bindings: crate::empty_bindings(),
    };
    crate::eval::matches(&ctx, filter, damage_source).unwrap_or(false)
}
