use mtg_store::{FaceRow, Store, StoredCard};

const EXPORT: &str = r#"[
    {"oracle_id":"new-scout", "name":"New Scout", "lang":"en", "layout":"normal", "type_line":"Creature — Bird"},
    {"oracle_id":"existing-scout", "name":"Changed Front // Rejected Back", "lang":"en", "layout":"modal_dfc", "card_faces":[
        {"name":"Changed Front", "type_line":"Creature — Bird"},
        {"name":"Rejected Back", "type_line":"Land"}
    ]}
]"#;

fn original_store() -> Store {
    let store = Store::in_memory().unwrap();
    store
        .put_card(
            &StoredCard {
                oracle_uuid: "existing-scout".into(),
                name: "Original Scout".into(),
                layout: "normal".into(),
                faces: vec![FaceRow {
                    name: "Original Scout".into(),
                    ..Default::default()
                }],
            },
            &["original scout".into()],
        )
        .unwrap();
    store.set_meta("cards_imported", "original").unwrap();
    store
}

#[test]
fn database_write_failure_rolls_back_the_entire_array_or_json_lines_import() {
    let rows: Vec<serde_json::Value> = serde_json::from_str(EXPORT).unwrap();
    let lines = rows
        .iter()
        .map(|row| serde_json::to_string(row).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    for export in [EXPORT, lines.as_str()] {
        let mut store = original_store();
        let before = store.all_cards().unwrap();
        store
            .conn()
            .execute_batch(
                "CREATE TRIGGER reject_face BEFORE INSERT ON faces
            WHEN NEW.name = 'Rejected Back' BEGIN SELECT RAISE(ABORT, 'face write failed'); END;",
            )
            .unwrap();
        let error = mtg_oracle::import(&mut store, export.as_bytes()).unwrap_err();
        assert!(error.contains("face write failed"), "{error}");
        assert_eq!(store.all_cards().unwrap(), before);
        assert_eq!(
            store.find_normalised("original scout").unwrap(),
            Some(before[0].0)
        );
        assert_eq!(store.find_normalised("changed front").unwrap(), None);
        assert_eq!(store.find_exact("New Scout").unwrap(), None);
        assert_eq!(
            store.meta("cards_imported").unwrap().as_deref(),
            Some("original")
        );
    }
}

#[test]
fn subtype_write_failure_aborts_import_instead_of_being_ignored() {
    let mut store = original_store();
    let before = store.all_cards().unwrap();
    store
        .conn()
        .execute_batch(
            "CREATE TRIGGER reject_subtype BEFORE INSERT ON subtypes
        WHEN NEW.name = 'Bird' BEGIN SELECT RAISE(ABORT, 'subtype write failed'); END;",
        )
        .unwrap();
    assert!(
        mtg_oracle::import(&mut store, EXPORT.as_bytes())
            .unwrap_err()
            .contains("subtype write failed")
    );
    assert_eq!(store.all_cards().unwrap(), before);
}

#[test]
fn metadata_write_failure_also_rolls_back_card_updates() {
    let mut store = original_store();
    let before = store.all_cards().unwrap();
    store
        .conn()
        .execute_batch(
            "CREATE TRIGGER reject_metadata BEFORE INSERT ON meta
        WHEN NEW.key = 'cards_imported' BEGIN SELECT RAISE(ABORT, 'metadata write failed'); END;",
        )
        .unwrap();
    assert!(
        mtg_oracle::import(&mut store, EXPORT.as_bytes())
            .unwrap_err()
            .contains("metadata write failed")
    );
    assert_eq!(store.all_cards().unwrap(), before);
    assert_eq!(
        store.meta("cards_imported").unwrap().as_deref(),
        Some("original")
    );
}

#[test]
fn trailing_array_content_is_rejected_before_committing() {
    for suffix in ["garbage", "[]", "{}"] {
        let mut store = original_store();
        let before = store.all_cards().unwrap();
        let export = format!("{EXPORT}\n{suffix}");
        assert!(
            mtg_oracle::import(&mut store, export.as_bytes()).is_err(),
            "accepted {suffix}"
        );
        assert_eq!(store.all_cards().unwrap(), before);
        assert_eq!(
            store.meta("cards_imported").unwrap().as_deref(),
            Some("original")
        );
    }
    let mut store = original_store();
    assert_eq!(
        mtg_oracle::import(&mut store, format!("{EXPORT}\n\t  ").as_bytes())
            .unwrap()
            .imported,
        2
    );
}

#[test]
fn refresh_timestamp_failure_preserves_cards_and_previous_metadata() {
    let mut store = original_store();
    store.set_meta("last_import_unix", "123").unwrap();
    let before = store.all_cards().unwrap();
    store
        .conn()
        .execute_batch(
            "CREATE TRIGGER reject_refresh BEFORE INSERT ON meta
        WHEN NEW.key = 'last_import_unix' BEGIN SELECT RAISE(ABORT, 'refresh write failed'); END;",
        )
        .unwrap();
    let error = mtg_oracle::fetch::import_from(&mut store, EXPORT.as_bytes()).unwrap_err();
    assert!(error.to_string().contains("refresh write failed"));
    assert_eq!(store.all_cards().unwrap(), before);
    assert_eq!(
        store.meta("cards_imported").unwrap().as_deref(),
        Some("original")
    );
    assert_eq!(
        store.meta("last_import_unix").unwrap().as_deref(),
        Some("123")
    );
}

#[test]
fn compressed_import_records_the_refresh_timestamp_with_the_cards() {
    use std::io::Write;
    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    gzip.write_all(EXPORT.as_bytes()).unwrap();
    let compressed = gzip.finish().unwrap();
    let mut store = Store::in_memory().unwrap();
    let report = mtg_oracle::fetch::import_from(&mut store, compressed.as_slice()).unwrap();
    assert_eq!(report.imported, 2);
    assert_eq!(store.card_count().unwrap(), 2);
    assert!(
        store
            .meta("last_import_unix")
            .unwrap()
            .unwrap()
            .parse::<u64>()
            .unwrap()
            > 0
    );
    assert_eq!(store.meta("cards_imported").unwrap().as_deref(), Some("2"));
}

fn gzip(bytes: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(bytes).unwrap();
    encoder.finish().unwrap()
}

#[test]
fn gzip_detection_handles_a_reader_that_delivers_one_byte_at_a_time() {
    struct OneByte<R>(R);
    impl<R: std::io::Read> std::io::Read for OneByte<R> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let len = buf.len().min(1);
            self.0.read(&mut buf[..len])
        }
    }
    let compressed = gzip(EXPORT.as_bytes());
    let mut store = Store::in_memory().unwrap();
    assert_eq!(
        mtg_oracle::import(&mut store, OneByte(compressed.as_slice()))
            .unwrap()
            .imported,
        2
    );
}

#[test]
fn excessive_gzip_nesting_is_rejected_without_changing_the_database() {
    let mut bytes = EXPORT.as_bytes().to_vec();
    for _ in 0..4 {
        bytes = gzip(&bytes);
    }
    let mut store = original_store();
    assert_eq!(
        mtg_oracle::import(&mut store, bytes.as_slice())
            .unwrap()
            .imported,
        2
    );
    let before = store.all_cards().unwrap();
    bytes = gzip(&bytes);
    assert!(
        mtg_oracle::import(&mut store, bytes.as_slice())
            .unwrap_err()
            .contains("nested gzip")
    );
    assert_eq!(store.all_cards().unwrap(), before);
}

#[test]
fn concatenated_gzip_members_import_every_json_line() {
    let rows: Vec<serde_json::Value> = serde_json::from_str(EXPORT).unwrap();
    let mut compressed = Vec::new();
    for row in rows {
        compressed.extend(gzip(
            format!("{}\n", serde_json::to_string(&row).unwrap()).as_bytes(),
        ));
    }
    let mut store = Store::in_memory().unwrap();
    let report = mtg_oracle::fetch::import_from(&mut store, compressed.as_slice()).unwrap();
    assert_eq!(report.imported, 2);
    assert_eq!(store.card_count().unwrap(), 2);
    assert!(store.find_exact("New Scout").unwrap().is_some());
    assert!(
        store
            .find_exact("Changed Front // Rejected Back")
            .unwrap()
            .is_some()
    );
}

#[test]
fn a_truncated_later_gzip_member_rolls_back_earlier_rows() {
    let rows: Vec<serde_json::Value> = serde_json::from_str(EXPORT).unwrap();
    let mut compressed = gzip(format!("{}\n", rows[0]).as_bytes());
    let mut later = gzip(format!("{}\n", rows[1]).as_bytes());
    later.truncate(later.len() - 3);
    compressed.extend(later);
    let mut store = original_store();
    let before = store.all_cards().unwrap();
    assert!(mtg_oracle::fetch::import_from(&mut store, compressed.as_slice()).is_err());
    assert_eq!(store.all_cards().unwrap(), before);
    assert_eq!(
        store.meta("cards_imported").unwrap().as_deref(),
        Some("original")
    );
    assert_eq!(store.meta("last_import_unix").unwrap(), None);
}
