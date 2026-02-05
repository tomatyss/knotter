use knotter_store::error::StoreError;
use knotter_store::repo::{ContactNew, ContactSourceNew};
use knotter_store::Store;
use rusqlite::params;

#[test]
fn contact_sources_upsert_and_find() {
    let store = Store::open_in_memory().expect("open store");
    store.migrate().expect("migrate");
    let now = 1_700_000_000;

    let contact = store
        .contacts()
        .create(
            now,
            ContactNew {
                display_name: "Ada".to_string(),
                email: None,
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: None,
                cadence_days: None,
                archived_at: None,
            },
        )
        .expect("create contact");

    store
        .contact_sources()
        .upsert(
            now,
            ContactSourceNew {
                contact_id: contact.id,
                source: "carddav:test".to_string(),
                external_id: "uid-1".to_string(),
            },
        )
        .expect("insert source");

    let found = store
        .contact_sources()
        .find_contact_id("carddav:test", "uid-1")
        .expect("find source");
    assert_eq!(found, Some(contact.id));
    let found = store
        .contact_sources()
        .find_contact_id("carddav:test", "UID-1")
        .expect("find source");
    assert_eq!(found, Some(contact.id));

    store
        .contact_sources()
        .upsert(
            now + 10,
            ContactSourceNew {
                contact_id: contact.id,
                source: "carddav:test".to_string(),
                external_id: "uid-1".to_string(),
            },
        )
        .expect("update source");
}

#[test]
fn contact_sources_rejects_duplicate_external_id() {
    let store = Store::open_in_memory().expect("open store");
    store.migrate().expect("migrate");
    let now = 1_700_000_000;

    let first = store
        .contacts()
        .create(
            now,
            ContactNew {
                display_name: "Ada".to_string(),
                email: None,
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: None,
                cadence_days: None,
                archived_at: None,
            },
        )
        .expect("create contact");
    let second = store
        .contacts()
        .create(
            now + 10,
            ContactNew {
                display_name: "Ada 2".to_string(),
                email: None,
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: None,
                cadence_days: None,
                archived_at: None,
            },
        )
        .expect("create contact");

    store
        .contact_sources()
        .upsert(
            now,
            ContactSourceNew {
                contact_id: first.id,
                source: "carddav:test".to_string(),
                external_id: "uid-1".to_string(),
            },
        )
        .expect("insert source");

    let err = store
        .contact_sources()
        .upsert(
            now + 10,
            ContactSourceNew {
                contact_id: second.id,
                source: "carddav:test".to_string(),
                external_id: "uid-1".to_string(),
            },
        )
        .expect_err("reject duplicate");

    assert!(matches!(err, StoreError::DuplicateContactSource(_, _)));
}

#[test]
fn contact_sources_rejects_case_insensitive_duplicate_external_id() {
    let store = Store::open_in_memory().expect("open store");
    store.migrate().expect("migrate");
    let now = 1_700_000_000;

    let first = store
        .contacts()
        .create(
            now,
            ContactNew {
                display_name: "Ada".to_string(),
                email: None,
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: None,
                cadence_days: None,
                archived_at: None,
            },
        )
        .expect("create contact");
    let second = store
        .contacts()
        .create(
            now + 10,
            ContactNew {
                display_name: "Ada 2".to_string(),
                email: None,
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: None,
                cadence_days: None,
                archived_at: None,
            },
        )
        .expect("create contact");

    store
        .contact_sources()
        .upsert(
            now,
            ContactSourceNew {
                contact_id: first.id,
                source: "carddav:test".to_string(),
                external_id: "uid-1".to_string(),
            },
        )
        .expect("insert source");

    let err = store
        .contact_sources()
        .upsert(
            now + 10,
            ContactSourceNew {
                contact_id: second.id,
                source: "carddav:test".to_string(),
                external_id: "UID-1".to_string(),
            },
        )
        .expect_err("reject duplicate");

    assert!(matches!(err, StoreError::DuplicateContactSource(_, _)));
}

#[test]
fn contact_sources_case_insensitive_matches() {
    let store = Store::open_in_memory().expect("open store");
    store.migrate().expect("migrate");
    let now = 1_700_000_000;

    let contact = store
        .contacts()
        .create(
            now,
            ContactNew {
                display_name: "Ada".to_string(),
                email: None,
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: None,
                cadence_days: None,
                archived_at: None,
            },
        )
        .expect("create contact");

    store
        .contact_sources()
        .upsert(
            now,
            ContactSourceNew {
                contact_id: contact.id,
                source: "carddav:test".to_string(),
                external_id: "Uid-Abc".to_string(),
            },
        )
        .expect("insert source");

    let matches = store
        .contact_sources()
        .find_case_insensitive_matches("carddav:test", "uid-abc")
        .expect("case-insensitive match");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].contact_id, contact.id);
    assert_eq!(matches[0].external_id, "Uid-Abc");
    assert_eq!(matches[0].source, "carddav:test");
}

#[test]
fn contact_sources_case_insensitive_returns_multiple_matches() {
    let store = Store::open_in_memory().expect("open store");
    store.migrate().expect("migrate");
    let now = 1_700_000_000;

    let first = store
        .contacts()
        .create(
            now,
            ContactNew {
                display_name: "Ada".to_string(),
                email: None,
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: None,
                cadence_days: None,
                archived_at: None,
            },
        )
        .expect("create contact");
    let second = store
        .contacts()
        .create(
            now + 10,
            ContactNew {
                display_name: "Ada 2".to_string(),
                email: None,
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: None,
                cadence_days: None,
                archived_at: None,
            },
        )
        .expect("create contact");

    let conn = store.connection();
    conn.execute(
        "INSERT INTO contact_sources (contact_id, source, external_id, external_id_norm, created_at, updated_at)\n         VALUES (?1, 'carddav:test', 'uid-abc', NULL, ?2, ?2);",
        params![first.id.to_string(), now],
    )
    .expect("insert source");
    conn.execute(
        "INSERT INTO contact_sources (contact_id, source, external_id, external_id_norm, created_at, updated_at)\n         VALUES (?1, 'carddav:test', 'UID-ABC', NULL, ?2, ?2);",
        params![second.id.to_string(), now + 10],
    )
    .expect("insert source");

    let matches = store
        .contact_sources()
        .find_case_insensitive_matches("carddav:test", "Uid-Abc")
        .expect("case-insensitive match");
    assert_eq!(matches.len(), 2);
}

#[test]
fn contact_sources_case_insensitive_matches_trimmed_external_id() {
    let store = Store::open_in_memory().expect("open store");
    store.migrate().expect("migrate");
    let now = 1_700_000_000;

    let contact = store
        .contacts()
        .create(
            now,
            ContactNew {
                display_name: "Ada".to_string(),
                email: None,
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: None,
                cadence_days: None,
                archived_at: None,
            },
        )
        .expect("create contact");

    let conn = store.connection();
    conn.execute(
        "INSERT INTO contact_sources (contact_id, source, external_id, external_id_norm, created_at, updated_at)\n         VALUES (?1, 'carddav:test', '  UID-ABC  ', 'uid-abc', ?2, ?2);",
        params![contact.id.to_string(), now],
    )
    .expect("insert source");

    let matches = store
        .contact_sources()
        .find_case_insensitive_matches("carddav:test", "uid-abc")
        .expect("case-insensitive match");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].contact_id, contact.id);
    assert_eq!(matches[0].external_id, "  UID-ABC  ");
}
