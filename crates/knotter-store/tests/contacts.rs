use knotter_core::domain::{ContactId, TagName};
use knotter_store::repo::{ContactNew, ContactUpdate, EmailOps};
use knotter_store::Store;

#[test]
fn contact_crud_roundtrip() {
    let store = Store::open_in_memory().expect("open in memory");
    store.migrate().expect("migrate");

    let now = 1_700_000_000;
    let contact = store
        .contacts()
        .create(
            now,
            ContactNew {
                display_name: "Ada Lovelace".to_string(),
                email: Some("ada@example.com".to_string()),
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: None,
                cadence_days: Some(30),
                archived_at: None,
            },
        )
        .expect("create contact");

    let fetched = store
        .contacts()
        .get(contact.id)
        .expect("get contact")
        .expect("contact exists");
    assert_eq!(fetched.display_name, "Ada Lovelace");

    let updated = store
        .contacts()
        .update(
            now + 10,
            contact.id,
            ContactUpdate {
                display_name: Some("Ada Byron".to_string()),
                email: Some(None),
                ..Default::default()
            },
        )
        .expect("update contact");
    assert_eq!(updated.display_name, "Ada Byron");
    assert!(updated.email.is_none());
    let emails = store
        .emails()
        .list_emails_for_contact(&contact.id)
        .expect("list emails");
    assert!(emails.is_empty());

    store.contacts().delete(contact.id).expect("delete contact");
    let missing = store.contacts().get(contact.id).expect("get contact");
    assert!(missing.is_none());
}

#[test]
fn list_by_email_is_case_insensitive_and_prefers_active() {
    let store = Store::open_in_memory().expect("open in memory");
    store.migrate().expect("migrate");

    let now = 1_700_000_000;
    let active = store
        .contacts()
        .create(
            now,
            ContactNew {
                display_name: "Ada Lovelace".to_string(),
                email: Some("Ada@Example.com".to_string()),
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: None,
                cadence_days: None,
                archived_at: None,
            },
        )
        .expect("create contact");
    let archived = store
        .contacts()
        .create(
            now + 10,
            ContactNew {
                display_name: "Ada (Archived)".to_string(),
                email: Some("ada.archive@example.com".to_string()),
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: None,
                cadence_days: None,
                archived_at: Some(now + 20),
            },
        )
        .expect("create archived contact");

    let found = store
        .contacts()
        .list_by_email("ada@example.com")
        .expect("find");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, active.id);
    assert_ne!(found[0].id, archived.id);
}

#[test]
fn tags_attach_and_list() {
    let store = Store::open_in_memory().expect("open in memory");
    store.migrate().expect("migrate");
    let now = 1_700_000_000;

    let contact = store
        .contacts()
        .create(
            now,
            ContactNew {
                display_name: "Grace Hopper".to_string(),
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

    let tag = TagName::new("Friends").expect("tag");
    store
        .tags()
        .add_tag_to_contact(&contact.id.to_string(), tag)
        .expect("add tag");

    let tags = store
        .tags()
        .list_for_contact(&contact.id.to_string())
        .expect("list tags");
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].name.as_str(), "friends");

    let counts = store.tags().list_with_counts().expect("tag counts");
    assert_eq!(counts.len(), 1);
    assert_eq!(counts[0].0.name.as_str(), "friends");
    assert_eq!(counts[0].1, 1);

    let tag = TagName::new("friends").expect("tag");
    store
        .tags()
        .remove_tag_from_contact(&contact.id.to_string(), tag)
        .expect("remove tag");
    let tags = store
        .tags()
        .list_for_contact(&contact.id.to_string())
        .expect("list tags after remove");
    assert!(tags.is_empty());
}

#[test]
fn archive_and_unarchive_contact() {
    let store = Store::open_in_memory().expect("open in memory");
    store.migrate().expect("migrate");

    let now = 1_700_000_000;
    let contact = store
        .contacts()
        .create(
            now,
            ContactNew {
                display_name: "Ada Lovelace".to_string(),
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

    let archived = store
        .contacts()
        .archive(now + 10, contact.id)
        .expect("archive contact");
    assert!(archived.archived_at.is_some());

    let unarchived = store
        .contacts()
        .unarchive(now + 20, contact.id)
        .expect("unarchive contact");
    assert!(unarchived.archived_at.is_none());
}

#[test]
fn list_names_for_contacts_handles_large_inputs() {
    let store = Store::open_in_memory().expect("open in memory");
    store.migrate().expect("migrate");

    let now = 1_700_000_000;
    let contact = store
        .contacts()
        .create(
            now,
            ContactNew {
                display_name: "Ada Lovelace".to_string(),
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
        .tags()
        .add_tag_to_contact(&contact.id.to_string(), TagName::new("friends").unwrap())
        .expect("add tag");

    let mut ids = Vec::with_capacity(1100);
    ids.push(contact.id);
    for _ in 0..1099 {
        ids.push(ContactId::new());
    }

    let map = store
        .tags()
        .list_names_for_contacts(&ids)
        .expect("bulk tag list");
    let tags = map.get(&contact.id).expect("tag list");
    assert_eq!(tags, &vec!["friends".to_string()]);
}

#[test]
fn update_with_email_ops_updates_timestamp() {
    let store = Store::open_in_memory().expect("open in memory");
    store.migrate().expect("migrate");
    let now = 1_700_000_000;

    let contact = store
        .contacts()
        .create(
            now,
            ContactNew {
                display_name: "Ada".to_string(),
                email: Some("ada@example.com".to_string()),
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: None,
                cadence_days: None,
                archived_at: None,
            },
        )
        .expect("create contact");

    let updated = store
        .contacts()
        .update_with_email_ops(
            now + 10,
            contact.id,
            ContactUpdate::default(),
            EmailOps::Mutate {
                clear: false,
                add: vec!["ada.work@example.com".to_string()],
                remove: Vec::new(),
                source: None,
            },
        )
        .expect("update");

    assert_eq!(updated.updated_at, now + 10);
}

#[test]
fn list_names_for_contacts_does_not_touch_main_temp_contact_ids_table() {
    let store = Store::open_in_memory().expect("open in memory");
    store.migrate().expect("migrate");

    let now = 1_700_000_000;
    let contact = store
        .contacts()
        .create(
            now,
            ContactNew {
                display_name: "Grace Hopper".to_string(),
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
        .connection()
        .execute(
            "CREATE TABLE temp_contact_ids (id TEXT PRIMARY KEY, marker TEXT);",
            [],
        )
        .expect("create main table");
    store
        .connection()
        .execute(
            "INSERT INTO temp_contact_ids (id, marker) VALUES (?1, ?2);",
            ["keep", "persist"],
        )
        .expect("insert marker");

    let map = store
        .tags()
        .list_names_for_contacts(&[contact.id])
        .expect("bulk tag list");
    assert!(!map.contains_key(&contact.id));

    let marker: String = store
        .connection()
        .query_row(
            "SELECT marker FROM temp_contact_ids WHERE id = ?1;",
            ["keep"],
            |row| row.get(0),
        )
        .expect("marker row");
    assert_eq!(marker, "persist");
}
