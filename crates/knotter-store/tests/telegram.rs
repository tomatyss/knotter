use knotter_store::repo::{
    ContactNew, TelegramAccountNew, TelegramAccountsRepo, TelegramMessageRecord, TelegramSyncRepo,
    TelegramSyncState,
};
use knotter_store::Store;

#[test]
fn telegram_accounts_upsert_and_lookup() {
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

    let repo = TelegramAccountsRepo::new(store.connection());
    repo.upsert(
        now,
        TelegramAccountNew {
            contact_id: contact.id,
            telegram_user_id: 42,
            username: Some("@Ada".to_string()),
            phone: Some("+15551234567".to_string()),
            first_name: Some("Ada".to_string()),
            last_name: Some("Lovelace".to_string()),
            source: Some("telegram:test".to_string()),
        },
    )
    .expect("upsert account");

    let found = repo
        .find_contact_id_by_user_id(42)
        .expect("find by user id");
    assert_eq!(found, Some(contact.id));

    let found = repo
        .list_contact_ids_by_username("ada")
        .expect("find by username");
    assert_eq!(found, vec![contact.id]);

    let found = repo
        .list_contact_ids_by_username("@ADA")
        .expect("find by username with @");
    assert_eq!(found, vec![contact.id]);

    let accounts = repo.list_for_contact(contact.id).expect("list accounts");
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].telegram_user_id, 42);
}

#[test]
fn telegram_sync_records_messages_and_state() {
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

    let repo = TelegramSyncRepo::new(store.connection());
    let inserted = repo
        .record_message(&TelegramMessageRecord {
            account: "primary".to_string(),
            peer_id: 42,
            message_id: 100,
            contact_id: contact.id,
            occurred_at: now - 10,
            direction: "inbound".to_string(),
            snippet: Some("hi".to_string()),
            created_at: now,
        })
        .expect("record message");
    assert!(inserted);

    let inserted = repo
        .record_message(&TelegramMessageRecord {
            account: "primary".to_string(),
            peer_id: 42,
            message_id: 100,
            contact_id: contact.id,
            occurred_at: now - 10,
            direction: "inbound".to_string(),
            snippet: Some("hi".to_string()),
            created_at: now,
        })
        .expect("record message duplicate");
    assert!(!inserted);

    let state = TelegramSyncState {
        account: "primary".to_string(),
        peer_id: 42,
        last_message_id: 100,
        last_seen_at: Some(now),
    };
    repo.upsert_state(&state).expect("upsert state");

    let loaded = repo
        .load_state("primary", 42)
        .expect("load state")
        .expect("state exists");
    assert_eq!(loaded.last_message_id, 100);
}
