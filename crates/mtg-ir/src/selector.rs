//! Selectors: how an ability names the things it acts on.
//!
//! A selector is a *description*, never a resolved list. The engine resolves one
//! against state at the moment the rules say to (targets on announcement,
//! CR 601.2c; everything else on resolution, CR 608.2). Keeping the description
//! around after resolution is what lets the engine re-check target legality and
//! explain "this fizzled because the target became illegal".

use mtg_core::{CardType, Color, CounterKind, Subtype, Supertype, Zone};
use serde::{Deserialize, Serialize};

use crate::value::Value;

/// Which set of objects or players an effect acts on.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Selector {
    /// The ability's source (CR 113.7) — "this creature".
    SelfSource,
    /// The controller of the ability.
    You,
    /// Each opponent of the controller.
    Opponents,
    EachPlayer,
    /// The player whose turn it is.
    ActivePlayer,
    /// The defending player of this combat (CR 506.2).
    DefendingPlayer,
    /// The player this Aura is attached to (a Curse: "enchant player").
    EnchantedPlayer,
    /// A target chosen on announcement. `index` selects which of the ability's
    /// targets, so "destroy target creature, then target player draws" is
    /// unambiguous.
    Target {
        index: u8,
    },
    /// A binding introduced earlier in the same effect tree — the "it" in
    /// "exile it", or the subject of the event that triggered the ability.
    Bound(Binding),
    /// Every object matching a filter, in a zone.
    All {
        zone: Zone,
        filter: ObjectFilter,
    },
    /// A subset the given player chooses, used for effects that say "choose".
    ChosenBy {
        chooser: Box<Selector>,
        zone: Zone,
        filter: ObjectFilter,
        count: Value,
        /// Whether choosing fewer than `count` is allowed ("up to").
        up_to: bool,
    },
    /// The top N of a library, for scry/surveil/cascade shapes.
    TopOfLibrary {
        player: Box<Selector>,
        count: Value,
    },
    Union(Vec<Selector>),
    Except(Box<Selector>, Box<Selector>),
    /// The controllers of the objects a selector picks out: "its controller". An object
    /// that has left the battlefield answers from last-known information.
    ControllerOf(Box<Selector>),
    /// The owners of the objects a selector picks out: "its owner".
    OwnerOf(Box<Selector>),
}

/// Named slots the engine fills in while walking an effect tree.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum Binding {
    /// The object the triggering event happened to.
    EventSubject,
    /// The other party to the triggering event (the damage source, the drawer).
    EventOther,
    /// The most recent set produced by an earlier step in this effect.
    It,
    /// A named slot, for effects with more than one intermediate.
    Named(u16),
}

impl Binding {
    /// Where a ninjutsu ability keeps what the returned creature was attacking.
    pub const NINJUTSU_DEFENDER: Self = Binding::Named(u16::MAX - 4);
}

/// A predicate over objects. Composable so that printed text maps structurally.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum ObjectFilter {
    Any,
    /// The object whose ability this is: "this creature" in "when this creature enters".
    ///
    /// Without it a self-trigger could only be written as "whenever *a* creature enters",
    /// which fires for every creature — the most common trigger shape there is, unsayable.
    IsSelf,
    HasType(CardType),
    HasSubtype(Subtype),
    HasSupertype(Supertype),
    HasColor(Color),
    Colorless,
    Tapped(bool),
    AttackingOrBlocking,
    Attacking,
    Blocking,
    /// The object this ability's source is attached to: "enchanted creature",
    /// "equipped creature", "fortified land".
    AttachedToSelf,
    /// Dealt damage this turn by this ability's source: "a creature dealt damage by this
    /// creature this turn".
    DealtDamageBySelfThisTurn,
    /// Has a keyword, printed or granted: "creature with flying".
    HasKeyword(crate::ability::Keyword),
    ToughnessAtMost(Value),
    ManaValueAtLeast(Value),
    /// Controlled by whoever the inner selector picks out.
    ControlledBy(Box<Selector>),
    OwnedBy(Box<Selector>),
    PowerAtMost(Value),
    PowerAtLeast(Value),
    /// Shares at least one color with one of these objects (conspire: "that share a color
    /// with it").
    SharesColorWith(Box<Selector>),
    ManaValueAtMost(Value),
    HasCounter(CounterKind),
    /// Of the creature type / color the source chose as it entered ("creatures you control
    /// of the chosen type").
    HasChosenSubtype,
    HasChosenColor,
    /// Entered the battlefield this turn — "summoning sick" and friends.
    EnteredThisTurn,
    /// A token (CR 111) — "nontoken creature" is its negation.
    Token,
    /// Attached to this ability's source: "Equipment attached to it", "~ is enchanted".
    AttachedToSource,
    /// Blocking this ability's source: "for each creature blocking it" (rampage).
    BlockingSource,
    /// Has the same name as this ability's source: "a card named ~".
    NamedLikeSource,
    /// Two or more colors (CR 105.2).
    Multicolored,
    /// Could be legally targeted by the ability being resolved; folds in
    /// protection, shroud and hexproof (CR 115) rather than restating them.
    Targetable,
    /// A **spell** — a card on the stack (CR 112.1).
    ///
    /// Distinct from "an object on the stack": an activated or triggered ability is on
    /// the stack too and is not a spell, so "target spell" must not offer one.
    IsSpell,
    /// An ability on the stack, the complement of [`ObjectFilter::IsSpell`].
    IsAbility,
    Not(Box<ObjectFilter>),
    And(Vec<ObjectFilter>),
    Or(Vec<ObjectFilter>),
}

/// Declares one target slot of an ability, checked on announcement and re-checked
/// on resolution (CR 608.2b).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct TargetSpec {
    pub zone: Zone,
    pub filter: ObjectFilter,
    /// `true` for "target player"/"target opponent" slots.
    pub allows_players: bool,
    /// Which players are legal when `allows_players` is set: `None` for any player,
    /// or a selector such as [`Selector::Opponents`] for "target opponent".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub players: Option<Selector>,
    /// For a modal spell or ability: the mode this slot belongs to. The slot is only
    /// filled when that mode is chosen (CR 700.2b).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<u8>,
    pub count: Value,
    pub up_to: bool,
    /// CR 601.2c — two target slots normally may not pick the same object.
    pub distinct_from_other_targets: bool,
}
