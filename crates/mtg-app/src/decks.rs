//! Decks as the app handles them: the built-in demo deck, decks saved in the local database,
//! importing a pasted list, and the card source a match is built from.

use mtg_core::{CardId, Subtype};
use mtg_headless::{DemoCards, cards as demo};
use mtg_ir::PrintedCards;
use mtg_oracle::{convert, library::StoreResolver};
use mtg_session::game::{CardKey, CardSource, DeckSpec, SourcedCard};
use mtg_store::{Section, Store};

/// Which deck a player picked.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DeckId {
    /// The invented demo deck, always available.
    Demo,
    /// A deck saved in the local database.
    Stored(i64),
}

/// A 60-card deck of the invented demo cards, so a game can be played before any import.
pub fn demo_deck() -> DeckSpec {
    let key = |c: CardId| CardKey::Demo(c.0);
    DeckSpec {
        name: "Demo deck".into(),
        main: vec![
            // The demo's two lands are basic, so any number is legal; four of each spell.
            (key(demo::PLAINS), 24),
            (key(demo::DUAL), 12),
            (key(demo::DUMMY), 4),
            (key(demo::SENTRY), 4),
            (key(demo::SERGEANT), 4),
            (key(demo::ADDER), 4),
            (key(demo::COLOSSUS), 4),
            (key(demo::BANNER), 4),
        ],
        side: Vec::new(),
        commander: None,
    }
}

/// The playtest opponent's deck: sixty basic lands it never plays. It only has to exist so the
/// opponent has a library to draw from.
pub fn goldfish_deck() -> DeckSpec {
    DeckSpec {
        name: "Goldfish".into(),
        main: vec![(CardKey::Demo(demo::PLAINS.0), 60)],
        ..Default::default()
    }
}

/// The saved decks, newest first, after the demo deck.
pub fn deck_list(store: Option<&Store>) -> Result<Vec<(DeckId, String)>, String> {
    let mut out = vec![(DeckId::Demo, "Demo deck (invented cards)".to_string())];
    if let Some(store) = store {
        let decks = store.deck_list().map_err(|error| error.to_string())?;
        out.extend(
            decks
                .into_iter()
                .map(|(id, name)| (DeckId::Stored(id), name)),
        );
    }
    Ok(out)
}

/// A deck in the portable form a match uses.
pub fn deck_spec(store: Option<&Store>, id: DeckId) -> Result<DeckSpec, String> {
    let DeckId::Stored(id) = id else {
        return Ok(demo_deck());
    };
    let store = store.ok_or("no card database")?;
    let row = store
        .deck(id)
        .map_err(|e| e.to_string())?
        .ok_or("that deck no longer exists")?;

    let uuid = |oracle: u32| -> Result<CardKey, String> {
        let card = store.card(oracle).map_err(|e| e.to_string())?;
        card.map(|c| CardKey::Oracle(c.oracle_uuid, None))
            .ok_or_else(|| format!("card {oracle} is missing from the database"))
    };
    let mut spec = DeckSpec {
        name: row.name,
        ..Default::default()
    };
    for (oracle, count, section) in row.entries {
        if section == Section::Commander && (count != 1 || spec.commander.is_some()) {
            return Err("play currently supports exactly one commander per Commander deck".into());
        }
        let key = uuid(oracle)?;
        let count = u8::try_from(count).map_err(|_| {
            format!(
                "card {oracle} has {count} copies; play currently supports at most 255 per card in each section"
            )
        })?;
        match section {
            Section::Main => spec.main.push((key, count)),
            Section::Sideboard => spec.side.push((key, count)),
            Section::Commander => spec.commander = Some(key),
        }
    }
    Ok(spec)
}

/// What an import did.
#[derive(Clone, Debug, Default)]
pub struct ImportSummary {
    pub main: u32,
    pub side: u32,
    pub commander: bool,
    /// Names that matched no card, with suggestions.
    pub unmatched: Vec<String>,
    /// Lines that could not be read at all.
    pub unreadable: Vec<String>,
}

impl ImportSummary {
    /// Whether every pasted card line was read and resolved.
    pub fn is_complete(&self) -> bool {
        self.unmatched.is_empty() && self.unreadable.is_empty()
    }
}

/// Parse a pasted list (Moxfield, Archidekt, Arena, MTGO or plain text), resolve it against the
/// database and save it under `name`. Anything unmatched is reported, never guessed.
pub fn import_deck(store: &mut Store, name: &str, text: &str) -> Result<ImportSummary, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("give the deck a name".into());
    }
    let parsed = mtg_deck::parse(text);
    if parsed
        .entries
        .iter()
        .any(|entry| entry.section == mtg_deck::Section::Companion)
    {
        return Err("companion designation is not supported yet; the deck was not saved".into());
    }
    let commanders: u64 = parsed
        .entries
        .iter()
        .filter(|entry| entry.section == mtg_deck::Section::Commander)
        .map(|entry| u64::from(entry.count))
        .sum();
    if commanders > 1 {
        return Err("multiple commanders are not supported yet; the deck was not saved".into());
    }
    let resolved = mtg_deck::resolve::resolve(&parsed, &StoreResolver(store));

    let mut summary = ImportSummary {
        unreadable: parsed.unparsed.clone(),
        ..Default::default()
    };
    for failure in &resolved.failed {
        let hints: Vec<&str> = failure
            .suggestions
            .iter()
            .map(|(_, n)| n.as_str())
            .collect();
        summary.unmatched.push(if hints.is_empty() {
            failure.entry.name.clone()
        } else {
            format!(
                "{} (did you mean {}?)",
                failure.entry.name,
                hints.join(", ")
            )
        });
    }

    let mut entries: Vec<(u32, u32, Section)> = Vec::new();
    entries.extend(
        resolved
            .main
            .iter()
            .map(|(id, n)| (id.0, *n, Section::Main)),
    );
    entries.extend(
        resolved
            .sideboard
            .iter()
            .map(|(id, n)| (id.0, *n, Section::Sideboard)),
    );
    entries.extend(
        resolved
            .commanders
            .iter()
            .map(|id| (id.0, 1, Section::Commander)),
    );
    let total = |entries: &[(CardId, u32)]| {
        entries
            .iter()
            .try_fold(0u32, |sum, (_, count)| sum.checked_add(*count))
            .ok_or_else(|| {
                "too many cards in the imported list; the deck was not saved".to_string()
            })
    };
    summary.main = total(&resolved.main)?;
    summary.side = total(&resolved.sideboard)?;
    summary.commander = !resolved.commanders.is_empty();

    if entries.is_empty() {
        return Err("nothing in the list matched a card — is the card database imported?".into());
    }
    store
        .put_deck(name, None, &entries)
        .map_err(|e| e.to_string())?;
    Ok(summary)
}

/// How many distinct cards pass the whole-card rules and layout support check.
/// Demo cards are supported by the built-in card set.
pub fn coverage(store: Option<&Store>, deck: &DeckSpec) -> (usize, usize) {
    let mut keys: Vec<&CardKey> = deck.keys().collect();
    keys.sort();
    keys.dedup();
    let subtypes = mtg_oracle::compile::SubtypeNames(
        store.and_then(|s| s.subtypes().ok()).unwrap_or_default(),
    );
    let understood = keys
        .iter()
        .filter(|k| match k {
            CardKey::Demo(_) => true,
            CardKey::Oracle(uuid, _printing_id) => store
                .and_then(|s| {
                    s.find_by_uuid(uuid)
                        .ok()
                        .flatten()
                        .and_then(|o| s.card(o).ok().flatten())
                })
                .is_some_and(|c| mtg_oracle::compile::card_understood(&c, &subtypes)),
        })
        .count();
    (understood, keys.len())
}

/// Cards for a match: the demo set, and the local database when there is one.
pub struct LocalSource {
    store: Option<Store>,
    demo: DemoCards,
    subtypes: Vec<String>,
}

impl LocalSource {
    pub fn new(store: Option<Store>) -> Self {
        let mut subtypes = Vec::new();
        if let Some(s) = &store
            && let Ok(all) = s.subtypes()
        {
            let highest = all.iter().map(|(id, _)| *id).max().unwrap_or(0);
            subtypes = vec![String::new(); highest as usize + 1];
            for (id, name) in all {
                subtypes[id as usize] = name;
            }
        }
        Self {
            store,
            demo: DemoCards::default(),
            subtypes,
        }
    }

    /// Open the standard database if it exists; demo cards only if not.
    pub fn standard() -> Self {
        Self::new(open_store())
    }
}

impl convert::Subtypes for LocalSource {
    fn intern(&self, name: &str) -> Option<u16> {
        self.subtypes
            .iter()
            .position(|n| n == name)
            .map(|i| i as u16)
    }
}

impl CardSource for LocalSource {
    fn card(&self, key: &CardKey) -> Option<SourcedCard> {
        match key {
            CardKey::Demo(i) => Some(SourcedCard {
                layout: mtg_ir::Layout::Normal,
                faces: vec![self.demo.face(CardId(*i), 0)?.clone()],
                color_identity: String::new(),
            }),
            CardKey::Oracle(uuid, _printing_id) => {
                let store = self.store.as_ref()?;
                let oracle = store.find_by_uuid(uuid).ok()??;
                let card = store.card(oracle).ok()??;
                Some(SourcedCard {
                    layout: convert::to_engine_layout(&card.layout),
                    faces: card
                        .faces
                        .iter()
                        .map(|row| convert::to_engine_face(row, self))
                        .collect(),
                    color_identity: store.color_identity(oracle).unwrap_or_default(),
                })
            }
        }
    }

    fn subtype_name(&self, subtype: Subtype) -> Option<String> {
        self.subtypes
            .get(subtype.0 as usize)
            .filter(|s| !s.is_empty())
            .cloned()
    }
}

/// The local database, if one has been created. Never creates one.
pub fn open_store() -> Option<Store> {
    let path = mtg_store::default_path();
    if !path.exists() {
        return None;
    }
    Store::open(path).ok()
}

/// The local database, creating it if needed — for importing.
pub fn create_store() -> Result<Store, String> {
    let path = mtg_store::default_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    Store::open(path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deck_listing_reports_read_errors_and_recovers() {
        let mut store = Store::in_memory().unwrap();
        let id = store.put_deck("Saved", None, &[]).unwrap();
        let listed = deck_list(Some(&store)).unwrap();
        assert!(listed.contains(&(DeckId::Stored(id), "Saved".into())));
        store
            .conn()
            .execute_batch("ALTER TABLE decks RENAME TO unavailable_decks")
            .unwrap();
        assert!(deck_list(Some(&store)).is_err());
        store
            .conn()
            .execute_batch("ALTER TABLE unavailable_decks RENAME TO decks")
            .unwrap();
        assert_eq!(deck_list(Some(&store)).unwrap(), listed);
        assert_eq!(deck_list(None).unwrap().len(), 1);
    }
    use mtg_session::game::{Format, GameCards, deck_problems};

    #[test]
    fn unsupported_command_sections_do_not_replace_a_saved_deck() {
        let mut store = Store::in_memory().unwrap();
        let id = store.put_deck("Existing", None, &[]).unwrap();
        let original = store.deck(id).unwrap();
        for (text, message) in [
            (
                "Commander\n1 First Captain\n1 Second Captain\nDeck\n60 Field",
                "multiple commanders",
            ),
            (
                "Commander\n2 First Captain\nDeck\n60 Field",
                "multiple commanders",
            ),
            (
                "Companion\n1 Companion Friend\nDeck\n60 Field",
                "companion designation",
            ),
        ] {
            let error = import_deck(&mut store, "Existing", text).unwrap_err();
            assert!(error.contains(message), "{error}");
            assert_eq!(store.deck(id).unwrap(), original);
            assert_eq!(store.deck_list().unwrap(), vec![(id, "Existing".into())]);
        }
    }

    #[test]
    fn saved_deck_counts_are_never_silently_capped() {
        let mut store = Store::in_memory().unwrap();
        let oracle = store
            .put_card(
                &mtg_store::StoredCard {
                    oracle_uuid: "invented-count-test".into(),
                    name: "Counted Field".into(),
                    layout: "normal".into(),
                    faces: vec![mtg_store::FaceRow {
                        name: "Counted Field".into(),
                        type_line: "Basic Land".into(),
                        ..Default::default()
                    }],
                },
                &[],
            )
            .unwrap();
        let id = store
            .put_deck("Exact", None, &[(oracle, 255, Section::Main)])
            .unwrap();
        assert_eq!(
            deck_spec(Some(&store), DeckId::Stored(id)).unwrap().main[0].1,
            255
        );
        for section in [Section::Main, Section::Sideboard] {
            let id = store
                .put_deck("Too many", None, &[(oracle, 256, section)])
                .unwrap();
            assert!(
                deck_spec(Some(&store), DeckId::Stored(id))
                    .unwrap_err()
                    .contains("256 copies")
            );
            assert_eq!(store.deck(id).unwrap().unwrap().entries[0].1, 256);
        }
        let id = store
            .put_deck("Two commanders", None, &[(oracle, 2, Section::Commander)])
            .unwrap();
        assert!(
            deck_spec(Some(&store), DeckId::Stored(id))
                .unwrap_err()
                .contains("exactly one commander")
        );
    }

    #[test]
    fn the_demo_deck_is_a_legal_sixty() {
        let deck = demo_deck();
        assert_eq!(deck.main_count(), 60);
        let cards = GameCards::build([&deck], &LocalSource::new(None)).unwrap();
        assert_eq!(
            deck_problems(Format::Constructed, &deck, &cards),
            Vec::<String>::new()
        );
    }

    #[test]
    fn demo_cards_come_from_the_built_in_set_without_a_database() {
        let source = LocalSource::new(None);
        let card = source
            .card(&CardKey::Demo(demo::PLAINS.0))
            .expect("demo card");
        assert_eq!(card.faces[0].name.as_ref(), "Training Field");
        assert!(
            source
                .card(&CardKey::Oracle("anything".into(), None))
                .is_none()
        );
    }

    #[test]
    fn a_pasted_list_is_imported_and_read_back_as_a_portable_deck() {
        let mut store = Store::in_memory().unwrap();
        let hawk = mtg_store::StoredCard {
            oracle_uuid: "u-hawk".into(),
            name: "Invented Hawk".into(),
            layout: "normal".into(),
            faces: vec![mtg_store::FaceRow {
                name: "Invented Hawk".into(),
                type_line: "Creature — Bird".into(),
                oracle_text: Some("Flying".into()),
                ..Default::default()
            }],
        };
        store
            .put_card(&hawk, &[mtg_deck::resolve::normalise("Invented Hawk")])
            .unwrap();

        let summary = import_deck(
            &mut store,
            "Birds",
            "4 Invented Hawk\n2 Nobody Card\n4294967296 Invented Hawk\n",
        )
        .unwrap();
        assert_eq!(summary.main, 4);
        assert_eq!(summary.unmatched.len(), 1, "the unknown card is reported");
        assert_eq!(summary.unreadable, vec!["4294967296 Invented Hawk"]);
        assert!(
            !summary.is_complete(),
            "the original input is still needed for correction"
        );

        let (id, _) = store.deck_list().unwrap()[0].clone();
        let spec = deck_spec(Some(&store), DeckId::Stored(id)).unwrap();
        assert_eq!(spec.main, vec![(CardKey::Oracle("u-hawk".into(), None), 4)]);
        assert_eq!(
            coverage(Some(&store), &spec),
            (1, 1),
            "a keyword-only card is fully playable"
        );
        assert!(
            import_deck(&mut store, "Complete", "4 Invented Hawk")
                .unwrap()
                .is_complete()
        );
        for text in [
            "4294967295 Invented Hawk\n1 Invented Hawk",
            "Sideboard\n4294967295 Invented Hawk\n1 Invented Hawk",
        ] {
            let original = store.deck(id).unwrap();
            let error = import_deck(&mut store, "Birds", text).unwrap_err();
            assert!(error.contains("too many cards"), "{error}");
            assert_eq!(store.deck(id).unwrap(), original);
        }
    }
}
