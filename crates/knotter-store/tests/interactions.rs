use knotter_core::domain::InteractionKind;
use knotter_core::rules::schedule_next;
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

#[test]
fn touch_contact_inserts_interaction_and_reschedules_when_requested() {
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
                next_touchpoint_at: Some(now + 123),
                cadence_days: Some(7),
                archived_at: None,
            },
        )
        .expect("create contact");

    let touched = store
        .interactions()
        .touch_contact(now, contact.id, false)
        .expect("touch contact");
    assert_eq!(touched.contact_id, contact.id);

    let after_touch = store
        .contacts()
        .get(contact.id)
        .expect("get contact")
        .expect("contact exists");
    assert_eq!(after_touch.next_touchpoint_at, Some(now + 123));

    store
        .interactions()
        .touch_contact(now, contact.id, true)
        .expect("touch contact reschedule");

    let after_reschedule = store
        .contacts()
        .get(contact.id)
        .expect("get contact")
        .expect("contact exists");
    let expected_next = schedule_next(now, 7).expect("schedule");
    assert_eq!(after_reschedule.next_touchpoint_at, Some(expected_next));
}

#[test]
fn add_with_reschedule_updates_next_touchpoint() {
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
                cadence_days: Some(14),
                archived_at: None,
            },
        )
        .expect("create contact");

    let occurred_at = now - 60;
    store
        .interactions()
        .add_with_reschedule(
            now,
            InteractionNew {
                contact_id: contact.id,
                occurred_at,
                created_at: now,
                kind: InteractionKind::Call,
                note: "catch-up".to_string(),
                follow_up_at: None,
            },
            true,
        )
        .expect("add interaction with reschedule");

    let after = store
        .contacts()
        .get(contact.id)
        .expect("get contact")
        .expect("contact exists");
    let expected = schedule_next(now, 14).expect("schedule");
    assert_eq!(after.next_touchpoint_at, Some(expected));
}

#[test]
fn interactions_latest_occurred_at_for_contacts() {
    let store = Store::open_in_memory().expect("open in memory");
    store.migrate().expect("migrate");

    let now = 1_700_000_000;
    let first = store
        .contacts()
        .create(
            now,
            ContactNew {
                display_name: "First".to_string(),
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
            now,
            ContactNew {
                display_name: "Second".to_string(),
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
            contact_id: first.id,
            occurred_at: now - 200,
            created_at: now,
            kind: InteractionKind::Call,
            note: "first early".to_string(),
            follow_up_at: None,
        })
        .expect("add interaction");
    store
        .interactions()
        .add(InteractionNew {
            contact_id: first.id,
            occurred_at: now - 50,
            created_at: now,
            kind: InteractionKind::Email,
            note: "first latest".to_string(),
            follow_up_at: None,
        })
        .expect("add interaction");

    store
        .interactions()
        .add(InteractionNew {
            contact_id: second.id,
            occurred_at: now - 10,
            created_at: now,
            kind: InteractionKind::Text,
            note: "second latest".to_string(),
            follow_up_at: None,
        })
        .expect("add interaction");

    let latest = store
        .interactions()
        .latest_occurred_at_for_contacts(&[first.id, second.id])
        .expect("latest interactions");
    assert_eq!(latest.get(&first.id), Some(&(now - 50)));
    assert_eq!(latest.get(&second.id), Some(&(now - 10)));
}
