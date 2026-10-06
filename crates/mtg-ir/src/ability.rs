//! Abilities and card definitions.

use mtg_core::{AbilityId, CardId, CardType, Characteristics, ManaCost, Subtype, Supertype, Zone};
use serde::{Deserialize, Serialize};

use crate::{
    ObjectFilter,
    cost::Cost,
    effect::{Effect, Modification, Replacement},
    selector::{Selector, TargetSpec},
    trigger::{Condition, Trigger},
    value::Value,
};

/// Everything the engine knows about one printed card.
///
/// Loaded from a data file keyed by oracle id. The `Characteristics` here are the
/// **printed** values; the layer system derives the actual ones.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct CardDefinition {
    pub oracle: CardId,
    /// Split, adventure, modal-DFC and transforming cards have more than one
    /// face. Index 0 is the front.
    pub faces: Vec<CardFace>,
    /// How the faces relate, which determines what casting a face means.
    pub layout: Layout,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Layout {
    Normal,
    /// A layout the engine cannot yet identify. Never infer face casting rules.
    Unknown,
    Split,
    Adventure,
    /// Transforms in place; both faces are the same object.
    Transforming,
    /// Modal double-faced: choose a face as you play it.
    ModalDfc,
    Leveler,
    Saga,
    Class,
    Token,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct CardFace {
    pub name: Box<str>,
    pub mana_cost: ManaCost,
    pub card_types: Vec<CardType>,
    pub subtypes: Vec<Subtype>,
    pub supertypes: Vec<Supertype>,
    /// Printed power/toughness. `None` where a characteristic-defining ability
    /// sets them instead (layer 7a).
    pub power: Option<i32>,
    pub toughness: Option<i32>,
    pub loyalty: Option<i32>,
    pub abilities: Vec<Ability>,
    /// Kept only to render the card and to let a human diff the IR against the
    /// printed card during review. Populated at import from the local card
    /// database, never compiled in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oracle_text: Option<Box<str>>,
    /// Colours that do not come from the mana cost (CR 202.2): a token's, a colour
    /// indicator's, or none at all for devoid. `None` means "from the mana cost".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colors: Option<Vec<mtg_core::Color>>,
}

impl CardFace {
    /// Printed characteristics, before any continuous effect applies.
    pub fn printed_characteristics(&self) -> Characteristics {
        Characteristics {
            name: self.name.clone(),
            mana_cost: self.mana_cost.clone(),
            colors: match &self.colors {
                Some(cs) => cs.iter().fold(mtg_core::ColorSet::COLORLESS, |a, c| {
                    a.union(mtg_core::ColorSet::single(*c))
                }),
                None => crate::color_of(&self.mana_cost),
            },
            card_types: self.card_types.clone(),
            subtypes: self.subtypes.clone(),
            supertypes: self.supertypes.clone(),
            abilities: (0..self.abilities.len() as u16).map(AbilityId).collect(),
            granted_keywords: Vec::new(),
            // A characteristic-defining ability (CR 604.3), so it holds in every zone.
            granted_abilities: Vec::new(),
            every_creature_type: self
                .abilities
                .iter()
                .any(|a| matches!(a.kind, AbilityKind::Keyword(mtg_core::Keyword::Changeling))),
            power: self.power,
            toughness: self.toughness,
            loyalty: self.loyalty,
            defense: None,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Ability {
    /// Index within the face's ability list.
    pub id: AbilityId,
    pub kind: AbilityKind,
    /// Target slots, shared by all ability kinds that can target.
    #[serde(default)]
    pub targets: Vec<TargetSpec>,
    /// The printed clause this came from. Shown verbatim in the "why?" trace, so
    /// a player never has to trust the engine's paraphrase.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_text: Option<Box<str>>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum AbilityKind {
    /// What an instant or sorcery does on resolution (CR 112.1a — technically not
    /// an ability, but it shares the whole resolution path).
    SpellEffect(Effect),

    /// CR 602. Cost, then effect.
    Activated {
        cost: Cost,
        effect: Effect,
        /// `Battlefield` unless the ability works from another zone.
        functions_from: Zone,
        /// CR 605 — a mana ability does not use the stack and cannot be responded
        /// to, which is why it is flagged rather than inferred.
        is_mana_ability: bool,
        /// CR 606 — loyalty abilities carry their own timing restriction.
        is_loyalty_ability: bool,
        timing: ActivationTiming,
    },

    /// CR 603.
    Triggered { trigger: Trigger, effect: Effect },

    /// CR 604. A continuous effect that exists while the source does.
    Static {
        what: Selector,
        modification: Modification,
        /// "As long as …" — the effect exists only while this holds (CR 611.3b).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        condition: Option<Condition>,
    },

    /// CR 614/615. Rewrites pending events rather than resolving.
    ReplacementEffect(Replacement),

    /// CR 303.4 / 702.5 — "Enchant <quality>". An Aura spell targets what its enchant
    /// ability allows (the ability's single target slot), becomes attached to it as it
    /// resolves, and is put into the graveyard when attached to anything the slot's
    /// filter would not allow (CR 704.5m).
    Enchant,

    /// CR 601.2f — "As an additional cost to cast this spell, …": part of what casting it
    /// costs, paid with the mana cost.
    AdditionalCastCost { cost: Cost },

    /// "Cast this spell only during combat", "… only if you control a creature": a
    /// condition on casting it at all (CR 601.3), however it is cast.
    CastOnlyIf {
        condition: crate::trigger::Condition,
    },

    /// CR 702.33 — "Kicker <cost>": an optional additional cost, announced with the spell.
    ///
    /// CR 702.27 — buyback is the same optional additional cost; `buyback` means that when
    /// it was paid, the spell returns to its owner's hand as it resolves.
    Kicker {
        cost: Cost,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        buyback: bool,
        /// CR 702.42 — entwine: paying it chooses every mode of the modal spell.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        entwine: bool,
    },

    /// A card that may be cast from another zone for another cost: flashback
    /// (CR 702.34) is `zone: Graveyard, exile: true`. `exile` means the card is exiled
    /// instead of going anywhere else once it leaves the stack.
    CastFrom {
        zone: Zone,
        cost: Cost,
        exile: bool,
        /// Disturb (CR 702.146): this card is cast transformed — its back face.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        transformed: bool,
    },

    /// CR 714 — a Saga: it enters with a lore counter, gets another as its controller's
    /// precombat main phase begins, and is sacrificed once `chapters` is reached and no
    /// chapter ability of it is waiting (CR 704.5s). Its chapter abilities are compiled
    /// beside this as triggers.
    Saga {
        lore: mtg_core::CounterKind,
        chapters: u8,
    },

    /// An alternative cost to cast the card from the hand (CR 118.9), and what casting it
    /// that way means once it is a permanent.
    AlternativeCost {
        cost: Cost,
        kind: AltCost,
        /// What the spell does instead when cast this way: overload's "each" for "target".
        #[serde(default, skip_serializing_if = "Option::is_none")]
        instead: Option<Box<crate::effect::Effect>>,
    },

    /// Exile from the hand now as a special action, cast on a later turn: foretell
    /// (CR 702.143 — `cost` {2}, any time during your turn, face down; cast later for
    /// `later`) and plot (CR 702.170 — `cost` the plot cost, as a sorcery, face up; cast
    /// later without paying its mana cost, as a sorcery).
    ExileToCastLater {
        cost: Cost,
        face_down: bool,
        /// The mana cost to cast it later; `None` is "without paying its mana cost".
        later: Option<mtg_core::ManaCost>,
        /// Plot: the special action and the later cast are both at sorcery speed.
        sorcery: bool,
    },

    /// CR 702.62 — "suspend N—{cost}": as a special action from the hand, any time the
    /// card could be cast, pay `cost` and exile it with `time` time counters on it. Its
    /// upkeep and last-counter triggers are compiled beside this as ordinary abilities.
    Suspend {
        time: u32,
        cost: Cost,
        /// The time counter, as the card data numbers it.
        counter: mtg_core::CounterKind,
    },

    /// CR 702.37 — morph: the card may be cast face down as a 2/2 creature for {3}, and
    /// turned face up any time its controller has priority by paying `cost`. Megamorph
    /// (CR 702.37b) also puts a +1/+1 counter on it as it turns face up.
    Morph {
        cost: Cost,
        megamorph: bool,
        /// CR 702.168 — disguise: the face-down permanent also has ward {2}.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        disguise: bool,
    },

    /// CR 702.127: this split half may be cast only from a graveyard, for its
    /// printed cost, and is exiled whenever that spell leaves the stack.
    Aftermath,

    /// CR 702.16 — "protection from <quality>": damage from sources with the quality is
    /// prevented, and the permanent can't be enchanted, equipped, blocked or targeted by
    /// anything with it. The filter is the quality, tested against the other object.
    Protection { from: ObjectFilter },

    /// A rule about deck construction or the command zone — partner, "can be your
    /// commander", "choose a Background", "Doctor's companion" (CR 702.124, 903.3). It does
    /// nothing during a game, so a card with it plays exactly as its other text says.
    DeckRule(Box<str>),

    /// CR 702.52 — dredge N: while this card is in its owner's graveyard, if they would
    /// draw a card they may instead mill N and return this card to their hand (only with
    /// at least N cards in their library).
    Dredge(u8),
    /// A keyword that the engine implements directly because it participates in
    /// turn structure or combat rather than producing an effect.
    Keyword(Keyword),

    /// The escape hatch. Opaque to footprint analysis: a `Native` ability is
    /// treated as reading and writing everything, so it always prompts for
    /// ordering. Use only when the IR genuinely cannot express the card.
    Native { key: Box<str> },
}

/// Which alternative cost a spell was cast for.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum AltCost {
    /// CR 702.109 — dash: it gains haste, and returns to its owner's hand at the
    /// beginning of the next end step.
    Dash,
    /// CR 702.152 — blitz: it gains haste and "when this creature dies, draw a card", and
    /// is sacrificed at the beginning of the next end step.
    Blitz,
    /// CR 702.117 — surge: castable for this cost if its controller has cast another spell
    /// this turn (the condition is the cost's `timing`).
    Surge,
    /// CR 702.137 — spectacle: castable for this cost if an opponent lost life this turn.
    Spectacle,
    /// CR 702.74 — evoke: when it enters, if it was evoked, its controller sacrifices it
    /// (a trigger compiled beside this ability).
    Evoke,
    /// CR 702.96 — overload: "target" becomes "each"; the spell has no targets.
    Overload,
    /// CR 702.185 — warp: exiled at the beginning of the next end step, and castable from
    /// exile after this turn.
    Warp,
    /// CR 702.103 — bestow: cast as an Aura spell with enchant creature (the ability's
    /// target), and a creature again once it is unattached.
    Bestow,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum ActivationTiming {
    /// Any time the player has priority.
    Instant,
    /// Main phase, own turn, empty stack.
    SorcerySpeed,
    /// "Activate only once each turn."
    InstantOncePerTurn,
    /// "Activate no more than twice each turn."
    InstantTwicePerTurn,
    /// "Activate only as a sorcery and only once each turn."
    SorceryOncePerTurn,
    /// A Class's level ability (CR 716.2a): as a sorcery, and only while the Class is
    /// the level below this one.
    ClassLevel(u8),
    /// "Activate only during your turn."
    YourTurn,
    /// "Activate only during your upkeep."
    YourUpkeep,
    /// "Max speed — {cost}: …": only while its controller has max speed.
    MaxSpeed,
    /// "Activate only during your turn, before attackers are declared."
    YourTurnBeforeAttackers,
}

impl ActivationTiming {
    pub fn sorcery_only(self) -> bool {
        matches!(
            self,
            Self::SorcerySpeed | Self::SorceryOncePerTurn | Self::ClassLevel(_)
        )
    }
    /// Whether a turn-and-step restriction allows activation now.
    pub fn allows(self, yours: bool, step: mtg_core::Step) -> bool {
        use mtg_core::Step as S;
        match self {
            Self::YourTurn => yours,
            Self::YourUpkeep => yours && step == S::Upkeep,
            Self::YourTurnBeforeAttackers => {
                yours
                    && matches!(
                        step,
                        S::Untap | S::Upkeep | S::Draw | S::PrecombatMain | S::BeginCombat
                    )
            }
            _ => true,
        }
    }
    pub fn once_per_turn(self) -> bool {
        matches!(self, Self::InstantOncePerTurn | Self::SorceryOncePerTurn)
    }
    /// How many times each turn it may be activated, when that is limited.
    pub fn per_turn(self) -> Option<u32> {
        match self {
            Self::InstantOncePerTurn | Self::SorceryOncePerTurn => Some(1),
            Self::InstantTwicePerTurn => Some(2),
            _ => None,
        }
    }
}

/// Keywords with rules baked into the engine. Defined in `mtg-core` because the layer
/// system grants them (layer 6) and the computed characteristics must carry them.
pub use mtg_core::Keyword;

/// A conditional clause attached to an ability, exposed for the UI's explain view.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Conditional {
    pub cond: Condition,
    pub value: Value,
}
