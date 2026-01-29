use crate::commands::{print_json, Context};
use crate::error::invalid_input;
use crate::notify::{Notifier, StdoutNotifier};
use crate::util::{format_date_parts, format_timestamp_date, local_offset, now_utc};
use anyhow::Result;
use clap::Args;
use knotter_config::{NotificationBackend, NotificationsEmailConfig};
use knotter_core::dto::{ContactListItemDto, DateReminderItemDto, ReminderOutputDto};
use knotter_core::rules::{compute_due_state, validate_soon_days};
#[cfg(feature = "desktop-notify")]
use tracing::warn;

#[cfg(feature = "desktop-notify")]
use crate::notify::DesktopNotifier;
#[cfg(feature = "email-notify")]
use crate::notify::EmailNotifier;
#[cfg(feature = "desktop-notify")]
use anyhow::Context as _;

#[derive(Debug, Args)]
pub struct RemindArgs {
    #[arg(long)]
    pub soon_days: Option<i64>,
    #[arg(long)]
    pub notify: bool,
    #[arg(long, conflicts_with = "notify")]
    pub no_notify: bool,
}

pub fn remind(ctx: &Context<'_>, args: RemindArgs) -> Result<()> {
    let soon_days = validate_soon_days(args.soon_days.unwrap_or(ctx.config.due_soon_days))?;
    let notify_requested = if args.no_notify {
        false
    } else if args.notify {
        true
    } else if ctx.json {
        false
    } else {
        ctx.config.notifications.enabled
    };
    let backend = ctx.config.notifications.backend;
    let email_config = ctx.config.notifications.email.as_ref();

    let now = now_utc();
    let offset = local_offset();

    let contacts = ctx
        .store
        .contacts()
        .list_due_contacts(now, soon_days, offset)?;
    let contact_ids = contacts
        .iter()
        .map(|contact| contact.id)
        .collect::<Vec<_>>();
    let tags_by_contact = ctx.store.tags().list_names_for_contacts(&contact_ids)?;

    let mut items = Vec::with_capacity(contacts.len());
    for contact in contacts {
        let tag_names = tags_by_contact
            .get(&contact.id)
            .cloned()
            .unwrap_or_default();
        let due_state = compute_due_state(now, contact.next_touchpoint_at, soon_days, offset)?;
        items.push(ContactListItemDto {
            id: contact.id,
            display_name: contact.display_name,
            due_state,
            next_touchpoint_at: contact.next_touchpoint_at,
            archived_at: contact.archived_at,
            tags: tag_names,
        });
    }

    let mut output = ReminderOutputDto::from_items(items);
    let dates_today = ctx
        .store
        .contact_dates()
        .list_today(now, offset)?
        .into_iter()
        .map(|item| DateReminderItemDto {
            contact_id: item.contact_id,
            display_name: item.display_name,
            kind: item.kind,
            label: item.label,
            month: item.month,
            day: item.day,
            year: item.year,
        })
        .collect();
    output.dates_today = dates_today;

    if ctx.json {
        print_json(&output)?;
    } else if !notify_requested {
        print_human(&output);
    }

    if notify_requested {
        notify(&output, ctx.json, backend, email_config)?;
    }

    Ok(())
}

fn print_human(output: &ReminderOutputDto) {
    if output.is_empty() {
        println!("no reminders");
        return;
    }

    print_bucket("overdue", &output.overdue);
    print_bucket("today", &output.today);
    print_bucket("soon", &output.soon);
    print_date_bucket("dates today", &output.dates_today);
}

fn print_bucket(label: &str, items: &[ContactListItemDto]) {
    if items.is_empty() {
        return;
    }

    println!("{label}:");
    for item in items {
        let date = item
            .next_touchpoint_at
            .map(format_timestamp_date)
            .unwrap_or_else(|| "-".to_string());
        let tag_suffix = if item.tags.is_empty() {
            String::new()
        } else {
            let tags = item
                .tags
                .iter()
                .map(|tag| format!("#{}", tag))
                .collect::<Vec<_>>()
                .join(" ");
            format!(" {}", tags)
        };
        println!(
            "  {}  {}  {}{}",
            item.id, item.display_name, date, tag_suffix
        );
    }
}

fn print_date_bucket(label: &str, items: &[DateReminderItemDto]) {
    if items.is_empty() {
        return;
    }

    println!("{label}:");
    for item in items {
        let date = format_date_parts(item.month, item.day, item.year);
        let label = format_date_label(item);
        println!(
            "  {}  {}  {}  {}",
            item.contact_id, item.display_name, label, date
        );
    }
}

fn notify(
    output: &ReminderOutputDto,
    json_mode: bool,
    backend: NotificationBackend,
    email_config: Option<&NotificationsEmailConfig>,
) -> Result<()> {
    #[cfg(not(feature = "email-notify"))]
    let _ = email_config;

    if output.is_empty() {
        return Ok(());
    }

    let title = "knotter reminders";
    let body = notification_body(output, 5);

    if backend == NotificationBackend::Stdout {
        if json_mode {
            return Err(invalid_input(
                "stdout notifications are unavailable in --json mode; drop --json or use desktop backend",
            ));
        }
        print_human(output);
        return Ok(());
    }

    if backend == NotificationBackend::Email {
        #[cfg(feature = "email-notify")]
        {
            let email_config = email_config.ok_or_else(|| {
                invalid_input("notifications.email config is required for email backend")
            })?;
            let subject = email_subject(output, &email_config.subject_prefix);
            let body = email_body(output);
            let notifier = EmailNotifier::new(email_config)?;
            notifier.send(&subject, &body)?;
            return Ok(());
        }

        #[cfg(not(feature = "email-notify"))]
        {
            return Err(invalid_input(
                "email notifications unavailable (build with email-notify feature)",
            ));
        }
    }

    #[cfg(feature = "desktop-notify")]
    {
        let desktop = DesktopNotifier;
        match desktop.send(title, &body) {
            Ok(()) => return Ok(()),
            Err(err) => {
                if json_mode {
                    return Err(err).context("desktop notification failed");
                }
                warn!(error = %err, "desktop notification failed, falling back to stdout");
            }
        }
    }

    #[cfg(not(feature = "desktop-notify"))]
    {
        if json_mode {
            return Err(invalid_input(
                "desktop notifications unavailable (build with desktop-notify feature)",
            ));
        }
    }

    if json_mode {
        return Ok(());
    }

    let stdout = StdoutNotifier;
    stdout.send(title, &body)
}

fn notification_body(output: &ReminderOutputDto, max_names: usize) -> String {
    let mut lines = Vec::new();
    if !output.overdue.is_empty() {
        lines.push(format!(
            "Overdue ({}): {}",
            output.overdue.len(),
            join_names(&output.overdue, max_names)
        ));
    }
    if !output.today.is_empty() {
        lines.push(format!(
            "Today ({}): {}",
            output.today.len(),
            join_names(&output.today, max_names)
        ));
    }
    if !output.soon.is_empty() {
        lines.push(format!(
            "Soon ({}): {}",
            output.soon.len(),
            join_names(&output.soon, max_names)
        ));
    }
    if !output.dates_today.is_empty() {
        lines.push(format!(
            "Dates today ({}): {}",
            output.dates_today.len(),
            join_date_names(&output.dates_today, max_names)
        ));
    }
    lines.join("\n")
}

#[cfg(feature = "email-notify")]
fn email_subject(output: &ReminderOutputDto, prefix: &str) -> String {
    let total =
        output.overdue.len() + output.today.len() + output.soon.len() + output.dates_today.len();
    let trimmed = prefix.trim();
    if total == 0 {
        if trimmed.is_empty() {
            "knotter reminders".to_string()
        } else {
            trimmed.to_string()
        }
    } else if trimmed.is_empty() {
        format!(
            "knotter reminders (overdue {}, today {}, soon {}, dates {})",
            output.overdue.len(),
            output.today.len(),
            output.soon.len(),
            output.dates_today.len()
        )
    } else {
        format!(
            "{} (overdue {}, today {}, soon {}, dates {})",
            trimmed,
            output.overdue.len(),
            output.today.len(),
            output.soon.len(),
            output.dates_today.len()
        )
    }
}

#[cfg(feature = "email-notify")]
fn email_body(output: &ReminderOutputDto) -> String {
    let mut lines = Vec::new();
    push_email_bucket(&mut lines, "Overdue", &output.overdue);
    push_email_bucket(&mut lines, "Today", &output.today);
    push_email_bucket(&mut lines, "Soon", &output.soon);
    push_email_date_bucket(&mut lines, "Dates today", &output.dates_today);
    lines.join("\n")
}

#[cfg(feature = "email-notify")]
fn push_email_bucket(lines: &mut Vec<String>, label: &str, items: &[ContactListItemDto]) {
    if items.is_empty() {
        return;
    }
    lines.push(format!("{label} ({})", items.len()));
    for item in items {
        let date = item
            .next_touchpoint_at
            .map(format_timestamp_date)
            .unwrap_or_else(|| "-".to_string());
        let tag_suffix = if item.tags.is_empty() {
            String::new()
        } else {
            let tags = item
                .tags
                .iter()
                .map(|tag| format!("#{}", tag))
                .collect::<Vec<_>>()
                .join(" ");
            format!(" {}", tags)
        };
        lines.push(format!(
            "  {}  {}  {}{}",
            item.id, item.display_name, date, tag_suffix
        ));
    }
    lines.push(String::new());
}

#[cfg(feature = "email-notify")]
fn push_email_date_bucket(lines: &mut Vec<String>, label: &str, items: &[DateReminderItemDto]) {
    if items.is_empty() {
        return;
    }
    lines.push(format!("{label} ({})", items.len()));
    for item in items {
        let date = format_date_parts(item.month, item.day, item.year);
        let label = format_date_label(item);
        lines.push(format!("  {}  {}  {}", item.display_name, label, date));
    }
    lines.push(String::new());
}

fn join_names(items: &[ContactListItemDto], max_names: usize) -> String {
    let mut names = items
        .iter()
        .take(max_names)
        .map(|item| item.display_name.clone())
        .collect::<Vec<_>>();
    let remaining = items.len().saturating_sub(max_names);
    if remaining > 0 {
        names.push(format!("+{} more", remaining));
    }
    names.join(", ")
}

fn join_date_names(items: &[DateReminderItemDto], max_names: usize) -> String {
    let mut names = items
        .iter()
        .take(max_names)
        .map(|item| format!("{} ({})", item.display_name, format_date_label(item)))
        .collect::<Vec<_>>();
    let remaining = items.len().saturating_sub(max_names);
    if remaining > 0 {
        names.push(format!("+{} more", remaining));
    }
    names.join(", ")
}

fn format_date_label(item: &DateReminderItemDto) -> String {
    use knotter_core::domain::ContactDateKind;
    match item.kind {
        ContactDateKind::Birthday => "Birthday".to_string(),
        ContactDateKind::NameDay => match item.label.as_deref() {
            Some(label) => format!("Name day ({})", label),
            None => "Name day".to_string(),
        },
        ContactDateKind::Custom => item.label.clone().unwrap_or_else(|| "Custom".to_string()),
    }
}

#[cfg(all(test, feature = "email-notify"))]
mod tests {
    use super::{email_body, email_subject, push_email_bucket};
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
            tags: vec!["friends".to_string()],
        }
    }

    #[test]
    fn email_subject_includes_counts() {
        let output = ReminderOutputDto {
            overdue: vec![item("Ada", DueState::Overdue, Some(1))],
            today: vec![item("Grace", DueState::Today, Some(2))],
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

        let subject = email_subject(&output, "Knotter");
        assert!(subject.contains("Knotter"));
        assert!(subject.contains("overdue 1"));
        assert!(subject.contains("today 1"));
        assert!(subject.contains("soon 0"));
        assert!(subject.contains("dates 1"));
    }

    #[test]
    fn email_body_formats_buckets() {
        let output = ReminderOutputDto {
            overdue: vec![item("Ada", DueState::Overdue, Some(1))],
            today: vec![],
            soon: vec![item("Grace", DueState::Soon, Some(2))],
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

        let body = email_body(&output);
        assert!(body.contains("Overdue (1)"));
        assert!(body.contains("Soon (1)"));
        assert!(body.contains("Dates today (1)"));
        assert!(body.contains("Ada"));
        assert!(body.contains("Grace"));
        assert!(body.contains("Anniversary"));

        let mut lines = Vec::new();
        push_email_bucket(&mut lines, "Overdue", &output.overdue);
        assert!(lines.iter().any(|line| line.contains("Overdue (1)")));
    }
}

#[cfg(test)]
mod reminder_tests {
    use super::notification_body;
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

        let body = notification_body(&output, 5);
        assert!(body.contains("Dates today (1)"));
        assert!(body.contains("Grace (Birthday)"));
    }
}
