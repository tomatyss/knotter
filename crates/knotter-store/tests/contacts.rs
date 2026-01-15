use knotter_core::domain::TagName;
use knotter_store::repo::{ContactNew, ContactUpdate};
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

    store.contacts().delete(contact.id).expect("delete contact");
    let missing = store.contacts().get(contact.id).expect("get contact");
    assert!(missing.is_none());
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
