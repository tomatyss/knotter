use knotter_store::error::StoreError;
use knotter_store::repo::ContactNew;
use knotter_store::Store;

#[test]
fn contact_emails_track_primary_and_secondary() {
    let store = Store::open_in_memory().expect("open in memory");
    store.migrate().expect("migrate");
    let now = 1_700_000_000;

    let contact = store
        .contacts()
        .create(
            now,
            ContactNew {
                display_name: "Ada".to_string(),
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

    let emails = store
        .emails()
        .list_for_contact(&contact.id)
        .expect("list emails");
    assert_eq!(emails.len(), 1);
    assert!(emails[0].is_primary);
    assert_eq!(emails[0].email, "ada@example.com");

    store
        .emails()
        .add_email(now, &contact.id, "ada.work@example.com", None, false)
        .expect("add email");
    let emails = store
        .emails()
        .list_for_contact(&contact.id)
        .expect("list emails");
    assert_eq!(emails.len(), 2);
    assert_eq!(emails[0].email, "ada@example.com");
}

#[test]
fn contact_emails_enforces_global_uniqueness() {
    let store = Store::open_in_memory().expect("open in memory");
    store.migrate().expect("migrate");
    let now = 1_700_000_000;

    let contact_a = store
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
        .expect("create contact a");

    let contact_b = store
        .contacts()
        .create(
            now,
            ContactNew {
                display_name: "Grace".to_string(),
                email: None,
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: None,
                cadence_days: None,
                archived_at: None,
            },
        )
        .expect("create contact b");

    let err = store
        .emails()
        .add_email(now, &contact_b.id, "ada@example.com", None, false)
        .unwrap_err();
    assert!(matches!(err, StoreError::DuplicateEmail(_)));

    let emails = store
        .emails()
        .list_for_contact(&contact_a.id)
        .expect("list emails");
    assert_eq!(emails.len(), 1);
}

#[test]
fn replace_emails_rejects_duplicates_without_partial_update() {
    let store = Store::open_in_memory().expect("open in memory");
    store.migrate().expect("migrate");
    let now = 1_700_000_000;

    let contact_a = store
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
        .expect("create contact a");

    let contact_b = store
        .contacts()
        .create(
            now,
            ContactNew {
                display_name: "Grace".to_string(),
                email: Some("grace@example.com".to_string()),
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: None,
                cadence_days: None,
                archived_at: None,
            },
        )
        .expect("create contact b");

    let err = store
        .emails()
        .replace_emails(
            now,
            &contact_b.id,
            vec![
                "grace@example.com".to_string(),
                "ada@example.com".to_string(),
            ],
            Some("grace@example.com".to_string()),
            None,
        )
        .unwrap_err();
    assert!(matches!(err, StoreError::DuplicateEmail(_)));

    let emails_b = store
        .emails()
        .list_emails_for_contact(&contact_b.id)
        .expect("list emails b");
    assert_eq!(emails_b, vec!["grace@example.com".to_string()]);

    let contact_b_fresh = store
        .contacts()
        .get(contact_b.id)
        .expect("load contact b")
        .expect("contact b");
    assert_eq!(contact_b_fresh.email.as_deref(), Some("grace@example.com"));
    assert_eq!(contact_a.email.as_deref(), Some("ada@example.com"));
}

#[test]
fn add_email_sets_primary_when_missing() {
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

    store
        .emails()
        .set_primary(&contact.id, None)
        .expect("clear primary");
    let updated = store
        .emails()
        .add_email(now, &contact.id, "ada@example.com", None, true)
        .expect("add email");
    assert!(!updated);

    let emails = store
        .emails()
        .list_for_contact(&contact.id)
        .expect("list emails");
    assert!(emails.iter().any(|email| email.is_primary));
}

#[test]
fn replace_emails_includes_primary_when_missing() {
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

    store
        .emails()
        .replace_emails(
            now,
            &contact.id,
            vec!["ada.work@example.com".to_string()],
            Some("ada@example.com".to_string()),
            None,
        )
        .expect("replace emails");

    let emails = store
        .emails()
        .list_for_contact(&contact.id)
        .expect("list emails");
    assert!(emails.iter().any(|email| email.email == "ada@example.com"));
    assert!(emails.iter().any(|email| email.is_primary));

    let contact = store
        .contacts()
        .get(contact.id)
        .expect("get contact")
        .expect("contact");
    assert_eq!(contact.email.as_deref(), Some("ada@example.com"));
}

#[test]
fn replace_emails_preserves_metadata_for_existing_entries() {
    let store = Store::open_in_memory().expect("open in memory");
    store.migrate().expect("migrate");
    let now = 1_700_000_000;

    let contact = store
        .contacts()
        .create_with_emails_and_tags(
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
            Vec::new(),
            vec![
                "ada@example.com".to_string(),
                "ada.work@example.com".to_string(),
            ],
            Some("cli"),
        )
        .expect("create contact");

    let original = store
        .emails()
        .list_for_contact(&contact.id)
        .expect("list emails");
    let mut original_map = std::collections::HashMap::new();
    for email in &original {
        original_map.insert(
            email.email.clone(),
            (email.created_at, email.source.clone()),
        );
    }

    store
        .emails()
        .replace_emails(
            now + 10,
            &contact.id,
            vec![
                "ada.work@example.com".to_string(),
                "ada@example.com".to_string(),
            ],
            Some("ada.work@example.com".to_string()),
            Some("tui"),
        )
        .expect("replace emails");

    let updated = store
        .emails()
        .list_for_contact(&contact.id)
        .expect("list emails");
    let primary = updated
        .iter()
        .find(|email| email.is_primary)
        .expect("primary");
    assert_eq!(primary.email, "ada.work@example.com");

    for email in &updated {
        let (created_at, source) = original_map
            .get(&email.email)
            .expect("existing email metadata");
        assert_eq!(&email.created_at, created_at);
        assert_eq!(&email.source, source);
    }
}
