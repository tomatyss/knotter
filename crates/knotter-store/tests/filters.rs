use chrono::{FixedOffset, TimeZone, Utc};
use knotter_core::parse_filter;
use knotter_store::query::ContactQuery;
use knotter_store::repo::ContactNew;
use knotter_store::Store;

#[test]
fn filter_tags_and_due() {
    let store = Store::open_in_memory().expect("open");
    store.migrate().expect("migrate");

    let now = Utc.with_ymd_and_hms(2024, 1, 10, 12, 0, 0).unwrap().timestamp();
    let offset = FixedOffset::east_opt(0).unwrap();

    let overdue = store
        .contacts()
        .create(
            now,
            ContactNew {
                display_name: "Ada".to_string(),
                email: None,
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: Some(now - 3600),
                cadence_days: None,
                archived_at: None,
            },
        )
        .expect("create contact");

    let today = store
        .contacts()
        .create(
            now,
            ContactNew {
                display_name: "Grace".to_string(),
                email: None,
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: Some(now + 3600),
                cadence_days: None,
                archived_at: None,
            },
        )
        .expect("create contact");

    let _soon = store
        .contacts()
        .create(
            now,
            ContactNew {
                display_name: "Tim".to_string(),
                email: None,
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: Some(now + 2 * 86_400),
                cadence_days: None,
                archived_at: None,
            },
        )
        .expect("create contact");

    let unscheduled = store
        .contacts()
        .create(
            now,
            ContactNew {
                display_name: "Linus".to_string(),
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
        .add_tag_to_contact(&overdue.id.to_string(), knotter_core::TagName::new("friends").unwrap())
        .expect("tag overdue");
    store
        .tags()
        .add_tag_to_contact(&today.id.to_string(), knotter_core::TagName::new("friends").unwrap())
        .expect("tag today");
    store
        .tags()
        .add_tag_to_contact(&today.id.to_string(), knotter_core::TagName::new("work").unwrap())
        .expect("tag today work");

    let filter = parse_filter("#friends").expect("parse filter");
    let query = ContactQuery::from_filter(&filter).expect("build query");
    let results = store
        .contacts()
        .list_contacts(&query, now, 7, offset)
        .expect("list contacts");
    assert_eq!(results.len(), 2);

    let filter = parse_filter("#friends #work").expect("parse filter");
    let query = ContactQuery::from_filter(&filter).expect("build query");
    let results = store
        .contacts()
        .list_contacts(&query, now, 7, offset)
        .expect("list contacts");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].display_name, "Grace");

    let filter = parse_filter("due:overdue").expect("parse filter");
    let query = ContactQuery::from_filter(&filter).expect("build query");
    let results = store
        .contacts()
        .list_contacts(&query, now, 7, offset)
        .expect("list contacts");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].display_name, "Ada");

    let filter = parse_filter("due:today").expect("parse filter");
    let query = ContactQuery::from_filter(&filter).expect("build query");
    let results = store
        .contacts()
        .list_contacts(&query, now, 7, offset)
        .expect("list contacts");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].display_name, "Grace");

    let filter = parse_filter("due:soon").expect("parse filter");
    let query = ContactQuery::from_filter(&filter).expect("build query");
    let results = store
        .contacts()
        .list_contacts(&query, now, 7, offset)
        .expect("list contacts");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].display_name, "Tim");

    let filter = parse_filter("due:none").expect("parse filter");
    let query = ContactQuery::from_filter(&filter).expect("build query");
    let results = store
        .contacts()
        .list_contacts(&query, now, 7, offset)
        .expect("list contacts");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].display_name, "Linus");
}
