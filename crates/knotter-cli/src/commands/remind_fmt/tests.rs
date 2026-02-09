use super::{notification_body, RandomContactPick};
use knotter_core::domain::{ContactDateKind, ContactId};
use knotter_core::dto::{ContactListItemDto, DateReminderItemDto, ReminderOutputDto};
use knotter_core::rules::DueState;

fn item(name: &str, due_state: DueState, next: Option<i64>) -> ContactListItemDto {
    ContactListItemDto {
        id: ContactId::new(),
        display_name: name.to_string(),
        due_state,
        next_touchpoint_at: next,
        archived_at: None,
        tags: vec![],
    }
}

#[test]
fn notification_body_includes_dates_today() {
    let output = ReminderOutputDto {
        overdue: vec![item("Ada", DueState::Overdue, Some(1))],
        today: vec![],
        soon: vec![],
        dates_today: vec![DateReminderItemDto {
            contact_id: ContactId::new(),
            display_name: "Grace".to_string(),
            kind: ContactDateKind::Birthday,
            label: None,
            month: 3,
            day: 5,
            year: None,
        }],
    };

    let body = notification_body(&output, &[], 5);
    assert!(body.contains("Dates today (1)"));
    assert!(body.contains("Grace (Birthday)"));
}

#[test]
fn notification_body_includes_random_contacts() {
    let output = ReminderOutputDto {
        overdue: vec![],
        today: vec![],
        soon: vec![],
        dates_today: vec![],
    };
    let picks = vec![
        RandomContactPick {
            id: ContactId::new(),
            display_name: "Ada".to_string(),
        },
        RandomContactPick {
            id: ContactId::new(),
            display_name: "Grace".to_string(),
        },
    ];

    let body = notification_body(&output, &picks, 5);
    assert!(body.contains("Random contacts (2)"));
    assert!(body.contains("Ada"));
    assert!(body.contains("Grace"));
}

#[cfg(feature = "email-notify")]
mod email {
    use super::*;
    use crate::commands::remind_fmt::{email_body, email_subject};

    fn tagged_item(name: &str, due_state: DueState, next: Option<i64>) -> ContactListItemDto {
        ContactListItemDto {
            id: ContactId::new(),
            display_name: name.to_string(),
            due_state,
            next_touchpoint_at: next,
            archived_at: None,
            tags: vec!["friends".to_string()],
        }
    }

    #[test]
    fn email_subject_includes_counts() {
        let output = ReminderOutputDto {
            overdue: vec![tagged_item("Ada", DueState::Overdue, Some(1))],
            today: vec![tagged_item("Grace", DueState::Today, Some(2))],
            soon: vec![],
            dates_today: vec![DateReminderItemDto {
                contact_id: ContactId::new(),
                display_name: "Tim".to_string(),
                kind: ContactDateKind::Birthday,
                label: None,
                month: 1,
                day: 2,
                year: None,
            }],
        };

        let subject = email_subject(&output, &[], "Knotter");
        assert!(subject.contains("Knotter"));
        assert!(subject.contains("overdue 1"));
        assert!(subject.contains("today 1"));
        assert!(subject.contains("soon 0"));
        assert!(subject.contains("dates 1"));
    }

    #[test]
    fn email_body_formats_buckets() {
        let output = ReminderOutputDto {
            overdue: vec![tagged_item("Ada", DueState::Overdue, Some(1))],
            today: vec![],
            soon: vec![tagged_item("Grace", DueState::Soon, Some(2))],
            dates_today: vec![DateReminderItemDto {
                contact_id: ContactId::new(),
                display_name: "Tim".to_string(),
                kind: ContactDateKind::Custom,
                label: Some("Anniversary".to_string()),
                month: 2,
                day: 14,
                year: None,
            }],
        };

        let body = email_body(&output, &[]);
        assert!(body.contains("Overdue (1)"));
        assert!(body.contains("Soon (1)"));
        assert!(body.contains("Dates today (1)"));
        assert!(body.contains("Ada"));
        assert!(body.contains("Grace"));
        assert!(body.contains("Anniversary"));
    }
}
