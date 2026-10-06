use mtg_store::{FaceRow, Section, Store, StoredCard};

#[test]
fn invalid_persisted_counts_are_reported_instead_of_wrapping() {
    let mut store = Store::in_memory().unwrap();
    let card = StoredCard {
        oracle_uuid: "invented-deck-count".into(),
        name: "Invented Scout".into(),
        layout: "normal".into(),
        faces: vec![FaceRow::default()],
    };
    let oracle = store.put_card(&card, &[]).unwrap();
    let id = store
        .put_deck("Scouts", None, &[(oracle, 1, Section::Main)])
        .unwrap();
    for count in [-1i64, i64::from(u32::MAX) + 1] {
        store
            .conn()
            .execute(
                "UPDATE deck_entries SET count = ?1 WHERE deck = ?2",
                (count, id),
            )
            .unwrap();
        assert!(
            store.deck(id).is_err(),
            "invalid count {count} was accepted"
        );
    }
    store
        .conn()
        .execute(
            "UPDATE deck_entries SET count = ?1 WHERE deck = ?2",
            (u32::MAX, id),
        )
        .unwrap();
    assert_eq!(
        store.deck(id).unwrap().unwrap().entries,
        vec![(oracle, u32::MAX, Section::Main)]
    );
}

#[test]
fn unknown_persisted_sections_are_reported_instead_of_becoming_main() {
    let mut store = Store::in_memory().unwrap();
    let card = StoredCard {
        oracle_uuid: "invented-deck-section".into(),
        name: "Invented Captain".into(),
        layout: "normal".into(),
        faces: vec![FaceRow::default()],
    };
    let oracle = store.put_card(&card, &[]).unwrap();
    let id = store
        .put_deck("Captain", None, &[(oracle, 1, Section::Commander)])
        .unwrap();
    store
        .conn()
        .execute(
            "UPDATE deck_entries SET section = 'companion' WHERE deck = ?1",
            [id],
        )
        .unwrap();
    assert!(store.deck(id).is_err());
    store
        .conn()
        .execute(
            "UPDATE deck_entries SET section = 'commander' WHERE deck = ?1",
            [id],
        )
        .unwrap();
    assert_eq!(
        store.deck(id).unwrap().unwrap().entries,
        vec![(oracle, 1, Section::Commander)]
    );
}
