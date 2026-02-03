use knotter_core::domain::ContactDateKind;
use knotter_store::repo::{
    ContactDateNew, ContactMergeOptions, ContactNew, ContactSourceNew, InteractionNew,
    MergeCandidateCreate, MergeCandidateStatus, TelegramAccountNew, TelegramMessageRecord,
};
use knotter_store::Store;

#[test]
fn merge_candidates_dedupe_open_pairs() {
    let store = Store::open_in_memory().expect("open store");
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
                display_name: "Ada L".to_string(),
                email: Some("ada@work.test".to_string()),
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: None,
                cadence_days: None,
                archived_at: None,
            },
        )
        .expect("create contact b");

    let created = store
        .merge_candidates()
        .create(
            now,
            contact_b.id,
            contact_a.id,
            MergeCandidateCreate {
                reason: "test".to_string(),
                source: Some("import".to_string()),
                preferred_contact_id: Some(contact_a.id),
            },
        )
        .expect("create candidate");

    assert!(created.created);
    assert_eq!(created.candidate.status, MergeCandidateStatus::Open);

    let deduped = store
        .merge_candidates()
        .create(
            now,
            contact_a.id,
            contact_b.id,
            MergeCandidateCreate {
                reason: "test".to_string(),
                source: Some("import".to_string()),
                preferred_contact_id: Some(contact_a.id),
            },
        )
        .expect("dedupe candidate");

    assert!(!deduped.created);
    assert_eq!(created.candidate.id, deduped.candidate.id);
}

#[test]
fn merge_contacts_unifies_emails_tags_and_interactions() {
    let store = Store::open_in_memory().expect("open store");
    store.migrate().expect("migrate");
    let now = 1_700_000_000;

    let primary = store
        .contacts()
        .create(
            now,
            ContactNew {
                display_name: "Ada".to_string(),
                email: Some("ada@example.com".to_string()),
                phone: Some("111".to_string()),
                handle: None,
                timezone: None,
                next_touchpoint_at: Some(2_000),
                cadence_days: Some(30),
                archived_at: None,
            },
        )
        .expect("create primary");

    let secondary = store
        .contacts()
        .create(
            now,
            ContactNew {
                display_name: "Ada Lovelace".to_string(),
                email: Some("ada@work.test".to_string()),
                phone: None,
                handle: Some("@ada".to_string()),
                timezone: None,
                next_touchpoint_at: Some(1_000),
                cadence_days: None,
                archived_at: Some(now),
            },
        )
        .expect("create secondary");

    store
        .emails()
        .add_email(now, &secondary.id, "ada@alt.test", Some("test"), false)
        .expect("add secondary email");

    store
        .tags()
        .set_contact_tags(
            &primary.id.to_string(),
            vec![knotter_core::domain::TagName::new("friend").unwrap()],
        )
        .expect("set primary tags");
    store
        .tags()
        .set_contact_tags(
            &secondary.id.to_string(),
            vec![knotter_core::domain::TagName::new("work").unwrap()],
        )
        .expect("set secondary tags");

    store
        .interactions()
        .add(InteractionNew {
            contact_id: primary.id,
            occurred_at: now - 10,
            created_at: now - 10,
            kind: knotter_core::domain::InteractionKind::Call,
            note: "Call".to_string(),
            follow_up_at: None,
        })
        .expect("add interaction primary");
    store
        .interactions()
        .add(InteractionNew {
            contact_id: secondary.id,
            occurred_at: now - 5,
            created_at: now - 5,
            kind: knotter_core::domain::InteractionKind::Email,
            note: "Email".to_string(),
            follow_up_at: None,
        })
        .expect("add interaction secondary");

    let merged = store
        .contacts()
        .merge_contacts(
            now + 20,
            primary.id,
            secondary.id,
            ContactMergeOptions::default(),
        )
        .expect("merge contacts");

    assert_eq!(merged.display_name, "Ada");
    assert_eq!(merged.email, Some("ada@example.com".to_string()));
    assert_eq!(merged.next_touchpoint_at, Some(1_000));
    assert!(merged.archived_at.is_none());

    let tags = store
        .tags()
        .list_for_contact(&primary.id.to_string())
        .expect("list tags");
    let tag_names: Vec<_> = tags
        .into_iter()
        .map(|t| t.name.as_str().to_string())
        .collect();
    assert!(tag_names.contains(&"friend".to_string()));
    assert!(tag_names.contains(&"work".to_string()));

    let interactions = store
        .interactions()
        .list_for_contact(primary.id, 50, 0)
        .expect("list interactions");
    assert_eq!(interactions.len(), 2);

    let emails = store
        .emails()
        .list_emails_for_contact(&primary.id)
        .expect("list emails");
    assert!(emails.contains(&"ada@example.com".to_string()));
    assert!(emails.contains(&"ada@work.test".to_string()));
    assert!(emails.contains(&"ada@alt.test".to_string()));

    let missing = store.contacts().get(secondary.id).expect("get secondary");
    assert!(missing.is_none());
}

#[test]
fn merge_contacts_moves_telegram_accounts_and_messages() {
    let store = Store::open_in_memory().expect("open store");
    store.migrate().expect("migrate");
    let now = 1_700_000_000;

    let primary = store
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
        .expect("create primary");

    let secondary = store
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
        .expect("create secondary");

    store
        .telegram_accounts()
        .upsert(
            now,
            TelegramAccountNew {
                contact_id: secondary.id,
                telegram_user_id: 42,
                username: Some("ada".to_string()),
                phone: None,
                first_name: Some("Ada".to_string()),
                last_name: Some("Lovelace".to_string()),
                source: Some("telegram:test".to_string()),
            },
        )
        .expect("upsert telegram account");

    store
        .telegram_sync()
        .record_message(&TelegramMessageRecord {
            account: "primary".to_string(),
            peer_id: 42,
            message_id: 100,
            contact_id: secondary.id,
            occurred_at: now - 5,
            direction: "inbound".to_string(),
            snippet: Some("hello".to_string()),
            created_at: now,
        })
        .expect("record telegram message");

    store
        .contacts()
        .merge_contacts(
            now + 10,
            primary.id,
            secondary.id,
            ContactMergeOptions::default(),
        )
        .expect("merge contacts");

    let accounts = store
        .telegram_accounts()
        .list_for_contact(primary.id)
        .expect("list telegram accounts");
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].telegram_user_id, 42);

    let mapped = store
        .telegram_accounts()
        .find_contact_id_by_user_id(42)
        .expect("lookup telegram user");
    assert_eq!(mapped, Some(primary.id));

    let contact_id: String = store
        .connection()
        .query_row(
            "SELECT contact_id FROM telegram_messages WHERE account = ?1 AND peer_id = ?2;",
            rusqlite::params!["primary", 42],
            |row| row.get(0),
        )
        .expect("query telegram message");
    assert_eq!(contact_id, primary.id.to_string());
}

#[test]
fn merge_contacts_moves_contact_sources() {
    let store = Store::open_in_memory().expect("open store");
    store.migrate().expect("migrate");
    let now = 1_700_000_000;

    let primary = store
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
        .expect("create primary");

    let secondary = store
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
        .expect("create secondary");

    store
        .contact_sources()
        .upsert(
            now,
            ContactSourceNew {
                contact_id: secondary.id,
                source: "carddav:test".to_string(),
                external_id: "uid-1".to_string(),
            },
        )
        .expect("insert source");

    let options = ContactMergeOptions::default();
    store
        .contacts()
        .merge_contacts(now + 10, primary.id, secondary.id, options)
        .expect("merge contacts");

    let found = store
        .contact_sources()
        .find_contact_id("carddav:test", "uid-1")
        .expect("find source");
    assert_eq!(found, Some(primary.id));
}

#[test]
fn merge_contacts_resolves_open_merge_candidates_for_secondary() {
    let store = Store::open_in_memory().expect("open store");
    store.migrate().expect("migrate");
    let now = 1_700_000_000;

    let primary = store
        .contacts()
        .create(
            now,
            ContactNew {
                display_name: "Primary".to_string(),
                email: Some("primary@example.com".to_string()),
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: None,
                cadence_days: None,
                archived_at: None,
            },
        )
        .expect("create primary");

    let secondary = store
        .contacts()
        .create(
            now,
            ContactNew {
                display_name: "Secondary".to_string(),
                email: Some("secondary@example.com".to_string()),
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: None,
                cadence_days: None,
                archived_at: None,
            },
        )
        .expect("create secondary");

    let other = store
        .contacts()
        .create(
            now,
            ContactNew {
                display_name: "Other".to_string(),
                email: Some("other@example.com".to_string()),
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: None,
                cadence_days: None,
                archived_at: None,
            },
        )
        .expect("create other");

    let candidate_primary = store
        .merge_candidates()
        .create(
            now,
            primary.id,
            secondary.id,
            MergeCandidateCreate {
                reason: "test".to_string(),
                source: None,
                preferred_contact_id: Some(primary.id),
            },
        )
        .expect("create candidate primary");

    let candidate_secondary = store
        .merge_candidates()
        .create(
            now,
            secondary.id,
            other.id,
            MergeCandidateCreate {
                reason: "test".to_string(),
                source: None,
                preferred_contact_id: Some(other.id),
            },
        )
        .expect("create candidate secondary");

    store
        .contacts()
        .merge_contacts(
            now + 5,
            primary.id,
            secondary.id,
            ContactMergeOptions::default(),
        )
        .expect("merge contacts");

    let merged = store
        .merge_candidates()
        .get(candidate_primary.candidate.id)
        .expect("get primary candidate")
        .expect("missing primary candidate");
    assert_eq!(merged.status, MergeCandidateStatus::Merged);

    let dismissed = store
        .merge_candidates()
        .get(candidate_secondary.candidate.id)
        .expect("get secondary candidate")
        .expect("missing secondary candidate");
    assert_eq!(dismissed.status, MergeCandidateStatus::Dismissed);
}

#[test]
fn merge_contacts_dedupes_contact_dates() {
    let store = Store::open_in_memory().expect("open");
    store.migrate().expect("migrate");
    let now = 1_700_000_000;

    let primary = store
        .contacts()
        .create(
            now,
            ContactNew {
                display_name: "Primary".to_string(),
                email: None,
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: None,
                cadence_days: None,
                archived_at: None,
            },
        )
        .expect("create primary");
    let secondary = store
        .contacts()
        .create(
            now,
            ContactNew {
                display_name: "Secondary".to_string(),
                email: None,
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: None,
                cadence_days: None,
                archived_at: None,
            },
        )
        .expect("create secondary");

    store
        .contact_dates()
        .upsert(
            now,
            ContactDateNew {
                contact_id: primary.id,
                kind: ContactDateKind::Birthday,
                label: None,
                month: 1,
                day: 1,
                year: None,
                source: None,
            },
        )
        .expect("add primary date");
    store
        .contact_dates()
        .upsert(
            now,
            ContactDateNew {
                contact_id: secondary.id,
                kind: ContactDateKind::Birthday,
                label: None,
                month: 1,
                day: 1,
                year: Some(1990),
                source: None,
            },
        )
        .expect("add secondary date");

    store
        .contacts()
        .merge_contacts(
            now,
            primary.id,
            secondary.id,
            ContactMergeOptions::default(),
        )
        .expect("merge contacts");

    let dates = store
        .contact_dates()
        .list_for_contact(primary.id)
        .expect("list dates");
    assert_eq!(dates.len(), 1);
    assert_eq!(dates[0].month, 1);
    assert_eq!(dates[0].day, 1);
    assert_eq!(dates[0].year, Some(1990));
}

#[test]
fn merge_contacts_prefers_secondary_primary_email() {
    let store = Store::open_in_memory().expect("open store");
    store.migrate().expect("migrate");
    let now = 1_700_000_000;

    let primary = store
        .contacts()
        .create(
            now,
            ContactNew {
                display_name: "Primary".to_string(),
                email: Some("primary@example.com".to_string()),
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: None,
                cadence_days: None,
                archived_at: None,
            },
        )
        .expect("create primary");

    let secondary = store
        .contacts()
        .create(
            now,
            ContactNew {
                display_name: "Secondary".to_string(),
                email: Some("secondary@example.com".to_string()),
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: None,
                cadence_days: None,
                archived_at: None,
            },
        )
        .expect("create secondary");

    let options = ContactMergeOptions {
        prefer: knotter_store::repo::MergePreference::Secondary,
        ..ContactMergeOptions::default()
    };
    let merged = store
        .contacts()
        .merge_contacts(now + 10, primary.id, secondary.id, options)
        .expect("merge");

    assert_eq!(merged.email, Some("secondary@example.com".to_string()));

    let emails = store
        .emails()
        .list_for_contact(&primary.id)
        .expect("list emails");
    let primary_email = emails
        .iter()
        .find(|email| email.is_primary)
        .map(|email| email.email.clone());
    assert_eq!(primary_email, Some("secondary@example.com".to_string()));
}
