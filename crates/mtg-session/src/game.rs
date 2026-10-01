//! What a match is made of: its format, the decks, the cards they use, and how a game starts.
//!
//! # A card table per match
//!
//! Two installations number their cards differently — each interns Scryfall's oracle ids in its
//! own database — so no local id can cross the wire. A deck names its cards by [`CardKey`]
//! instead, and a match builds its own table, [`GameCards`], numbering exactly the cards its two
//! decks use. The engine plays from that table, and the host sends the whole table to the guest,
//! which is then all the guest needs to name and draw every card it will see. Demo cards and
//! imported ones can sit in the same table, since both are just keys.

use std::collections::BTreeMap;

use mtg_core::{CardId, CardType, PlayerId, Subtype, Supertype, Zone, ZoneRef};
use mtg_engine::state::GameState;
use mtg_ir::{CardFace, PrintedCards};
use mtg_net::fairness::{Seed, shuffle_order};
use serde::{Deserialize, Serialize};

/// The two seats. The host, or the local player, is always the first.
pub const SEATS: [PlayerId; 2] = [PlayerId(0), PlayerId(1)];

/// How the decks are built and how the game starts.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Format {
    /// 60 or more cards, at most four of each but basic lands, 20 life.
    Constructed,
    /// A commander and 99 others, one of each but basic lands, all within the commander's
    /// colour identity; 40 life (CR 903).
    Commander,
}

impl Format {
    pub fn starting_life(self) -> i32 {
        match self {
            Format::Constructed => 20,
            Format::Commander => 40,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Format::Constructed => "Constructed",
            Format::Commander => "Commander",
        }
    }

    /// Sideboarding between games is a Constructed practice; Commander has none.
    pub fn has_sideboard(self) -> bool {
        self == Format::Constructed
    }
}

/// What the host chose for the match.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct MatchSettings {
    pub format: Format,
    /// 1 or 3.
    pub best_of: u8,
}

impl MatchSettings {
    /// Games needed to win the match.
    pub fn wins_needed(self) -> u8 {
        self.best_of / 2 + 1
    }
}

/// A card as a deck names it, the same on every installation.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
pub enum CardKey {
    /// Scryfall's oracle id, and optionally a specific printing id (scryfall_id).
    /// If printing_id is None, use the oracle default (first printing).
    Oracle(String, Option<String>),
    /// One of the built-in invented demo cards, by index.
    Demo(u32),
}

/// A deck, independent of any one database.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct DeckSpec {
    pub name: String,
    pub main: Vec<(CardKey, u8)>,
    pub side: Vec<(CardKey, u8)>,
    /// Commander only.
    pub commander: Option<CardKey>,
}

impl DeckSpec {
    /// Every distinct card the deck uses, anywhere in it.
    pub fn keys(&self) -> impl Iterator<Item = &CardKey> {
        self.main
            .iter()
            .chain(&self.side)
            .map(|(k, _)| k)
            .chain(&self.commander)
    }

    pub fn main_count(&self) -> u32 {
        self.main.iter().map(|(_, n)| u32::from(*n)).sum()
    }

    /// Whether `other` has exactly the same cards, only moved between main deck and
    /// sideboard — the one change sideboarding allows.
    pub fn same_cards_as(&self, other: &DeckSpec) -> bool {
        fn all(d: &DeckSpec) -> BTreeMap<&CardKey, u32> {
            let mut m = BTreeMap::new();
            for (k, n) in d.main.iter().chain(&d.side) {
                *m.entry(k).or_insert(0) += u32::from(*n);
            }
            m
        }
        all(self) == all(other) && self.commander == other.commander
    }
}

/// A card as a source supplies it: its faces, and its colour identity as letters.
pub struct SourcedCard {
    pub layout: mtg_ir::Layout,
    pub faces: Vec<CardFace>,
    pub color_identity: String,
}

/// Where a match's cards come from: the local database, the demo set, or both.
pub trait CardSource {
    fn card(&self, key: &CardKey) -> Option<SourcedCard>;
    /// Names for the subtype ids in this source's faces.
    fn subtype_name(&self, subtype: Subtype) -> Option<String>;
}

/// The cards in one match, numbered for this match only. See the module docs.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GameCards {
    keys: Vec<CardKey>,
    faces: Vec<Vec<CardFace>>,
    #[serde(default)]
    layouts: Vec<mtg_ir::Layout>,
    identities: Vec<String>,
    subtypes: Vec<String>,
    /// Faces for the tokens these cards can create, numbered from
    /// [`mtg_ir::walk::TOKEN_CARD_BASE`].
    #[serde(default)]
    tokens: Vec<CardFace>,
}

impl GameCards {
    /// A table holding every card in `decks`, or the keys `source` could not supply.
    pub fn build<'a>(
        decks: impl IntoIterator<Item = &'a DeckSpec>,
        source: &dyn CardSource,
    ) -> Result<Self, Vec<CardKey>> {
        let mut table = Self::default();
        let mut missing = Vec::new();
        for deck in decks {
            for key in deck.keys() {
                if table.id(key).is_some() || missing.contains(key) {
                    continue;
                }
                match source.card(key) {
                    Some(card) => table.add(key.clone(), card, source),
                    None => missing.push(key.clone()),
                }
            }
        }
        if missing.is_empty() {
            table.register_tokens();
            Ok(table)
        } else {
            Err(missing)
        }
    }

    fn add(&mut self, key: CardKey, card: SourcedCard, source: &dyn CardSource) {
        // Subtype ids belong to the source; re-intern them into this table's own list —
        // the face's own subtypes and every subtype its abilities mention ("other Elves
        // you control"), which would otherwise compare against the wrong numbering.
        let faces = card
            .faces
            .into_iter()
            .map(|mut f| {
                f.subtypes = f
                    .subtypes
                    .iter()
                    .filter_map(|s| source.subtype_name(*s))
                    .map(|name| Subtype(self.intern_subtype(name)))
                    .collect();
                for a in &mut f.abilities {
                    mtg_ir::walk::map_subtypes(a, &mut |s| match source.subtype_name(s) {
                        Some(name) => Subtype(self.intern_subtype(name)),
                        // Unknown to the source: keep a number that matches nothing here.
                        None => Subtype(u16::MAX),
                    });
                }
                f
            })
            .collect();
        self.layouts.push(card.layout);
        self.keys.push(key);
        self.faces.push(faces);
        self.identities.push(card.color_identity);
    }

    /// Give the tokens this table's cards create faces of their own.
    fn register_tokens(&mut self) {
        let first = mtg_ir::walk::TOKEN_CARD_BASE + self.tokens.len() as u32;
        let made = mtg_ir::walk::register_tokens(
            self.faces
                .iter_mut()
                .flatten()
                .chain(self.tokens.iter_mut()),
            first,
        );
        self.tokens.extend(made);
    }

    fn intern_subtype(&mut self, name: String) -> u16 {
        match self.subtypes.iter().position(|s| *s == name) {
            Some(i) => i as u16,
            None => {
                self.subtypes.push(name);
                (self.subtypes.len() - 1) as u16
            }
        }
    }

    /// This match's id for a card.
    pub fn id(&self, key: &CardKey) -> Option<CardId> {
        self.keys
            .iter()
            .position(|k| k == key)
            .map(|i| CardId(i as u32))
    }

    pub fn key(&self, id: CardId) -> Option<&CardKey> {
        self.keys.get(id.0 as usize)
    }

    /// Every card id in the table.
    pub fn ids(&self) -> impl Iterator<Item = CardId> {
        (0..self.keys.len() as u32).map(CardId)
    }

    pub fn color_identity(&self, id: CardId) -> &str {
        self.identities
            .get(id.0 as usize)
            .map_or("", String::as_str)
    }
}

impl PrintedCards for GameCards {
    fn face(&self, card: CardId, face: u8) -> Option<&CardFace> {
        if let Some(i) = card.0.checked_sub(mtg_ir::walk::TOKEN_CARD_BASE) {
            return (face == 0).then(|| self.tokens.get(i as usize)).flatten();
        }
        self.faces.get(card.0 as usize)?.get(face as usize)
    }

    fn layout(&self, card: CardId) -> mtg_ir::Layout {
        if card.0 >= mtg_ir::walk::TOKEN_CARD_BASE {
            return mtg_ir::Layout::Token;
        }
        self.layouts
            .get(card.0 as usize)
            .copied()
            .unwrap_or(mtg_ir::Layout::Unknown)
    }

    fn subtype_name(&self, subtype: Subtype) -> Option<&str> {
        self.subtypes.get(subtype.0 as usize).map(String::as_str)
    }
}

/// Why a deck cannot be played in a format. Empty means it can.
pub fn deck_problems(format: Format, deck: &DeckSpec, cards: &GameCards) -> Vec<String> {
    let mut problems = Vec::new();
    let face = |key: &CardKey| cards.id(key).and_then(|id| cards.face(id, 0));
    let name = |key: &CardKey| face(key).map_or_else(|| format!("{key:?}"), |f| f.name.to_string());
    let english_name = |key: &CardKey| face(key).map(|f| f.name.as_ref());
    let is_basic =
        |key: &CardKey| face(key).is_some_and(|f| f.supertypes.contains(&Supertype::Basic));

    let mut main_names: BTreeMap<&str, u32> = BTreeMap::new();
    for (key, count) in &deck.main {
        if let Some(card_name) = english_name(key) {
            *main_names.entry(card_name).or_default() += u32::from(*count);
        }
    }
    // Keep per-card identities for colour checks, but use English names for singleton rules.
    let mut main_cards: BTreeMap<&CardKey, u32> = BTreeMap::new();
    for (k, n) in &deck.main {
        *main_cards.entry(k).or_insert(0) += u32::from(*n);
    }

    match format {
        Format::Constructed => {
            if deck.main_count() < 60 {
                problems.push(format!(
                    "the main deck has {} cards; it needs at least 60",
                    deck.main_count()
                ));
            }
            let side: u32 = deck.side.iter().map(|(_, n)| u32::from(*n)).sum();
            if side > 15 {
                problems.push(format!(
                    "the sideboard has {side} cards; at most 15 are allowed"
                ));
            }
            let mut constructed_copies: BTreeMap<&str, u32> = BTreeMap::new();
            for (k, n) in deck.main.iter().chain(&deck.side) {
                if let Some(card_name) = english_name(k) {
                    *constructed_copies.entry(card_name).or_default() += u32::from(*n);
                }
            }
            for (card_name, n) in &constructed_copies {
                let basic = deck
                    .main
                    .iter()
                    .chain(&deck.side)
                    .filter(|(key, _)| english_name(key) == Some(*card_name))
                    .all(|(key, _)| is_basic(key));
                if *n > 4 && !basic {
                    problems.push(format!(
                        "{n} copies of {card_name} across the main deck and sideboard; at most 4 are allowed"
                    ));
                }
            }
        }
        Format::Commander => {
            if !deck.side.is_empty() {
                problems.push("Commander decks cannot have a sideboard".into());
            }
            let Some(commander) = &deck.commander else {
                problems.push("a Commander deck needs a commander".into());
                return problems;
            };
            let legendary_creature = face(commander).is_some_and(|f| {
                f.supertypes.contains(&Supertype::Legendary)
                    && f.card_types.contains(&CardType::Creature)
            });
            if !legendary_creature {
                problems.push(format!("{} is not a legendary creature", name(commander)));
            }
            if deck.main_count() != 99 {
                problems.push(format!(
                    "the deck has {} cards besides the commander; it needs exactly 99",
                    deck.main_count()
                ));
            }
            let commander_name = english_name(commander);
            if commander_name.is_some_and(|n| main_names.contains_key(n)) {
                problems.push(format!(
                    "{} is both the commander and in the deck",
                    name(commander)
                ));
            }
            for (card_name, n) in &main_names {
                let basic = deck
                    .main
                    .iter()
                    .filter(|(key, _)| english_name(key) == Some(*card_name))
                    .all(|(key, _)| is_basic(key));
                if *n > 1 && !basic {
                    problems.push(format!("{n} copies of {card_name}; Commander allows one"));
                }
            }
            let allowed = cards
                .id(commander)
                .map_or("", |id| cards.color_identity(id));
            for k in main_cards.keys() {
                let identity = cards.id(k).map_or("", |id| cards.color_identity(id));
                if identity.chars().any(|c| !allowed.contains(c)) {
                    problems.push(format!("{} is outside the commander's colours", name(k)));
                }
            }
        }
    }
    problems
}

/// The shuffle seed for one seat in one game, derived from the match's agreed seed so that
/// every game and every library gets its own order and neither player chose any of them.
pub fn game_seed(match_seed: &Seed, game: u8, seat: PlayerId) -> Seed {
    let mut tweak = [0u8; 32];
    tweak[0] = game;
    tweak[1] = seat.0;
    tweak[31] = 0xA5;
    Seed::combine(*match_seed, Seed(tweak))
}

/// A game ready to start: libraries shuffled, seven cards dealt, commanders set aside.
///
/// There is no mulligan yet: the engine's `KeepOrMulligan` question exists but is not asked.
pub fn new_game(
    format: Format,
    cards: &GameCards,
    decks: [&DeckSpec; 2],
    match_seed: &Seed,
    game: u8,
    first: PlayerId,
) -> GameState {
    let mut state = GameState::new(&SEATS, format.starting_life());
    state.turn = 1;
    state.active_player = first;
    state.priority = Some(first);
    // Seating starts with whoever plays first, so turn order follows.
    state.turn_order = if first == SEATS[0] {
        SEATS.to_vec()
    } else {
        vec![SEATS[1], SEATS[0]]
    };
    // Opening hands are decided before turn 1, starting player first (CR 103.5), and every
    // mulligan's shuffle comes from this game's own seed.
    state.pregame = Some(mtg_engine::state::Pregame::new(state.turn_order.clone()));
    state.rng = mtg_engine::state::Rng::from_seed(&game_seed(match_seed, game, PlayerId(0xEE)).0);

    for (seat, deck) in SEATS.into_iter().zip(decks) {
        if format == Format::Commander
            && let Some(id) = deck.commander.as_ref().and_then(|k| cards.id(k))
        {
            state.place(id, seat, ZoneRef::of(Zone::Command, seat));
            state.commander.commanders.insert(seat, id);
        }

        let mut library: Vec<CardId> = deck
            .main
            .iter()
            .filter_map(|(k, n)| Some((cards.id(k)?, *n)))
            .flat_map(|(id, n)| std::iter::repeat_n(id, n as usize))
            .collect();
        let order = shuffle_order(&game_seed(match_seed, game, seat), library.len());
        library = order.into_iter().map(|i| library[i]).collect();

        // The top seven are the opening hand; the rest is the library, top first.
        let hand = library.len().min(7);
        for id in &library[..hand] {
            state.place(*id, seat, ZoneRef::of(Zone::Hand, seat));
        }
        for id in &library[hand..] {
            state.place(*id, seat, ZoneRef::of(Zone::Library, seat));
        }
    }
    state
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtg_core::ManaCost;

    /// A source of invented cards: key `Demo(n)` is card n.
    struct Invented(Vec<(CardFace, &'static str)>);

    impl CardSource for Invented {
        fn card(&self, key: &CardKey) -> Option<SourcedCard> {
            let CardKey::Demo(n) = key else { return None };
            let (face, identity) = self.0.get(*n as usize)?;
            Some(SourcedCard {
                layout: mtg_ir::Layout::Normal,
                faces: vec![face.clone()],
                color_identity: identity.to_string(),
            })
        }
        fn subtype_name(&self, s: Subtype) -> Option<String> {
            Some(format!("Kind{}", s.0))
        }
    }

    fn face(name: &str, types: Vec<CardType>, supertypes: Vec<Supertype>) -> CardFace {
        CardFace {
            name: name.into(),
            mana_cost: ManaCost::FREE,
            card_types: types,
            subtypes: vec![Subtype(7)],
            supertypes,
            power: Some(1),
            toughness: Some(1),
            loyalty: None,
            abilities: Vec::new(),
            oracle_text: None,
            colors: None,
        }
    }

    fn source() -> Invented {
        Invented(vec![
            (
                face("Plain Land", vec![CardType::Land], vec![Supertype::Basic]),
                "",
            ),
            (face("Green Bear", vec![CardType::Creature], vec![]), "G"),
            (
                face(
                    "Captain",
                    vec![CardType::Creature],
                    vec![Supertype::Legendary],
                ),
                "G",
            ),
            (face("Blue Bird", vec![CardType::Creature], vec![]), "U"),
        ])
    }

    fn deck(main: &[(u32, u8)]) -> DeckSpec {
        DeckSpec {
            name: "test".into(),
            main: main.iter().map(|(k, n)| (CardKey::Demo(*k), *n)).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn a_table_numbers_each_distinct_card_once_across_both_decks() {
        let a = deck(&[(0, 20), (1, 40)]);
        let b = deck(&[(0, 20), (3, 40)]);
        let cards = GameCards::build([&a, &b], &source()).unwrap();
        assert_eq!(cards.ids().count(), 3);
        assert_eq!(
            cards.key(cards.id(&CardKey::Demo(3)).unwrap()),
            Some(&CardKey::Demo(3))
        );
    }

    #[test]
    fn subtypes_are_renumbered_into_the_table() {
        let cards = GameCards::build([&deck(&[(1, 1)])], &source()).unwrap();
        let f = cards.face(CardId(0), 0).unwrap();
        assert_eq!(cards.subtype_name(f.subtypes[0]), Some("Kind7"));
    }

    #[test]
    fn missing_cards_are_all_reported() {
        let d = DeckSpec {
            main: vec![
                (CardKey::Oracle("nope".into(), None), 1),
                (CardKey::Demo(99), 1),
            ],
            ..Default::default()
        };
        assert_eq!(GameCards::build([&d], &source()).unwrap_err().len(), 2);
    }

    #[test]
    fn constructed_checks_size_and_copies_but_not_basics() {
        let ok = deck(&[(0, 52), (1, 4), (3, 4)]);
        let cards = GameCards::build([&ok], &source()).unwrap();
        assert!(
            deck_problems(Format::Constructed, &ok, &cards).is_empty(),
            "52 basics are fine"
        );

        let bad = deck(&[(0, 10), (1, 5)]);
        let cards = GameCards::build([&bad], &source()).unwrap();
        let problems = deck_problems(Format::Constructed, &bad, &cards);
        assert_eq!(
            problems.len(),
            2,
            "too small, and five of a non-basic: {problems:?}"
        );
    }

    #[test]
    fn commander_checks_the_commander_singleton_and_colours() {
        let mut d = deck(&[(0, 98), (1, 1)]);
        d.commander = Some(CardKey::Demo(2));
        let cards = GameCards::build([&d], &source()).unwrap();
        assert!(deck_problems(Format::Commander, &d, &cards).is_empty());

        // A blue card under a green commander.
        let mut off = deck(&[(0, 98), (3, 1)]);
        off.commander = Some(CardKey::Demo(2));
        let cards = GameCards::build([&off], &source()).unwrap();
        let problems = deck_problems(Format::Commander, &off, &cards);
        assert!(
            problems.iter().any(|p| p.contains("outside")),
            "{problems:?}"
        );

        // A non-legendary commander.
        let mut plain = deck(&[(0, 99)]);
        plain.commander = Some(CardKey::Demo(1));
        let cards = GameCards::build([&plain], &source()).unwrap();
        assert!(
            deck_problems(Format::Commander, &plain, &cards)
                .iter()
                .any(|p| p.contains("legendary"))
        );
    }

    #[test]
    fn sideboarding_may_only_move_cards() {
        let mut a = deck(&[(1, 4)]);
        a.side = vec![(CardKey::Demo(3), 2)];
        let mut swapped = deck(&[(1, 2), (3, 2)]);
        swapped.side = vec![(CardKey::Demo(1), 2)];
        assert!(a.same_cards_as(&swapped));
        let added = deck(&[(1, 4), (3, 3)]);
        assert!(!a.same_cards_as(&added));
    }

    #[test]
    fn a_new_game_deals_seven_and_keeps_the_rest_in_the_library() {
        let a = deck(&[(0, 30), (1, 30)]);
        let cards = GameCards::build([&a], &source()).unwrap();
        let state = new_game(
            Format::Constructed,
            &cards,
            [&a, &a],
            &Seed([1; 32]),
            1,
            SEATS[1],
        );
        for seat in SEATS {
            assert_eq!(state.objects_in(ZoneRef::of(Zone::Hand, seat)).len(), 7);
            assert_eq!(state.objects_in(ZoneRef::of(Zone::Library, seat)).len(), 53);
        }
        assert_eq!(state.active_player, SEATS[1]);
        assert_eq!(state.turn_order[0], SEATS[1], "the first player sits first");
        assert_eq!(state.player(SEATS[0]).life, 20);
    }

    #[test]
    fn each_game_and_each_seat_shuffles_differently() {
        let seed = Seed([9; 32]);
        assert_ne!(game_seed(&seed, 1, SEATS[0]), game_seed(&seed, 1, SEATS[1]));
        assert_ne!(game_seed(&seed, 1, SEATS[0]), game_seed(&seed, 2, SEATS[0]));
    }

    #[test]
    fn a_commander_game_sets_the_commander_aside_at_forty_life() {
        let mut d = deck(&[(0, 98), (1, 1)]);
        d.commander = Some(CardKey::Demo(2));
        let cards = GameCards::build([&d], &source()).unwrap();
        let state = new_game(
            Format::Commander,
            &cards,
            [&d, &d],
            &Seed([2; 32]),
            1,
            SEATS[0],
        );
        assert_eq!(state.player(SEATS[0]).life, 40);
        for seat in SEATS {
            assert_eq!(state.objects_in(ZoneRef::of(Zone::Command, seat)).len(), 1);
            assert_eq!(
                state.commander.commanders[&seat],
                cards.id(&CardKey::Demo(2)).unwrap()
            );
        }
    }

    #[test]
    fn match_snapshot_preserves_layout_and_both_faces() {
        struct Modal;
        impl CardSource for Modal {
            fn card(&self, _: &CardKey) -> Option<SourcedCard> {
                Some(SourcedCard {
                    layout: mtg_ir::Layout::ModalDfc,
                    faces: vec![
                        face("Dawn Visitor", vec![CardType::Creature], vec![]),
                        face("Dusk Meadow", vec![CardType::Land], vec![]),
                    ],
                    color_identity: "G".into(),
                })
            }
            fn subtype_name(&self, _: Subtype) -> Option<String> {
                None
            }
        }
        let d = deck(&[(0, 1)]);
        let cards = GameCards::build([&d], &Modal).unwrap();
        let id = cards.id(&CardKey::Demo(0)).unwrap();
        let encoded = serde_json::to_value(&cards).unwrap();
        let decoded: GameCards = serde_json::from_value(encoded.clone()).unwrap();
        assert_eq!(decoded.layout(id), mtg_ir::Layout::ModalDfc);
        assert_eq!(decoded.face(id, 1).unwrap().name.as_ref(), "Dusk Meadow");

        // Older snapshots contain faces without their relationships. Keep that
        // uncertainty explicit rather than enabling a guessed casting rule.
        let mut legacy = encoded;
        legacy.as_object_mut().unwrap().remove("layouts");
        let legacy: GameCards = serde_json::from_value(legacy).unwrap();
        assert_eq!(legacy.layout(id), mtg_ir::Layout::Unknown);
        assert_eq!(legacy.face(id, 1).unwrap().name.as_ref(), "Dusk Meadow");
    }
}
