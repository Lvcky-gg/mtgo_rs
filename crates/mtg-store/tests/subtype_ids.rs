use mtg_store::Store;

#[test]
fn subtype_exhaustion_is_reported_without_aliasing_an_existing_type() {
    let store = Store::in_memory().unwrap();
    store
        .conn()
        .execute(
            "INSERT INTO subtypes (id, name) VALUES (?1, 'Boundary Type')",
            [u16::MAX],
        )
        .unwrap();
    assert_eq!(store.intern_subtype("Boundary Type").unwrap(), u16::MAX);
    let before = store.subtypes().unwrap();
    assert!(store.intern_subtype("Overflow Type").is_err());
    assert_eq!(store.subtypes().unwrap(), before);
}

#[test]
fn an_invalid_persisted_subtype_id_is_reported_instead_of_wrapping() {
    let store = Store::in_memory().unwrap();
    store
        .conn()
        .execute(
            "INSERT INTO subtypes (id, name) VALUES (?1, 'Invalid Type')",
            [u32::from(u16::MAX) + 1],
        )
        .unwrap();
    assert!(store.subtypes().is_err());
    assert!(store.intern_subtype("Invalid Type").is_err());
}
