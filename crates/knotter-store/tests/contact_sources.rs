use knotter_store::error::StoreError;
use knotter_store::repo::{ContactNew, ContactSourceNew};
use knotter_store::Store;

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
