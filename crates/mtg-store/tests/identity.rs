//! Identity and friend persistence.

use mtg_store::{Store, StoredIdentity};

fn an_identity(tag: u8) -> StoredIdentity {
    StoredIdentity {
        seed: [tag; 32],
        public: [tag.wrapping_add(1); 32],
    }
}

#[test]
fn a_fresh_database_has_no_identity() {
    let store = Store::in_memory().unwrap();
    assert_eq!(store.identity().unwrap(), None);
}

#[test]
fn an_identity_round_trips() {
    let store = Store::in_memory().unwrap();
    let id = an_identity(7);
    store.put_identity(&id).unwrap();
    assert_eq!(store.identity().unwrap(), Some(id));
}

#[test]
fn storing_an_identity_replaces_the_previous_one() {
    // Restoring from a backup has to just work.
    let store = Store::in_memory().unwrap();
    store.put_identity(&an_identity(1)).unwrap();
    store.put_identity(&an_identity(2)).unwrap();
    assert_eq!(store.identity().unwrap(), Some(an_identity(2)));
}

#[test]
fn friends_are_pinned_by_key_and_renamed_in_place() {
    let store = Store::in_memory().unwrap();
    let key = [9u8; 32];

    store.put_friend(&key, "Sam").unwrap();
    assert_eq!(store.friend(&key).unwrap().unwrap().nickname, "Sam");

    // The key is the identity; the nickname is only yours.
    store.put_friend(&key, "Sam (work laptop)").unwrap();
    assert_eq!(
        store.friends().unwrap().len(),
        1,
        "renaming does not add a second friend"
    );
    assert_eq!(
        store.friend(&key).unwrap().unwrap().nickname,
        "Sam (work laptop)"
    );
}

#[test]
fn friends_can_be_listed_and_forgotten() {
    let store = Store::in_memory().unwrap();
    store.put_friend(&[1u8; 32], "Ada").unwrap();
    store.put_friend(&[2u8; 32], "Bo").unwrap();
    assert_eq!(store.friends().unwrap().len(), 2);

    store.forget_friend(&[1u8; 32]).unwrap();
    let left = store.friends().unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].nickname, "Bo");
}

#[test]
fn an_unknown_key_is_not_a_friend() {
    let store = Store::in_memory().unwrap();
    assert_eq!(store.friend(&[3u8; 32]).unwrap(), None);
}

#[test]
fn the_new_tables_coexist_with_card_data() {
    // v2 added identity and friends; a database holding cards must be unaffected.
    let store = Store::in_memory().unwrap();
    store
        .put_card(
            &mtg_store::StoredCard {
                oracle_uuid: "x1".into(),
                name: "Invented Card".into(),
                layout: "normal".into(),
                faces: vec![Default::default()],
            },
            &["invented card".to_string()],
        )
        .unwrap();
    assert_eq!(store.card_count().unwrap(), 1);
    assert_eq!(store.identity().unwrap(), None);
}
