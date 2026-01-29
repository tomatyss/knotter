use chrono::{FixedOffset, TimeZone, Utc};
use knotter_core::domain::ContactDateKind;
use knotter_store::repo::ContactDateNew;
use knotter_store::repo::ContactNew;
use knotter_store::Store;

#[test]
fn contact_dates_upsert_updates_year() {
    let store = Store::open_in_memory().expect("open");
    store.migrate().expect("migrate");

    let now = Utc
        .with_ymd_and_hms(2024, 1, 10, 12, 0, 0)
        .unwrap()
        .timestamp();

    let contact = store
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
        .expect("create contact");

    store
        .contact_dates()
        .upsert(
            now,
            ContactDateNew {
                contact_id: contact.id,
                kind: ContactDateKind::Birthday,
                label: None,
                month: 12,
                day: 3,
                year: None,
                source: Some("test".to_string()),
            },
        )
        .expect("upsert date");

    store
        .contact_dates()
        .upsert(
            now + 10,
            ContactDateNew {
                contact_id: contact.id,
                kind: ContactDateKind::Birthday,
                label: None,
                month: 12,
                day: 3,
                year: Some(1990),
                source: Some("test".to_string()),
            },
        )
        .expect("upsert date");

    let dates = store
        .contact_dates()
        .list_for_contact(contact.id)
        .expect("list");
    assert_eq!(dates.len(), 1);
    assert_eq!(dates[0].year, Some(1990));
}

#[test]
fn contact_dates_upsert_clears_year_when_missing() {
    let store = Store::open_in_memory().expect("open");
    store.migrate().expect("migrate");

    let now = Utc
        .with_ymd_and_hms(2024, 1, 10, 12, 0, 0)
        .unwrap()
        .timestamp();

    let contact = store
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
        .expect("create contact");

    store
        .contact_dates()
        .upsert(
            now,
            ContactDateNew {
                contact_id: contact.id,
                kind: ContactDateKind::Birthday,
                label: None,
                month: 12,
                day: 3,
                year: Some(1990),
                source: Some("test".to_string()),
            },
        )
        .expect("upsert date");

    store
        .contact_dates()
        .upsert(
            now + 10,
            ContactDateNew {
                contact_id: contact.id,
                kind: ContactDateKind::Birthday,
                label: None,
                month: 12,
                day: 3,
                year: None,
                source: Some("test".to_string()),
            },
        )
        .expect("upsert date");

    let dates = store
        .contact_dates()
        .list_for_contact(contact.id)
        .expect("list");
    assert_eq!(dates.len(), 1);
    assert_eq!(dates[0].year, None);
}

#[test]
fn contact_dates_upsert_preserve_year_keeps_existing() {
    let store = Store::open_in_memory().expect("open");
    store.migrate().expect("migrate");

    let now = Utc
        .with_ymd_and_hms(2024, 1, 10, 12, 0, 0)
        .unwrap()
        .timestamp();

    let contact = store
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
        .expect("create contact");

    store
        .contact_dates()
        .upsert_preserve_year(
            now,
            ContactDateNew {
                contact_id: contact.id,
                kind: ContactDateKind::Birthday,
                label: None,
                month: 12,
                day: 3,
                year: Some(1990),
                source: Some("test".to_string()),
            },
        )
        .expect("upsert date");

    store
        .contact_dates()
        .upsert_preserve_year(
            now + 10,
            ContactDateNew {
                contact_id: contact.id,
                kind: ContactDateKind::Birthday,
                label: None,
                month: 12,
                day: 3,
                year: None,
                source: Some("test".to_string()),
            },
        )
        .expect("upsert date");

    let dates = store
        .contact_dates()
        .list_for_contact(contact.id)
        .expect("list");
    assert_eq!(dates.len(), 1);
    assert_eq!(dates[0].year, Some(1990));
}

#[test]
fn list_today_includes_leap_day_on_feb_28_non_leap_year() {
    let store = Store::open_in_memory().expect("open");
    store.migrate().expect("migrate");

    let now = Utc
        .with_ymd_and_hms(2023, 2, 28, 12, 0, 0)
        .unwrap()
        .timestamp();
    let offset = FixedOffset::east_opt(0).unwrap();

    let contact = store
        .contacts()
        .create(
            now,
            ContactNew {
                display_name: "Leap".to_string(),
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
        .contact_dates()
        .upsert(
            now,
            ContactDateNew {
                contact_id: contact.id,
                kind: ContactDateKind::Birthday,
                label: None,
                month: 2,
                day: 29,
                year: None,
                source: None,
            },
        )
        .expect("upsert date");

    let items = store
        .contact_dates()
        .list_today(now, offset)
        .expect("list today");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].display_name, "Leap");
}

#[test]
fn contact_dates_custom_label_trigger_rejects_empty_label() {
    let store = Store::open_in_memory().expect("open");
    store.migrate().expect("migrate");

    let now = Utc
        .with_ymd_and_hms(2024, 1, 10, 12, 0, 0)
        .unwrap()
        .timestamp();

    let contact = store
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
        .expect("create contact");

    let id = knotter_core::domain::ContactDateId::new().to_string();
    let err = store
        .connection()
        .execute(
            "INSERT INTO contact_dates
             (id, contact_id, kind, label, month, day, year, created_at, updated_at, source)
             VALUES (?1, ?2, 'custom', '', 1, 1, NULL, ?3, ?3, NULL);",
            rusqlite::params![id, contact.id.to_string(), now],
        )
        .expect_err("insert should fail");
    assert!(err.to_string().contains("custom date label required"));

    let created = store
        .contact_dates()
        .upsert(
            now,
            ContactDateNew {
                contact_id: contact.id,
                kind: ContactDateKind::Custom,
                label: Some("Anniversary".to_string()),
                month: 2,
                day: 1,
                year: None,
                source: None,
            },
        )
        .expect("create date");

    let err = store
        .connection()
        .execute(
            "UPDATE contact_dates SET label = '' WHERE id = ?1;",
            [created.id.to_string()],
        )
        .expect_err("update should fail");
    assert!(err.to_string().contains("custom date label required"));
}
