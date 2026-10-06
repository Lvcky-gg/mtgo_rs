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
fn malformed_identity_bytes_are_an_error_rather_than_a_missing_identity() {
    let store = Store::in_memory().unwrap();
    for sql in [
        "UPDATE identity SET seed = ?1",
        "UPDATE identity SET public = ?1",
    ] {
        for size in [0, 31, 33] {
            store.put_identity(&an_identity(7)).unwrap();
            store.conn().execute(sql, [vec![7u8; size]]).unwrap();
            assert!(store.identity().is_err(), "{sql}: accepted {size} bytes");
        }
    }
    store.put_identity(&an_identity(7)).unwrap();
    assert_eq!(store.identity().unwrap(), Some(an_identity(7)));
}

#[test]
fn malformed_friend_keys_are_reported_instead_of_hidden_from_the_list() {
    let store = Store::in_memory().unwrap();
    store.put_friend(&[9; 32], "Sam").unwrap();
    store
        .conn()
        .execute("UPDATE friends SET public = ?1", [vec![9u8; 31]])
        .unwrap();
    assert!(store.friends().is_err());
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
fn first_use_insertion_never_replaces_an_identity_saved_by_another_caller() {
    let store = Store::in_memory().unwrap();
    assert_eq!(store.identity().unwrap(), None);
    let first = an_identity(1);
    let second = an_identity(2);
    assert_eq!(store.identity_or_insert(&first).unwrap(), first);
    assert_eq!(store.identity_or_insert(&second).unwrap(), first);
    assert_eq!(store.identity().unwrap(), Some(first));
}

#[test]
fn concurrent_first_use_connections_agree_on_one_persisted_identity() {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "mtgo-rs-identity-{}-{stamp}.sqlite",
        std::process::id()
    ));
    let first_store = Store::open(&path).unwrap();
    let second_store = Store::open(&path).unwrap();
    assert_eq!(first_store.identity().unwrap(), None);
    assert_eq!(second_store.identity().unwrap(), None);
    let ready = std::sync::Arc::new(std::sync::Barrier::new(2));
    let first_ready = ready.clone();
    let first = std::thread::spawn(move || {
        first_ready.wait();
        first_store.identity_or_insert(&an_identity(1)).unwrap()
    });
    let second = std::thread::spawn(move || {
        ready.wait();
        second_store.identity_or_insert(&an_identity(2)).unwrap()
    });
    let winner = first.join().unwrap();
    assert_eq!(second.join().unwrap(), winner);
    assert!(winner == an_identity(1) || winner == an_identity(2));
    let reopened = Store::open(&path).unwrap();
    assert_eq!(reopened.identity().unwrap(), Some(winner));
    drop(reopened);
    std::fs::remove_file(path).unwrap();
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
