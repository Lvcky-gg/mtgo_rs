//! State-based actions (CR 704).
//!
//! These are the rules that just *happen*: a creature with lethal damage dies, a
//! player at zero life loses, an Aura attached to nothing goes to the graveyard.
//! Three properties make them unlike everything else in the engine, and all three
//! are easy to get wrong:
//!
//! 1. **They use no stack and nobody gets priority for them.** They are not
//!    abilities and cannot be responded to.
//! 2. **All applicable actions happen simultaneously, as one event** (CR 704.3).
//!    Two creatures that would kill each other die together; neither dies first.
//! 3. **Then they are checked again**, because performing them can make more apply.
//!    This repeats until a check finds nothing — a fixpoint loop, not a pass.
//!
//! And the sequencing that matters most: this loop runs to completion **before**
//! any triggered ability is put on the stack (CR 117.5). Get that right and a
//! permanent dying to an SBA, plus the trigger that killed it, resolve in the
//! correct order with no special-casing anywhere.

use mtg_core::{CardType, CounterKind, LossReason, ObjectId, PlayerId, Zone};

use crate::{
    eval::{ComputedChars, Ctx},
    layers::PrintedCards,
    state::GameState,
};

/// One state-based action that currently applies.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Sba {
    PlayerLoses {
        player: PlayerId,
        reason: LossReason,
    },
    /// CR 704.5f/i — put into the graveyard directly. Not destruction, so
    /// indestructible and regeneration do not apply.
    PutIntoGraveyard {
        object: ObjectId,
        rule: &'static str,
    },
    /// CR 704.5g/h — destroyed, which *is* replaceable.
    Destroy {
        object: ObjectId,
        rule: &'static str,
    },
    /// CR 704.5q — an Equipment attached to something illegal comes unattached but
    /// stays on the battlefield.
    Unattach { object: ObjectId },
    /// CR 704.5e — a token outside the battlefield ceases to exist.
    CeaseToExist { object: ObjectId },
    /// CR 903.9a — a commander in a graveyard or exile goes to the command zone.
    ///
    /// The rule lets the owner choose; it is taken without asking, because keeping a
    /// commander in the graveyard is almost never wanted and a prompt after every death
    /// would be exactly the interruption the client is built to avoid.
    ReturnCommander { object: ObjectId },
}

/// The legend rule (CR 704.5j) needs its controller to choose which one to keep,
/// so it is reported separately from the actions that can be applied outright.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LegendConflict {
    pub controller: PlayerId,
    pub name: Box<str>,
    pub candidates: Vec<ObjectId>,
}

#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct Check {
    pub actions: Vec<Sba>,
    pub legend_conflicts: Vec<LegendConflict>,
}

impl Check {
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty() && self.legend_conflicts.is_empty()
    }
}

/// Find every state-based action that applies right now.
///
/// Pure: it inspects and reports, it does not mutate. The caller applies the whole
/// batch at once and calls again, which is what makes the simultaneity in CR 704.3
/// fall out of the structure instead of needing care at each site.
pub fn check(state: &GameState, cards: &dyn PrintedCards) -> Check {
    let mut out = Check::default();

    // ---- players (CR 704.5a-c) -----------------------------------------
    for p in &state.turn_order {
        let ps = state.player(*p);
        if ps.has_lost {
            continue;
        }
        if ps.life <= 0 {
            out.actions.push(Sba::PlayerLoses {
                player: *p,
                reason: LossReason::ZeroOrLessLife,
            });
        } else if ps.attempted_draw_from_empty {
            // CR 704.5b: the *attempt* is what loses the game, and it is checked
            // here rather than at draw time, so a replacement effect that refills
            // the library in between still saves the player.
            out.actions.push(Sba::PlayerLoses {
                player: *p,
                reason: LossReason::DrawFromEmptyLibrary,
            });
        } else if ps.poison >= 10 {
            out.actions.push(Sba::PlayerLoses {
                player: *p,
                reason: LossReason::PoisonCounters,
            });
        } else if state.commander.damage.iter().any(|((victim, _), n)| {
            victim == p && *n >= crate::state::CommanderState::LETHAL_DAMAGE
        }) {
            out.actions.push(Sba::PlayerLoses {
                player: *p,
                reason: LossReason::CommanderDamage,
            });
        }
    }

    // ---- commanders (CR 903.9a) -----------------------------------------
    if !state.commander.commanders.is_empty() {
        for (id, obj) in &state.objects {
            let where_ = obj.zone.zone;
            if matches!(where_, Zone::Graveyard | Zone::Exile)
                && state.commander.is_commander(obj.owner, obj.card)
            {
                out.actions.push(Sba::ReturnCommander { object: *id });
            }
        }
    }

    let chars = ComputedChars(cards);

    // ---- permanents (CR 704.5e-q) ---------------------------------------
    for id in state.battlefield() {
        let Some(obj) = state.objects.get(&id) else {
            continue;
        };
        let ctx = Ctx {
            state,
            cards,
            chars: &chars,
            source: id,
            controller: obj.controller,
            targets: &[],
            target_legal: &[],
            x: 0,
            bindings: crate::empty_bindings(),
        };
        let Ok(ch) = chars_of(&ctx, id) else { continue };

        if ch.has_type(CardType::Creature) {
            let toughness = ch.toughness.unwrap_or(0);

            // 704.5f — toughness 0 or less. Checked first: a creature with zero
            // toughness is put into the graveyard rather than destroyed, so
            // indestructible does not save it.
            if toughness <= 0 {
                out.actions.push(Sba::PutIntoGraveyard {
                    object: id,
                    rule: "704.5f",
                });
                continue;
            }

            // An indestructible creature is not destroyed by damage (CR 702.12b), so
            // neither rule below applies to it — reporting one would repeat forever.
            let destroyable = !crate::resolve::destruction(state, cards, id).is_empty();

            // 704.5h — any damage from a deathtouch source is lethal.
            if destroyable && obj.dealt_deathtouch_damage && obj.damage > 0 {
                out.actions.push(Sba::Destroy {
                    object: id,
                    rule: "704.5h",
                });
                continue;
            }

            // 704.5g — damage marked equal to or greater than toughness.
            if destroyable && i64::from(obj.damage) >= i64::from(toughness) {
                out.actions.push(Sba::Destroy {
                    object: id,
                    rule: "704.5g",
                });
                continue;
            }
        }

        // 704.5i — a planeswalker with no loyalty counters.
        if ch.has_type(CardType::Planeswalker)
            && obj
                .counters
                .get(&CounterKind::Loyalty)
                .copied()
                .unwrap_or(0)
                <= 0
        {
            out.actions.push(Sba::PutIntoGraveyard {
                object: id,
                rule: "704.5i",
            });
            continue;
        }

        // 704.5m — an Aura attached to nothing, or to something illegal. A bestowed one
        // becomes unattached instead, and a creature again (CR 702.103f).
        let is_aura = ch.has_type(CardType::Enchantment)
            && ch.subtypes.iter().any(|s| is_aura_subtype(cards, *s));
        // An Aura on a player is legal while that player is in the game.
        let on_player = obj
            .attached_player
            .is_some_and(|p| state.players.get(&p).is_some_and(|s| !s.has_lost));
        if (is_aura || obj.bestowed())
            && !on_player
            && !(attached_legally(state, obj.attached_to)
                && enchant_allows(state, cards, id, obj.attached_to)
                && !obj
                    .attached_to
                    .is_some_and(|to| crate::eval::protected_from(state, cards, to, id)))
        {
            if obj.bestowed() {
                out.actions.push(Sba::Unattach { object: id });
            } else {
                out.actions.push(Sba::PutIntoGraveyard {
                    object: id,
                    rule: "704.5m",
                });
            }
            continue;
        }

        // 704.5q — Equipment attached to something that is not a creature it can
        // legally be attached to. It unattaches rather than dying.
        if ch.has_type(CardType::Artifact)
            && obj.attached_to.is_some()
            && (!attached_to_creature(state, cards, obj.attached_to)
                || obj
                    .attached_to
                    .is_some_and(|to| crate::eval::protected_from(state, cards, to, id)))
        {
            out.actions.push(Sba::Unattach { object: id });
        }
    }

    // 704.5s — a Saga whose lore counters reached its final chapter, with no chapter
    // ability of it on the stack or waiting to go there.
    for id in state.battlefield() {
        let Some(obj) = state.objects.get(&id).filter(|o| !o.face_down) else {
            continue;
        };
        let Some((lore, chapters)) = cards.face(obj.card, obj.face).and_then(|f| {
            f.abilities.iter().find_map(|a| match a.kind {
                mtg_ir::AbilityKind::Saga { lore, chapters } => Some((lore, chapters)),
                _ => None,
            })
        }) else {
            continue;
        };
        let count = obj.counters.get(&lore).copied().unwrap_or(0);
        let waiting = state.objects.values().any(|o| {
            o.zone.zone == Zone::Stack
                && o.cast_context
                    .as_ref()
                    .is_some_and(|c| c.source == Some(id) && c.ability.is_some())
        }) || state
            .pending_triggers
            .pending
            .iter()
            .any(|t| t.source == id);
        if count >= i32::from(chapters) && !waiting {
            out.actions.push(Sba::PutIntoGraveyard {
                object: id,
                rule: "704.5s",
            });
        }
    }

    // 704.5e — a token that has left the battlefield.
    for obj in state.objects.values() {
        let is_ability = obj
            .cast_context
            .as_ref()
            .is_some_and(|c| c.ability.is_some());
        if obj.is_token && obj.zone.zone != Zone::Battlefield && !is_ability {
            out.actions.push(Sba::CeaseToExist { object: obj.id });
        }
        // A copy of a spell anywhere but the stack (CR 704.5e).
        if obj.is_spell_copy && obj.zone.zone != Zone::Stack {
            out.actions.push(Sba::CeaseToExist { object: obj.id });
        }
    }

    out.legend_conflicts = legend_conflicts(state, cards);
    out
}

/// CR 704.5j — a player controlling two or more legendary permanents with the same
/// name chooses one to keep; the rest go to their owners' graveyards.
fn legend_conflicts(state: &GameState, cards: &dyn PrintedCards) -> Vec<LegendConflict> {
    use std::collections::BTreeMap;

    let chars = ComputedChars(cards);
    let mut groups: BTreeMap<(PlayerId, Box<str>), Vec<ObjectId>> = BTreeMap::new();

    for id in state.battlefield() {
        let Some(obj) = state.objects.get(&id) else {
            continue;
        };
        let ctx = Ctx {
            state,
            cards,
            chars: &chars,
            source: id,
            controller: obj.controller,
            targets: &[],
            target_legal: &[],
            x: 0,
            bindings: crate::empty_bindings(),
        };
        let Ok(ch) = chars_of(&ctx, id) else { continue };
        if !ch.supertypes.contains(&mtg_core::Supertype::Legendary) {
            continue;
        }
        let Some(who) = crate::layers::controller(state, id) else {
            continue;
        };
        groups.entry((who, ch.name.clone())).or_default().push(id);
    }

    groups
        .into_iter()
        .filter(|(_, ids)| ids.len() > 1)
        .map(|((controller, name), candidates)| LegendConflict {
            controller,
            name,
            candidates,
        })
        .collect()
}

fn chars_of(ctx: &Ctx, id: ObjectId) -> Result<mtg_core::Characteristics, crate::eval::EvalError> {
    ctx.chars
        .characteristics(ctx.state, id)
        .ok_or(crate::eval::EvalError::UnknownCard(id))
}

fn attached_legally(state: &GameState, to: Option<ObjectId>) -> bool {
    to.is_some_and(|t| {
        state
            .objects
            .get(&t)
            .is_some_and(|o| o.zone.zone == Zone::Battlefield)
    })
}

/// Whether an Aura's enchant ability allows what it is attached to (CR 303.4d). An Aura
/// with no enchant ability recorded allows anything, which is the permissive direction —
/// the older behaviour for hand-built fixtures.
fn enchant_allows(
    state: &GameState,
    cards: &dyn PrintedCards,
    aura: ObjectId,
    to: Option<ObjectId>,
) -> bool {
    let (Some(to), Some(obj)) = (to, state.objects.get(&aura)) else {
        return false;
    };
    let Some(spec) = cards.face(obj.card, obj.face).and_then(|f| {
        if obj.bestowed() {
            return crate::targeting::bestow_targets(f).first().cloned();
        }
        f.abilities
            .iter()
            .find(|a| matches!(a.kind, mtg_ir::AbilityKind::Enchant))
            .and_then(|a| a.targets.first().cloned())
    }) else {
        return true;
    };
    let chars = ComputedChars(cards);
    let ctx = Ctx {
        state,
        cards,
        chars: &chars,
        source: aura,
        controller: obj.controller,
        targets: &[],
        target_legal: &[],
        x: 0,
        bindings: crate::empty_bindings(),
    };
    crate::eval::matches(&ctx, &spec.filter, to).unwrap_or(false)
}

fn attached_to_creature(state: &GameState, cards: &dyn PrintedCards, to: Option<ObjectId>) -> bool {
    to.is_some_and(|t| {
        crate::layers::compute(state, cards, t).is_some_and(|c| c.has_type(CardType::Creature))
    })
}

/// Whether a subtype is the Aura subtype. Subtypes are interned at import, so this
/// is resolved through the card database rather than hardcoded.
fn is_aura_subtype(cards: &dyn PrintedCards, s: mtg_core::Subtype) -> bool {
    cards.subtype_name(s) == Some("Aura")
}
