//! Game state.
//!
//! One rule governs this module: **computed values are never stored.** A
//! permanent's power, its types, its controller and its abilities are all
//! *derived* by [`crate::layers`] from the printed card plus every continuous
//! effect in play. Caching a computed power onto the object is the classic way a
//! rules engine goes subtly wrong, because every effect that could change it then
//! has to remember to invalidate it.
//!
//! What *is* stored is the irreducible per-object state: which zone it is in,
//! what counters are on it, how much damage is marked on it, whether it is
//! tapped. Those are facts, not derivations.

use std::collections::{BTreeMap, BTreeSet};

use mtg_core::{
    CardId, CounterKind, EventId, ManaPool, ObjectId, PlayerId, Step, Timestamp, Zone, ZoneRef,
};
use mtg_ir::effect::{Duration, Modification};

use crate::triggers::TriggerQueue;

#[derive(Clone, Debug)]
#[cfg_attr(feature = "verification", derive(serde::Serialize))]
pub struct RestrictedMana {
    pub player: PlayerId,
    pub source: ObjectId,
    pub amounts: [u16; 6],
    /// Every restriction must hold; nesting intersects permissions.
    pub restrictions: Vec<(mtg_ir::ObjectFilter, bool)>,
}

#[derive(Clone, Debug)]
pub struct GameState {
    /// Every object in every zone, including the stack.
    pub objects: BTreeMap<ObjectId, GameObject>,
    /// Ordered zones keep their order here; the battlefield and exile are
    /// unordered and derive membership from `GameObject::zone`.
    pub zone_order: BTreeMap<ZoneRef, Vec<ObjectId>>,
    pub players: BTreeMap<PlayerId, PlayerState>,
    /// Restricted units are a subset of the ordinary mana pool counts.
    pub restricted_mana: Vec<RestrictedMana>,
    /// Seating order. Turn order and APNAP order both derive from this.
    pub turn_order: Vec<PlayerId>,

    pub turn: u32,
    pub active_player: PlayerId,
    pub step: Step,
    /// Who has priority, or `None` during untap/cleanup and mid-resolution.
    pub priority: Option<PlayerId>,
    /// Consecutive passes with an unchanged stack; equals player count when the
    /// top of the stack should resolve (CR 117.4).
    pub consecutive_passes: u8,

    /// Live continuous effects, the input to the layer system.
    pub continuous: Vec<ContinuousEffect>,
    /// Global Fog-style prevention, lasting until cleanup.
    pub prevent_combat_damage: bool,
    /// "Damage can't be prevented this turn" (CR 615.12), until cleanup.
    pub damage_unpreventable: bool,
    /// Recipient identities shielded from all damage until cleanup.
    pub prevent_damage_to: Vec<mtg_core::Target>,
    /// For each player, when their previous and their most recent upkeep began — what
    /// "since the beginning of your last upkeep" (echo) compares timestamps against.
    pub upkeeps: BTreeMap<PlayerId, (Timestamp, Timestamp)>,
    /// Cards exiled "until <source> leaves the battlefield" (CR 610.3), as (source, card
    /// in exile). Returned by the engine as soon as the source is gone.
    pub linked_exile: Vec<(ObjectId, ObjectId)>,
    /// Cards exiled with a source by `Effect::ExileLinked`, as (source, card in exile),
    /// returned only by that source's own `Effect::ReturnExiledWith`.
    pub exiled_with: Vec<(ObjectId, ObjectId)>,
    /// Cards with "if ~ would be put into a graveyard from anywhere, exile it instead",
    /// so the replacement can be applied where events are, which has no card data.
    pub exiled_instead_of_graveyard: BTreeSet<CardId>,
    /// Cards with "if ~ would be put into a graveyard from anywhere, reveal ~ and shuffle it
    /// into its owner's library instead" (the Eldrazi titans' kin).
    pub shuffled_instead_of_graveyard: BTreeSet<CardId>,
    /// CR 726 — day (`Some(true)`), night, or neither yet.
    pub day: Option<bool>,
    /// Players who attacked with a creature this turn (raid).
    pub attacked_this_turn: BTreeSet<PlayerId>,
    /// Creatures declared as attackers this turn, by the identity they attacked with.
    pub attacked_creatures: BTreeSet<ObjectId>,
    /// Additional combat phases still to come this turn, each followed by a main phase
    /// (CR 500.8), and whether one taken after the precombat main phase still owes the
    /// turn its regular combat.
    pub extra_combats: u32,
    pub owed_combat: bool,
    /// Players dealt damage this turn (bloodthirst).
    pub damaged_this_turn: BTreeSet<PlayerId>,
    /// Players who lost life this turn, by damage or otherwise (CR 119.3).
    pub lost_life_this_turn: BTreeSet<PlayerId>,
    /// Players who gained life this turn.
    pub gained_life_this_turn: BTreeSet<PlayerId>,
    /// How much life each player gained this turn.
    pub life_gained_amount: BTreeMap<PlayerId, i32>,
    /// (source, object) for each object dealt damage this turn, by what dealt it.
    pub damaged_by_this_turn: BTreeSet<(ObjectId, ObjectId)>,
    /// Spells cast this turn, by anyone (storm).
    pub spells_cast_this_turn: u32,
    /// Spells each player cast this turn and last turn (werewolves, CR 702.145).
    pub spells_by_player: BTreeMap<PlayerId, u32>,
    pub spells_by_player_last_turn: BTreeMap<PlayerId, u32>,
    /// Noncreature spells each player cast this turn ("their first noncreature spell
    /// each turn").
    pub noncreature_spells_by_player: BTreeMap<PlayerId, u32>,
    /// Cards each player drew this turn ("your second card each turn").
    pub draws_this_turn: BTreeMap<PlayerId, u32>,
    /// Permanents put into a graveyard from the battlefield this turn, by the id they had
    /// there (their last-known information is in `last_known`).
    pub died_this_turn: Vec<ObjectId>,
    /// CR 724 — the monarch, and the command-zone object carrying the designation's
    /// triggered abilities.
    pub monarch: Option<PlayerId>,
    /// Extra turns to come, the most recently created last (taken first, CR 500.7), and
    /// whose turn the ordinary rotation continues from once they are taken.
    pub extra_turns: Vec<PlayerId>,
    pub rotation_from: Option<PlayerId>,
    /// One entry per turn a player is to skip ("you skip your next turn").
    pub skipped_turns: Vec<PlayerId>,
    /// Players who can't gain life ("players can't gain life"), as of the last time the
    /// engine settled — what `apply::replace` consults.
    pub no_life_gain: Vec<PlayerId>,
    /// "You gain that much life plus 1 instead", "twice that much": per player, what is
    /// added to a gain and how many times it is then doubled — refreshed with
    /// `no_life_gain`.
    pub life_gain_boost: BTreeMap<PlayerId, (i32, u32)>,
    /// Players who may look at the top card of their library any time, and players whose
    /// top card is revealed to everyone — refreshed with `no_life_gain`, read by the view.
    pub sees_top: Vec<PlayerId>,
    pub top_revealed: Vec<PlayerId>,
    /// Announced identities retained for projected views, without hidden-zone positions.
    pub revealed_cards: Vec<crate::view::RevealedCard>,
    /// Cards shown to one player only ("look at target player's hand"): that player's
    /// view lists them with the public reveals.
    pub looked_at: Vec<(PlayerId, crate::view::RevealedCard)>,
    /// Permanents a static "if a creature … would die, exile it instead" currently covers,
    /// recomputed as the engine settles (event application has no card data).
    pub exile_if_dies_now: std::collections::BTreeSet<ObjectId>,
    pub monarch_emblem: Option<ObjectId>,
    /// Prevention shields until cleanup, oldest first (see [`crate::prevention`]).
    pub damage_shields: Vec<mtg_core::DamageShield>,
    pub next_shield: u32,

    /// Who is attacking whom, and who is blocking. Reset in end-of-combat.
    pub combat: Combat,

    /// Triggers that have fired but are waiting to be put on the stack (CR 603.3).
    pub pending_triggers: TriggerQueue,

    /// Objects as they last existed on the battlefield, keyed by the identity they
    /// had there.
    ///
    /// CR 603.10: a "leaves the battlefield" trigger checks its condition against the
    /// game state *before* the event, because by the time it could look, the thing it
    /// cares about is gone. The same information answers last-known-information
    /// questions generally. Cleared each turn, since nothing looks further back.
    pub last_known: BTreeMap<ObjectId, GameObject>,

    /// How many times each ability has triggered this turn, for `TriggerLimit`.
    /// Cleared at the start of each turn.
    pub triggered_this_turn: BTreeMap<(ObjectId, mtg_core::AbilityId), u32>,
    /// CR 603.7 — delayed triggered abilities waiting to fire, each once.
    pub delayed: Vec<DelayedTrigger>,
    pub next_delayed: u32,
    /// Activations this turn, for "activate only once each turn".
    pub activated_this_turn: BTreeMap<(ObjectId, mtg_core::AbilityId), u32>,
    /// Every ability ever activated, for exhaust ("activate only once").
    pub activated_ever: std::collections::BTreeSet<(ObjectId, mtg_core::AbilityId)>,

    /// CR 606.3: once per permanent per turn, shared by all its loyalty abilities
    /// and all players who control it during that turn.
    pub loyalty_activated_this_turn: std::collections::BTreeSet<ObjectId>,
    /// Sources that did their "do this only once each turn" thing this turn.
    pub done_once_this_turn: std::collections::BTreeSet<ObjectId>,

    /// How far through the log trigger detection has scanned.
    ///
    /// Detection is a scan over newly logged events rather than a hook inside
    /// `apply`, which keeps `apply` free of card-data dependencies and means a
    /// replayed log detects exactly the same triggers.
    pub scanned_upto: usize,

    /// Commander rules (CR 903), when the game is a Commander game. Empty otherwise, and
    /// every Commander rule is then inert.
    pub commander: CommanderState,

    /// Opening-hand decisions still to make (CR 103.5). `None` once every player has kept,
    /// and for a game set up without mulligans.
    pub pregame: Option<Pregame>,

    /// The game's source of randomness. Seeded when the game is created — from the match's
    /// agreed seed — so the engine stays a pure function of its inputs: the same seed makes
    /// the same shuffles.
    pub rng: Rng,

    /// Bumped on every mutation. Doubles as the characteristics cache generation.
    pub generation: Timestamp,
    /// Next event sequence number.
    pub next_event: EventId,
    next_object: u32,
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "verification", derive(serde::Serialize))]
pub struct GameObject {
    pub id: ObjectId,
    /// Which printed card this is. Tokens and copies point at a synthesised entry.
    pub card: CardId,
    /// Which face is up, for multi-face cards.
    pub face: u8,
    /// Tokens cease to exist once they leave the battlefield (CR 704.5e), and they
    /// are never shuffled back into a library, so the distinction has to be carried
    /// on the object rather than inferred from its card data.
    pub is_token: bool,
    pub owner: PlayerId,
    /// Stored controller. Layer 2 can override it, so read the *computed*
    /// controller through the layer system, not this field.
    pub controller: PlayerId,
    pub zone: ZoneRef,
    /// CR 613.7. Set when the object enters a zone and when it changes control.
    pub timestamp: Timestamp,

    pub tapped: bool,
    pub face_down: bool,
    pub phased_out: bool,
    pub summoning_sick: bool,
    /// The turn this object last entered the battlefield, for "if ~ entered this turn".
    /// (Summoning sickness lasts until its controller's turn, which can be later.)
    pub entered_turn: Option<u32>,
    /// Damage marked this turn, cleared in cleanup (CR 514.2).
    pub damage: u32,
    /// Whether any of that damage came from a deathtouch source. Tracked as a flag
    /// rather than recomputed, because CR 704.5h cares that the damage *was dealt*
    /// by a deathtouch source — losing deathtouch afterwards does not save the
    /// creature.
    pub dealt_deathtouch_damage: bool,
    pub counters: BTreeMap<CounterKind, i32>,
    pub attached_to: Option<ObjectId>,
    /// CR 701.15 — regeneration shields, each replacing one destruction this turn.
    pub regeneration_shields: u32,
    /// For a permanent that was a spell: the `X` it was cast with, and whether it was
    /// kicked — what "enters with X counters" and "if it was kicked" ask (CR 107.3m).
    pub cast_x: u32,
    /// For a permanent that was a spell: the mana spent to cast it, by color.
    pub mana_spent: Vec<(Option<mtg_core::Color>, u16)>,
    pub kicked: bool,
    /// How many times a multikicker was paid (CR 702.33c).
    pub kicks: u32,
    /// The alternative cost it was cast for, if any (dash, evoke).
    pub cast_for: Option<mtg_ir::ability::AltCost>,
    /// For objects on the stack: the targets chosen on announcement, the value of
    /// `X`, and the modes selected.
    pub cast_context: Option<CastContext>,
    /// CR 715.3d: permission belongs to the player who resolved the Adventure
    /// and lasts only while this object remains exiled.
    pub adventure_player: Option<PlayerId>,
    /// "You may play that card this turn": who may, through which turn, and whether only
    /// as a spell. Lasts while this object stays where it is.
    pub may_play: Option<(PlayerId, u32, bool)>,
    /// Foretold or plotted: who may cast it after which turn, for what (`None`: free), and
    /// whether only as a sorcery.
    pub cast_later: Option<(PlayerId, u32, Option<mtg_core::ManaCost>, bool)>,
    /// "If it would die this turn, exile it instead": the turn it holds for.
    pub exile_if_dies: Option<u32>,
    /// An Aura attached to a player rather than an object.
    pub attached_player: Option<PlayerId>,
    /// While this permanent is a copy of something else (CR 707.2), `card` and `face` are
    /// the copied object's and this is what they were. A copy effect lasts only while
    /// the permanent is on the battlefield, so leaving puts them back.
    ///
    /// Swapping the card rather than overlaying it means everything that reads printed
    /// characteristics and abilities — the layer system, triggers, costs — sees the copy
    /// without knowing copies exist.
    pub original: Option<(CardId, u8)>,
    /// "If it would leave the battlefield, exile it instead" applies to it (unearth).
    /// Lost with the zone change it replaces.
    pub exile_if_leaves: bool,
    /// A copy of a spell (CR 707.10): it ceases to exist anywhere but the stack, and
    /// becomes a token if it resolves as a permanent.
    pub is_spell_copy: bool,
    /// An Aura with "you control enchanted …" (layer 2): whoever controls it controls what
    /// it is attached to. Set as it enters, from its text, so the controller can be worked
    /// out without card data (see [`crate::layers::controller`]).
    pub controls_host: bool,
    /// CR 702.112b — renowned, until it leaves the battlefield.
    pub renowned: bool,
    /// CR 701.37b — monstrous, until it leaves the battlefield.
    pub monstrous: bool,
    /// CR 716 — a Class's level beyond its first; 0 until it gains one.
    pub class_level: u8,
    /// What this was attached to until that left the battlefield: last-known information
    /// for "when enchanted creature dies" (CR 603.10a).
    pub was_attached_to: Option<ObjectId>,
    /// For a permanent that was a spell: the zone it was cast from ("if you cast it").
    pub cast_from: Option<Zone>,
    /// What its controller chose as it entered ("the chosen color/type").
    pub chosen_color: Option<mtg_core::Color>,
    pub chosen_subtype: Option<mtg_core::Subtype>,
}

impl GameObject {
    /// Whether this is a bestowed Aura (CR 702.103b): a spell cast for its bestow cost, or
    /// the permanent it became, until it is unattached (CR 702.103f).
    pub fn bestowed(&self) -> bool {
        let bestow = Some(mtg_ir::ability::AltCost::Bestow);
        match self.zone.zone {
            Zone::Stack => self.cast_context.as_ref().and_then(|c| c.alt_cost) == bestow,
            Zone::Battlefield => self.cast_for == bestow,
            _ => false,
        }
    }

    /// A fresh object with no accumulated state.
    pub fn new(id: ObjectId, card: CardId, owner: PlayerId, zone: ZoneRef) -> Self {
        Self {
            id,
            card,
            face: 0,
            is_token: false,
            owner,
            controller: owner,
            zone,
            timestamp: Timestamp::ZERO,
            tapped: false,
            face_down: false,
            phased_out: false,
            summoning_sick: false,
            entered_turn: None,
            damage: 0,
            dealt_deathtouch_damage: false,
            counters: BTreeMap::new(),
            attached_to: None,
            regeneration_shields: 0,
            cast_x: 0,
            kicked: false,
            kicks: 0,
            cast_for: None,
            cast_context: None,
            attached_player: None,
            exile_if_dies: None,
            mana_spent: Vec::new(),
            cast_later: None,
            may_play: None,
            adventure_player: None,
            original: None,
            exile_if_leaves: false,
            is_spell_copy: false,
            controls_host: false,
            renowned: false,
            monstrous: false,
            class_level: 0,
            was_attached_to: None,
            cast_from: None,
            chosen_color: None,
            chosen_subtype: None,
        }
    }
}

impl GameState {
    /// Put an object straight into a zone. For setting up tests and for loading a
    /// decklist at game start, where no zone-change event has occurred yet.
    pub fn place(&mut self, card: CardId, owner: PlayerId, zone: ZoneRef) -> ObjectId {
        let id = self.new_object_id();
        let ts = self.bump();
        let mut obj = GameObject::new(id, card, owner, zone);
        obj.timestamp = ts;
        self.objects.insert(id, obj);
        if zone.zone.is_ordered() {
            self.zone_order.entry(zone).or_default().push(id);
        }
        id
    }
}

/// The London mulligan in progress (CR 103.5): players decide in turn order.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "verification", derive(serde::Serialize))]
pub struct Pregame {
    /// Players in the order they decide: the starting player first.
    pub order: Vec<PlayerId>,
    /// Index into `order` of the player deciding now.
    pub next: usize,
    /// Mulligans taken so far, per player.
    pub taken: BTreeMap<PlayerId, u32>,
    /// A player who has kept after mulliganing, still to put this many cards on the bottom.
    pub to_bottom: Option<(PlayerId, u32)>,
    /// Once everyone has kept: index into `order` of the player now choosing which cards
    /// to begin the game with on the battlefield (CR 103.6).
    pub leylines: Option<usize>,
}

impl Pregame {
    pub fn new(order: Vec<PlayerId>) -> Self {
        Self {
            order,
            ..Default::default()
        }
    }
}

/// A small deterministic generator (SplitMix64). Not cryptographic: the host runs the engine
/// and is trusted with its state already; what matters is that a seed fixes every shuffle.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "verification", derive(serde::Serialize))]
pub struct Rng(u64);

impl Rng {
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        let folded = seed
            .chunks(8)
            .map(|c| u64::from_le_bytes(c.try_into().unwrap_or([0; 8])))
            .fold(0x9E37_79B9_7F4A_7C15u64, |acc, w| acc.rotate_left(17) ^ w);
        Self(folded)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Fisher–Yates.
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = (self.next_u64() % (i as u64 + 1)) as usize;
            items.swap(i, j);
        }
    }
}

/// What Commander adds to a game's state (CR 903).
///
/// A commander is tracked by its owner and card, not by object: an object's identity changes
/// every time it changes zones (CR 400.7), and the rules follow the card. A Commander deck is
/// singleton, so owner and card name exactly one card.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "verification", derive(serde::Serialize))]
pub struct CommanderState {
    /// CR 903.9a: graveyard/exile identities whose return opportunity was
    /// already considered. A zone change creates a fresh eligible identity.
    pub return_considered: BTreeSet<ObjectId>,
    /// Each player's commander.
    pub commanders: BTreeMap<PlayerId, CardId>,
    /// CR 903.8 — how many times each player has cast their commander from the command zone.
    pub casts: BTreeMap<PlayerId, u32>,
    /// Tax owed by a commander spell now on the stack, fixed when it was cast.
    pub tax_due: BTreeMap<ObjectId, u32>,
    /// CR 903.10a — combat damage dealt to a player by a commander, keyed by
    /// (player dealt damage, commander's owner).
    pub damage: BTreeMap<(PlayerId, PlayerId), u32>,
}

impl CommanderState {
    /// CR 903.10a: this much combat damage from one commander loses the game.
    pub const LETHAL_DAMAGE: u32 = 21;

    /// Whether `card`, owned by `owner`, is that player's commander.
    pub fn is_commander(&self, owner: PlayerId, card: CardId) -> bool {
        self.commanders.get(&owner) == Some(&card)
    }

    /// CR 903.8: {2} more for each previous cast from the command zone.
    pub fn tax(&self, owner: PlayerId) -> u32 {
        2 * self.casts.get(&owner).copied().unwrap_or(0)
    }
}

#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "verification", derive(serde::Serialize))]
pub struct CastContext {
    /// One per target slot, in slot order.
    pub targets: Vec<mtg_core::Target>,
    /// One frozen specification per chosen target (or empty-slot placeholder).
    /// Target counts may change later; group membership must not change with them.
    pub target_specs: Option<Vec<mtg_ir::selector::TargetSpec>>,
    /// Announcement occurrence for each target; distinctness within a group survives copying.
    pub target_groups: Option<Vec<usize>>,
    /// Indices into `targets` that are placeholders for a slot left empty — an "up to"
    /// slot with nothing chosen, or a slot of a mode that was not chosen. They select
    /// nothing and do not count towards a spell's targets when it resolves (CR 608.2b).
    pub empty_slots: Vec<u8>,
    pub x: u32,
    pub modes: Vec<u8>,
    /// Which ability of the source this stack object represents, for abilities.
    pub ability: Option<mtg_core::AbilityId>,
    pub source: Option<ObjectId>,
    /// Slots captured from the event that triggered this, carried here because the
    /// `PendingTrigger` is consumed when the ability goes on the stack.
    pub bindings: BTreeMap<mtg_ir::selector::Binding, Vec<mtg_core::Target>>,
    /// The mana cost paid instead of the printed one (flashback).
    pub cost_override: Option<mtg_core::ManaCost>,
    /// Exiled instead of going anywhere else when it leaves the stack (flashback).
    pub exile_on_leave: bool,
    /// CR 702.33 — the kicker cost was paid.
    pub kicked: bool,
    /// How many times a multikicker or replicate cost was paid (CR 702.33c, 702.56a).
    pub kicks: u32,
    /// Cast at a time a sorcery couldn't have been (for a flash surcharge).
    pub flashed: bool,
    /// Emerge: the sacrificed creature's mana value, off the generic cost (CR 702.119a).
    pub emerge_reduction: u32,
    /// Which of an "A or B" additional casting cost was chosen.
    pub cost_choice: Option<u8>,
    /// A delayed trigger's effect, which is not on any card.
    pub effect: Option<mtg_ir::Effect>,
    /// The alternative cost it was cast for (dash, evoke).
    pub alt_cost: Option<mtg_ir::ability::AltCost>,
    /// A creature spell that gains haste as it becomes a permanent (suspend).
    pub gains_haste: bool,
    /// The zone a spell was cast from (rebound asks whether it was the hand).
    pub cast_from: Option<Zone>,
    /// How many spells had been cast this turn before this one (storm, CR 702.40a).
    pub cast_index: u32,
    /// The mana spent to cast this spell, by color (`None` colorless).
    pub mana_spent: Vec<(Option<mtg_core::Color>, u16)>,
    /// A granted ability's text, copied as it goes on the stack: what granted it may
    /// leave play before it resolves, and the ability still resolves (CR 113.7a).
    pub granted: Option<Box<mtg_ir::Ability>>,
    /// A mana ability being announced because its cost is a choice (`mana::announced`):
    /// the colour picked as it was activated. It resolves as soon as it is paid.
    pub mana_choice: Option<Option<mtg_core::Color>>,
}

/// Combat assignments for the current turn (CR 506–511).
///
/// Held on the state rather than derived, because "is attacking" is a fact
/// established by a turn-based action, not a characteristic.
#[derive(Clone, Default, Debug)]
#[cfg_attr(feature = "verification", derive(serde::Serialize))]
pub struct Combat {
    /// The defending seat stays known if its attacked planeswalker leaves combat.
    pub defending_player: Option<PlayerId>,
    /// Multiplayer association for each attacker, including an attacked walker that left.
    #[cfg_attr(
        feature = "verification",
        serde(skip_serializing_if = "BTreeMap::is_empty")
    )]
    pub defenders: BTreeMap<ObjectId, PlayerId>,
    /// Attacker to what it is attacking.
    pub attackers: BTreeMap<ObjectId, mtg_core::Target>,
    /// Attacker to its blockers. Their order does not constrain damage assignment.
    pub blocks: BTreeMap<ObjectId, Vec<ObjectId>>,
    /// Attackers that were blocked at least once. Tracked separately from
    /// `blocks` because a blocker leaving combat does not make the attacker
    /// unblocked (CR 509.1h).
    pub was_blocked: BTreeSet<ObjectId>,
    /// Creatures with first/double strike when the first damage step began (CR 510.4).
    /// None means combat has not had a first-strike damage step.
    pub first_strike_participants: Option<BTreeSet<ObjectId>>,
}

impl Combat {
    pub fn is_attacking(&self, id: ObjectId) -> bool {
        self.attackers.contains_key(&id)
    }

    pub fn is_blocking(&self, id: ObjectId) -> bool {
        self.blocks.values().any(|bs| bs.contains(&id))
    }

    pub fn clear(&mut self) {
        self.defending_player = None;
        self.defenders.clear();
        self.attackers.clear();
        self.blocks.clear();
        self.was_blocked.clear();
        self.first_strike_participants = None;
    }
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "verification", derive(serde::Serialize))]
pub struct PlayerState {
    pub id: PlayerId,
    pub life: i32,
    pub mana: ManaPool,
    pub poison: u32,
    /// Energy counters (CR 122.1c).
    pub energy: u32,
    pub city_blessing: bool,
    /// CR 702.179 — speed, once a player has any; and the turn it last increased.
    pub speed: Option<u8>,
    pub speed_raised_turn: u32,
    /// Lands played this turn, against the one-per-turn limit (CR 305.2).
    pub lands_played: u32,
    pub has_lost: bool,
    /// Whether this player has already passed in the current priority round.
    pub passed: bool,
    /// CR 704.5b: a player who *attempted* to draw from an empty library loses the
    /// next time state-based actions are checked. The attempt is the trigger, not
    /// the empty library, so it has to be recorded when it happens.
    pub attempted_draw_from_empty: bool,
    /// CR 103.4 — the life total the game began with.
    pub starting_life: i32,
}

impl PlayerState {
    pub fn new(id: PlayerId, starting_life: i32) -> Self {
        Self {
            id,
            life: starting_life,
            mana: ManaPool::default(),
            poison: 0,
            energy: 0,
            city_blessing: false,
            speed: None,
            speed_raised_turn: 0,
            lands_played: 0,
            has_lost: false,
            passed: false,
            attempted_draw_from_empty: false,
            starting_life,
        }
    }
}

/// A delayed triggered ability (CR 603.7), created by a resolving spell or ability.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "verification", derive(serde::Serialize))]
pub struct DelayedTrigger {
    pub id: u32,
    /// What created it: its card names the trigger, and it is "this" for the effect.
    pub source: ObjectId,
    pub card: CardId,
    pub controller: PlayerId,
    pub on: mtg_ir::EventPattern,
    pub effect: mtg_ir::Effect,
    /// "It" and friends, as they were when it was created.
    pub bindings: BTreeMap<mtg_ir::selector::Binding, Vec<mtg_core::Target>>,
    /// What it targets, chosen as it goes on the stack (a reflexive trigger's).
    pub targets: Vec<mtg_ir::selector::TargetSpec>,
}

/// A continuous effect currently applying.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "verification", derive(serde::Serialize))]
pub struct ContinuousEffect {
    pub id: ObjectId,
    /// The ability or resolved spell that created it.
    pub source: ObjectId,
    /// Objects affected. Static abilities re-evaluate their selector every time
    /// the layer system runs; one-shot effects freeze the set at resolution.
    pub affected: AffectedSet,
    pub modification: Modification,
    pub duration: Duration,
    pub timestamp: Timestamp,
    /// CR 613.8 dependency resolution needs to know whether applying this effect
    /// changes what another effect applies to.
    pub layer: u8,
    /// For an effect generated by a static ability, which ability of `source` it is.
    pub ability: Option<mtg_core::AbilityId>,
    /// Who "you" is for this effect when its source may be gone — a resolved spell's
    /// controller, for "gain control of target creature" (layer 2).
    pub controller: Option<PlayerId>,
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "verification", derive(serde::Serialize))]
pub enum AffectedSet {
    /// Frozen at resolution — "target creature gets +1/+1 until end of turn"
    /// keeps affecting that creature even if it stops matching.
    Fixed(Vec<ObjectId>),
    /// Re-evaluated on every layer pass — a static anthem affects whatever
    /// currently matches.
    Dynamic(mtg_ir::Selector),
}

impl GameState {
    /// An empty game with the given seats, before any deck is loaded.
    pub fn new(seats: &[PlayerId], starting_life: i32) -> Self {
        Self {
            objects: BTreeMap::new(),
            zone_order: BTreeMap::new(),
            restricted_mana: Vec::new(),
            players: seats
                .iter()
                .map(|p| (*p, PlayerState::new(*p, starting_life)))
                .collect(),
            turn_order: seats.to_vec(),
            turn: 0,
            active_player: seats.first().copied().unwrap_or(PlayerId(0)),
            step: Step::Untap,
            priority: None,
            consecutive_passes: 0,
            continuous: Vec::new(),
            revealed_cards: Vec::new(),
            looked_at: Vec::new(),
            exile_if_dies_now: Default::default(),
            prevent_combat_damage: false,
            damage_unpreventable: false,
            prevent_damage_to: Vec::new(),
            upkeeps: BTreeMap::new(),
            spells_cast_this_turn: 0,
            spells_by_player: BTreeMap::new(),
            spells_by_player_last_turn: BTreeMap::new(),
            noncreature_spells_by_player: BTreeMap::new(),
            draws_this_turn: BTreeMap::new(),
            died_this_turn: Vec::new(),
            monarch: None,
            extra_turns: Vec::new(),
            rotation_from: None,
            skipped_turns: Vec::new(),
            no_life_gain: Vec::new(),
            life_gain_boost: BTreeMap::new(),
            sees_top: Vec::new(),
            top_revealed: Vec::new(),
            monarch_emblem: None,
            damaged_this_turn: BTreeSet::new(),
            lost_life_this_turn: BTreeSet::new(),
            gained_life_this_turn: BTreeSet::new(),
            life_gained_amount: BTreeMap::new(),
            damaged_by_this_turn: BTreeSet::new(),
            attacked_this_turn: BTreeSet::new(),
            attacked_creatures: BTreeSet::new(),
            extra_combats: 0,
            owed_combat: false,
            day: None,
            exiled_instead_of_graveyard: BTreeSet::new(),
            shuffled_instead_of_graveyard: BTreeSet::new(),
            linked_exile: Vec::new(),
            exiled_with: Vec::new(),
            damage_shields: Vec::new(),
            next_shield: 0,
            combat: Combat::default(),
            pending_triggers: TriggerQueue::default(),
            last_known: BTreeMap::new(),
            triggered_this_turn: BTreeMap::new(),
            activated_this_turn: BTreeMap::new(),
            activated_ever: Default::default(),
            loyalty_activated_this_turn: Default::default(),
            done_once_this_turn: Default::default(),
            delayed: Vec::new(),
            next_delayed: 0,
            commander: CommanderState::default(),
            pregame: None,
            rng: Rng::default(),
            scanned_upto: 0,
            generation: Timestamp::ZERO,
            next_event: EventId::FIRST,
            next_object: 0,
        }
    }

    /// Allocate a fresh object identity. Called on every zone change, because
    /// CR 400.7 makes the object in the new zone a different object.
    pub fn new_object_id(&mut self) -> ObjectId {
        self.next_object += 1;
        ObjectId(self.next_object)
    }

    pub fn bump(&mut self) -> Timestamp {
        self.generation.advance()
    }

    /// Players in APNAP order starting from the active player (CR 101.4). This is
    /// the order triggers go on the stack and the order simultaneous choices are
    /// made in. Only players still in the game (CR 800.4a); the active player's seat
    /// still starts the order after they have left (CR 800.4j).
    pub fn apnap(&self) -> Vec<PlayerId> {
        let n = self.turn_order.len();
        let start = self
            .turn_order
            .iter()
            .position(|p| *p == self.active_player)
            .unwrap_or(0);
        (0..n)
            .map(|i| self.turn_order[(start + i) % n])
            .filter(|p| self.players.get(p).is_some_and(|s| !s.has_lost))
            .collect()
    }

    pub fn objects_in(&self, zone: ZoneRef) -> Vec<ObjectId> {
        if zone.zone.is_ordered() {
            self.zone_order.get(&zone).cloned().unwrap_or_default()
        } else {
            self.objects
                .values()
                .filter(|o| o.zone == zone)
                .map(|o| o.id)
                .collect()
        }
    }

    /// The permanents on the battlefield — not those phased out, which are treated as
    /// though they don't exist (CR 702.26b).
    pub fn battlefield(&self) -> Vec<ObjectId> {
        self.objects_in(ZoneRef::shared(Zone::Battlefield))
            .into_iter()
            .filter(|id| self.objects.get(id).is_none_or(|o| !o.phased_out))
            .collect()
    }

    /// Everything on the battlefield, phased out or not.
    pub fn battlefield_with_phased_out(&self) -> Vec<ObjectId> {
        self.objects_in(ZoneRef::shared(Zone::Battlefield))
    }

    pub fn player(&self, p: PlayerId) -> &PlayerState {
        &self.players[&p]
    }
}

#[cfg(feature = "verification")]
pub(crate) struct DiagnosticState<'a>(pub &'a GameState);
#[cfg(feature = "verification")]
impl serde::Serialize for DiagnosticState<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        // Keep this pattern exhaustive: new state fields require an explicit
        // diagnostic/canonicalization decision rather than silently disappearing.
        let GameState {
            objects: _,
            zone_order: _,
            players: _,
            restricted_mana: _,
            turn_order: _,
            turn: _,
            active_player: _,
            step: _,
            priority: _,
            consecutive_passes: _,
            continuous: _,
            prevent_combat_damage: _,
            damage_unpreventable: _,
            prevent_damage_to: _,
            upkeeps: _,
            linked_exile: _,
            exiled_with: _,
            exiled_instead_of_graveyard: _,
            shuffled_instead_of_graveyard: _,
            day: _,
            attacked_this_turn: _,
            attacked_creatures: _,
            extra_combats: _,
            owed_combat: _,
            damaged_this_turn: _,
            lost_life_this_turn: _,
            gained_life_this_turn: _,
            life_gained_amount: _,
            damaged_by_this_turn: _,
            spells_cast_this_turn: _,
            spells_by_player: _,
            spells_by_player_last_turn: _,
            noncreature_spells_by_player: _,
            draws_this_turn: _,
            died_this_turn: _,
            monarch: _,
            extra_turns: _,
            rotation_from: _,
            skipped_turns: _,
            no_life_gain: _,
            life_gain_boost: _,
            sees_top: _,
            top_revealed: _,
            revealed_cards: _,
            looked_at: _,
            exile_if_dies_now: _,
            monarch_emblem: _,
            damage_shields: _,
            next_shield: _,
            combat: _,
            pending_triggers: _,
            last_known: _,
            triggered_this_turn: _,
            delayed: _,
            next_delayed: _,
            activated_this_turn: _,
            activated_ever: _,
            loyalty_activated_this_turn: _,
            done_once_this_turn: _,
            scanned_upto: _,
            commander: _,
            pregame: _,
            rng: _,
            generation: _,
            next_event: _,
            next_object: _,
        } = self.0;
        let mut out = serializer.serialize_struct(
            "GameStateDiagnostic",
            63 + usize::from(!self.0.restricted_mana.is_empty()),
        )?;
        out.serialize_field("objects", &self.0.objects)?;
        out.serialize_field("zone_order", &self.0.zone_order)?;
        out.serialize_field("players", &self.0.players)?;
        out.serialize_field("turn_order", &self.0.turn_order)?;
        out.serialize_field("turn", &self.0.turn)?;
        out.serialize_field("active_player", &self.0.active_player)?;
        out.serialize_field("step", &self.0.step)?;
        out.serialize_field("priority", &self.0.priority)?;
        out.serialize_field("consecutive_passes", &self.0.consecutive_passes)?;
        out.serialize_field("continuous", &self.0.continuous)?;
        out.serialize_field("prevent_combat_damage", &self.0.prevent_combat_damage)?;
        if !self.0.restricted_mana.is_empty() {
            out.serialize_field("restricted_mana", &self.0.restricted_mana)?;
        }
        out.serialize_field("damage_unpreventable", &self.0.damage_unpreventable)?;
        out.serialize_field("prevent_damage_to", &self.0.prevent_damage_to)?;
        out.serialize_field("upkeeps", &self.0.upkeeps)?;
        out.serialize_field("linked_exile", &self.0.linked_exile)?;
        out.serialize_field("exiled_with", &self.0.exiled_with)?;
        out.serialize_field(
            "exiled_instead_of_graveyard",
            &self.0.exiled_instead_of_graveyard,
        )?;
        out.serialize_field(
            "shuffled_instead_of_graveyard",
            &self.0.shuffled_instead_of_graveyard,
        )?;
        out.serialize_field("day", &self.0.day)?;
        out.serialize_field("attacked_this_turn", &self.0.attacked_this_turn)?;
        out.serialize_field("attacked_creatures", &self.0.attacked_creatures)?;
        out.serialize_field("extra_combats", &self.0.extra_combats)?;
        out.serialize_field("owed_combat", &self.0.owed_combat)?;
        out.serialize_field("damaged_this_turn", &self.0.damaged_this_turn)?;
        out.serialize_field("lost_life_this_turn", &self.0.lost_life_this_turn)?;
        out.serialize_field("gained_life_this_turn", &self.0.gained_life_this_turn)?;
        out.serialize_field("life_gained_amount", &self.0.life_gained_amount)?;
        out.serialize_field("damaged_by_this_turn", &self.0.damaged_by_this_turn)?;
        out.serialize_field("spells_cast_this_turn", &self.0.spells_cast_this_turn)?;
        out.serialize_field("spells_by_player", &self.0.spells_by_player)?;
        out.serialize_field(
            "spells_by_player_last_turn",
            &self.0.spells_by_player_last_turn,
        )?;
        // Omitted while empty, so snapshots of games without noncreature spells keep the
        // digests they had before this field existed.
        if self.0.noncreature_spells_by_player.is_empty() {
            out.skip_field("noncreature_spells_by_player")?;
        } else {
            out.serialize_field(
                "noncreature_spells_by_player",
                &self.0.noncreature_spells_by_player,
            )?;
        }
        out.serialize_field("draws_this_turn", &self.0.draws_this_turn)?;
        out.serialize_field("died_this_turn", &self.0.died_this_turn)?;
        out.serialize_field("monarch", &self.0.monarch)?;
        out.serialize_field("extra_turns", &self.0.extra_turns)?;
        out.serialize_field("rotation_from", &self.0.rotation_from)?;
        out.serialize_field("skipped_turns", &self.0.skipped_turns)?;
        out.serialize_field("no_life_gain", &self.0.no_life_gain)?;
        out.serialize_field("life_gain_boost", &self.0.life_gain_boost)?;
        out.serialize_field("sees_top", &self.0.sees_top)?;
        out.serialize_field("top_revealed", &self.0.top_revealed)?;
        out.serialize_field("revealed_cards", &self.0.revealed_cards)?;
        out.serialize_field("looked_at", &self.0.looked_at)?;
        out.serialize_field("exile_if_dies_now", &self.0.exile_if_dies_now)?;
        out.serialize_field("monarch_emblem", &self.0.monarch_emblem)?;
        out.serialize_field("damage_shields", &self.0.damage_shields)?;
        out.serialize_field("next_shield", &self.0.next_shield)?;
        out.serialize_field("combat", &self.0.combat)?;
        out.serialize_field("pending_triggers", &self.0.pending_triggers)?;
        out.serialize_field("last_known", &self.0.last_known)?;
        out.serialize_field("triggered_this_turn", &self.0.triggered_this_turn)?;
        out.serialize_field("delayed", &self.0.delayed)?;
        out.serialize_field("next_delayed", &self.0.next_delayed)?;
        out.serialize_field("activated_this_turn", &self.0.activated_this_turn)?;
        out.serialize_field("activated_ever", &self.0.activated_ever)?;
        out.serialize_field(
            "loyalty_activated_this_turn",
            &self.0.loyalty_activated_this_turn,
        )?;
        out.serialize_field("done_once_this_turn", &self.0.done_once_this_turn)?;
        out.serialize_field("scanned_upto", &self.0.scanned_upto)?;
        out.serialize_field("commander", &self.0.commander)?;
        out.serialize_field("pregame", &self.0.pregame)?;
        out.serialize_field("rng", &self.0.rng)?;
        // This clock also timestamps new rules effects and upkeep history.
        // Canonicalization preserves its relation to existing timestamps.
        out.serialize_field("generation", &self.0.generation)?;
        out.serialize_field("allocation_cursor", &ObjectId(self.0.next_object))?;
        out.end()
    }
}
