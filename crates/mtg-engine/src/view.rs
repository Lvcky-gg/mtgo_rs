//! Redacted per-player views.
//!
//! The engine holds complete truth, including both libraries in order. A client
//! must never see that. `PlayerView` is the *only* type the network layer is
//! allowed to serialise, and the only way to build one is [`project`], which
//! takes the full state and a viewer and drops everything that viewer is not
//! entitled to.
//!
//! Making this a distinct type rather than a "redact before sending" convention
//! is the point. A convention gets forgotten in one code path and leaks a
//! library order; a type means the leak does not compile. `mtg-net` has no
//! `Serialize` impl for `GameState` at all.

use std::collections::BTreeMap;

use mtg_core::{CardId, CounterKind, ObjectId, PlayerId, Step, Target, Zone, ZoneRef};

use crate::state::GameState;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PlayerView {
    pub viewer: PlayerId,
    pub turn: u32,
    pub active_player: PlayerId,
    pub step: Step,
    pub priority: Option<PlayerId>,
    pub players: BTreeMap<PlayerId, PlayerSummary>,
    /// Objects the viewer can see, with the detail they are entitled to.
    pub visible: BTreeMap<ObjectId, ObjectView>,
    /// Stack from bottom to top.
    pub stack: Vec<ObjectId>,
    /// Public global combat-damage prevention until cleanup.
    #[serde(default)]
    pub prevent_combat_damage: bool,
    /// Public recipients shielded from all damage until cleanup.
    #[serde(default)]
    pub prevent_damage_to: Vec<Target>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PlayerSummary {
    pub id: PlayerId,
    pub life: i32,
    pub poison: u32,
    /// Energy counters (CR 122.1c), public like poison.
    #[serde(default)]
    pub energy: u32,
    pub hand_size: u32,
    pub library_size: u32,
    pub graveyard: Vec<ObjectId>,
    /// Floating mana, by colour slot as in [`mtg_core::ManaPool`]. Public information:
    /// mana abilities are announced, so what a player has floating is known to all.
    pub mana: [u16; 6],
    /// Commander games: combat damage this player has taken from each opponent's commander,
    /// by that commander's owner (CR 903.10a). Empty otherwise.
    #[serde(default)]
    pub commander_damage: BTreeMap<PlayerId, u32>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ObjectView {
    pub id: ObjectId,
    pub zone: ZoneRef,
    pub controller: PlayerId,
    /// `None` when the viewer may see that an object exists but not what it is:
    /// a card in an opponent's hand, a face-down permanent, an unrevealed library
    /// card. The identity is absent from the message, not merely flagged.
    pub card: Option<CardId>,
    /// Face up on the stack or battlefield; hidden objects expose no face choice.
    #[serde(default)]
    pub face: u8,
    /// Public permission to play a card exiled by a resolving Adventure.
    #[serde(default)]
    pub adventure_player: Option<PlayerId>,
    pub tapped: bool,
    pub damage: u32,
    pub counters: BTreeMap<CounterKind, i32>,
    pub attached_to: Option<ObjectId>,
    pub targets: Vec<Target>,
    /// On the stack as an activated or triggered ability rather than a spell. Public:
    /// abilities are announced as they go on the stack.
    #[serde(default)]
    pub is_ability: bool,
    /// Public identity of the announced ability on the stack.
    #[serde(default)]
    pub ability: Option<mtg_core::AbilityId>,
    /// Declared as an attacker this combat.
    #[serde(default)]
    pub attacking: bool,
    /// Public attack destination, so the defending client can see what is threatened.
    #[serde(default)]
    pub attacking_target: Option<Target>,
    /// The attacker this creature is blocking, if it is blocking.
    #[serde(default)]
    pub blocking: Option<ObjectId>,
}

/// Build the view a single player is entitled to.
///
/// The visibility rules, in one place so they can be audited:
///
/// - Battlefield, stack, graveyard, exile and command: identities visible to all,
///   except face-down objects, which are visible only to their controller (and to
///   anyone an effect has shown them to).
/// - Own hand and revealed cards: visible.
/// - Opponent hands: count only.
/// - Libraries: count only, and **order is never transmitted**, not even shuffled,
///   because a client that receives the order can be modified to display it.
pub fn project(state: &GameState, viewer: PlayerId) -> PlayerView {
    let mut visible = BTreeMap::new();

    // The top card of a library some effect lets its owner, or everyone, see.
    let top_seen = |obj: &crate::state::GameObject| {
        obj.zone.zone == Zone::Library
            && (state.top_revealed.contains(&obj.owner)
                || (obj.owner == viewer && state.sees_top.contains(&obj.owner)))
            && state.objects_in(obj.zone).first() == Some(&obj.id)
    };
    for obj in state.objects.values() {
        let entitled = match obj.zone.zone {
            Zone::Battlefield | Zone::Stack | Zone::Graveyard | Zone::Exile | Zone::Command => {
                !obj.face_down || obj.controller == viewer
            }
            Zone::Hand => obj.owner == viewer,
            // Library membership is not revealed at all: a library card only
            // appears in a view once an effect has revealed or moved it.
            Zone::Library => top_seen(obj),
        };
        if obj.zone.zone == Zone::Library && !entitled {
            continue;
        }

        visible.insert(
            obj.id,
            ObjectView {
                id: obj.id,
                zone: obj.zone,
                controller: obj.controller,
                card: entitled.then_some(obj.card),
                face: if entitled { obj.face } else { 0 },
                adventure_player: entitled.then_some(obj.adventure_player).flatten(),
                tapped: obj.tapped,
                damage: obj.damage,
                counters: obj.counters.clone(),
                attached_to: obj.attached_to,
                targets: obj
                    .cast_context
                    .as_ref()
                    .map(|c| c.targets.clone())
                    .unwrap_or_default(),
                is_ability: obj
                    .cast_context
                    .as_ref()
                    .is_some_and(|c| c.ability.is_some()),
                ability: obj
                    .cast_context
                    .as_ref()
                    .filter(|_| entitled && obj.zone.zone == Zone::Stack)
                    .and_then(|c| c.ability),
                attacking: state.combat.is_attacking(obj.id),
                attacking_target: state.combat.attackers.get(&obj.id).copied(),
                blocking: state
                    .combat
                    .blocks
                    .iter()
                    .find(|(_, blockers)| blockers.contains(&obj.id))
                    .map(|(attacker, _)| *attacker),
            },
        );
    }

    let players = state
        .players
        .values()
        .map(|p| {
            (
                p.id,
                PlayerSummary {
                    id: p.id,
                    life: p.life,
                    poison: p.poison,
                    energy: p.energy,
                    hand_size: state.objects_in(ZoneRef::of(Zone::Hand, p.id)).len() as u32,
                    library_size: state.objects_in(ZoneRef::of(Zone::Library, p.id)).len() as u32,
                    graveyard: state.objects_in(ZoneRef::of(Zone::Graveyard, p.id)),
                    mana: p.mana.amounts,
                    commander_damage: state
                        .commander
                        .damage
                        .iter()
                        .filter(|((victim, _), _)| *victim == p.id)
                        .map(|((_, owner), n)| (*owner, *n))
                        .collect(),
                },
            )
        })
        .collect();

    PlayerView {
        prevent_combat_damage: state.prevent_combat_damage,
        prevent_damage_to: state.prevent_damage_to.clone(),
        viewer,
        turn: state.turn,
        active_player: state.active_player,
        step: state.step,
        priority: state.priority,
        players,
        visible,
        stack: state.objects_in(ZoneRef::shared(Zone::Stack)),
    }
}
