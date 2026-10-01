//! End-to-end import: JSON in, queryable database and playable library out.
//!
//! The fixture below is **invented**. The client ships no card data, so its tests must not
//! depend on any real card existing — and a fixture of made-up cards exercises the
//! pipeline exactly as well.

use mtg_deck::Resolver;
use mtg_ir::PrintedCards;
use mtg_oracle::{CardLibrary, library::StoreResolver};
use mtg_store::Store;

/// A small bulk export. Covers the shapes that matter: a plain creature, a multi-face
/// card, an accented name, a `*` power, a hybrid cost, and three rows that must be skipped.
const FIXTURE: &str = r#"[
  {
    "oracle_id": "0001",
    "name": "Ashen Pilgrim",
    "lang": "en",
    "layout": "normal",
    "mana_cost": "{1}{W}",
    "type_line": "Creature — Human Cleric",
    "oracle_text": "Ashen Pilgrim cannot be blocked by walls.",
    "power": "2",
    "toughness": "1",
    "colors": ["W"]
  },
  {
    "oracle_id": "0002",
    "name": "Ætherwing Drake",
    "lang": "en",
    "layout": "normal",
    "mana_cost": "{3}{U}",
    "type_line": "Creature — Drake",
    "oracle_text": "Flying.",
    "power": "2",
    "toughness": "3",
    "colors": ["U"]
  },
  {
    "oracle_id": "0003",
    "name": "Tidewrack Colossus",
    "lang": "en",
    "layout": "normal",
    "mana_cost": "{5}{U}{U}",
    "type_line": "Legendary Creature — Elemental Leviathan",
    "oracle_text": "Tidewrack Colossus's power is equal to the number of Islands you control.",
    "power": "*",
    "toughness": "6",
    "colors": ["U"]
  },
  {
    "oracle_id": "0004",
    "name": "Split Verdict // Final Verdict",
    "lang": "en",
    "layout": "split",
    "card_faces": [
      {
        "name": "Split Verdict",
        "mana_cost": "{W/U}",
        "type_line": "Instant",
        "oracle_text": "Draw a card."
      },
      {
        "name": "Final Verdict",
        "mana_cost": "{2}{W}",
        "type_line": "Instant",
        "oracle_text": "Destroy target creature."
      }
    ]
  },
  {
    "oracle_id": "0005",
    "name": "Quiet Grove",
    "lang": "en",
    "layout": "normal",
    "mana_cost": "",
    "type_line": "Basic Land — Forest",
    "oracle_text": "{T}: Add {G}."
  },
  { "oracle_id": "0006", "name": "Pilger", "lang": "de", "layout": "normal",
    "type_line": "Kreatur — Mensch" },
  { "name": "No Identity", "lang": "en", "layout": "normal", "type_line": "Instant" },
  { "oracle_id": "0007", "name": "Some Token", "lang": "en", "layout": "token",
    "type_line": "Token Creature — Bear" }
]"#;

fn imported() -> (Store, mtg_oracle::ImportReport) {
    let mut store = Store::in_memory().expect("open");
    let report = mtg_oracle::import(&mut store, FIXTURE.as_bytes()).expect("import");
    (store, report)
}

// ---- what gets imported ------------------------------------------------

#[test]
fn the_playable_cards_are_imported() {
    let (store, report) = imported();
    assert_eq!(report.imported, 5, "five playable cards in the fixture");
    assert_eq!(store.card_count().unwrap(), 5);
}

#[test]
fn unplayable_rows_are_skipped_with_a_reason() {
    let (_store, report) = imported();
    assert_eq!(report.skipped.get("not English"), Some(&1));
    assert_eq!(report.skipped.get("not a card"), Some(&1));
    assert_eq!(report.skipped.get("not a playable layout"), Some(&1));
}

#[test]
fn a_multi_face_card_keeps_both_faces() {
    let (store, _) = imported();
    let oracle = store
        .find_exact("Split Verdict // Final Verdict")
        .unwrap()
        .unwrap();
    let card = store.card(oracle).unwrap().unwrap();
    assert_eq!(card.faces.len(), 2);
    assert_eq!(card.faces[0].name, "Split Verdict");
    assert_eq!(card.faces[1].name, "Final Verdict");
}

#[test]
fn subtypes_are_interned() {
    let (store, report) = imported();
    let names: Vec<String> = store
        .subtypes()
        .unwrap()
        .into_iter()
        .map(|(_, n)| n)
        .collect();
    assert!(names.contains(&"Human".to_string()));
    assert!(names.contains(&"Leviathan".to_string()));
    assert!(report.subtypes >= 5, "several distinct subtypes seen");
}

#[test]
fn re_importing_the_same_data_does_not_duplicate_it() {
    // Keyed on oracle id, so a second run updates rather than appends.
    let mut store = Store::in_memory().unwrap();
    mtg_oracle::import(&mut store, FIXTURE.as_bytes()).unwrap();
    mtg_oracle::import(&mut store, FIXTURE.as_bytes()).unwrap();
    assert_eq!(store.card_count().unwrap(), 5);
}

#[test]
fn the_report_summarises_what_it_did() {
    let (_store, report) = imported();
    let s = report.summary();
    assert!(s.contains("5 cards imported"), "got {s:?}");
    assert!(s.contains("3 skipped"), "got {s:?}");
}

// ---- name resolution, which is what deck import needs -------------------

#[test]
fn an_exact_name_resolves() {
    let (store, _) = imported();
    let r = StoreResolver(&store);
    assert!(r.exact("Ashen Pilgrim").is_some());
    assert!(r.exact("No Such Card").is_none());
}

#[test]
fn an_accented_name_resolves_from_its_ascii_spelling() {
    // The single most common deck-list breakage.
    let (store, _) = imported();
    let r = StoreResolver(&store);
    let key = mtg_deck::resolve::normalise("AEtherwing Drake");
    assert!(
        r.normalised(&key).is_some(),
        "'AEtherwing Drake' should find 'Ætherwing Drake'"
    );
}

#[test]
fn one_face_of_a_split_card_resolves_to_the_whole_card() {
    let (store, _) = imported();
    let r = StoreResolver(&store);
    let whole = r.exact("Split Verdict // Final Verdict").unwrap();
    assert_eq!(r.by_face_name("Final Verdict"), Some(whole));
}

#[test]
fn a_failed_lookup_offers_suggestions() {
    let (store, _) = imported();
    let r = StoreResolver(&store);
    let hints = r.suggest("Ashen", 5);
    assert!(
        hints.iter().any(|(_, n)| n == "Ashen Pilgrim"),
        "a prefix should suggest the full name, got {hints:?}"
    );
}

#[test]
fn search_finds_cards_by_text_and_type() {
    let (store, _) = imported();
    assert!(!store.search("Pilgrim", 10).unwrap().is_empty(), "by name");
    assert!(
        !store.search("Leviathan", 10).unwrap().is_empty(),
        "by type line"
    );
    assert!(
        !store.search("blocked walls", 10).unwrap().is_empty(),
        "by rules text"
    );
    assert!(store.search("nonexistentwordhere", 10).unwrap().is_empty());
}

// ---- the library the engine plays from ---------------------------------

#[test]
fn the_library_loads_and_exposes_printed_characteristics() {
    let (store, _) = imported();
    let lib = CardLibrary::load(&store).unwrap();
    assert_eq!(lib.len(), 5);

    let oracle = store.find_exact("Ashen Pilgrim").unwrap().unwrap();
    let face = lib.face(mtg_core::CardId(oracle), 0).expect("front face");
    assert_eq!(&*face.name, "Ashen Pilgrim");
    assert!(face.card_types.contains(&mtg_core::CardType::Creature));
    assert_eq!((face.power, face.toughness), (Some(2), Some(1)));
    assert_eq!(face.mana_cost.mana_value(), 2);
}

#[test]
fn a_star_power_loads_as_absent_not_as_zero() {
    let (store, _) = imported();
    let lib = CardLibrary::load(&store).unwrap();
    let oracle = store.find_exact("Tidewrack Colossus").unwrap().unwrap();
    let face = lib.face(mtg_core::CardId(oracle), 0).unwrap();
    assert_eq!(
        face.power, None,
        "'*' is set by an ability, not a printed number"
    );
    assert_eq!(face.toughness, Some(6));
}

#[test]
fn a_hybrid_cost_survives_the_round_trip() {
    let (store, _) = imported();
    let lib = CardLibrary::load(&store).unwrap();
    let oracle = store
        .find_exact("Split Verdict // Final Verdict")
        .unwrap()
        .unwrap();
    let front = lib.face(mtg_core::CardId(oracle), 0).unwrap();
    assert_eq!(front.mana_cost.symbols.len(), 1);
    assert!(matches!(
        front.mana_cost.symbols[0],
        mtg_core::ManaSymbol::Hybrid(..)
    ));
}

#[test]
fn subtype_names_resolve_back_through_the_library() {
    let (store, _) = imported();
    let lib = CardLibrary::load(&store).unwrap();
    let oracle = store.find_exact("Ashen Pilgrim").unwrap().unwrap();
    let face = lib.face(mtg_core::CardId(oracle), 0).unwrap();
    let names: Vec<&str> = face
        .subtypes
        .iter()
        .filter_map(|s| lib.subtype_name(*s))
        .collect();
    assert!(names.contains(&"Human"), "got {names:?}");
}

#[test]
fn imported_cards_have_no_abilities_until_behaviour_is_authored() {
    // The honest gap: rules text is imported, behaviour is not. A card with text the
    // engine cannot act on must not look playable.
    let (store, _) = imported();
    let lib = CardLibrary::load(&store).unwrap();
    let oracle = store.find_exact("Ashen Pilgrim").unwrap().unwrap();
    let face = lib.face(mtg_core::CardId(oracle), 0).unwrap();

    assert!(
        face.oracle_text.is_some(),
        "the text is kept for display and review"
    );
    assert!(face.abilities.is_empty(), "but nothing is inferred from it");
    assert_eq!(lib.authored_count(), 0, "and coverage is reported as zero");
}

// ---- deck import, end to end -------------------------------------------

#[test]
fn a_pasted_deck_list_resolves_against_imported_cards() {
    let (store, _) = imported();
    let r = StoreResolver(&store);

    let pasted = "\
4 Ashen Pilgrim (ABC) 12
2 AEtherwing Drake
20 Quiet Grove

1 Tidewrack Colossus
1 Not A Real Card";

    let parsed = mtg_deck::parse(pasted);
    let resolved = mtg_deck::resolve::resolve(&parsed, &r);

    let main: u32 = resolved.main.iter().map(|(_, n)| n).sum();
    assert_eq!(main, 26, "4 + 2 + 20");
    assert_eq!(resolved.sideboard.iter().map(|(_, n)| n).sum::<u32>(), 1);
    assert_eq!(resolved.failed.len(), 1, "the invented name failed");
    assert_eq!(resolved.failed[0].entry.name, "Not A Real Card");
}

#[test]
fn a_resolved_deck_round_trips_through_the_database() {
    let (mut store, _) = imported();
    let entries = {
        let r = StoreResolver(&store);
        let parsed = mtg_deck::parse("4 Ashen Pilgrim\n20 Quiet Grove\n\n2 AEtherwing Drake");
        let resolved = mtg_deck::resolve::resolve(&parsed, &r);
        resolved
            .main
            .iter()
            .map(|(id, n)| (id.0, *n, mtg_store::Section::Main))
            .chain(
                resolved
                    .sideboard
                    .iter()
                    .map(|(id, n)| (id.0, *n, mtg_store::Section::Sideboard)),
            )
            .collect::<Vec<_>>()
    };

    let id = store
        .put_deck("Test Deck", Some("casual"), &entries)
        .unwrap();
    let back = store.deck(id).unwrap().expect("saved deck");

    assert_eq!(back.name, "Test Deck");
    assert_eq!(back.entries.len(), entries.len());
    assert_eq!(
        back.entries
            .iter()
            .filter(|(_, _, s)| *s == mtg_store::Section::Main)
            .map(|(_, n, _)| n)
            .sum::<u32>(),
        24
    );
    assert_eq!(store.deck_list().unwrap().len(), 1);
}

#[test]
fn saving_a_deck_under_an_existing_name_replaces_it() {
    let (mut store, _) = imported();
    let oracle = store.find_exact("Quiet Grove").unwrap().unwrap();
    store
        .put_deck("Mine", None, &[(oracle, 20, mtg_store::Section::Main)])
        .unwrap();
    store
        .put_deck("Mine", None, &[(oracle, 24, mtg_store::Section::Main)])
        .unwrap();

    let list = store.deck_list().unwrap();
    assert_eq!(list.len(), 1, "re-pasting an edited list replaces it");
    let deck = store.deck(list[0].0).unwrap().unwrap();
    assert_eq!(deck.entries[0].1, 24);
}

// ---- what the bulk importer records beyond text --------------------------

/// Invented cards carrying the fields the importer must not drop.
const RICH: &str = r#"[
  {
    "oracle_id": "r001",
    "name": "Invented Grove",
    "lang": "en",
    "layout": "normal",
    "type_line": "Basic Land — Forest",
    "oracle_text": "({T}: Add {G}.)",
    "color_identity": ["G"],
    "image_uris": { "normal": "https://cdn.test/grove.jpg" }
  },
  {
    "oracle_id": "r002",
    "name": "Invented Hawk",
    "lang": "en",
    "layout": "normal",
    "mana_cost": "{1}{W}{U}",
    "type_line": "Creature — Bird",
    "oracle_text": "Flying, vigilance",
    "power": "2",
    "toughness": "2",
    "color_identity": ["W", "U"],
    "legalities": { "standard": "not_legal", "modern": "legal", "vintage": "restricted", "legacy": "banned" },
    "image_uris": { "normal": "https://cdn.test/hawk.jpg" }
  }
]"#;

fn rich() -> Store {
    let mut store = Store::in_memory().unwrap();
    mtg_oracle::import(&mut store, RICH.as_bytes()).unwrap();
    store
}

#[test]
fn the_bulk_importer_records_image_urls() {
    // The importer writes through its own transaction-scoped SQL, so this is checked end to
    // end rather than through `Store::put_card`, which it does not use.
    let store = rich();
    assert_eq!(
        store.image_uri("Invented Hawk").unwrap().as_deref(),
        Some("https://cdn.test/hawk.jpg")
    );
}

#[test]
fn the_bulk_importer_records_colour_identity() {
    let store = rich();
    let hawk = store
        .find_by_uuid("r002")
        .unwrap()
        .expect("found by Scryfall id");
    assert_eq!(store.color_identity(hawk).unwrap(), "WU");
}

#[test]
fn imported_basics_make_mana_and_keyword_creatures_keep_their_keywords() {
    let store = rich();
    let library = CardLibrary::load(&store).unwrap();
    let grove = store.find_by_uuid("r001").unwrap().unwrap();
    let hawk = store.find_by_uuid("r002").unwrap().unwrap();
    let face = |id: u32| library.face(mtg_core::CardId(id), 0).unwrap().clone();
    assert!(matches!(
        face(grove).abilities[0].kind,
        mtg_ir::AbilityKind::Activated {
            is_mana_ability: true,
            ..
        }
    ));
    assert_eq!(face(hawk).abilities.len(), 2, "flying and vigilance");
}

// ---- the formats Scryfall has shipped ------------------------------------

/// `RICH` as JSON Lines: one card object per line, no enclosing array.
fn rich_as_lines() -> String {
    let cards: Vec<serde_json::Value> = serde_json::from_str(RICH).unwrap();
    cards
        .iter()
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

#[test]
fn json_lines_import_like_an_array() {
    let mut store = Store::in_memory().unwrap();
    let report = mtg_oracle::import(&mut store, rich_as_lines().as_bytes()).unwrap();
    assert_eq!(report.imported, 2);
    assert!(store.find_by_uuid("r002").unwrap().is_some());
}

#[test]
fn a_gzipped_export_is_decompressed_on_the_fly() {
    use std::io::Write;
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    gz.write_all(rich_as_lines().as_bytes()).unwrap();
    let bytes = gz.finish().unwrap();

    let mut store = Store::in_memory().unwrap();
    let report = mtg_oracle::import(&mut store, bytes.as_slice()).unwrap();
    assert_eq!(report.imported, 2);
}

#[test]
fn an_unreadable_line_is_counted_not_fatal() {
    let text = format!("{}not json at all\n", rich_as_lines());
    let mut store = Store::in_memory().unwrap();
    let report = mtg_oracle::import(&mut store, text.as_bytes()).unwrap();
    assert_eq!((report.imported, report.failed), (2, 1));
}

#[test]
fn something_that_is_not_an_export_is_refused() {
    let mut store = Store::in_memory().unwrap();
    assert!(mtg_oracle::import(&mut store, "<html>".as_bytes()).is_err());
    assert!(mtg_oracle::import(&mut store, "   ".as_bytes()).is_err());
}

#[test]
fn the_bulk_importer_records_where_a_card_is_legal() {
    let store = rich();
    let modern = mtg_store::CardQuery {
        legal_in: Some("modern".into()),
        ..Default::default()
    };
    let hits: Vec<String> = store
        .browse(&modern, 10, 0)
        .unwrap()
        .into_iter()
        .map(|(_, n)| n)
        .collect();
    assert_eq!(hits, vec!["Invented Hawk"]);
    let legacy = mtg_store::CardQuery {
        legal_in: Some("legacy".into()),
        ..Default::default()
    };
    assert!(
        store.browse(&legacy, 10, 0).unwrap().is_empty(),
        "banned is not legal"
    );
    let vintage = mtg_store::CardQuery {
        legal_in: Some("vintage".into()),
        ..Default::default()
    };
    assert_eq!(
        store.browse(&vintage, 10, 0).unwrap().len(),
        1,
        "restricted still counts"
    );
}

#[test]
fn imported_library_preserves_face_relationships() {
    let (store, _) = imported();
    let library = CardLibrary::load(&store).unwrap();
    let split = mtg_core::CardId(store.find_by_uuid("0004").unwrap().unwrap());
    assert_eq!(library.layout(split), mtg_ir::Layout::Split);
    assert_eq!(
        library.face(split, 1).unwrap().name.as_ref(),
        "Final Verdict"
    );
    let normal = mtg_core::CardId(store.find_by_uuid("0005").unwrap().unwrap());
    assert_eq!(library.layout(normal), mtg_ir::Layout::Normal);
    assert_eq!(
        library.layout(mtg_core::CardId(900_000)),
        mtg_ir::Layout::Unknown
    );
}

#[test]
fn layout_conversion_distinguishes_modal_and_transforming_cards() {
    use mtg_ir::Layout;
    use mtg_oracle::convert::to_engine_layout;
    for (name, expected) in [
        ("normal", Layout::Normal),
        ("split", Layout::Split),
        ("adventure", Layout::Adventure),
        ("transform", Layout::Transforming),
        ("modal_dfc", Layout::ModalDfc),
        ("leveler", Layout::Leveler),
        ("saga", Layout::Saga),
        ("class", Layout::Class),
        ("token", Layout::Token),
        ("double_faced_token", Layout::Unknown),
        ("future_layout", Layout::Unknown),
    ] {
        assert_eq!(to_engine_layout(name), expected, "{name}");
    }
}

#[test]
fn modal_playability_requires_exactly_two_fully_understood_faces() {
    use mtg_oracle::compile::{SubtypeNames, card_understood};
    use mtg_store::{FaceRow, StoredCard};
    let mut card = StoredCard {
        oracle_uuid: "invented-modal".into(),
        name: "Sunrise // Sunset".into(),
        layout: "modal_dfc".into(),
        faces: vec![
            FaceRow {
                name: "Sunrise".into(),
                type_line: "Sorcery".into(),
                mana_cost: "{R}".into(),
                oracle_text: Some("You gain 3 life.".into()),
                ..Default::default()
            },
            FaceRow {
                name: "Sunset".into(),
                type_line: "Land".into(),
                oracle_text: Some("{T}: Add {R}.".into()),
                ..Default::default()
            },
        ],
    };
    let names = SubtypeNames(Vec::new());
    assert!(card_understood(&card, &names));
    card.faces[1].oracle_text = Some("Invent a rule the compiler has never seen.".into());
    assert!(!card_understood(&card, &names));
    card.faces[1].oracle_text = Some("{T}: Add {R}.".into());
    for layout in ["split", "adventure", "flip", "unknown"] {
        card.layout = layout.into();
        assert!(!card_understood(&card, &names));
    }
    card.layout = "modal_dfc".into();
    card.faces.pop();
    assert!(!card_understood(&card, &names));
    card.layout = "normal".into();
    assert!(card_understood(&card, &names));
    card.faces.clear();
    assert!(!card_understood(&card, &names));
}

#[test]
fn progress_reports_rows_and_commit_for_arrays_lines_and_gzip() {
    use mtg_oracle::import::{ImportProgress, import_with_progress};
    use std::io::Write;
    let row = r#"{"oracle_id":"progress","name":"Progress Scout","lang":"en","layout":"normal","type_line":"Creature","power":"1","toughness":"1"}"#;
    let lines = format!("{row}\n{row}\nnot json\n");
    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gzip.write_all(lines.as_bytes()).unwrap();
    let zipped = gzip.finish().unwrap();
    for (bytes, expected) in [
        (format!("[{row},{row}]").into_bytes(), 2),
        (lines.into_bytes(), 3),
        (zipped, 3),
    ] {
        let mut store = Store::in_memory().unwrap();
        let mut updates = Vec::new();
        let report =
            import_with_progress(&mut store, bytes.as_slice(), &mut |p| updates.push(p)).unwrap();
        assert_eq!(report.imported, 2);
        assert_eq!(
            updates.first(),
            Some(&ImportProgress::Reading {
                processed: 0,
                imported: 0
            })
        );
        assert_eq!(
            updates[updates.len() - 2],
            ImportProgress::Reading {
                processed: expected,
                imported: 2
            }
        );
        assert_eq!(updates.last(), Some(&ImportProgress::Committing));
        assert_eq!(
            store.card_count().unwrap(),
            1,
            "repeated imports replace the same identity"
        );
    }
}

#[test]
fn progress_is_periodic_and_a_broken_stream_rolls_back() {
    use mtg_oracle::import::{ImportProgress, import_with_progress};
    let row = r#"{"oracle_id":"batch","name":"Batch Scout","lang":"en","layout":"normal","type_line":"Creature"}"#;
    let lines = format!("{row}\n").repeat(1001);
    let mut store = Store::in_memory().unwrap();
    let mut updates = Vec::new();
    import_with_progress(&mut store, lines.as_bytes(), &mut |p| updates.push(p)).unwrap();
    assert!(updates.contains(&ImportProgress::Reading {
        processed: 500,
        imported: 500
    }));
    assert!(updates.contains(&ImportProgress::Reading {
        processed: 1000,
        imported: 1000
    }));
    let mut updates = Vec::new();
    let broken = format!("[{row},");
    let mut empty = Store::in_memory().unwrap();
    assert!(import_with_progress(&mut empty, broken.as_bytes(), &mut |p| updates.push(p)).is_err());
    assert_eq!(empty.card_count().unwrap(), 0);
    assert!(!updates.contains(&ImportProgress::Committing));
}

#[test]
fn reimport_updates_search_document_by_stable_id_without_duplicates() {
    let (mut store, _) = imported();
    mtg_oracle::import(&mut store, FIXTURE.as_bytes()).unwrap();
    let count: i64 = store
        .conn_mut()
        .query_row(
            "SELECT COUNT(*) FROM card_search WHERE rowid != oracle",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
    let count: i64 = store
        .conn_mut()
        .query_row("SELECT COUNT(*) FROM card_search", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 5);
    assert_eq!(store.search("Flying", 10).unwrap().len(), 1);
}

#[test]
fn adventure_playability_checks_both_faces_and_requires_a_spell_adventure() {
    use mtg_oracle::compile::{SubtypeNames, card_understood};
    use mtg_store::{FaceRow, StoredCard};
    let mut card = StoredCard {
        oracle_uuid: "invented-adventure".into(),
        name: "Trail Scout // Trail Lesson".into(),
        layout: "adventure".into(),
        faces: vec![
            FaceRow {
                name: "Trail Scout".into(),
                type_line: "Creature".into(),
                mana_cost: "{2}{G}".into(),
                power: Some("3".into()),
                toughness: Some("3".into()),
                oracle_text: Some("Reach".into()),
                ..Default::default()
            },
            FaceRow {
                name: "Trail Lesson".into(),
                type_line: "Sorcery — Adventure".into(),
                mana_cost: "{G}".into(),
                oracle_text: Some("You gain 3 life.".into()),
                ..Default::default()
            },
        ],
    };
    let names = SubtypeNames(Vec::new());
    assert!(card_understood(&card, &names));
    card.faces[1].oracle_text = Some("Invent a completely new rule.".into());
    assert!(!card_understood(&card, &names));
    card.faces[1].oracle_text = Some("You gain 3 life.".into());
    card.faces[1].type_line = "Creature".into();
    assert!(!card_understood(&card, &names));
    card.faces.pop();
    assert!(!card_understood(&card, &names));
}

#[test]
fn split_gate_accepts_aftermath_but_rejects_fuse_permanents_and_off_stack_abilities() {
    use mtg_oracle::compile::{SubtypeNames, card_understood};
    use mtg_store::{FaceRow, StoredCard};
    let mut card = StoredCard {
        oracle_uuid: "invented-split".into(),
        name: "Dawn // Dusk".into(),
        layout: "split".into(),
        faces: vec![
            FaceRow {
                name: "Dawn".into(),
                type_line: "Sorcery".into(),
                mana_cost: "{U}".into(),
                oracle_text: Some("You gain 3 life.".into()),
                ..Default::default()
            },
            FaceRow {
                name: "Dusk".into(),
                type_line: "Instant".into(),
                mana_cost: "{1}{R}".into(),
                oracle_text: Some("You gain 2 life.".into()),
                ..Default::default()
            },
        ],
    };
    let names = SubtypeNames(Vec::new());
    assert!(card_understood(&card, &names));
    card.faces[1].oracle_text = Some(
        "You gain 2 life.\nAftermath (Cast this spell only from your graveyard. Then exile it.)"
            .into(),
    );
    assert!(card_understood(&card, &names));
    for text in ["You gain 2 life.\nFuse", "You gain 2 life.\nCycling {1}"] {
        card.faces[1].oracle_text = Some(text.into());
        assert!(!card_understood(&card, &names), "{text}");
    }
    card.faces[1].oracle_text = Some("You gain 2 life.".into());
    card.faces[1].type_line = "Enchantment".into();
    assert!(!card_understood(&card, &names));
    card.faces.pop();
    assert!(!card_understood(&card, &names));
}
