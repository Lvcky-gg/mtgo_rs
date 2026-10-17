//! A position as numbers: the input a learned evaluation or policy reads.
//!
//! [`Encoder::encode`] turns a game, seen from one player's seat, into a fixed-length vector
//! of [`GLOBAL`] features and a set of [`Token`]s, one per object whose identity the player can
//! see. A network reads the global vector directly and pools the tokens by zone; nothing about
//! a token depends on its position in the list.
//!
//! Visibility follows [`mtg_engine::view::project`] exactly: the opponent's hand and every
//! library are counts only, and face-down objects carry only what their flags say. Encoding
//! never sees more than the player could.
//!
//! A card's features come from its current characteristics and its compiled abilities, not
//! from a table of known cards, so a card never seen in training still lands near cards that
//! do similar things. Two parts carry that:
//!
//! - **Characteristics** — mana value, colours, types, power and toughness, keywords, ability
//!   counts, state flags and counters.
//! - **A bag of IR words** — every enum variant and struct name in the card's compiled
//!   abilities (`DealDamage`, `Destroy`, `CreateToken`, `Counter`…), feature-hashed into
//!   [`IR_BUCKETS`] counts. This is how "what the card does" reaches the network.
//!
//! Each token also carries a stable hash of the card's name ([`Token::name`]) so a network can
//! learn an embedding for the cards it sees often. [`SCHEMA_VERSION`] changes whenever the
//! layout does, so recorded data from an older layout is never read as the new one.

use std::collections::HashMap;

use mtg_core::{CardId, CardType, Color, CounterKind, Keyword, ObjectId, PlayerId, Step, Zone};
use mtg_engine::{GameState, layers::PrintedCards, view::ObjectView};
use mtg_ir::AbilityKind;

/// Bumped whenever [`GLOBAL`], [`card_features`] or the zone codes change.
pub const SCHEMA_VERSION: u32 = 1;

/// Feature-hash buckets for the bag of IR words.
pub const IR_BUCKETS: usize = 64;

/// Per-player features, written once for the viewer and once for their strongest opponent.
const PER_PLAYER: [&str; 11] = [
    "life",
    "poison",
    "hand",
    "library",
    "graveyard",
    "lands",
    "untapped lands",
    "creatures",
    "power",
    "mana floating",
    "energy",
];

/// Features of the position as a whole, in order.
pub const GLOBAL: [&str; 2 * PER_PLAYER.len() + 6 + STEPS.len()] = {
    let mut out = [""; 2 * PER_PLAYER.len() + 6 + STEPS.len()];
    let mut i = 0;
    while i < PER_PLAYER.len() {
        out[i] = PER_PLAYER[i];
        out[PER_PLAYER.len() + i] = PER_PLAYER[i];
        i += 1;
    }
    let tail = [
        "turn",
        "my turn",
        "i have priority",
        "stack depth",
        "attackers",
        "opponents",
    ];
    let mut k = 0;
    while k < tail.len() {
        out[2 * PER_PLAYER.len() + k] = tail[k];
        k += 1;
    }
    let mut s = 0;
    while s < STEPS.len() {
        out[2 * PER_PLAYER.len() + tail.len() + s] = STEPS[s].1;
        s += 1;
    }
    out
};
pub const NG: usize = GLOBAL.len();

const STEPS: [(Step, &str); 13] = [
    (Step::Untap, "step untap"),
    (Step::Upkeep, "step upkeep"),
    (Step::Draw, "step draw"),
    (Step::PrecombatMain, "step main 1"),
    (Step::BeginCombat, "step begin combat"),
    (Step::DeclareAttackers, "step attackers"),
    (Step::DeclareBlockers, "step blockers"),
    (Step::FirstStrikeCombatDamage, "step first strike damage"),
    (Step::CombatDamage, "step combat damage"),
    (Step::EndCombat, "step end combat"),
    (Step::PostcombatMain, "step main 2"),
    (Step::End, "step end"),
    (Step::Cleanup, "step cleanup"),
];

const TYPES: [CardType; 8] = [
    CardType::Land,
    CardType::Creature,
    CardType::Artifact,
    CardType::Enchantment,
    CardType::Planeswalker,
    CardType::Instant,
    CardType::Sorcery,
    CardType::Battle,
];

const KEYWORDS: [Keyword; 16] = [
    Keyword::Flying,
    Keyword::Reach,
    Keyword::FirstStrike,
    Keyword::DoubleStrike,
    Keyword::Deathtouch,
    Keyword::Lifelink,
    Keyword::Trample,
    Keyword::Vigilance,
    Keyword::Haste,
    Keyword::Defender,
    Keyword::Menace,
    Keyword::Hexproof,
    Keyword::Shroud,
    Keyword::Indestructible,
    Keyword::Flash,
    Keyword::Ward,
];

/// The characteristic part of a card's features, before the IR bag.
const CHARACTERISTICS: usize = 1 + 5 + TYPES.len() + 2 + 4 + 6 + 2 + KEYWORDS.len() + 5 + 1;
/// Length of [`Token::features`].
pub const NC: usize = CHARACTERISTICS + IR_BUCKETS;

/// Which zone, and whose, a token is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Slot {
    MyBattlefield = 0,
    TheirBattlefield = 1,
    MyHand = 2,
    MyGraveyard = 3,
    TheirGraveyard = 4,
    MyStack = 5,
    TheirStack = 6,
    MyExile = 7,
    TheirExile = 8,
    MyCommand = 9,
    TheirCommand = 10,
}
pub const SLOTS: usize = 11;

impl Slot {
    /// The slot a recorded code names.
    pub fn from_index(i: u8) -> Option<Self> {
        use Slot::*;
        [
            MyBattlefield,
            TheirBattlefield,
            MyHand,
            MyGraveyard,
            TheirGraveyard,
            MyStack,
            TheirStack,
            MyExile,
            TheirExile,
            MyCommand,
            TheirCommand,
        ]
        .get(usize::from(i))
        .copied()
    }
}

/// One visible object.
#[derive(Clone, Debug)]
pub struct Token {
    pub slot: Slot,
    /// FNV-1a of the card's lower-cased name: stable across games, decks and databases.
    pub name: u32,
    pub features: [f32; NC],
}

/// A position, from one seat.
#[derive(Clone, Debug)]
pub struct Encoded {
    pub global: [f32; NG],
    pub tokens: Vec<Token>,
}

/// Names of the token features, in order: for the schema file training reads.
pub fn card_features() -> Vec<String> {
    let mut out: Vec<String> = ["mana value"].map(String::from).to_vec();
    out.extend(["white", "blue", "black", "red", "green"].map(String::from));
    out.extend(TYPES.iter().map(|t| format!("{t:?}").to_lowercase()));
    out.extend(["legendary", "token"].map(String::from));
    out.extend(["power", "toughness", "damage", "loyalty"].map(String::from));
    out.extend(
        [
            "tapped",
            "summoning sick",
            "attacking",
            "blocking",
            "attached",
            "targets",
        ]
        .map(String::from),
    );
    out.extend(["+1/+1 counters", "-1/-1 counters"].map(String::from));
    out.extend(KEYWORDS.iter().map(|k| format!("{k:?}").to_lowercase()));
    out.extend(
        [
            "activated",
            "mana abilities",
            "triggered",
            "static",
            "replacement",
        ]
        .map(String::from),
    );
    out.push("recurring engine".into());
    out.extend((0..IR_BUCKETS).map(|b| format!("ir {b}")));
    debug_assert_eq!(out.len(), NC);
    out
}

/// Encodes positions, remembering each card face's IR bag between positions.
#[derive(Default)]
pub struct Encoder {
    /// By face name: card ids are only meaningful within one game's card table.
    faces: HashMap<Box<str>, Face>,
}

#[derive(Clone)]
struct Face {
    name: u32,
    bag: [f32; IR_BUCKETS],
}

impl Encoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// `state` as `me` may see it.
    pub fn encode(&mut self, state: &GameState, cards: &dyn PrintedCards, me: PlayerId) -> Encoded {
        let view = mtg_engine::view::project(state, me);
        let opponent = strongest_opponent(state, me);
        let mut global = [0f32; NG];
        let mut tokens = Vec::new();

        let (mine, theirs) = global.split_at_mut(PER_PLAYER.len());
        for (who, out) in [(Some(me), mine), (opponent, theirs)] {
            let Some(who) = who else { continue };
            let Some(p) = view.players.get(&who) else {
                continue;
            };
            out[0] = p.life as f32 / 20.0;
            out[1] = p.poison as f32 / 10.0;
            out[2] = p.hand_size as f32 / 7.0;
            out[3] = p.library_size as f32 / 40.0;
            out[4] = p.graveyard.len() as f32 / 20.0;
            out[9] = p.mana.iter().map(|m| f32::from(*m)).sum::<f32>() / 5.0;
            out[10] = p.energy as f32 / 5.0;
        }
        let tail = 2 * PER_PLAYER.len();
        global[tail] = state.turn as f32 / 20.0;
        global[tail + 1] = f32::from(u8::from(state.active_player == me));
        global[tail + 2] = f32::from(u8::from(state.priority == Some(me)));
        global[tail + 3] = view.stack.len() as f32 / 3.0;
        global[tail + 4] = state.combat.attackers.len() as f32 / 5.0;
        global[tail + 5] = state.players.values().filter(|p| !p.has_lost).count() as f32 - 1.0;
        if let Some(s) = STEPS.iter().position(|(s, _)| *s == state.step) {
            global[tail + 6 + s] = 1.0;
        }

        for (id, o) in &view.visible {
            let mine = o.controller == me;
            let slot = match (o.zone.zone, mine) {
                (Zone::Battlefield, true) => Slot::MyBattlefield,
                (Zone::Battlefield, false) => Slot::TheirBattlefield,
                (Zone::Hand, true) => Slot::MyHand,
                (Zone::Graveyard, true) => Slot::MyGraveyard,
                (Zone::Graveyard, false) => Slot::TheirGraveyard,
                (Zone::Stack, true) => Slot::MyStack,
                (Zone::Stack, false) => Slot::TheirStack,
                (Zone::Exile, true) => Slot::MyExile,
                (Zone::Exile, false) => Slot::TheirExile,
                (Zone::Command, true) => Slot::MyCommand,
                (Zone::Command, false) => Slot::TheirCommand,
                _ => continue,
            };
            // Abilities on the stack are their source's card; spells and permanents are
            // themselves. Unknown identities stay out.
            let Some(card) = o.card else { continue };
            let Some(token) = self.token(state, cards, *id, card, o, slot) else {
                continue;
            };
            if slot == Slot::MyBattlefield || slot == Slot::TheirBattlefield {
                let side = if mine { 0 } else { PER_PLAYER.len() };
                let f = &token.features;
                if f[6] > 0.0 {
                    global[side + 5] += 1.0 / 7.0;
                    if f[CHARACTERISTICS_TAPPED] == 0.0 {
                        global[side + 6] += 1.0 / 7.0;
                    }
                } else if f[7] > 0.0 {
                    global[side + 7] += 1.0 / 5.0;
                    global[side + 8] += f[CHARACTERISTICS_POWER] / 4.0;
                }
            }
            tokens.push(token);
        }
        Encoded { global, tokens }
    }

    fn token(
        &mut self,
        state: &GameState,
        cards: &dyn PrintedCards,
        id: ObjectId,
        card: CardId,
        o: &ObjectView,
        slot: Slot,
    ) -> Option<Token> {
        let object = state.objects.get(&id)?;
        let face_index = if o.is_ability { 0 } else { o.face };
        let face = cards.face(card, face_index);
        let key = face.map_or("", |f| &f.name);
        let entry = match self.faces.get(key) {
            Some(e) => e.clone(),
            None => {
                let e = describe(face);
                self.faces.insert(key.into(), e.clone());
                e
            }
        };
        let mut f = [0f32; NC];
        let mut i = 0;
        let mut put = |f: &mut [f32; NC], v: f32| {
            f[i] = v;
            i += 1;
        };
        // An ability on the stack is described by its source card, but has no body.
        let ch = if o.is_ability {
            face.map(|face| face.printed_characteristics())
        } else {
            mtg_engine::layers::compute(state, cards, id)
        }?;
        put(&mut f, ch.mana_cost.mana_value() as f32 / 5.0);
        for c in [
            Color::White,
            Color::Blue,
            Color::Black,
            Color::Red,
            Color::Green,
        ] {
            put(&mut f, f32::from(u8::from(ch.colors.contains(c))));
        }
        for t in TYPES {
            put(&mut f, f32::from(u8::from(ch.has_type(t))));
        }
        put(
            &mut f,
            f32::from(u8::from(
                ch.supertypes.contains(&mtg_core::Supertype::Legendary),
            )),
        );
        put(&mut f, f32::from(u8::from(object.is_token)));
        put(&mut f, ch.power.unwrap_or(0) as f32 / 5.0);
        put(&mut f, ch.toughness.unwrap_or(0) as f32 / 5.0);
        put(&mut f, o.damage as f32 / 5.0);
        let counter = |k| o.counters.get(&k).copied().unwrap_or(0) as f32;
        put(&mut f, counter(CounterKind::Loyalty) / 5.0);
        put(&mut f, f32::from(u8::from(o.tapped)));
        put(&mut f, f32::from(u8::from(object.summoning_sick)));
        put(&mut f, f32::from(u8::from(o.attacking)));
        put(&mut f, f32::from(u8::from(o.blocking.is_some())));
        put(&mut f, f32::from(u8::from(o.attached_to.is_some())));
        put(&mut f, o.targets.len() as f32);
        put(&mut f, counter(CounterKind::PlusOnePlusOne) / 3.0);
        put(&mut f, counter(CounterKind::MinusOneMinusOne) / 3.0);
        let own: Vec<_> = face
            .map(|face| {
                face.abilities
                    .iter()
                    .filter(|a| ch.abilities.contains(&a.id))
                    .collect()
            })
            .unwrap_or_default();
        for k in KEYWORDS {
            let has = ch.granted_keywords.contains(&k)
                || own
                    .iter()
                    .any(|a| matches!(a.kind, AbilityKind::Keyword(x) if x == k));
            put(&mut f, f32::from(u8::from(has)));
        }
        let count = |pred: &dyn Fn(&AbilityKind) -> bool| {
            (own.iter().filter(|a| pred(&a.kind)).count() as f32).ln_1p()
        };
        put(
            &mut f,
            count(&|k| {
                matches!(
                    k,
                    AbilityKind::Activated {
                        is_mana_ability: false,
                        ..
                    }
                )
            }),
        );
        put(
            &mut f,
            count(&|k| {
                matches!(
                    k,
                    AbilityKind::Activated {
                        is_mana_ability: true,
                        ..
                    }
                )
            }),
        );
        put(
            &mut f,
            count(&|k| matches!(k, AbilityKind::Triggered { .. })),
        );
        put(&mut f, count(&|k| matches!(k, AbilityKind::Static { .. })));
        put(
            &mut f,
            count(&|k| matches!(k, AbilityKind::ReplacementEffect(_))),
        );
        put(
            &mut f,
            own.iter()
                .map(|a| crate::bot::recurring_ability_value(a))
                .sum::<i64>() as f32
                / 10.0,
        );
        debug_assert_eq!(i, CHARACTERISTICS);
        f[CHARACTERISTICS..].copy_from_slice(&entry.bag);
        Some(Token {
            slot,
            name: entry.name,
            features: f,
        })
    }
}

// Offsets read back while summing the battlefield into the global features.
const CHARACTERISTICS_POWER: usize = 1 + 5 + TYPES.len() + 2;
const CHARACTERISTICS_TAPPED: usize = CHARACTERISTICS_POWER + 4;

/// A face's name hash and IR bag.
fn describe(face: Option<&mtg_ir::CardFace>) -> Face {
    let mut bag = [0f32; IR_BUCKETS];
    let Some(face) = face else {
        return Face { name: 0, bag };
    };
    // Every capitalised identifier in the compiled abilities: enum variants and type names.
    // Field names are lower case and fall away; so do the numbers.
    let text = format!("{:?}", face.abilities);
    for word in text.split(|c: char| !c.is_ascii_alphanumeric() && c != '_') {
        if word.starts_with(|c: char| c.is_ascii_uppercase()) {
            bag[fnv(word) as usize % IR_BUCKETS] += 1.0;
        }
    }
    for v in &mut bag {
        *v = v.ln_1p();
    }
    Face {
        name: fnv(&face.name.to_lowercase()),
        bag,
    }
}

/// FNV-1a, 32 bits: stable across runs and platforms, unlike the standard hasher.
pub fn fnv(s: &str) -> u32 {
    s.bytes().fold(0x811C_9DC5, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
    })
}

/// The living opponent with the most life — the one the global features describe.
fn strongest_opponent(state: &GameState, me: PlayerId) -> Option<PlayerId> {
    state
        .players
        .values()
        .filter(|p| p.id != me && !p.has_lost)
        .max_by_key(|p| p.life)
        .map(|p| p.id)
}

/// The names of the cards in `who`'s hand: a training label for hand inference, never an
/// input. Only recorders may call this; it reads hidden information.
pub fn hidden_hand(state: &GameState, cards: &dyn PrintedCards, who: PlayerId) -> Vec<u32> {
    state
        .objects_in(mtg_core::ZoneRef {
            zone: Zone::Hand,
            player: Some(who),
        })
        .iter()
        .filter_map(|id| {
            let o = state.objects.get(id)?;
            Some(fnv(&cards.face(o.card, 0)?.name.to_lowercase()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtg_core::ZoneRef;

    struct Bears(Vec<mtg_ir::CardFace>);
    impl PrintedCards for Bears {
        fn face(&self, card: CardId, _face: u8) -> Option<&mtg_ir::CardFace> {
            self.0.get(card.0 as usize)
        }
        fn subtype_name(&self, _: mtg_core::Subtype) -> Option<&str> {
            None
        }
    }

    fn bear(name: &str) -> mtg_ir::CardFace {
        mtg_ir::CardFace {
            name: name.into(),
            mana_cost: Default::default(),
            card_types: vec![CardType::Creature],
            subtypes: Vec::new(),
            supertypes: Vec::new(),
            power: Some(2),
            toughness: Some(2),
            loyalty: None,
            abilities: Vec::new(),
            oracle_text: None,
            colors: None,
        }
    }

    #[test]
    fn the_opponents_hand_is_a_count_and_never_a_token() {
        let cards = Bears((0..4).map(|i| bear(&format!("Bear {i}"))).collect());
        let (me, them) = (PlayerId(0), PlayerId(1));
        let mut state = GameState::new(&[me, them], 20);
        let zone = |zone, p| ZoneRef {
            zone,
            player: Some(p),
        };
        state.place(CardId(0), me, zone(Zone::Hand, me));
        state.place(CardId(1), them, zone(Zone::Hand, them));
        state.place(CardId(2), them, zone(Zone::Hand, them));
        let field = state.place(
            CardId(3),
            them,
            ZoneRef {
                zone: Zone::Battlefield,
                player: None,
            },
        );
        state.objects.get_mut(&field).unwrap().controller = them;

        let e = Encoder::new().encode(&state, &cards, me);
        let slots: Vec<(Slot, u32)> = e.tokens.iter().map(|t| (t.slot, t.name)).collect();
        assert_eq!(
            slots,
            vec![
                (Slot::MyHand, fnv("bear 0")),
                (Slot::TheirBattlefield, fnv("bear 3")),
            ]
        );
        assert_eq!(e.global[2], 1.0 / 7.0, "my hand size");
        assert_eq!(e.global[PER_PLAYER.len() + 2], 2.0 / 7.0, "their hand size");
        assert_eq!(e.global[PER_PLAYER.len() + 7], 1.0 / 5.0, "their creatures");
        let bear = &e.tokens[1].features;
        assert_eq!(bear[7], 1.0, "a creature");
        assert_eq!(bear[CHARACTERISTICS_POWER], 2.0 / 5.0);
        assert_eq!(card_features().len(), NC);
        assert_eq!(GLOBAL.len(), NG);
    }
}
