use knotter_store::repo::{EmailMessageRecord, EmailSyncRepo};
use knotter_store::Store;

#[test]
fn email_sync_dedupes_null_message_id_by_account_mailbox_uid() {
    let store = Store::open_in_memory().expect("open in memory");
    store.migrate().expect("migrate");
    let now = 1_700_000_000;
    let contact = store
        .contacts()
        .create(
            now,
            knotter_store::repo::ContactNew {
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

    let repo = EmailSyncRepo::new(store.connection());
    let record = EmailMessageRecord {
        account: "test".to_string(),
        mailbox: "INBOX".to_string(),
        uidvalidity: 1,
        uid: 42,
        message_id: None,
        contact_id: contact.id,
        occurred_at: now,
        direction: "inbound".to_string(),
        subject: None,
        created_at: now,
    };

    assert!(repo.record_message(&record).expect("insert"));

    let second = record.clone();
    assert!(!repo.record_message(&second).expect("dedupe"));

    let mut third = record.clone();
    third.uidvalidity = 2;
    assert!(repo.record_message(&third).expect("uidvalidity change"));
}

#[test]
fn email_sync_dedupes_message_id_per_account() {
    let store = Store::open_in_memory().expect("open in memory");
    store.migrate().expect("migrate");
    let now = 1_700_000_000;
    let contact = store
        .contacts()
        .create(
            now,
            knotter_store::repo::ContactNew {
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

    let repo = EmailSyncRepo::new(store.connection());
    let record = EmailMessageRecord {
        account: "test".to_string(),
        mailbox: "INBOX".to_string(),
        uidvalidity: 1,
        uid: 42,
        message_id: Some("abc@example.com".to_string()),
        contact_id: contact.id,
        occurred_at: now,
        direction: "inbound".to_string(),
        subject: None,
        created_at: now,
    };

    assert!(repo.record_message(&record).expect("insert"));

    let mut second = record.clone();
    second.mailbox = "Sent".to_string();
    second.uid = 55;
    assert!(!repo.record_message(&second).expect("dedupe"));

    let mut third = record.clone();
    third.account = "other".to_string();
    third.uid = 99;
    assert!(repo.record_message(&third).expect("different account"));
}
