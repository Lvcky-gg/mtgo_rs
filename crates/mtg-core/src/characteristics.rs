//! Characteristics (CR 109.3) — the values the layer system computes.

use crate::mana::ManaCost;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum Color {
    White = 0,
    Blue = 1,
    Black = 2,
    Red = 3,
    Green = 4,
}

/// A set of colors as a bitmask, so color identity and colorlessness are cheap.
#[derive(
    Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize,
)]
pub struct ColorSet(pub u8);

impl ColorSet {
    pub const COLORLESS: Self = Self(0);

    pub const fn single(c: Color) -> Self {
        Self(1 << (c as u8))
    }
    pub const fn contains(self, c: Color) -> bool {
        self.0 & (1 << (c as u8)) != 0
    }
    pub const fn is_colorless(self) -> bool {
        self.0 == 0
    }
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
    pub const fn intersect(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }
    pub const fn count(self) -> u32 {
        self.0.count_ones()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum CardType {
    Artifact,
    Battle,
    Creature,
    Enchantment,
    Instant,
    Kindred,
    Land,
    Planeswalker,
    Sorcery,
    Dungeon,
    Plane,
    Scheme,
    Vanguard,
    Conspiracy,
    Phenomenon,
}

impl CardType {
    /// CR 110.1 — which types can exist on the battlefield.
    pub const fn is_permanent(self) -> bool {
        matches!(
            self,
            CardType::Artifact
                | CardType::Battle
                | CardType::Creature
                | CardType::Enchantment
                | CardType::Land
                | CardType::Planeswalker
        )
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum Supertype {
    Basic,
    Legendary,
    Snow,
    World,
    Ongoing,
    Host,
}

/// Subtypes are interned strings rather than an enum: new sets introduce creature
/// types constantly, and the engine only ever compares them for equality.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub struct Subtype(pub u16);

/// The computed characteristics of an object.
///
/// Never stored on a `GameObject`. It is the **output** of the layer system
/// (CR 613) applied to a printed card plus every continuous effect in play, and
/// is cached against the state's generation counter.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Characteristics {
    pub name: Box<str>,
    pub mana_cost: ManaCost,
    pub colors: ColorSet,
    pub card_types: Vec<CardType>,
    pub subtypes: Vec<Subtype>,
    pub supertypes: Vec<Supertype>,
    /// Ability ids granted by the printed card and by layer 6 effects.
    pub abilities: Vec<crate::AbilityId>,
    /// Keywords granted by layer 6 effects ("creatures you control have flying"). Kept
    /// apart from `abilities`, whose ids index the object's own printed ability list.
    #[serde(default)]
    pub granted_keywords: Vec<Keyword>,
    /// Changeling (CR 702.73a): the object is every creature type. A flag rather than a
    /// list because the set of creature types is open-ended; subtype tests consult it
    /// through [`Characteristics::has_subtype`].
    #[serde(default)]
    pub every_creature_type: bool,
    /// Non-keyword abilities granted by layer 6 effects ("enchanted land has '{T}: Add
    /// {G}{G}'"), each named by where its text lives. Their ids are
    /// [`crate::AbilityId::granted`] of their index here.
    #[serde(default)]
    pub granted_abilities: Vec<GrantedAbility>,
    pub power: Option<i32>,
    pub toughness: Option<i32>,
    pub loyalty: Option<i32>,
    pub defense: Option<i32>,
}

impl Characteristics {
    pub fn has_type(&self, t: CardType) -> bool {
        self.card_types.contains(&t)
    }
    pub fn is_permanent(&self) -> bool {
        self.card_types.iter().any(|t| t.is_permanent())
    }
    /// CR 708.2a — a face-down spell or permanent: a 2/2 creature with no name, no
    /// mana cost, no colour, no subtypes and no abilities.
    pub fn face_down() -> Self {
        Self {
            name: "".into(),
            mana_cost: ManaCost::default(),
            colors: ColorSet::COLORLESS,
            card_types: vec![CardType::Creature],
            subtypes: Vec::new(),
            supertypes: Vec::new(),
            abilities: Vec::new(),
            granted_keywords: Vec::new(),
            every_creature_type: false,
            granted_abilities: Vec::new(),
            power: Some(2),
            toughness: Some(2),
            loyalty: None,
            defense: None,
        }
    }
    /// Whether the object has a subtype. `name` resolves an interned subtype to its
    /// printed name, which is only needed to tell whether a changeling has it.
    pub fn has_subtype<'a>(
        &self,
        s: Subtype,
        name: impl FnOnce(Subtype) -> Option<&'a str>,
    ) -> bool {
        self.subtypes.contains(&s)
            || (self.every_creature_type && name(s).is_some_and(crate::is_creature_type))
    }
}

/// Where a granted ability's text lives: in the static ability `static_ability` of the
/// object `effect`, or — when that is `None` — in the continuous effect with id `effect`
/// created by a resolved spell or ability.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct GrantedAbility {
    pub effect: crate::ObjectId,
    pub static_ability: Option<crate::AbilityId>,
}

/// Keywords with rules baked into the engine.
///
/// These are *not* sugar for IR trees. Each one changes a rule the engine already
/// owns — combat, damage ordering, the untap step — so implementing them as
/// effects would mean the engine consulting card data in the middle of combat.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum Keyword {
    /// Grants the city's blessing once its controller has ten permanents.
    Ascend,
    Flying,
    Reach,
    FirstStrike,
    DoubleStrike,
    Deathtouch,
    Lifelink,
    Trample,
    Vigilance,
    Haste,
    Defender,
    Menace,
    Hexproof,
    Shroud,
    Indestructible,
    Flash,
    Protection,
    Ward,
    /// CR 702.28 — blocked only by, and blocks only, creatures with shadow.
    Shadow,
    /// CR 702.36 — blocked only by artifact and/or black creatures.
    Fear,
    /// CR 702.13 — blocked only by artifact creatures and creatures sharing a colour.
    Intimidate,
    /// CR 702.31 — blocked only by creatures with horsemanship.
    Horsemanship,
    /// CR 702.118 — can't be blocked by creatures with greater power.
    Skulk,
    /// CR 702.14 — landwalk: unblockable while the defending player controls such a land.
    Plainswalk,
    Islandwalk,
    Swampwalk,
    Mountainwalk,
    Forestwalk,
    /// CR 702.80 — damage to creatures is dealt as -1/-1 counters.
    Wither,
    /// CR 702.90 — damage to creatures as -1/-1 counters, to players as poison.
    Infect,
    /// CR 702.73 — every creature type, in every zone.
    Changeling,
    /// CR 702.164 — combat damage to a player also gives that player this many poison
    /// counters.
    Toxic(u8),
    /// CR 702.51 — creatures you tap help pay: generic, or one of their colours.
    Convoke,
    /// CR 702.126 — artifacts you tap each pay {1}.
    Improvise,
    /// CR 702.66 — cards exiled from your graveyard each pay {1}.
    Delve,
    /// CR 702.145 — daybound and nightbound: a transforming permanent whose face follows
    /// day and night (CR 726).
    Daybound,
    Nightbound,
    /// CR 702.25 — a creature without flanking that blocks it gets -1/-1.
    Flanking,
    /// CR 702.61 — while this spell is on the stack, players can't cast spells or
    /// activate abilities that aren't mana abilities.
    SplitSecond,
    /// CR 702.88 — cast from hand, it is exiled as it resolves, and may be cast again
    /// for free at the beginning of your next upkeep.
    Rebound,
    /// CR 702.116 — whenever it attacks, for each opponent other than the defending player,
    /// a token copy attacks that player. Matches are two-player: there is no such opponent,
    /// so it does nothing there.
    Myriad,
    /// CR 702.179 — start your engines!: its controller's speed starts at 1.
    StartYourEngines,
    /// CR 702.89 — umbra armor (an Aura's): if the enchanted creature would be destroyed,
    /// instead remove all damage from it and destroy the Aura.
    UmbraArmor,
}
