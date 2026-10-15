use mtg_store::{FaceRow, Store, StoredCard};

#[test]
fn alternate_names_resolve_to_existing_oracle_ids_without_duplicate_cards() {
    let store = Store::in_memory().unwrap();
    for (alias, canonical) in [
        ("Balin’s Tomb", "Ancient Tomb"),
        ("DOL AMROTH", "Minamo, School at Water's Edge"),
        ("Hope's Aero Magic", "Cyclonic Rift"),
        ("Mayor Tong of Chin Village", "Drannith Magistrate"),
        ("Slimed", "Trickbind"),
        ("Widow-Making Infiltrator", "Dauthi Voidwalker"),
    ] {
        let card = StoredCard {
            oracle_uuid: canonical.into(),
            name: canonical.into(),
            layout: "normal".into(),
            faces: vec![FaceRow {
                name: canonical.into(),
                ..Default::default()
            }],
        };
        let id = store
            .put_card(&card, &[mtg_deck::resolve::normalise(canonical)])
            .unwrap();
        assert_eq!(
            store
                .find_normalised(&mtg_deck::resolve::normalise(alias))
                .unwrap(),
            Some(id)
        );
        assert_eq!(store.card(id).unwrap().unwrap().name, canonical);
    }
    assert_eq!(
        store.find_normalised("unknown alternate name").unwrap(),
        None
    );
}

#[test]
fn a_stored_name_takes_precedence_over_the_curated_fallback() {
    let store = Store::in_memory().unwrap();
    let card = StoredCard {
        oracle_uuid: "explicit-slimed".into(),
        name: "Slimed".into(),
        layout: "normal".into(),
        faces: vec![FaceRow {
            name: "Slimed".into(),
            ..Default::default()
        }],
    };
    let id = store.put_card(&card, &["slimed".into()]).unwrap();
    assert_eq!(store.find_normalised("slimed").unwrap(), Some(id));
}
