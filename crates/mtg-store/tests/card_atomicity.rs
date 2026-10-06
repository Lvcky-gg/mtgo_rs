use mtg_store::{FaceRow, Store, StoredCard};

fn card(uuid: &str, name: &str, faces: &[&str]) -> StoredCard {
    StoredCard {
        oracle_uuid: uuid.into(),
        name: name.into(),
        layout: "normal".into(),
        faces: faces
            .iter()
            .map(|name| FaceRow {
                name: (*name).into(),
                ..Default::default()
            })
            .collect(),
    }
}

fn fail_on_second_face(store: &Store) {
    store
        .conn()
        .execute_batch(
            "CREATE TRIGGER reject_second_face BEFORE INSERT ON faces
         WHEN NEW.face_index = 1 BEGIN SELECT RAISE(ABORT, 'write failed'); END;",
        )
        .unwrap();
}

#[test]
fn failed_replacement_preserves_card_faces_lookup_keys_and_search() {
    let store = Store::in_memory().unwrap();
    let original = card(
        "invented-atomic-card",
        "Original Scout",
        &["Original Scout"],
    );
    let oracle = store
        .put_card(&original, &["original scout".into()])
        .unwrap();
    fail_on_second_face(&store);
    let replacement = card(
        "invented-atomic-card",
        "Changed Scout",
        &["Changed Scout", "Rejected Back"],
    );
    assert!(
        store
            .put_card(
                &replacement,
                &["changed scout".into(), "rejected back".into()]
            )
            .is_err()
    );
    assert_eq!(store.card(oracle).unwrap(), Some(original));
    assert_eq!(
        store.find_normalised("original scout").unwrap(),
        Some(oracle)
    );
    assert_eq!(store.find_normalised("changed scout").unwrap(), None);
    assert_eq!(
        store.search("Original", 10).unwrap(),
        vec![(oracle, "Original Scout".into())]
    );
    assert!(store.search("Changed", 10).unwrap().is_empty());
    store
        .conn()
        .execute_batch("DROP TRIGGER reject_second_face")
        .unwrap();
    assert_eq!(
        store
            .put_card(
                &replacement,
                &["changed scout".into(), "rejected back".into()]
            )
            .unwrap(),
        oracle
    );
    assert_eq!(store.card(oracle).unwrap(), Some(replacement));
    assert_eq!(store.find_normalised("original scout").unwrap(), None);
}

#[test]
fn failed_new_card_leaves_no_partial_identity_or_faces() {
    let store = Store::in_memory().unwrap();
    fail_on_second_face(&store);
    let new = card(
        "invented-failed-new-card",
        "New Scout",
        &["New Scout", "Rejected Back"],
    );
    assert!(
        store
            .put_card(&new, &["new scout".into(), "rejected back".into()])
            .is_err()
    );
    assert_eq!(store.card_count().unwrap(), 0);
    assert_eq!(store.find_normalised("new scout").unwrap(), None);
    assert!(store.all_cards().unwrap().is_empty());
    assert!(store.search("New", 10).unwrap().is_empty());
}
