use knotter_core::domain::InteractionKind;
use knotter_store::repo::{ContactNew, InteractionNew};
use knotter_store::Store;

#[test]
fn interactions_add_and_list() {
    let store = Store::open_in_memory().expect("open in memory");
    store.migrate().expect("migrate");

    let now = 1_700_000_000;
    let contact = store
        .contacts()
        .create(
            now,
            ContactNew {
                display_name: "Linus Torvalds".to_string(),
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
        .interactions()
        .add(InteractionNew {
            contact_id: contact.id,
            occurred_at: now - 100,
            created_at: now,
            kind: InteractionKind::Email,
            note: "Sent a follow-up.".to_string(),
            follow_up_at: None,
        })
        .expect("add interaction");

    store
        .interactions()
        .add(InteractionNew {
            contact_id: contact.id,
            occurred_at: now - 50,
            created_at: now,
            kind: InteractionKind::Call,
            note: "Quick call.".to_string(),
            follow_up_at: None,
        })
        .expect("add interaction");

    let list = store
        .interactions()
        .list_for_contact(contact.id, 10, 0)
        .expect("list interactions");
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].note, "Quick call.");
    assert_eq!(list[1].note, "Sent a follow-up.");
}
