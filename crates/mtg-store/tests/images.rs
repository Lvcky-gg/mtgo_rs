//! Image URLs survive a round trip through the store, and resolve by the names the UI has.

use mtg_store::{FaceRow, Store, StoredCard};

fn face(name: &str, image: Option<&str>) -> FaceRow {
    FaceRow {
        name: name.into(),
        image_uri: image.map(str::to_string),
        ..Default::default()
    }
}

fn store_with(card: StoredCard) -> Store {
    let store = Store::in_memory().unwrap();
    store.put_card(&card, &[]).unwrap();
    store
}

#[test]
fn a_face_image_is_found_by_the_face_name() {
    let store = store_with(StoredCard {
        oracle_uuid: "u1".into(),
        name: "Invented Bear".into(),
        layout: "normal".into(),
        faces: vec![face("Invented Bear", Some("https://cdn.test/bear.jpg"))],
    });
    assert_eq!(
        store.image_uri("Invented Bear").unwrap().as_deref(),
        Some("https://cdn.test/bear.jpg")
    );
    assert_eq!(store.image_uri("Nobody").unwrap(), None);
}

#[test]
fn the_back_face_resolves_to_its_own_image_and_the_full_name_to_the_front() {
    let store = store_with(StoredCard {
        oracle_uuid: "u2".into(),
        name: "Day // Night".into(),
        layout: "transform".into(),
        faces: vec![
            face("Day", Some("https://cdn.test/d.jpg")),
            face("Night", Some("https://cdn.test/n.jpg")),
        ],
    });
    assert_eq!(
        store.image_uri("Night").unwrap().as_deref(),
        Some("https://cdn.test/n.jpg")
    );
    assert_eq!(
        store.image_uri("Day // Night").unwrap().as_deref(),
        Some("https://cdn.test/d.jpg")
    );
}

#[test]
fn a_card_imported_before_images_were_recorded_has_none() {
    let store = store_with(StoredCard {
        oracle_uuid: "u3".into(),
        name: "Old Import".into(),
        layout: "normal".into(),
        faces: vec![face("Old Import", None)],
    });
    assert_eq!(store.image_uri("Old Import").unwrap(), None);
}

// ---- browsing, for the deck builder ----------------------------------------

fn browse_store() -> Store {
    let store = Store::in_memory().unwrap();
    for (uuid, name, type_line, text, identity) in [
        ("b1", "Invented Bolt", "Instant", "deals 3 damage", "R"),
        ("b2", "Invented Bear", "Creature — Bear", "", "G"),
        ("b3", "Invented Bird", "Creature — Bird", "Flying", "WU"),
    ] {
        let card = StoredCard {
            oracle_uuid: uuid.into(),
            name: name.into(),
            layout: "normal".into(),
            faces: vec![FaceRow {
                name: name.into(),
                type_line: type_line.into(),
                oracle_text: Some(text.into()),
                ..Default::default()
            }],
        };
        let id = store.put_card(&card, &[]).unwrap();
        store.set_color_identity(id, identity).unwrap();
    }
    store
}

fn names(rows: Vec<(u32, String)>) -> Vec<String> {
    rows.into_iter().map(|(_, n)| n).collect()
}

#[test]
fn browsing_with_no_filter_lists_everything_by_name() {
    let store = browse_store();
    let all = store
        .browse(&mtg_store::CardQuery::default(), 10, 0)
        .unwrap();
    assert_eq!(
        names(all),
        vec!["Invented Bear", "Invented Bird", "Invented Bolt"]
    );
}

#[test]
fn words_match_prefixes_of_name_type_or_text() {
    let store = browse_store();
    let q = |text: &str| mtg_store::CardQuery {
        text: text.into(),
        ..Default::default()
    };
    assert_eq!(
        names(store.browse(&q("bol"), 10, 0).unwrap()),
        vec!["Invented Bolt"]
    );
    assert_eq!(
        names(store.browse(&q("flying"), 10, 0).unwrap()),
        vec!["Invented Bird"]
    );
    assert_eq!(
        names(store.browse(&q("\"odd (syntax"), 10, 0).unwrap()),
        Vec::<String>::new()
    );
}

#[test]
fn type_and_colour_filters_narrow_the_list() {
    let store = browse_store();
    let creatures = mtg_store::CardQuery {
        type_contains: "creature".into(),
        ..Default::default()
    };
    assert_eq!(
        names(store.browse(&creatures, 10, 0).unwrap()),
        vec!["Invented Bear", "Invented Bird"]
    );
    let white_blue = mtg_store::CardQuery {
        colors_within: Some("WU".into()),
        ..Default::default()
    };
    assert_eq!(
        names(store.browse(&white_blue, 10, 0).unwrap()),
        vec!["Invented Bird"]
    );
}

#[test]
fn results_come_a_page_at_a_time() {
    let store = browse_store();
    let q = mtg_store::CardQuery::default();
    assert_eq!(
        names(store.browse(&q, 2, 2).unwrap()),
        vec!["Invented Bolt"]
    );
}

#[test]
fn only_cards_legal_somewhere_or_in_a_named_format_can_be_asked_for() {
    let store = browse_store();
    let bolt = store.find_exact("Invented Bolt").unwrap().unwrap();
    let bear = store.find_exact("Invented Bear").unwrap().unwrap();
    store.set_legal_in(bolt, &["modern", "legacy"]).unwrap();
    store.set_legal_in(bear, &["commander"]).unwrap();
    let q = |legal: Option<&str>| mtg_store::CardQuery {
        legal_in: legal.map(str::to_string),
        ..Default::default()
    };
    assert_eq!(
        names(store.browse(&q(Some("")), 10, 0).unwrap()),
        vec!["Invented Bear", "Invented Bolt"],
        "the bird is legal nowhere"
    );
    assert_eq!(
        names(store.browse(&q(Some("modern")), 10, 0).unwrap()),
        vec!["Invented Bolt"]
    );
    assert_eq!(names(store.browse(&q(None), 10, 0).unwrap()).len(), 3);
}

#[test]
fn a_cards_own_name_outranks_cards_that_mention_it() {
    let store = browse_store();
    // The bird's text mentions "flying"; add a card *named* for it.
    let card = StoredCard {
        oracle_uuid: "b9".into(),
        name: "Zz Flying".into(),
        layout: "normal".into(),
        faces: vec![face("Zz Flying", None)],
    };
    store.put_card(&card, &[]).unwrap();
    let q = mtg_store::CardQuery {
        text: "flying".into(),
        ..Default::default()
    };
    assert_eq!(
        names(store.browse(&q, 10, 0).unwrap()),
        vec!["Zz Flying", "Invented Bird"]
    );
}
