//! The deck builder: browse every card, build a deck beside the results, and test it.
//!
//! Two halves. [`Draft`] is the deck being edited — counts per card, a sideboard, a commander —
//! with everything a deck list needs to show about itself (groups by type, the mana curve, its
//! colours) and nothing about drawing, so it is tested without a window. [`Builder`] is the
//! screen: a search with filters over the whole card database, results as card images you click
//! to add, and the deck beside them, with a sample hand and a playtest one click away.

use std::collections::{BTreeMap, VecDeque};

use egui::{Color32, RichText, Ui, Vec2};
use mtg_net::fairness::{Seed, shuffle_order};
use mtg_session::game::{
    CardKey, CardSource, DeckSpec, Format, GameCards, SourcedCard, deck_problems,
};
use mtg_store::{CardQuery, Section, Store};

use crate::{art::CardArt, widgets};

use crate::theme::{GOLD as ACCENT, GOOD, WARN};

/// Formats the "legal in" filter offers, by Scryfall's key.
const LEGALITY_FORMATS: [(&str, &str); 8] = [
    ("standard", "Standard"),
    ("pioneer", "Pioneer"),
    ("modern", "Modern"),
    ("legacy", "Legacy"),
    ("vintage", "Vintage"),
    ("pauper", "Pauper"),
    ("commander", "Commander"),
    ("brawl", "Brawl"),
];

/// Search results per page.
const PAGE: usize = 30;
const HISTORY_LIMIT: usize = 100;
/// Bound the expanded library and shuffle permutation used for practice.
const MAX_SAMPLE_CARDS: u64 = 10_000;
/// A result tile, sized to the card's 488×680 proportions.
const TILE: Vec2 = Vec2::new(146.0, 146.0 * 680.0 / 488.0);

/// The same whole-card gate used by match coverage, with useful printed-text reasons.
#[derive(Clone, Debug)]
struct CardSupport {
    issues: Vec<String>,
}

impl CardSupport {
    fn load(store: &Store, oracle: u32) -> Self {
        let card = match store.card(oracle) {
            Ok(Some(card)) => card,
            Ok(None) => {
                return Self {
                    issues: vec!["Card is missing from the database".into()],
                };
            }
            Err(error) => {
                return Self {
                    issues: vec![format!("Could not read card: {error}")],
                };
            }
        };
        let subtypes = match store.subtypes() {
            Ok(names) => mtg_oracle::compile::SubtypeNames(names),
            Err(error) => {
                return Self {
                    issues: vec![format!("Could not read card types: {error}")],
                };
            }
        };
        if mtg_oracle::compile::card_understood(&card, &subtypes) {
            return Self { issues: Vec::new() };
        }
        let mut issues = Vec::new();
        if !mtg_oracle::compile::layout_understood(&card) {
            issues.push("This card's face layout is not supported yet".into());
        }
        for row in &card.faces {
            let types = mtg_oracle::typeline::parse(&row.type_line);
            let compiled = mtg_oracle::compile::compile(
                &mtg_oracle::compile::FaceText {
                    name: &row.name,
                    card_types: &types.card_types,
                    subtypes: &types.subtypes,
                    oracle_text: row.oracle_text.as_deref(),
                    mana_cost: &row.mana_cost,
                },
                &subtypes,
            );
            for text in &compiled.unparsed {
                issues.push(format!("{}: {text}", row.name));
            }
            if card.layout == "split" && !mtg_oracle::compile::split_abilities_understood(&compiled)
            {
                issues.push(format!(
                    "{}: these split-card abilities are not supported yet",
                    row.name
                ));
            }
        }
        if issues.is_empty() {
            issues.push("This card is not fully supported yet".into());
        }
        Self { issues }
    }

    fn ready(&self) -> bool {
        self.issues.is_empty()
    }

    fn explain(&self, ui: &mut Ui) {
        ui.label(
            RichText::new(if self.ready() {
                "Fully supported"
            } else {
                "Rules not fully supported yet"
            })
            .color(if self.ready() { GOOD } else { WARN })
            .small(),
        );
        for issue in &self.issues {
            ui.label(RichText::new(issue).small());
        }
    }
}

/// Validation reads the builder's active database, including in-memory stores.
struct BuilderSource<'a> {
    store: &'a Store,
    subtypes: Vec<(u16, String)>,
}

impl mtg_oracle::convert::Subtypes for BuilderSource<'_> {
    fn intern(&self, name: &str) -> Option<u16> {
        self.subtypes
            .iter()
            .find(|(_, n)| n == name)
            .map(|(id, _)| *id)
    }
}

impl CardSource for BuilderSource<'_> {
    fn card(&self, key: &CardKey) -> Option<SourcedCard> {
        let CardKey::Oracle(uuid, _printing_id) = key else {
            return None;
        };
        let oracle = self.store.find_by_uuid(uuid).ok()??;
        let card = self.store.card(oracle).ok()??;
        Some(SourcedCard {
            layout: mtg_oracle::convert::to_engine_layout(&card.layout),
            faces: card
                .faces
                .iter()
                .map(|row| mtg_oracle::convert::to_engine_face(row, self))
                .collect(),
            color_identity: self.store.color_identity(oracle).ok()?,
        })
    }

    fn subtype_name(&self, subtype: mtg_core::Subtype) -> Option<String> {
        self.subtypes
            .iter()
            .find(|(id, _)| *id == subtype.0)
            .map(|(_, name)| name.clone())
    }
}

/// What the deck list shows about one card.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DraftCard {
    pub oracle: u32,
    pub name: String,
    pub type_line: String,
    /// As printed: `{2}{G}`.
    pub cost: String,
    pub mana_value: u32,
    /// Colour identity letters.
    pub identity: String,
}

impl DraftCard {
    pub fn load(store: &Store, oracle: u32) -> Option<Self> {
        let card = store.card(oracle).ok()??;
        let front = card.faces.first()?;
        let (mut cost, _) = mtg_oracle::manacost::parse(&front.mana_cost);
        let mut shown_cost = front.mana_cost.clone();
        let mut type_line = front.type_line.clone();
        if card.layout == "split"
            && let Some(right) = card.faces.get(1)
        {
            cost.symbols
                .extend(mtg_oracle::manacost::parse(&right.mana_cost).0.symbols);
            shown_cost.push_str(&right.mana_cost);
            if type_line != right.type_line {
                type_line.push_str(" // ");
                type_line.push_str(&right.type_line);
            }
        }
        Some(Self {
            oracle,
            name: card.name.clone(),
            type_line,
            cost: shown_cost,
            mana_value: cost.mana_value(),
            identity: store.color_identity(oracle).unwrap_or_default(),
        })
    }

    pub fn group(&self) -> Group {
        let t = self.type_line.split(['—', '-']).next().unwrap_or("");
        let has = |w: &str| t.split_whitespace().any(|x| x == w);
        if has("Creature") {
            Group::Creatures
        } else if has("Planeswalker") {
            Group::Planeswalkers
        } else if has("Land") {
            Group::Lands
        } else if has("Instant") {
            Group::Instants
        } else if has("Sorcery") {
            Group::Sorceries
        } else if has("Artifact") {
            Group::Artifacts
        } else if has("Enchantment") {
            Group::Enchantments
        } else {
            Group::Other
        }
    }

    pub fn is_land(&self) -> bool {
        self.group() == Group::Lands
    }
}

/// How the deck list is sectioned, in display order.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Group {
    Creatures,
    Planeswalkers,
    Instants,
    Sorceries,
    Artifacts,
    Enchantments,
    Other,
    Lands,
}

impl Group {
    pub fn label(self) -> &'static str {
        match self {
            Group::Creatures => "Creatures",
            Group::Planeswalkers => "Planeswalkers",
            Group::Instants => "Instants",
            Group::Sorceries => "Sorceries",
            Group::Artifacts => "Artifacts",
            Group::Enchantments => "Enchantments",
            Group::Other => "Other",
            Group::Lands => "Lands",
        }
    }
}

/// A deck being edited.
#[derive(Clone, Debug, Default)]
pub struct Draft {
    /// The saved deck this edits, if it has been saved.
    pub id: Option<i64>,
    pub name: String,
    pub main: BTreeMap<u32, u32>,
    pub side: BTreeMap<u32, u32>,
    pub commander: Option<u32>,
    /// Display details for every card in the deck.
    pub cards: BTreeMap<u32, DraftCard>,
    /// Artwork preference for each oracle identity; rules and counts stay oracle based.
    pub printings: BTreeMap<u32, String>,
    /// Changed since last saved.
    pub dirty: bool,
    undo: Vec<DeckContents>,
    redo: Vec<DeckContents>,
    saved: Option<(String, DeckContents)>,
}

/// Editing history changes deck composition; text fields retain their own undo.
#[derive(Clone, Debug, PartialEq, Eq)]
struct DeckContents {
    printings: BTreeMap<u32, String>,
    main: BTreeMap<u32, u32>,
    side: BTreeMap<u32, u32>,
    commander: Option<u32>,
}

/// Opening-hand practice, independent from the match engine and saved deck.
#[derive(Clone, Debug)]
struct SampleHand {
    hand: Vec<u32>,
    library: VecDeque<u32>,
    bottom: Vec<usize>,
    mulligans: usize,
    first_free: bool,
    kept: bool,
}

impl SampleHand {
    fn new(mut deck: Vec<u32>, first_free: bool) -> Self {
        let hand = deck.drain(..deck.len().min(7)).collect();
        Self {
            hand,
            library: deck.into(),
            bottom: Vec::new(),
            mulligans: 0,
            first_free,
            kept: false,
        }
    }

    fn bottom_count(&self) -> usize {
        self.mulligans
            .saturating_sub(usize::from(self.first_free))
            .min(self.hand.len())
    }

    fn toggle_bottom(&mut self, index: usize) {
        if self.kept || index >= self.hand.len() {
            return;
        }
        if self.bottom.contains(&index) {
            self.bottom.retain(|i| *i != index);
        } else if self.bottom.len() < self.bottom_count() {
            self.bottom.push(index);
        }
    }

    fn keep(&mut self) -> bool {
        if self.kept || self.bottom.len() != self.bottom_count() {
            return false;
        }
        // Selection order is the chosen order on the bottom, first selected first drawn.
        self.library
            .extend(self.bottom.iter().map(|i| self.hand[*i]));
        let mut index = 0;
        self.hand.retain(|_| {
            let keep = !self.bottom.contains(&index);
            index += 1;
            keep
        });
        self.bottom.clear();
        self.kept = true;
        true
    }

    fn mulligan(&mut self, deck: Vec<u32>) -> bool {
        if self.kept || self.bottom_count() >= self.hand.len() {
            return false;
        }
        let taken = self.mulligans + 1;
        *self = Self::new(deck, self.first_free);
        self.mulligans = taken;
        true
    }

    fn draw(&mut self) -> bool {
        if !self.kept {
            return false;
        }
        let Some(card) = self.library.pop_front() else {
            return false;
        };
        self.hand.push(card);
        true
    }
}

impl Draft {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            dirty: true,
            ..Default::default()
        }
    }

    /// Open a saved deck for editing.
    pub fn load(store: &Store, id: i64) -> Result<Self, String> {
        let row = store
            .deck(id)
            .map_err(|e| e.to_string())?
            .ok_or("that deck no longer exists")?;
        let mut draft = Self {
            id: Some(id),
            name: row.name,
            ..Default::default()
        };
        for (oracle, count, section) in row.entries {
            draft.know(store, oracle);
            match section {
                Section::Main | Section::Sideboard => {
                    let list = if section == Section::Main {
                        &mut draft.main
                    } else {
                        &mut draft.side
                    };
                    let total = list.entry(oracle).or_insert(0);
                    *total = total
                        .checked_add(count)
                        .ok_or_else(|| format!("card #{oracle} has too many copies to edit"))?;
                }
                Section::Commander => {
                    if count != 1 || draft.commander.is_some() {
                        return Err("the builder currently supports exactly one commander per Commander deck".into());
                    }
                    draft.commander = Some(oracle);
                }
            }
        }
        draft.printings = store
            .deck_printings(id)
            .map_err(|e| e.to_string())?
            .into_iter()
            .collect();
        draft.saved = Some((draft.name.trim().to_string(), draft.contents()));
        Ok(draft)
    }

    fn know(&mut self, store: &Store, oracle: u32) {
        if !self.cards.contains_key(&oracle)
            && let Some(card) = DraftCard::load(store, oracle)
        {
            self.cards.insert(oracle, card);
        }
    }

    /// One more copy, in the main deck or the sideboard.
    pub fn add(&mut self, store: &Store, oracle: u32, to_side: bool) {
        let list = if to_side { &self.side } else { &self.main };
        if list.get(&oracle) == Some(&u32::MAX) {
            return;
        }
        self.know(store, oracle);
        self.remember();
        let list = if to_side {
            &mut self.side
        } else {
            &mut self.main
        };
        *list.entry(oracle).or_insert(0) += 1;
        self.refresh_dirty();
    }

    /// One fewer copy.
    pub fn remove(&mut self, oracle: u32, from_side: bool) {
        if !(if from_side { &self.side } else { &self.main })
            .get(&oracle)
            .is_some_and(|n| *n > 0)
        {
            return;
        }
        self.remember();
        let list = if from_side {
            &mut self.side
        } else {
            &mut self.main
        };
        if let Some(n) = list.get_mut(&oracle) {
            *n -= 1;
            if *n == 0 {
                list.remove(&oracle);
            }
        }
        self.refresh_dirty();
    }

    /// Removing a row is one edit, regardless of how many copies it contained.
    pub fn remove_all(&mut self, oracle: u32, from_side: bool) {
        if !(if from_side { &self.side } else { &self.main }).contains_key(&oracle) {
            return;
        }
        self.remember();
        (if from_side {
            &mut self.side
        } else {
            &mut self.main
        })
        .remove(&oracle);
        self.refresh_dirty();
    }

    /// Move one copy between main deck and sideboard.
    pub fn move_one(&mut self, oracle: u32, to_side: bool) {
        let destination = if to_side { &self.side } else { &self.main };
        if destination.get(&oracle) == Some(&u32::MAX) {
            return;
        }
        let had = if to_side {
            self.main.get(&oracle)
        } else {
            self.side.get(&oracle)
        };
        if had.is_some_and(|n| *n > 0) {
            self.remember();
            let source = if to_side {
                &mut self.main
            } else {
                &mut self.side
            };
            let n = source.get_mut(&oracle).unwrap();
            *n -= 1;
            if *n == 0 {
                source.remove(&oracle);
            }
            let list = if to_side {
                &mut self.side
            } else {
                &mut self.main
            };
            *list.entry(oracle).or_insert(0) += 1;
            self.refresh_dirty();
        }
    }

    /// Make a card the commander. It leaves the 99 if it was there.
    pub fn set_commander(&mut self, store: &Store, oracle: Option<u32>) {
        if self.commander == oracle && !oracle.is_some_and(|o| self.main.contains_key(&o)) {
            return;
        }
        self.remember();
        if let Some(o) = oracle {
            self.know(store, o);
            self.main.remove(&o);
        }
        self.commander = oracle;
        self.refresh_dirty();
    }

    fn contents(&self) -> DeckContents {
        DeckContents {
            printings: self.printings.clone(),
            main: self.main.clone(),
            side: self.side.clone(),
            commander: self.commander,
        }
    }

    pub fn set_printing(&mut self, oracle: u32, printing: Option<String>) {
        if self.printings.get(&oracle) == printing.as_ref() {
            return;
        }
        self.remember();
        if let Some(id) = printing {
            self.printings.insert(oracle, id);
        } else {
            self.printings.remove(&oracle);
        }
        self.refresh_dirty();
    }

    fn remember(&mut self) {
        if self.undo.len() == HISTORY_LIMIT {
            self.undo.remove(0);
        }
        self.undo.push(self.contents());
        self.redo.clear();
    }

    fn restore(&mut self, contents: DeckContents) {
        self.printings = contents.printings;
        self.main = contents.main;
        self.side = contents.side;
        self.commander = contents.commander;
        self.refresh_dirty();
    }

    fn refresh_dirty(&mut self) {
        self.dirty = self.saved.as_ref().is_none_or(|(name, contents)| {
            self.name.trim() != name || self.contents() != *contents
        });
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.redo.push(self.contents());
        self.restore(previous);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(self.contents());
        self.restore(next);
        true
    }

    pub fn main_count(&self) -> u64 {
        self.main.values().map(|n| u64::from(*n)).sum()
    }

    pub fn side_count(&self) -> u64 {
        self.side.values().map(|n| u64::from(*n)).sum()
    }

    /// Non-land cards in the main deck by mana value, 0 through 6 and "7 or more".
    pub fn curve(&self) -> [u64; 8] {
        let mut curve = [0; 8];
        for (oracle, n) in &self.main {
            if let Some(card) = self.cards.get(oracle).filter(|c| !c.is_land()) {
                curve[(card.mana_value as usize).min(7)] += u64::from(*n);
            }
        }
        curve
    }

    /// The main deck in sections, each sorted by mana value then name.
    pub fn grouped(&self) -> Vec<(Group, Vec<(u32, u32)>)> {
        let mut groups: BTreeMap<Group, Vec<(u32, u32)>> = BTreeMap::new();
        for (oracle, n) in &self.main {
            let group = self
                .cards
                .get(oracle)
                .map_or(Group::Other, DraftCard::group);
            groups.entry(group).or_default().push((*oracle, *n));
        }
        for list in groups.values_mut() {
            list.sort_by_key(|(o, _)| {
                self.cards
                    .get(o)
                    .map(|c| (c.mana_value, c.name.clone()))
                    .unwrap_or_default()
            });
        }
        groups.into_iter().collect()
    }

    /// Every colour in the deck, WUBRG order.
    pub fn colors(&self) -> String {
        let all: String = self
            .main
            .keys()
            .chain(&self.commander)
            .filter_map(|o| self.cards.get(o))
            .map(|c| c.identity.as_str())
            .collect();
        "WUBRG".chars().filter(|c| all.contains(*c)).collect()
    }

    pub fn name_of(&self, oracle: u32) -> &str {
        self.cards.get(&oracle).map_or("?", |c| c.name.as_str())
    }

    /// Save, replacing the saved copy. A rename removes the old name.
    pub fn save(&mut self, store: &mut Store) -> Result<i64, String> {
        let name = self.name.trim().to_string();
        if name.is_empty() {
            return Err("give the deck a name".into());
        }
        let mut entries: Vec<(u32, u32, Section)> = Vec::new();
        entries.extend(self.main.iter().map(|(o, n)| (*o, *n, Section::Main)));
        entries.extend(self.side.iter().map(|(o, n)| (*o, *n, Section::Sideboard)));
        entries.extend(self.commander.map(|o| (o, 1, Section::Commander)));
        let id = store
            .put_deck_with_printings(
                self.id,
                &name,
                None,
                &entries,
                &self
                    .printings
                    .iter()
                    .map(|(o, p)| (*o, p.clone()))
                    .collect::<Vec<_>>(),
            )
            .map_err(|e| e.to_string())?;
        self.id = Some(id);
        self.name = name;
        self.saved = Some((self.name.clone(), self.contents()));
        self.dirty = false;
        Ok(id)
    }

    /// The deck as text, in the same form the importer reads: `4 Name` per line, the
    /// sideboard after a "Sideboard" line, the commander after a "Commander" line.
    pub fn to_text(&self) -> String {
        let line = |o: &u32, n: &u32| format!("{n} {}", self.name_of(*o));
        let mut out: Vec<String> = Vec::new();
        if let Some(c) = self.commander {
            out.push("Commander".into());
            out.push(line(&c, &1));
            out.push(String::new());
            out.push("Deck".into());
        }
        for (_, cards) in self.grouped() {
            out.extend(cards.iter().map(|(o, n)| line(o, n)));
        }
        if !self.side.is_empty() {
            out.push(String::new());
            out.push("Sideboard".into());
            out.extend(self.side.iter().map(|(o, n)| line(o, n)));
        }
        out.join("\n") + "\n"
    }

    /// The deck in the portable form a match plays.
    /// Returns an error when cards are unavailable or counts cannot be represented.
    pub fn spec(&self, store: &Store) -> Result<DeckSpec, String> {
        let key = |o: &u32| {
            store
                .card(*o)
                .map_err(|error| format!("Could not read card #{o}: {error}"))?
                .map(|c| CardKey::Oracle(c.oracle_uuid, self.printings.get(o).cloned()))
                .ok_or_else(|| {
                    format!(
                        "{} (card #{o}) is missing from the database",
                        self.name_of(*o)
                    )
                })
        };
        let entry = |(o, n): (&u32, &u32)| -> Result<_, String> {
            let count = u8::try_from(*n).map_err(|_| format!(
                "{} has {n} copies; play currently supports at most 255 per card in each section",
                self.name_of(*o)
            ))?;
            Ok((key(o)?, count))
        };
        Ok(DeckSpec {
            name: self.name.clone(),
            main: self.main.iter().map(entry).collect::<Result<_, _>>()?,
            side: self.side.iter().map(entry).collect::<Result<_, _>>()?,
            commander: self.commander.as_ref().map(key).transpose()?,
        })
    }

    /// The main deck shuffled with `seed`: an opening hand is the first seven.
    pub fn shuffled(&self, seed: &Seed) -> Result<Vec<u32>, String> {
        let count = self.main_count();
        if count > MAX_SAMPLE_CARDS {
            return Err(format!(
                "Sample hands support at most {MAX_SAMPLE_CARDS} main-deck cards; this deck has {count}"
            ));
        }
        let cards: Vec<u32> = self
            .main
            .iter()
            .flat_map(|(o, n)| std::iter::repeat_n(*o, *n as usize))
            .collect();
        Ok(shuffle_order(seed, cards.len())
            .into_iter()
            .map(|i| cards[i])
            .collect())
    }
}

/// What the builder asks its screen to do.
pub enum BuilderAction {
    Nothing,
    /// Leave the builder.
    Close,
    /// Play the deck against an opponent that does nothing, to see how it draws and curves.
    Playtest(DeckSpec, Format),
    /// The deck list changed on disk: refresh anything that shows it.
    Saved,
}

/// The deck builder screen's state.
pub struct Builder {
    pub draft: Draft,
    query: CardQuery,
    color_filter: [bool; 5],
    filter_colors: bool,
    page: usize,
    results: Vec<(u32, String)>,
    search_error: Option<String>,
    has_more: bool,
    /// The query the results are for, so a search runs only when something changed.
    searched: Option<(CardQuery, usize)>,
    format: Format,
    problems: Option<Vec<String>>,
    /// An independent opening hand, mulligans, and subsequent draws.
    sample: Option<SampleHand>,
    sample_contents: Option<DeckContents>,
    status: Option<(String, bool)>,
    confirm_close: bool,
    support: BTreeMap<u32, CardSupport>,
    printing_picker: Option<crate::printings::Picker>,
}

impl Builder {
    pub fn new(draft: Draft) -> Self {
        Self {
            format: if draft.commander.is_some() {
                Format::Commander
            } else {
                Format::Constructed
            },
            draft,
            query: CardQuery {
                legal_in: Some(String::new()),
                ..Default::default()
            },
            color_filter: [false; 5],
            filter_colors: false,
            page: 0,
            results: Vec::new(),
            search_error: None,
            has_more: false,
            searched: None,
            problems: None,
            sample: None,
            sample_contents: None,
            status: None,
            confirm_close: false,
            support: BTreeMap::new(),
            printing_picker: None,
        }
    }

    /// The top bar: name, save, format, tests, close.
    pub fn top_bar(&mut self, ui: &mut Ui, store: &mut Store) -> BuilderAction {
        if !self.draft.dirty {
            self.confirm_close = false;
        }
        let mut action = BuilderAction::Nothing;
        ui.horizontal_wrapped(|ui| {
            if self.confirm_close {
                ui.label(RichText::new("Unsaved changes —").color(WARN));
                if ui.button("Discard").clicked() {
                    action = BuilderAction::Close;
                }
                if ui.button("Keep editing").clicked() {
                    self.confirm_close = false;
                }
            } else if ui.button("⏴ Decks").clicked() {
                if self.draft.dirty
                    && (self.draft.id.is_some()
                        || self.draft.main_count() > 0
                        || self.draft.side_count() > 0
                        || self.draft.commander.is_some())
                {
                    self.confirm_close = true;
                } else {
                    action = BuilderAction::Close;
                }
            }
            ui.separator();
            ui.label("Name");
            if ui
                .add(egui::TextEdit::singleline(&mut self.draft.name).desired_width(220.0))
                .changed()
            {
                self.draft.refresh_dirty();
                self.status = None;
            }
            let typing = ui.ctx().egui_wants_keyboard_input();
            let redo_key = !typing
                && ui.input_mut(|i| {
                    i.consume_key(
                        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                        egui::Key::Z,
                    ) || i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y)
                });
            let undo_key = !typing
                && !redo_key
                && ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z));
            let undo = ui
                .add_enabled(self.draft.can_undo(), egui::Button::new("Undo"))
                .on_hover_text("Undo a card edit (Ctrl/Cmd+Z)")
                .clicked();
            let redo = ui
                .add_enabled(self.draft.can_redo(), egui::Button::new("Redo"))
                .on_hover_text("Redo a card edit (Ctrl/Cmd+Shift+Z or Ctrl+Y)")
                .clicked();
            if (undo || undo_key) && self.draft.undo() || (redo || redo_key) && self.draft.redo() {
                self.problems = None;
                self.sample = None;
                self.status = None;
                self.confirm_close = false;
            }
            let save = if self.draft.dirty {
                crate::theme::primary_button(
                    RichText::new("Save deck").strong().color(crate::theme::INK),
                )
            } else {
                egui::Button::new("Saved")
            };
            if ui
                .add_enabled(self.draft.dirty, save)
                .on_hover_text("Ctrl+S")
                .clicked()
                || (self.draft.dirty
                    && ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S)))
            {
                match self.draft.save(store) {
                    Ok(_) => {
                        self.confirm_close = false;
                        self.status = Some(("saved".into(), true));
                        action = BuilderAction::Saved;
                    }
                    Err(e) => self.status = Some((e, false)),
                }
            }
            ui.separator();
            ui.label("Check for");
            let before = self.format;
            ui.radio_value(&mut self.format, Format::Constructed, "Constructed");
            ui.radio_value(&mut self.format, Format::Commander, "Commander");
            if self.format != before {
                self.problems = None;
            }
            ui.separator();
            if ui
                .add_enabled(
                    self.draft.main_count() >= 7,
                    egui::Button::new("Sample hand"),
                )
                .clicked()
            {
                match self.draft.shuffled(&Seed::random()) {
                    Ok(cards) => {
                        self.sample = Some(SampleHand::new(cards, false));
                        self.sample_contents = Some(self.draft.contents());
                        self.status = None;
                    }
                    Err(error) => self.status = Some((error, false)),
                }
            }
            if ui
                .add_enabled(
                    self.draft.main_count() > 0,
                    egui::Button::new(RichText::new("Playtest ⏵").strong()),
                )
                .on_hover_text("Play the deck against an opponent who does nothing")
                .clicked()
            {
                match self.draft.spec(store) {
                    Ok(spec) => action = BuilderAction::Playtest(spec, self.format),
                    Err(error) => self.status = Some((error, false)),
                }
            }
            if ui
                .add_enabled(self.draft.main_count() > 0, egui::Button::new("Copy list"))
                .on_hover_text("copy the deck as text, to paste anywhere — or into Import")
                .clicked()
            {
                ui.ctx().copy_text(self.draft.to_text());
                self.status = Some(("list copied".into(), true));
            }
            if let Some((text, good)) = &self.status {
                ui.label(RichText::new(text).color(if *good { GOOD } else { WARN }));
            }
        });
        action
    }

    /// The search half: filters, then a page of results as cards.
    fn choose_printing(&mut self, ui: &Ui, store: &Store, oracle: u32) {
        if let Ok(Some(card)) = store.card(oracle) {
            self.printing_picker = Some(crate::printings::Picker::start(
                ui.ctx().clone(),
                oracle,
                card.oracle_uuid,
                card.name,
            ));
        }
    }

    pub fn search(&mut self, ui: &mut Ui, store: &Store, art: &mut CardArt) {
        if let Some(picker) = &mut self.printing_picker {
            let (open, selection) = picker.show(
                ui.ctx(),
                art,
                self.draft.printings.get(&picker.oracle).map(String::as_str),
            );
            if let Some(printing) = selection {
                self.draft.set_printing(picker.oracle, printing);
            }
            if !open {
                self.printing_picker = None;
            }
        }
        ui.heading("Card gallery");
        ui.label(RichText::new("Find the next card for your deck.").color(crate::theme::MUTED));
        ui.add_space(12.0);
        ui.horizontal_wrapped(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.query.text)
                    .hint_text("Search cards by name or rules…")
                    .desired_width(ui.available_width().min(340.0)),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.query.type_contains)
                    .hint_text("Card type or creature type")
                    .desired_width(160.0),
            );
        });
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut self.filter_colors, "Colours:");
            for (i, letter) in ["W", "U", "B", "R", "G"].iter().enumerate() {
                let on = self.color_filter[i];
                let rect_size = Vec2::splat(22.0);
                let (rect, response) = ui.allocate_exact_size(rect_size, egui::Sense::click());
                let painter = ui.painter().clone();
                widgets::paint_symbol(art, &painter, rect, letter);
                if !on || !self.filter_colors {
                    painter.rect_filled(rect, 11.0, Color32::from_black_alpha(150));
                }
                if response
                    .on_hover_text(format!("{letter}: include this colour"))
                    .clicked()
                {
                    self.color_filter[i] = !on;
                    self.filter_colors = true;
                }
            }
            let legal_label = |f: &Option<String>| match f.as_deref() {
                None => "all cards".to_string(),
                Some("") => "any format".to_string(),
                Some(f) => LEGALITY_FORMATS
                    .iter()
                    .find(|(k, _)| *k == f)
                    .map_or(f.to_string(), |(_, l)| l.to_string()),
            };
            ui.label("Legal in");
            egui::ComboBox::from_id_salt("legal-in")
                .selected_text(legal_label(&self.query.legal_in))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.query.legal_in,
                        Some(String::new()),
                        "any format",
                    )
                    .on_hover_text("hides joke, digital-only and playtest cards");
                    for (key, label) in LEGALITY_FORMATS {
                        ui.selectable_value(&mut self.query.legal_in, Some(key.to_string()), label);
                    }
                    ui.selectable_value(&mut self.query.legal_in, None, "all cards");
                });
            if ui.button("Reset filters").clicked() {
                self.query = CardQuery {
                    legal_in: Some(String::new()),
                    ..Default::default()
                };
                self.color_filter = [false; 5];
                self.filter_colors = false;
            }
        });
        self.query.colors_within = self.filter_colors.then(|| {
            ["W", "U", "B", "R", "G"]
                .iter()
                .zip(self.color_filter)
                .filter(|(_, on)| *on)
                .map(|(l, _)| *l)
                .collect()
        });

        if self.searched.as_ref() != Some(&(self.query.clone(), self.page)) {
            if self
                .searched
                .as_ref()
                .is_some_and(|(q, _)| *q != self.query)
            {
                self.page = 0;
            }
            self.load_search(store);
        }

        if let Some(error) = &self.search_error {
            ui.label(RichText::new(format!("Could not search cards: {error}")).color(WARN));
            if ui.button("Retry search").clicked() {
                self.searched = None;
                ui.ctx().request_repaint();
            }
            return;
        }

        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(self.page > 0, egui::Button::new("⏴ Prev"))
                .clicked()
            {
                self.page -= 1;
            }
            ui.label(RichText::new(format!("page {}", self.page + 1)).weak());
            if ui
                .add_enabled(self.has_more, egui::Button::new("Next ⏵"))
                .clicked()
            {
                self.page += 1;
            }
            ui.label(
                RichText::new("Hover to inspect · right-click for sideboard and commander")
                    .weak()
                    .small(),
            );
        });
        ui.separator();

        if self.results.is_empty() {
            // Check the whole database: a text/type/color search can have no matches even
            // when legality data is present, which should not be reported as a stale import.
            let legality_data_exists = store
                .browse(
                    &CardQuery {
                        legal_in: Some(String::new()),
                        ..CardQuery::default()
                    },
                    1,
                    0,
                )
                .is_ok_and(|cards| !cards.is_empty());
            if self.query.legal_in.is_some() && !legality_data_exists {
                ui.label(
                    RichText::new(
                        "No legality data yet — re-download the card database under Decks, \
                         or choose \"all cards\" above.",
                    )
                    .color(WARN),
                );
            } else {
                ui.label(RichText::new("No cards match this search and legality filter.").weak());
            }
            return;
        }
        let results = self.results.clone();
        egui::ScrollArea::vertical()
            .id_salt("results")
            .show(ui, |ui| {
                let spacing = 18.0;
                let columns = ((ui.available_width() + spacing) / (TILE.x + spacing))
                    .floor()
                    .max(1.0) as usize;
                egui::Grid::new("card-gallery")
                    .num_columns(columns)
                    .spacing(Vec2::new(spacing, spacing))
                    .show(ui, |ui| {
                        for (index, (oracle, name)) in results.into_iter().enumerate() {
                            let support = self
                                .support
                                .entry(oracle)
                                .or_insert_with(|| CardSupport::load(store, oracle))
                                .clone();
                            ui.vertical(|ui| {
                                ui.set_width(TILE.x);
                                let response = card_tile_with_art(
                                    ui,
                                    art,
                                    &name,
                                    &self.draft.printings.get(&oracle).map_or_else(
                                        || name.clone(),
                                        |id| format!("scryfall:{id}"),
                                    ),
                                    TILE,
                                    self.count_of(oracle),
                                );
                                let response = response.on_hover_ui(|ui| {
                                    if let Some(card) = DraftCard::load(store, oracle) {
                                        card_preview_printing(
                                            ui,
                                            art,
                                            &card,
                                            self.draft.printings.get(&oracle).map(String::as_str),
                                        );
                                    }
                                    support.explain(ui);
                                });
                                if response.clicked() {
                                    self.draft.add(store, oracle, false);
                                    self.problems = None;
                                }
                                response.context_menu(|ui| {
                                    if ui.button("Choose printing").clicked() {
                                        self.choose_printing(ui, store, oracle);
                                        ui.close();
                                    }
                                    if ui.button("Add to sideboard").clicked() {
                                        self.draft.add(store, oracle, true);
                                        self.problems = None;
                                        ui.close();
                                    }
                                    if ui.button("Set as commander").clicked() {
                                        self.draft.set_commander(store, Some(oracle));
                                        self.format = Format::Commander;
                                        self.problems = None;
                                        ui.close();
                                    }
                                });
                                let name_response = ui.add(
                                    egui::Label::new(RichText::new(&name).size(12.0)).truncate(),
                                );
                                self.hover_card(name_response, store, art, oracle);
                                ui.horizontal(|ui| {
                                    if ui
                                        .small_button("+ Add")
                                        .on_hover_text("Add one copy to the main deck")
                                        .clicked()
                                    {
                                        self.draft.add(store, oracle, false);
                                        self.problems = None;
                                    }
                                    let printing = ui.small_button("Printing");
                                    let printing = self.hover_card(printing, store, art, oracle);
                                    if printing.clicked() {
                                        self.choose_printing(ui, store, oracle);
                                    }
                                });
                                if !support.ready() {
                                    ui.label(RichText::new("Rules incomplete").color(WARN).small())
                                        .on_hover_ui(|ui| support.explain(ui));
                                }
                            });
                            if (index + 1) % columns == 0 {
                                ui.end_row();
                            }
                        }
                    });
            });
    }

    fn load_search(&mut self, store: &Store) {
        match store.browse(&self.query, PAGE + 1, self.page * PAGE) {
            Ok(mut results) => {
                self.has_more = results.len() > PAGE;
                results.truncate(PAGE);
                self.results = results;
                self.search_error = None;
            }
            Err(error) => {
                self.results.clear();
                self.has_more = false;
                self.search_error = Some(error.to_string());
            }
        }
        self.searched = Some((self.query.clone(), self.page));
    }

    fn count_of(&self, oracle: u32) -> u64 {
        u64::from(self.draft.main.get(&oracle).copied().unwrap_or(0))
            + u64::from(self.draft.side.get(&oracle).copied().unwrap_or(0))
            + u64::from(self.draft.commander == Some(oracle))
    }

    /// The deck half: counts, curve, legality, and the list itself.
    pub fn deck(&mut self, ui: &mut Ui, store: &Store, art: &mut CardArt) {
        let mut ids: Vec<u32> = self
            .draft
            .main
            .keys()
            .chain(self.draft.side.keys())
            .copied()
            .chain(self.draft.commander)
            .collect();
        ids.sort_unstable();
        ids.dedup();
        for oracle in &ids {
            self.support
                .entry(*oracle)
                .or_insert_with(|| CardSupport::load(store, *oracle));
        }
        let colors = self.draft.colors();
        ui.heading("Your deck");
        crate::theme::surface().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(self.draft.main_count().to_string())
                        .size(30.0)
                        .strong(),
                );
                ui.label(RichText::new("main deck").color(crate::theme::MUTED));
                let pips: String = colors.chars().map(|c| format!("{{{c}}}")).collect();
                if !pips.is_empty() {
                    widgets::mana_label(ui, art, &pips, 16.0, ui.visuals().text_color());
                }
            });
            ui.label(
                RichText::new(format!(
                    "{} sideboard · {} unique cards",
                    self.draft.side_count(),
                    ids.len()
                ))
                .color(crate::theme::MUTED)
                .small(),
            );
        });
        ui.add_space(8.0);
        ui.label(
            RichText::new("MANA CURVE")
                .size(10.0)
                .color(crate::theme::MUTED),
        );
        curve_chart(ui, &self.draft.curve());
        if !ids.is_empty() {
            let ready = ids.iter().filter(|id| self.support[id].ready()).count();
            ui.label(
                RichText::new(format!(
                    "{ready} of {} unique cards fully supported",
                    ids.len()
                ))
                .color(if ready == ids.len() { GOOD } else { WARN })
                .small(),
            );
            if ready != ids.len() {
                ui.collapsing("Cards with unsupported rules", |ui| {
                    for oracle in &ids {
                        let support = &self.support[oracle];
                        if !support.ready() {
                            ui.label(RichText::new(self.draft.name_of(*oracle)).strong());
                            support.explain(ui);
                        }
                    }
                });
            }
        }

        // Legality, recomputed when the deck changes.
        if self.problems.is_none() {
            self.problems = Some(self.check(store));
        }
        match self.problems.as_deref() {
            Some([]) => {
                ui.label(RichText::new(format!("Legal for {}", self.format.name())).color(GOOD));
            }
            Some(problems) => {
                for p in problems.iter().take(3) {
                    ui.label(RichText::new(p).color(WARN).small());
                }
                if problems.len() > 3 {
                    ui.collapsing(format!("Show {} more problems", problems.len() - 3), |ui| {
                        for problem in &problems[3..] {
                            ui.label(RichText::new(problem).color(WARN).small());
                        }
                    });
                }
            }
            None => {}
        }
        ui.separator();

        let mut changed = false;
        egui::ScrollArea::vertical().id_salt("deck").show(ui, |ui| {
            if let Some(commander) = self.draft.commander {
                ui.label(RichText::new("Commander").strong());
                let name = self.draft.name_of(commander).to_string();
                ui.horizontal(|ui| {
                    let r = ui.label(RichText::new(&name).color(ACCENT).strong());
                    self.hover_card(r, store, art, commander);
                    if ui.small_button("✖").on_hover_text("no commander").clicked() {
                        self.draft.set_commander(store, None);
                        changed = true;
                    }
                });
                ui.add_space(6.0);
            }
            for (group, cards) in self.draft.grouped() {
                let total: u32 = cards.iter().map(|(_, n)| n).sum();
                crate::theme::section(ui, group.label(), total);
                for (oracle, n) in cards {
                    changed |= self.deck_row(ui, store, art, oracle, n, false);
                }
                ui.add_space(6.0);
            }
            if !self.draft.side.is_empty() {
                crate::theme::section(ui, "Sideboard", self.draft.side_count());
                for (oracle, n) in self.draft.side.clone() {
                    changed |= self.deck_row(ui, store, art, oracle, n, true);
                }
            }
            if self.draft.main.is_empty() && self.draft.commander.is_none() {
                crate::theme::surface().show(ui, |ui| {
                    ui.label(RichText::new("Start with a card you love.").strong());
                    ui.label(
                        RichText::new("Search the gallery and use + Add to build your deck.")
                            .color(crate::theme::MUTED),
                    );
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(
                            "Hover to read a card. Choose Printing to find your favorite artwork.",
                        )
                        .small()
                        .color(crate::theme::MUTED),
                    );
                });
            }
        });
        if changed {
            self.problems = None;
        }

        self.sample_window(ui, art);
    }

    /// One line of the list: count, −/+, name (hover for the card), cost. Returns whether the
    /// deck changed.
    fn deck_row(
        &mut self,
        ui: &mut Ui,
        store: &Store,
        art: &mut CardArt,
        oracle: u32,
        n: u32,
        side: bool,
    ) -> bool {
        let mut changed = false;
        ui.horizontal(|ui| {
            if ui
                .small_button("−")
                .on_hover_text("Remove one copy")
                .clicked()
            {
                self.draft.remove(oracle, side);
                changed = true;
            }
            ui.label(RichText::new(format!("{n}")).strong());
            if ui.small_button("+").on_hover_text("Add one copy").clicked() {
                self.draft.add(store, oracle, side);
                changed = true;
            }
            let name = self.draft.name_of(oracle).to_string();
            let name_width = (ui.available_width() - 115.0).max(60.0);
            let response = ui.add_sized(
                Vec2::new(name_width, ui.text_style_height(&egui::TextStyle::Body)),
                egui::Label::new(name)
                    .truncate()
                    .sense(egui::Sense::click()),
            );
            let response = self.hover_card(response, store, art, oracle);
            if self.support.get(&oracle).is_some_and(|s| !s.ready()) {
                ui.label(RichText::new("!").color(WARN))
                    .on_hover_ui(|ui| self.support[&oracle].explain(ui));
            }
            let printing = ui.small_button("Art");
            let printing = self.hover_card(printing, store, art, oracle);
            if printing.clicked() {
                self.choose_printing(ui, store, oracle);
            }
            response.context_menu(|ui| {
                if ui.button("Choose printing").clicked() {
                    self.choose_printing(ui, store, oracle);
                    ui.close();
                }
                let (label, to_side) = if side {
                    ("Move one to main deck", false)
                } else {
                    ("Move one to sideboard", true)
                };
                if ui.button(label).clicked() {
                    self.draft.move_one(oracle, to_side);
                    changed = true;
                    ui.close();
                }
                if ui.button("Set as commander").clicked() {
                    self.draft.set_commander(store, Some(oracle));
                    self.format = Format::Commander;
                    changed = true;
                    ui.close();
                }
                if ui.button("Remove all").clicked() {
                    self.draft.remove_all(oracle, side);
                    changed = true;
                    ui.close();
                }
            });
            if let Some(cost) = self.draft.cards.get(&oracle).map(|c| c.cost.clone())
                && !cost.is_empty()
            {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    widgets::mana_label(ui, art, &cost, 12.0, ui.visuals().text_color());
                });
            }
        });
        changed
    }

    fn hover_card(
        &self,
        response: egui::Response,
        store: &Store,
        art: &mut CardArt,
        oracle: u32,
    ) -> egui::Response {
        response.on_hover_ui(|ui| {
            let card = self
                .draft
                .cards
                .get(&oracle)
                .cloned()
                .or_else(|| DraftCard::load(store, oracle));
            if let Some(card) = card {
                card_preview_printing(
                    ui,
                    art,
                    &card,
                    self.draft.printings.get(&oracle).map(String::as_str),
                );
            }
            if let Some(support) = self.support.get(&oracle) {
                support.explain(ui);
            }
        })
    }

    fn check(&self, store: &Store) -> Vec<String> {
        let mut ids: Vec<u32> = self
            .draft
            .main
            .keys()
            .chain(self.draft.side.keys())
            .copied()
            .chain(self.draft.commander)
            .collect();
        ids.sort_unstable();
        ids.dedup();
        let mut problems = Vec::new();
        for oracle in ids {
            match store.card(oracle) {
                Ok(Some(_)) => {}
                Ok(None) => problems.push(format!(
                    "{} (card #{oracle}) is missing from the database",
                    self.draft.name_of(oracle)
                )),
                Err(error) => problems.push(format!(
                    "Could not read {} (card #{oracle}): {error}",
                    self.draft.name_of(oracle)
                )),
            }
        }
        if !problems.is_empty() {
            return problems;
        }
        let subtypes = match store.subtypes() {
            Ok(names) => names,
            Err(error) => return vec![format!("Could not read card types: {error}")],
        };
        let spec = match self.draft.spec(store) {
            Ok(spec) => spec,
            Err(error) => return vec![error],
        };
        let source = BuilderSource { store, subtypes };
        match GameCards::build([&spec], &source) {
            Ok(cards) => deck_problems(self.format, &spec, &cards),
            Err(missing) => vec![format!("{} cards could not be loaded", missing.len())],
        }
    }

    /// A sample opening hand, and more draws on request.
    fn sample_window(&mut self, ui: &mut Ui, art: &mut CardArt) {
        self.refresh_sample();
        let Some(sample) = &mut self.sample else {
            return;
        };
        let mut open = true;
        let mut redraw = false;
        let mut mulligan = false;
        egui::Window::new("Sample hand")
            .open(&mut open)
            .default_width(900.0)
            .vscroll(true)
            .show(ui.ctx(), |ui| {
                if !sample.kept {
                    ui.add_enabled(
                        sample.mulligans == 0,
                        egui::Checkbox::new(
                            &mut sample.first_free,
                            "First mulligan free (multiplayer/Brawl)",
                        ),
                    );
                    let required = sample.bottom_count();
                    if required > 0 {
                        ui.label(format!(
                            "Choose {required} cards to put on the bottom: {} selected",
                            sample.bottom.len()
                        ));
                        ui.label(
                            RichText::new(
                                "Selected cards go below the remaining library, in click order.",
                            )
                            .weak()
                            .small(),
                        );
                    }
                }
                ui.horizontal_wrapped(|ui| {
                    for (index, oracle) in sample.hand.clone().into_iter().enumerate() {
                        let name = self.draft.name_of(oracle).to_string();
                        let artwork = self
                            .draft
                            .printings
                            .get(&oracle)
                            .map_or_else(|| name.clone(), |id| format!("scryfall:{id}"));
                        let response = card_tile_with_art(ui, art, &name, &artwork, TILE * 0.9, 0);
                        if let Some(position) = sample.bottom.iter().position(|i| *i == index) {
                            ui.painter().rect_stroke(
                                response.rect,
                                6.0,
                                egui::Stroke::new(3.0, WARN),
                                egui::StrokeKind::Inside,
                            );
                            let band = egui::Rect::from_min_max(
                                response.rect.left_bottom() + Vec2::new(3.0, -29.0),
                                response.rect.right_bottom() - Vec2::new(3.0, 3.0),
                            );
                            ui.painter()
                                .rect_filled(band, 3.0, Color32::from_black_alpha(220));
                            ui.painter().text(
                                response.rect.center_bottom() - Vec2::new(0.0, 16.0),
                                egui::Align2::CENTER_CENTER,
                                format!("Bottom {}", position + 1),
                                egui::FontId::proportional(15.0),
                                Color32::WHITE,
                            );
                        }
                        if response.clicked() {
                            sample.toggle_bottom(index);
                        }
                        response.on_hover_ui(|ui| {
                            if let Some(card) = self.draft.cards.get(&oracle) {
                                card_preview_printing(
                                    ui,
                                    art,
                                    card,
                                    self.draft.printings.get(&oracle).map(String::as_str),
                                );
                            }
                        });
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("New hand").clicked() {
                        redraw = true;
                    }
                    if ui
                        .add_enabled(
                            sample.kept && !sample.library.is_empty(),
                            egui::Button::new("Draw a card"),
                        )
                        .clicked()
                    {
                        sample.draw();
                    }
                    if !sample.kept {
                        let required = sample.bottom_count();
                        if ui
                            .add_enabled(
                                sample.bottom.len() == required,
                                egui::Button::new(format!("Keep {}", sample.hand.len() - required)),
                            )
                            .clicked()
                        {
                            sample.keep();
                        }
                        let next_bottom =
                            (sample.mulligans + 1).saturating_sub(usize::from(sample.first_free));
                        if ui
                            .add_enabled(
                                required < sample.hand.len(),
                                egui::Button::new(format!(
                                    "Mulligan (keep {})",
                                    sample.hand.len().saturating_sub(next_bottom)
                                )),
                            )
                            .clicked()
                        {
                            mulligan = true;
                        }
                    }
                    let lands = sample
                        .hand
                        .iter()
                        .filter(|o| self.draft.cards.get(o).is_some_and(DraftCard::is_land))
                        .count();
                    ui.label(
                        RichText::new(format!(
                            "{} cards · {lands} lands · {} left in library",
                            sample.hand.len(),
                            sample.library.len()
                        ))
                        .weak(),
                    );
                });
            });
        if !open {
            self.sample = None;
        } else if redraw {
            let first_free = sample.first_free;
            match self.draft.shuffled(&Seed::random()) {
                Ok(cards) => self.sample = Some(SampleHand::new(cards, first_free)),
                Err(error) => self.status = Some((error, false)),
            }
        } else if mulligan {
            match self.draft.shuffled(&Seed::random()) {
                Ok(cards) => {
                    sample.mulligan(cards);
                }
                Err(error) => self.status = Some((error, false)),
            }
        }
    }

    fn refresh_sample(&mut self) {
        if self.sample.is_some() && self.sample_contents.as_ref() != Some(&self.draft.contents()) {
            self.sample = None;
            self.sample_contents = None;
            self.status = Some(("Deck changed — generate a new sample hand".into(), true));
        }
    }
}

/// A card as a tile: its image when it has arrived, its name on a panel until then, and a
/// badge with how many the deck has.
fn card_tile_with_art(
    ui: &mut Ui,
    art: &mut CardArt,
    name: &str,
    artwork: &str,
    size: Vec2,
    count: u64,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let painter = ui.painter();
    painter.rect_filled(
        rect.translate(Vec2::new(2.0, 4.0)),
        8.0,
        Color32::from_black_alpha(90),
    );
    match art.get(ui.ctx(), artwork) {
        Some(texture) => {
            let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            painter.add(
                egui::epaint::RectShape::filled(rect, 6.0, Color32::WHITE)
                    .with_texture(texture.id(), uv),
            );
        }
        None => {
            // Until the image arrives: the name, wrapped to the tile.
            painter.rect_filled(rect, 6.0, crate::theme::SURFACE);
            let galley = painter.layout(
                name.to_string(),
                egui::FontId::proportional(12.0),
                Color32::WHITE,
                rect.width() - 12.0,
            );
            painter.galley(
                rect.left_top() + Vec2::new(6.0, 8.0),
                galley,
                Color32::WHITE,
            );
        }
    }
    painter.rect_stroke(
        rect,
        6.0,
        egui::Stroke::new(1.0, crate::theme::BORDER),
        egui::StrokeKind::Inside,
    );
    if response.hovered() {
        painter.rect_stroke(
            rect,
            6.0,
            egui::Stroke::new(2.0, ACCENT),
            egui::StrokeKind::Outside,
        );
    }
    if count > 0 {
        let badge = egui::Rect::from_min_size(
            rect.right_top() + Vec2::new(-30.0, 6.0),
            Vec2::new(24.0, 20.0),
        );
        painter.rect_filled(badge, 4.0, ACCENT);
        painter.text(
            badge.center(),
            egui::Align2::CENTER_CENTER,
            format!("{count}"),
            egui::FontId::proportional(13.0),
            Color32::BLACK,
        );
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The card large, with its type line — what hovering anywhere in the builder shows.
fn card_preview_printing(ui: &mut Ui, art: &mut CardArt, card: &DraftCard, printing: Option<&str>) {
    widgets::offer_printing_enlargement(ui, &card.name, printing);
    let artwork = printing.map_or_else(|| card.name.clone(), |id| format!("scryfall:{id}"));
    match art.get(ui.ctx(), &artwork) {
        Some(texture) => {
            ui.add(
                egui::Image::new(texture)
                    .fit_to_exact_size(Vec2::new(300.0, 300.0 * 680.0 / 488.0)),
            );
        }
        None => {
            ui.label(RichText::new(&card.name).strong());
            ui.label(RichText::new(&card.type_line).weak());
        }
    }
}

/// The mana curve as bars: 0 to 6, then 7+.
fn curve_chart(ui: &mut Ui, curve: &[u64; 8]) {
    let max = (*curve.iter().max().unwrap_or(&0)).max(1) as f32;
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width().min(320.0), 70.0),
        egui::Sense::hover(),
    );
    let painter = ui.painter();
    let slot = rect.width() / 8.0;
    for (i, n) in curve.iter().enumerate() {
        let h = (*n as f32 / max) * (rect.height() - 28.0);
        let x = rect.left() + slot * i as f32;
        let bar = egui::Rect::from_min_max(
            egui::pos2(x + 4.0, rect.bottom() - 14.0 - h),
            egui::pos2(x + slot - 4.0, rect.bottom() - 14.0),
        );
        painter.rect_filled(bar, 2.0, ACCENT);
        let label = if i == 7 {
            "7+".to_string()
        } else {
            i.to_string()
        };
        painter.text(
            egui::pos2(x + slot / 2.0, rect.bottom() - 6.0),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(11.0),
            Color32::GRAY,
        );
        if *n > 0 {
            painter.text(
                bar.center_top() - Vec2::new(0.0, 7.0),
                egui::Align2::CENTER_CENTER,
                n.to_string(),
                egui::FontId::proportional(11.0),
                Color32::WHITE,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtg_store::{FaceRow, StoredCard};

    /// A store with three invented cards: a land, a 2-drop creature, a 5-mana sorcery.
    fn store() -> (Store, [u32; 3]) {
        let store = Store::in_memory().unwrap();
        let mut ids = [0; 3];
        for (i, (name, type_line, cost, identity)) in [
            ("Quiet Field", "Basic Land — Plains", "", ""),
            ("Stone Bear", "Creature — Bear", "{1}{G}", "G"),
            ("Big Idea", "Sorcery", "{3}{U}{U}", "U"),
        ]
        .into_iter()
        .enumerate()
        {
            let card = StoredCard {
                oracle_uuid: format!("u{i}"),
                name: name.into(),
                layout: "normal".into(),
                faces: vec![FaceRow {
                    name: name.into(),
                    type_line: type_line.into(),
                    mana_cost: cost.into(),
                    ..Default::default()
                }],
            };
            ids[i] = store.put_card(&card, &[]).unwrap();
            store.set_color_identity(ids[i], identity).unwrap();
        }
        (store, ids)
    }

    #[test]
    fn adding_and_removing_counts_copies() {
        let (store, [land, bear, _]) = store();
        let mut d = Draft::new("t");
        for _ in 0..3 {
            d.add(&store, bear, false);
        }
        d.add(&store, land, false);
        d.remove(bear, false);
        assert_eq!(d.main.get(&bear), Some(&2));
        assert_eq!(d.main_count(), 3);
        d.remove(land, false);
        assert!(
            !d.main.contains_key(&land),
            "the last copy removes the line"
        );
    }

    #[test]
    fn failed_card_search_clears_stale_results_and_recovers_on_retry() {
        let (store, _) = store();
        let mut builder = Builder::new(Draft::new("Search"));
        builder.query.legal_in = None;
        builder.load_search(&store);
        assert_eq!(builder.results.len(), 3);
        store
            .conn()
            .execute_batch("ALTER TABLE cards RENAME TO unavailable_cards")
            .unwrap();
        builder.has_more = true;
        builder.load_search(&store);
        assert!(builder.search_error.is_some());
        assert!(builder.results.is_empty());
        assert!(!builder.has_more);
        assert_eq!(
            builder.searched,
            Some((builder.query.clone(), builder.page))
        );
        store
            .conn()
            .execute_batch("ALTER TABLE unavailable_cards RENAME TO cards")
            .unwrap();
        builder.load_search(&store);
        assert!(builder.search_error.is_none());
        assert_eq!(builder.results.len(), 3);
    }

    #[test]
    fn removing_zero_count_saved_rows_preserves_counts_and_redo() {
        let (mut store, [land, bear, _]) = store();
        let id = store
            .put_deck(
                "Empty rows",
                None,
                &[(land, 0, Section::Main), (land, 0, Section::Sideboard)],
            )
            .unwrap();
        let mut draft = Draft::load(&store, id).unwrap();
        draft.add(&store, bear, false);
        assert!(draft.undo());
        assert!(!draft.dirty);
        let before = draft.contents();
        draft.remove(land, false);
        draft.remove(land, true);
        assert_eq!(draft.contents(), before);
        assert!(!draft.dirty);
        assert!(!draft.can_undo());
        assert!(draft.redo());
        assert_eq!(draft.main.get(&bear), Some(&1));
    }

    #[test]
    fn full_rows_reject_additions_and_moves_without_changing_history() {
        let (store, [land, _, _]) = store();
        for to_side in [false, true] {
            let mut draft = Draft::new("Full row");
            if to_side {
                draft.main.insert(land, 1);
                draft.side.insert(land, u32::MAX);
            } else {
                draft.main.insert(land, u32::MAX);
                draft.side.insert(land, 1);
            }
            let before = draft.contents();
            draft.add(&store, land, to_side);
            draft.move_one(land, to_side);
            assert_eq!(draft.contents(), before);
            assert!(!draft.can_undo());
            draft.remove(land, to_side);
            draft.move_one(land, to_side);
            assert_eq!(
                (if to_side { &draft.side } else { &draft.main }).get(&land),
                Some(&u32::MAX)
            );
            assert!((if to_side { &draft.main } else { &draft.side }).is_empty());
            assert!(draft.undo());
            assert!(draft.undo());
            assert_eq!(draft.contents(), before);
        }
    }

    #[test]
    fn validation_uses_the_active_store_without_reopening_the_default_database() {
        let (store, [land, bear, _]) = store();
        let mut draft = Draft::new("Local deck");
        for _ in 0..56 {
            draft.add(&store, land, false);
        }
        for _ in 0..4 {
            draft.add(&store, bear, false);
        }
        let builder = Builder::new(draft);
        assert!(builder.check(&store).is_empty());
    }

    #[test]
    fn validation_reports_structure_errors_from_the_active_cards() {
        let (store, [_, bear, _]) = store();
        let mut draft = Draft::new("Too small");
        for _ in 0..5 {
            draft.add(&store, bear, false);
        }
        for _ in 0..16 {
            draft.add(&store, bear, true);
        }
        let problems = Builder::new(draft).check(&store);
        assert!(problems.iter().any(|p| p.contains("needs at least 60")));
        assert!(problems.iter().any(|p| p.contains("sideboard has 16")));
        assert!(
            problems
                .iter()
                .any(|p| p.contains("21 copies of Stone Bear across the main deck and sideboard"))
        );
        assert!(!problems.iter().any(|p| p.contains("could not be loaded")));
    }

    #[test]
    fn commander_validation_uses_local_colour_identity() {
        let (store, [land, _, blue_spell]) = store();
        let commander = store
            .put_card(
                &StoredCard {
                    oracle_uuid: "invented-local-commander".into(),
                    name: "Green Captain".into(),
                    layout: "normal".into(),
                    faces: vec![FaceRow {
                        name: "Green Captain".into(),
                        type_line: "Legendary Creature".into(),
                        mana_cost: "{G}".into(),
                        power: Some("2".into()),
                        toughness: Some("2".into()),
                        ..Default::default()
                    }],
                },
                &[],
            )
            .unwrap();
        store.set_color_identity(commander, "G").unwrap();
        let mut draft = Draft::new("Commander");
        for _ in 0..98 {
            draft.add(&store, land, false);
        }
        draft.add(&store, blue_spell, false);
        draft.set_commander(&store, Some(commander));
        let problems = Builder::new(draft).check(&store);
        assert_eq!(
            problems,
            vec!["Big Idea is outside the commander's colours"]
        );
    }

    #[test]
    fn missing_entries_are_reported_before_spec_can_silently_drop_them() {
        let (store, _) = store();
        let mut draft = Draft::new("Missing cards");
        draft.main.insert(u32::MAX, 60);
        draft.side.insert(u32::MAX - 1, 1);
        draft.commander = Some(u32::MAX);
        let problems = Builder::new(draft).check(&store);
        assert_eq!(
            problems.len(),
            2,
            "one problem per missing card, across every section"
        );
        assert!(
            problems
                .iter()
                .all(|p| p.contains("missing from the database"))
        );
        assert!(problems.iter().any(|p| p.contains(&u32::MAX.to_string())));
        assert!(
            problems
                .iter()
                .any(|p| p.contains(&(u32::MAX - 1).to_string()))
        );
    }

    #[test]
    fn support_labels_agree_with_the_whole_card_gate() {
        let (store, cards) = store();
        let subtypes = mtg_oracle::compile::SubtypeNames(store.subtypes().unwrap());
        for oracle in cards {
            let card = store.card(oracle).unwrap().unwrap();
            assert_eq!(
                CardSupport::load(&store, oracle).ready(),
                mtg_oracle::compile::card_understood(&card, &subtypes)
            );
            assert!(CardSupport::load(&store, oracle).ready());
        }
        assert!(!CardSupport::load(&store, u32::MAX).ready());
    }

    #[test]
    fn support_explains_unsupported_text_on_the_other_face() {
        let store = Store::in_memory().unwrap();
        let card = StoredCard {
            oracle_uuid: "invented-support-modal".into(),
            name: "One // Two".into(),
            layout: "modal_dfc".into(),
            faces: vec![
                FaceRow {
                    name: "One".into(),
                    type_line: "Sorcery".into(),
                    oracle_text: Some("You gain 2 life.".into()),
                    ..Default::default()
                },
                FaceRow {
                    name: "Two".into(),
                    type_line: "Sorcery".into(),
                    oracle_text: Some("Invent an impossible instruction.".into()),
                    ..Default::default()
                },
            ],
        };
        let oracle = store.put_card(&card, &[]).unwrap();
        let support = CardSupport::load(&store, oracle);
        assert!(!support.ready());
        assert!(
            support
                .issues
                .iter()
                .any(|s| s.contains("Two:") && s.contains("impossible instruction"))
        );
        assert!(!support.issues.iter().any(|s| s.starts_with("One:")));
    }

    #[test]
    fn supported_text_does_not_hide_an_unsupported_layout() {
        let store = Store::in_memory().unwrap();
        let card = StoredCard {
            oracle_uuid: "invented-support-layout".into(),
            name: "Unusual".into(),
            layout: "invented_layout".into(),
            faces: vec![FaceRow {
                name: "Unusual".into(),
                type_line: "Creature".into(),
                oracle_text: Some("Reach".into()),
                ..Default::default()
            }],
        };
        let oracle = store.put_card(&card, &[]).unwrap();
        let support = CardSupport::load(&store, oracle);
        assert!(!support.ready());
        assert!(support.issues.iter().any(|s| s.contains("face layout")));
    }

    #[test]
    fn cards_move_between_main_and_sideboard_one_at_a_time() {
        let (store, [_, bear, _]) = store();
        let mut d = Draft::new("t");
        d.add(&store, bear, false);
        d.add(&store, bear, false);
        d.move_one(bear, true);
        assert_eq!((d.main[&bear], d.side[&bear]), (1, 1));
        d.move_one(bear, false);
        assert_eq!(d.main[&bear], 2);
        assert!(!d.side.contains_key(&bear));
    }

    #[test]
    fn history_restores_card_edits_in_order_and_moves_are_atomic() {
        let (store, [_, bear, _]) = store();
        let mut d = Draft::new("t");
        assert!(!d.undo());
        d.add(&store, bear, false);
        d.add(&store, bear, false);
        d.move_one(bear, true);
        assert!(d.undo());
        assert_eq!(d.main.get(&bear), Some(&2));
        assert!(d.side.is_empty());
        assert!(d.redo());
        assert_eq!((d.main[&bear], d.side[&bear]), (1, 1));
        d.remove(bear, true);
        assert!(d.undo());
        assert_eq!(d.side[&bear], 1);
        for _ in 0..3 {
            assert!(d.undo());
        }
        assert!(d.main.is_empty());
        assert!(d.side.is_empty());
        assert!(!d.undo());
    }

    #[test]
    fn remove_all_and_commander_changes_are_single_reversible_edits() {
        let (store, [_, bear, idea]) = store();
        let mut d = Draft::new("t");
        for _ in 0..4 {
            d.add(&store, bear, false);
        }
        d.remove_all(bear, false);
        assert!(d.undo());
        assert_eq!(d.main[&bear], 4);
        d.set_commander(&store, Some(bear));
        assert!(d.main.is_empty());
        assert!(d.undo());
        assert_eq!(d.main[&bear], 4);
        assert_eq!(d.commander, None);
        assert!(d.redo());
        d.set_commander(&store, Some(idea));
        assert!(d.undo());
        assert_eq!(d.commander, Some(bear));
        d.set_commander(&store, None);
        assert!(d.undo());
        assert_eq!(d.commander, Some(bear));
    }

    #[test]
    fn new_edits_clear_redo_but_noop_edits_do_not() {
        let (store, [land, bear, _]) = store();
        let mut d = Draft::new("t");
        d.add(&store, bear, false);
        d.undo();
        d.remove(land, false);
        d.remove_all(land, true);
        d.move_one(land, true);
        d.set_commander(&store, None);
        assert!(d.can_redo());
        assert!(!d.can_undo());
        d.add(&store, land, false);
        assert!(!d.can_redo());
        assert!(!d.redo());
    }

    #[test]
    fn history_preserves_saved_identity_and_tracks_the_save_point() {
        let (mut store, [land, bear, _]) = store();
        let mut d = Draft::new("  My deck  ");
        d.add(&store, bear, false);
        let id = d.save(&mut store).unwrap();
        assert_eq!(d.name, "My deck");
        d.add(&store, land, true);
        assert!(d.dirty);
        assert!(d.undo());
        assert!(!d.dirty);
        assert_eq!(d.id, Some(id));
        assert!(d.redo());
        assert!(d.dirty);
        // Undo never writes to the database or reverts the saved deck identity.
        let loaded = Draft::load(&store, id).unwrap();
        assert!(loaded.side.is_empty());
        assert!(!loaded.can_undo());
        d.save(&mut store).unwrap();
        d.undo();
        assert!(d.dirty);
        d.redo();
        assert!(!d.dirty);
    }

    #[test]
    fn card_history_keeps_renames_and_checks_them_against_the_saved_name() {
        let (mut store, [land, bear, _]) = store();
        let mut d = Draft::new("Original");
        d.add(&store, bear, false);
        let id = d.save(&mut store).unwrap();
        d.add(&store, land, false);
        d.name = "Renamed".into();
        d.undo();
        assert_eq!(d.name, "Renamed");
        assert!(d.dirty);
        assert_eq!(d.id, Some(id));
        d.name = "Original".into();
        d.refresh_dirty();
        assert!(!d.dirty);
    }

    #[test]
    fn history_is_bounded_and_redo_restores_the_retained_edits() {
        let (store, [_, bear, _]) = store();
        let mut d = Draft::new("t");
        for _ in 0..HISTORY_LIMIT + 5 {
            d.add(&store, bear, false);
        }
        for _ in 0..HISTORY_LIMIT {
            assert!(d.undo());
        }
        assert!(!d.undo());
        assert_eq!(d.main[&bear], 5);
        for _ in 0..HISTORY_LIMIT {
            assert!(d.redo());
        }
        assert!(!d.redo());
        assert_eq!(d.main[&bear], (HISTORY_LIMIT + 5) as u32);
    }

    #[test]
    fn deck_aggregates_preserve_counts_above_u32_max() {
        let (store, [land, bear, idea]) = store();
        let mut draft = Draft::new("Large totals");
        for oracle in [land, bear, idea] {
            draft.know(&store, oracle);
        }
        draft.cards.get_mut(&idea).unwrap().mana_value = 2;
        draft.main.insert(bear, u32::MAX);
        draft.main.insert(idea, u32::MAX);
        draft.main.insert(land, 1);
        draft.side.insert(bear, u32::MAX);
        draft.side.insert(land, 2);
        draft.commander = Some(bear);
        let twice_max = 2 * u64::from(u32::MAX);
        assert_eq!(draft.main_count(), twice_max + 1);
        assert_eq!(draft.side_count(), u64::from(u32::MAX) + 2);
        assert_eq!(draft.curve(), [0, 0, twice_max, 0, 0, 0, 0, 0]);
        assert_eq!(Builder::new(draft).count_of(bear), twice_max + 1);
    }

    #[test]
    fn the_curve_counts_spells_by_mana_value_and_skips_lands() {
        let (store, [land, bear, idea]) = store();
        let mut d = Draft::new("t");
        for _ in 0..4 {
            d.add(&store, bear, false);
        }
        d.add(&store, idea, false);
        for _ in 0..10 {
            d.add(&store, land, false);
        }
        assert_eq!(d.curve(), [0, 0, 4, 0, 0, 1, 0, 0]);
    }

    #[test]
    fn the_list_is_grouped_by_type_with_lands_last() {
        let (store, [land, bear, idea]) = store();
        let mut d = Draft::new("t");
        d.add(&store, land, false);
        d.add(&store, idea, false);
        d.add(&store, bear, false);
        let groups: Vec<Group> = d.grouped().into_iter().map(|(g, _)| g).collect();
        assert_eq!(
            groups,
            vec![Group::Creatures, Group::Sorceries, Group::Lands]
        );
        assert_eq!(d.colors(), "UG");
    }

    #[test]
    fn a_commander_leaves_the_ninety_nine() {
        let (store, [_, bear, _]) = store();
        let mut d = Draft::new("t");
        d.add(&store, bear, false);
        d.set_commander(&store, Some(bear));
        assert_eq!(d.commander, Some(bear));
        assert!(d.main.is_empty());
    }

    #[test]
    fn saving_and_loading_round_trips_and_a_rename_replaces() {
        let (mut store, [land, bear, _]) = store();
        let mut d = Draft::new("First");
        d.add(&store, bear, false);
        d.add(&store, land, true);
        let id = d.save(&mut store).unwrap();
        assert!(!d.dirty);

        let loaded = Draft::load(&store, id).unwrap();
        assert_eq!(loaded.main, d.main);
        assert_eq!(loaded.side, d.side);

        d.name = "Second".into();
        d.save(&mut store).unwrap();
        let names: Vec<String> = store
            .deck_list()
            .unwrap()
            .into_iter()
            .map(|(_, n)| n)
            .collect();
        assert_eq!(names, vec!["Second".to_string()], "renamed, not duplicated");
    }

    #[test]
    fn a_failed_rename_save_preserves_the_original_and_draft() {
        let (mut store, [land, _, _]) = store();
        let mut draft = Draft::new("Original");
        draft.add(&store, land, false);
        let id = draft.save(&mut store).unwrap();
        let original = store.deck(id).unwrap().unwrap();
        draft.name = "Renamed".into();
        draft.refresh_dirty();
        // Inject a real SQLite write failure after the replacement has started.
        store
            .conn()
            .execute_batch(
                "CREATE TEMP TRIGGER reject_deck_entry BEFORE INSERT ON deck_entries
             BEGIN SELECT RAISE(ABORT, 'test write failure'); END;",
            )
            .unwrap();
        assert!(draft.save(&mut store).is_err());
        assert_eq!(store.deck(id).unwrap(), Some(original));
        assert_eq!(store.deck_list().unwrap(), vec![(id, "Original".into())]);
        assert_eq!(draft.id, Some(id));
        assert_eq!(draft.name, "Renamed");
        assert!(draft.dirty);
        store
            .conn()
            .execute_batch("DROP TRIGGER reject_deck_entry")
            .unwrap();
        let saved = draft.save(&mut store).unwrap();
        assert_eq!(store.deck_list().unwrap(), vec![(saved, "Renamed".into())]);
        assert!(!draft.dirty);
    }

    #[test]
    fn loaded_decks_merge_duplicate_rows_without_losing_counts_or_commanders() {
        let (mut store, [land, bear, _]) = store();
        let id = store
            .put_deck(
                "Duplicates",
                None,
                &[
                    (land, 2, Section::Main),
                    (land, 3, Section::Main),
                    (bear, 1, Section::Sideboard),
                    (bear, 2, Section::Sideboard),
                ],
            )
            .unwrap();
        let draft = Draft::load(&store, id).unwrap();
        assert_eq!(draft.main.get(&land), Some(&5));
        assert_eq!(draft.side.get(&bear), Some(&3));

        for entries in [
            vec![(land, u32::MAX, Section::Main), (land, 1, Section::Main)],
            vec![(land, 2, Section::Commander)],
            vec![(land, 1, Section::Commander), (bear, 1, Section::Commander)],
        ] {
            let id = store.put_deck("Invalid", None, &entries).unwrap();
            assert!(Draft::load(&store, id).is_err());
            assert_eq!(store.deck(id).unwrap().unwrap().entries, entries);
        }
    }

    #[test]
    fn sample_shuffling_rejects_large_libraries_before_expansion() {
        let mut draft = Draft::new("Large library");
        let seed = Seed([3; 32]);
        draft.main.insert(1, u32::MAX);
        assert!(draft.shuffled(&seed).unwrap_err().contains("4294967295"));
        draft.main.insert(1, MAX_SAMPLE_CARDS as u32);
        draft.side.insert(2, u32::MAX);
        let cards = draft.shuffled(&seed).unwrap();
        assert_eq!(cards.len(), MAX_SAMPLE_CARDS as usize);
        assert!(cards.iter().all(|card| *card == 1));
        draft.main.insert(3, 1);
        assert!(draft.shuffled(&seed).is_err());
    }

    #[test]
    fn a_sample_hand_is_drawn_from_the_main_deck_only() {
        let (store, [land, bear, idea]) = store();
        let mut d = Draft::new("t");
        for _ in 0..20 {
            d.add(&store, land, false);
        }
        d.add(&store, bear, false);
        d.add(&store, idea, true);
        let shuffled = d.shuffled(&Seed([3; 32])).unwrap();
        assert_eq!(shuffled.len(), 21);
        assert!(
            !shuffled.contains(&idea),
            "the sideboard is not shuffled in"
        );
        assert_ne!(
            d.shuffled(&Seed([3; 32])),
            d.shuffled(&Seed([4; 32])),
            "different seeds, different hands"
        );
    }

    #[test]
    fn london_mulligan_draws_seven_then_bottoms_selected_copies_in_order() {
        let deck: Vec<u32> = (0..10).collect();
        let mut sample = SampleHand::new(deck.clone(), false);
        assert!(!sample.draw());
        assert!(sample.mulligan(deck.clone()));
        assert!(sample.mulligan(deck));
        assert_eq!(sample.hand.len(), 7);
        assert_eq!(sample.bottom_count(), 2);
        assert!(!sample.keep());
        sample.toggle_bottom(5);
        sample.toggle_bottom(1);
        sample.toggle_bottom(3); // Cannot select more than required.
        assert_eq!(sample.bottom, vec![5, 1]);
        assert!(sample.keep());
        assert_eq!(sample.hand, vec![0, 2, 3, 4, 6]);
        assert_eq!(
            sample.library.iter().copied().collect::<Vec<_>>(),
            vec![7, 8, 9, 5, 1]
        );
        assert!(!sample.mulligan((0..10).collect()));
        assert!(!sample.keep());
        for _ in 0..5 {
            assert!(sample.draw());
        }
        assert!(!sample.draw());
        assert_eq!(sample.hand, vec![0, 2, 3, 4, 6, 7, 8, 9, 5, 1]);
    }

    #[test]
    fn duplicate_cards_are_selected_by_copy_and_redraw_clears_selections() {
        let mut sample = SampleHand::new(vec![1; 10], false);
        sample.mulligan(vec![1; 10]);
        sample.toggle_bottom(2);
        sample.toggle_bottom(2);
        assert!(sample.bottom.is_empty());
        sample.toggle_bottom(5);
        sample.mulligan(vec![1; 10]);
        assert!(sample.bottom.is_empty());
        assert_eq!(sample.bottom_count(), 2);
        sample.toggle_bottom(1);
        sample.toggle_bottom(5);
        sample.toggle_bottom(100);
        assert!(sample.keep());
        assert_eq!(sample.hand.len(), 5);
        assert_eq!(sample.library.len(), 5);
    }

    #[test]
    fn mulligans_stop_at_zero_with_an_optional_free_first_mulligan() {
        for first_free in [false, true] {
            let mut sample = SampleHand::new((0..10).collect(), first_free);
            let limit = 7 + usize::from(first_free);
            for taken in 1..=limit {
                assert!(sample.mulligan((0..10).collect()));
                assert_eq!(
                    sample.bottom_count(),
                    taken.saturating_sub(usize::from(first_free))
                );
            }
            assert!(!sample.mulligan((0..10).collect()));
            for index in 0..7 {
                sample.toggle_bottom(index);
            }
            assert!(sample.keep());
            assert!(sample.hand.is_empty());
            assert_eq!(sample.library.len(), 10);
        }
    }

    #[test]
    fn keeping_the_initial_hand_preserves_the_top_of_the_library() {
        let mut sample = SampleHand::new((0..9).collect(), false);
        sample.toggle_bottom(0);
        assert!(sample.bottom.is_empty());
        assert!(sample.keep());
        assert!(sample.draw());
        assert_eq!(sample.hand, (0..8).collect::<Vec<_>>());
        let mut empty = SampleHand::new(Vec::new(), false);
        assert!(!empty.mulligan(Vec::new()));
        assert!(empty.keep());
        assert!(!empty.draw());
    }

    #[test]
    fn editing_composition_invalidates_the_sample_but_renaming_does_not() {
        let (store, [land, _, _]) = store();
        let mut draft = Draft::new("Practice");
        for _ in 0..10 {
            draft.add(&store, land, false);
        }
        let mut builder = Builder::new(draft);
        builder.sample_contents = Some(builder.draft.contents());
        builder.sample = Some(SampleHand::new(
            builder.draft.shuffled(&Seed([1; 32])).unwrap(),
            false,
        ));
        builder.draft.name = "New name".into();
        builder.refresh_sample();
        assert!(builder.sample.is_some());
        builder.draft.remove(land, false);
        builder.refresh_sample();
        assert!(builder.sample.is_none());
    }

    #[test]
    fn a_deck_exported_as_text_imports_back_the_same() {
        let (mut store, [land, bear, idea]) = store();
        let mut d = Draft::new("Round trip");
        for _ in 0..4 {
            d.add(&store, bear, false);
        }
        d.add(&store, land, false);
        d.add(&store, idea, true);
        let text = d.to_text();
        assert!(
            text.contains("4 Stone Bear") && text.contains("Sideboard\n1 Big Idea"),
            "{text}"
        );

        // The store's name index is what the importer resolves against.
        for (id, name) in [
            (land, "Quiet Field"),
            (bear, "Stone Bear"),
            (idea, "Big Idea"),
        ] {
            let card = store.card(id).unwrap().unwrap();
            store
                .put_card(&card, &[mtg_deck::resolve::normalise(name)])
                .unwrap();
        }
        crate::decks::import_deck(&mut store, "Again", &text).unwrap();
        let again = store
            .deck_list()
            .unwrap()
            .into_iter()
            .find(|(_, n)| n == "Again")
            .unwrap()
            .0;
        let loaded = Draft::load(&store, again).unwrap();
        assert_eq!((loaded.main, loaded.side), (d.main, d.side));
    }

    #[test]
    fn selected_printings_survive_save_rename_history_and_portable_decks() {
        let (mut store, [land, bear, _]) = store();
        let mut draft = Draft::new("Artwork");
        draft.add(&store, bear, false);
        draft.add(&store, bear, true);
        draft.set_commander(&store, Some(land));
        let old_id = draft.save(&mut store).unwrap();
        store.put_deck("Other deck", None, &[]).unwrap();
        let printing = "12345678-1234-1234-1234-123456789abc".to_owned();
        draft.set_printing(bear, Some(printing.clone()));
        assert!(draft.dirty);
        assert!(draft.undo());
        assert!(!draft.dirty);
        assert!(!draft.printings.contains_key(&bear));
        assert!(draft.redo());
        draft.set_printing(land, Some(printing.clone()));
        draft.name = "Renamed artwork".into();
        let id = draft.save(&mut store).unwrap();
        assert!(store.deck(old_id).unwrap().is_none());
        assert!(store.deck_printings(old_id).unwrap().is_empty());
        let loaded = Draft::load(&store, id).unwrap();
        assert!(!loaded.dirty);
        assert_eq!(loaded.printings, draft.printings);
        let spec = loaded.spec(&store).unwrap();
        assert_eq!(
            crate::decks::deck_spec(Some(&store), crate::decks::DeckId::Stored(id)).unwrap(),
            spec
        );
        assert_eq!(
            spec.main[0].0,
            CardKey::Oracle("u1".into(), Some(printing.clone()))
        );
        assert_eq!(spec.side[0].0, spec.main[0].0);
        let subtypes = store.subtypes().unwrap();
        let source = BuilderSource {
            store: &store,
            subtypes,
        };
        let game_cards = GameCards::build([&spec], &source).unwrap();
        let texts = crate::cards_text::CardTexts::for_match(&game_cards);
        let card_id = game_cards.id(&spec.main[0].0).unwrap();
        assert_eq!(
            texts.get(card_id).unwrap().printing.as_ref(),
            Some(&printing)
        );
        assert_eq!(
            texts.get(card_id).unwrap().artwork(),
            format!("scryfall:{printing}")
        );
        assert_eq!(
            spec.commander,
            Some(CardKey::Oracle("u0".into(), Some(printing)))
        );
        store.delete_deck(id).unwrap();
        assert!(store.deck_printings(id).unwrap().is_empty());
    }

    #[test]
    fn a_draft_becomes_a_portable_deck() {
        let (store, [_, bear, _]) = store();
        let mut d = Draft::new("t");
        d.add(&store, bear, false);
        let spec = d.spec(&store).unwrap();
        assert_eq!(spec.main, vec![(CardKey::Oracle("u1".into(), None), 1)]);
    }

    #[test]
    fn portable_decks_reject_missing_cards_and_unrepresentable_counts() {
        let (store, [land, _, _]) = store();
        let mut draft = Draft::new("Exact counts");
        draft.main.insert(land, 255);
        assert_eq!(draft.spec(&store).unwrap().main[0].1, 255);
        draft.main.insert(land, 256);
        assert!(draft.spec(&store).unwrap_err().contains("256 copies"));
        let problems = Builder::new(draft.clone()).check(&store);
        assert!(
            problems
                .iter()
                .any(|problem| problem.contains("256 copies"))
        );
        draft.main.clear();
        draft.side.insert(land, 256);
        assert!(draft.spec(&store).unwrap_err().contains("256 copies"));
        draft.side.clear();
        draft.main.insert(u32::MAX, 1);
        assert!(
            draft
                .spec(&store)
                .unwrap_err()
                .contains("missing from the database")
        );
        draft.main.clear();
        draft.commander = Some(u32::MAX);
        assert!(
            draft
                .spec(&store)
                .unwrap_err()
                .contains("missing from the database")
        );
    }

    #[test]
    fn split_cards_use_the_combined_mana_value_in_the_deck_curve() {
        let store = Store::in_memory().unwrap();
        let card = StoredCard {
            oracle_uuid: "invented-curve-split".into(),
            name: "Dawn // Dusk".into(),
            layout: "split".into(),
            faces: vec![
                FaceRow {
                    name: "Dawn".into(),
                    mana_cost: "{1}{U}".into(),
                    type_line: "Instant".into(),
                    ..Default::default()
                },
                FaceRow {
                    name: "Dusk".into(),
                    mana_cost: "{2}{R}".into(),
                    type_line: "Sorcery".into(),
                    ..Default::default()
                },
            ],
        };
        let id = store.put_card(&card, &[]).unwrap();
        let shown = DraftCard::load(&store, id).unwrap();
        assert_eq!(shown.mana_value, 5);
        assert_eq!(shown.cost, "{1}{U}{2}{R}");
        assert!(shown.type_line.contains("Instant"));
        assert!(shown.type_line.contains("Sorcery"));
    }
}
