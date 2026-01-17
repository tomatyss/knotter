use chrono::{FixedOffset, TimeZone, Utc};
use knotter_store::repo::ContactNew;
use knotter_store::Store;

#[test]
fn list_due_contacts_only_includes_overdue_today_soon() {
    let store = Store::open_in_memory().expect("open");
    store.migrate().expect("migrate");

    let now = Utc
        .with_ymd_and_hms(2024, 1, 10, 12, 0, 0)
        .unwrap()
        .timestamp();
    let offset = FixedOffset::east_opt(0).unwrap();

    store
        .contacts()
        .create(
            now,
            ContactNew {
                display_name: "Overdue".to_string(),
                email: None,
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: Some(now - 3600),
                cadence_days: None,
                archived_at: None,
            },
        )
        .expect("create overdue");

    store
        .contacts()
        .create(
            now,
            ContactNew {
                display_name: "Today".to_string(),
                email: None,
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: Some(now + 3600),
                cadence_days: None,
                archived_at: None,
            },
        )
        .expect("create today");

    store
        .contacts()
        .create(
            now,
            ContactNew {
                display_name: "Soon".to_string(),
                email: None,
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: Some(now + 2 * 86_400),
                cadence_days: None,
                archived_at: None,
            },
        )
        .expect("create soon");

    store
        .contacts()
        .create(
            now,
            ContactNew {
                display_name: "Scheduled".to_string(),
                email: None,
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: Some(now + 30 * 86_400),
                cadence_days: None,
                archived_at: None,
            },
        )
        .expect("create scheduled");

    store
        .contacts()
        .create(
            now,
            ContactNew {
                display_name: "Unscheduled".to_string(),
                email: None,
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: None,
                cadence_days: None,
                archived_at: None,
            },
        )
        .expect("create unscheduled");

    store
        .contacts()
        .create(
            now,
            ContactNew {
                display_name: "Archived".to_string(),
                email: None,
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: Some(now - 7200),
                cadence_days: None,
                archived_at: Some(now - 60),
            },
        )
        .expect("create archived");

    let results = store
        .contacts()
        .list_due_contacts(now, 7, offset)
        .expect("list due contacts");

    let names: Vec<String> = results.into_iter().map(|c| c.display_name).collect();
    assert_eq!(names, vec!["Overdue", "Today", "Soon"]);
}
