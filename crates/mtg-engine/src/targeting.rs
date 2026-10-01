//! Targets: choosing them, and checking them again later.
//!
//! Targeting is checked at two distinct moments, and conflating them is the usual way
//! it goes wrong:
//!
//! - **On announcement** (CR 601.2c). Targets are chosen as part of casting. If a
//!   required target has no legal choice, the spell *cannot be cast at all* — it is not
//!   cast and then fizzled, the announcement is simply illegal.
//! - **On resolution** (CR 608.2b). Legality is checked again. A target that has become
//!   illegal is ignored, and the spell does as much as it still can. If **every** target
//!   has become illegal, the spell does not resolve at all: it is countered by the game
//!   rules, and none of its effect happens — not even the parts that did not target.
//!
//! That last point is the one worth being careful about. "Deal 2 damage to target
//! creature and you gain 2 life" gains no life if the creature is gone, because the
//! whole spell fails to resolve rather than skipping the impossible part.

use mtg_core::{ObjectId, PlayerId, Target, Zone, ZoneRef};
use mtg_ir::{ObjectFilter, selector::TargetSpec};

use crate::{
    eval::{self, ComputedChars, Ctx},
    layers::PrintedCards,
    state::GameState,
};

fn ctx<'a>(
    state: &'a GameState,
    cards: &'a dyn PrintedCards,
    chars: &'a ComputedChars<'a>,
    source: ObjectId,
    controller: PlayerId,
) -> Ctx<'a> {
    Ctx {
        state,
        cards,
        chars,
        source,
        controller,
        targets: &[],
        target_legal: &[],
        x: 0,
        bindings: crate::empty_bindings(),
    }
}

/// Everything that could legally be chosen for one target slot.
///
/// `already` holds targets chosen for other slots; a slot marked
/// `distinct_from_other_targets` excludes them (CR 601.2c).
pub fn legal_targets(
    state: &GameState,
    cards: &dyn PrintedCards,
    spec: &TargetSpec,
    source: ObjectId,
    controller: PlayerId,
    already: &[Target],
) -> Vec<Target> {
    let chars = ComputedChars(cards);
    let c = ctx(state, cards, &chars, source, controller);
    let mut out = Vec::new();

    for id in objects_in(state, spec.zone) {
        // The spell itself is on the stack while targets are chosen, and a spell does
        // not target itself.
        if id == source {
            continue;
        }
        // Protection, shroud and hexproof are folded into `Targetable` rather than
        // restated here.
        let targetable = eval::matches(&c, &ObjectFilter::Targetable, id).unwrap_or(false);
        let matches_spec = eval::matches(&c, &spec.filter, id).unwrap_or(false);
        if targetable && matches_spec {
            out.push(Target::Object(id));
        }
    }

    if spec.allows_players {
        let allowed = spec
            .players
            .as_ref()
            .map(|sel| eval::players(&c, sel).unwrap_or_default());
        for p in &state.turn_order {
            if !state.player(*p).has_lost
                && allowed.as_ref().is_none_or(|a| a.contains(p))
                && !player_hexproof_from(state, cards, *p, controller)
            {
                out.push(Target::Player(*p));
            }
        }
    }

    if spec.distinct_from_other_targets {
        out.retain(|t| !already.contains(t));
    }

    out
}

fn objects_in(state: &GameState, zone: Zone) -> Vec<ObjectId> {
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

/// How many targets a slot requires at minimum.
///
/// An "up to" slot may legally take none, so it never blocks casting.
pub fn required(
    spec: &TargetSpec,
    state: &GameState,
    cards: &dyn PrintedCards,
    source: ObjectId,
    controller: PlayerId,
) -> u32 {
    if spec.up_to {
        return 0;
    }
    let chars = ComputedChars(cards);
    let c = ctx(state, cards, &chars, source, controller);
    eval::value(&c, &spec.count).unwrap_or(1).max(0) as u32
}

/// Whether every target slot can be filled — the CR 601.2c legality of announcing.
///
/// A spell whose required target has no legal choice cannot be cast. This is checked
/// *before* offering the action, so a player is never shown a spell they cannot legally
/// announce.
pub fn can_be_announced(
    state: &GameState,
    cards: &dyn PrintedCards,
    specs: &[TargetSpec],
    source: ObjectId,
    controller: PlayerId,
) -> bool {
    let mut taken: Vec<Target> = Vec::new();

    for spec in specs {
        let need = required(spec, state, cards, source, controller);
        let available = legal_targets(state, cards, spec, source, controller, &taken);
        if (available.len() as u32) < need {
            return false;
        }
        // Reserve what this slot would consume, so a later distinct slot is judged
        // against a realistic remainder rather than the full board.
        taken.extend(available.into_iter().take(need as usize));
    }

    true
}

/// The result of re-checking targets on resolution (CR 608.2b).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Recheck {
    /// Per chosen target, whether it is still a legal target.
    pub still_legal: Vec<bool>,
    /// True when the spell had targets and none of them are legal any more, so it does
    /// not resolve at all.
    pub fizzles: bool,
}

/// Re-check chosen targets against current state.
pub fn recheck(
    state: &GameState,
    cards: &dyn PrintedCards,
    specs: &[TargetSpec],
    chosen: &[Target],
    source: ObjectId,
    controller: PlayerId,
) -> Recheck {
    if chosen.is_empty() {
        // A spell with no targets never fizzles.
        return Recheck {
            still_legal: Vec::new(),
            fizzles: false,
        };
    }

    let chars = ComputedChars(cards);
    let c = ctx(state, cards, &chars, source, controller);

    let still_legal: Vec<bool> = chosen
        .iter()
        .enumerate()
        .map(|(i, target)| {
            // Fall back to the last spec when a spell has more chosen targets than
            // declared slots, which happens for a slot taking several targets.
            let spec = specs.get(i).or_else(|| specs.last());
            let Some(spec) = spec else { return false };

            match target {
                Target::Object(id) => {
                    // Gone from the zone it was targeted in is illegal, and so is
                    // having gained protection since.
                    state.objects.contains_key(id)
                        && state.objects[id].zone.zone == spec.zone
                        && eval::matches(&c, &ObjectFilter::Targetable, *id).unwrap_or(false)
                        && eval::matches(&c, &spec.filter, *id).unwrap_or(false)
                }
                Target::Player(p) => {
                    spec.allows_players
                        && !state.player(*p).has_lost
                        && !player_hexproof_from(state, cards, *p, controller)
                }
            }
        })
        .collect();

    let fizzles = !still_legal.iter().any(|ok| *ok);
    Recheck {
        still_legal,
        fizzles,
    }
}

/// A modal spell's or ability's modes: their labels and how many to choose, when its
/// effect is modal at the top (CR 700.2). Chosen on announcement, before targets.
pub fn modes_of(
    state: &GameState,
    cards: &dyn PrintedCards,
    object: ObjectId,
) -> Option<(Vec<Box<str>>, u8, u8)> {
    let obj = state.objects.get(&object)?;
    if obj.face_down {
        return None;
    }
    let face = cards.face(obj.card, obj.face)?;
    let want = obj.cast_context.as_ref().and_then(|c| c.ability);
    let ability = match want {
        Some(id) => crate::abilities::find(state, cards, object, id)?,
        None => std::borrow::Cow::Borrowed(
            face.abilities
                .iter()
                .find(|a| matches!(a.kind, mtg_ir::AbilityKind::SpellEffect(_)))?,
        ),
    };
    let effect = match &ability.kind {
        mtg_ir::AbilityKind::SpellEffect(e) => e,
        mtg_ir::AbilityKind::Activated { effect, .. } => effect,
        mtg_ir::AbilityKind::Triggered { effect, .. } => effect,
        _ => return None,
    };
    let mtg_ir::Effect::Modal {
        choose: mtg_ir::Value::Fixed(n),
        modes,
        at_least,
    } = effect
    else {
        return None;
    };
    let most = (*n).clamp(0, modes.len() as i32) as u8;
    Some((
        modes.iter().map(|(label, _)| label.clone()).collect(),
        most,
        at_least.unwrap_or(most).min(most),
    ))
}

/// The target specs of whatever a stack object represents.
pub fn specs_of(state: &GameState, cards: &dyn PrintedCards, object: ObjectId) -> Vec<TargetSpec> {
    let Some(obj) = state.objects.get(&object) else {
        return Vec::new();
    };
    let Some(face) = cards.face(obj.card, obj.face) else {
        return Vec::new();
    };
    let want = obj.cast_context.as_ref().and_then(|c| c.ability);
    // A face-down spell has no text, so nothing to target with (CR 708.2).
    if obj.face_down && want.is_none() {
        return Vec::new();
    }
    if let Some(id) = want {
        return crate::abilities::find(state, cards, object, id)
            .map(|a| a.targets.clone())
            .unwrap_or_default();
    }
    // An overloaded spell has no targets (CR 702.96b).
    if obj.cast_context.as_ref().and_then(|c| c.alt_cost)
        == Some(mtg_ir::ability::AltCost::Overload)
    {
        return Vec::new();
    }
    // A bestowed Aura spell targets what it will enchant (CR 702.103b).
    if obj.bestowed() {
        return bestow_targets(face);
    }

    face.abilities
        .iter()
        // An Aura spell targets through its enchant ability (CR 303.4a).
        .find(|a| {
            matches!(
                a.kind,
                mtg_ir::AbilityKind::SpellEffect(_) | mtg_ir::AbilityKind::Enchant
            )
        })
        .map(|a| a.targets.clone())
        .unwrap_or_default()
}

/// What a card's bestow ability enchants: its one target spec.
pub fn bestow_targets(face: &mtg_ir::CardFace) -> Vec<TargetSpec> {
    face.abilities
        .iter()
        .find(|a| {
            matches!(
                a.kind,
                mtg_ir::AbilityKind::AlternativeCost {
                    kind: mtg_ir::ability::AltCost::Bestow,
                    ..
                }
            )
        })
        .map(|a| a.targets.clone())
        .unwrap_or_default()
}

/// Whether `player` has hexproof against things `by` controls (CR 702.11c): "you have
/// hexproof" from a permanent they control, and `by` an opponent.
pub fn player_hexproof_from(
    state: &GameState,
    cards: &dyn PrintedCards,
    player: PlayerId,
    by: PlayerId,
) -> bool {
    player != by
        && crate::layers::effects(state, cards).iter().any(|e| {
            matches!(
                e.modification,
                mtg_ir::effect::Modification::Restriction(
                    mtg_ir::effect::Restriction::PlayerHexproof
                )
            ) && crate::layers::controller(state, e.source) == Some(player)
        })
}
