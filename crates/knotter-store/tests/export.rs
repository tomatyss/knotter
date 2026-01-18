use knotter_core::domain::InteractionKind;
use knotter_store::repo::contacts::ContactNew;
use knotter_store::repo::interactions::InteractionNew;
use knotter_store::Store;

#[test]
fn list_interactions_for_contacts_groups_and_orders() {
    let store = Store::open_in_memory().expect("open in memory");
    store.migrate().expect("migrate");

    let now = 1_700_000_000;
    let contact_one = store
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
        .expect("create contact one");

    let contact_two = store
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
        .expect("create contact two");

    store
        .interactions()
        .add(InteractionNew {
            contact_id: contact_one.id,
            occurred_at: 100,
            created_at: 100,
            kind: InteractionKind::Call,
            note: "First".to_string(),
            follow_up_at: None,
        })
        .expect("add interaction 1");

    store
        .interactions()
        .add(InteractionNew {
            contact_id: contact_one.id,
            occurred_at: 200,
            created_at: 200,
            kind: InteractionKind::Email,
            note: "Second".to_string(),
            follow_up_at: None,
        })
        .expect("add interaction 2");

    store
        .interactions()
        .add(InteractionNew {
            contact_id: contact_two.id,
            occurred_at: 150,
            created_at: 150,
            kind: InteractionKind::Text,
            note: "Third".to_string(),
            follow_up_at: None,
        })
        .expect("add interaction 3");

    let map = store
        .interactions()
        .list_for_contacts(&[contact_one.id, contact_two.id])
        .expect("list interactions");

    let first = map.get(&contact_one.id).expect("contact one");
    assert_eq!(first.len(), 2);
    assert_eq!(first[0].note, "Second");
    assert_eq!(first[1].note, "First");

    let second = map.get(&contact_two.id).expect("contact two");
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].note, "Third");
}
