use crate::util::{format_date_parts, format_timestamp_date};
use knotter_core::domain::ContactId;
use knotter_core::dto::{ContactListItemDto, DateReminderItemDto, ReminderOutputDto};

#[derive(Debug, Clone)]
pub(crate) struct RandomContactPick {
    pub(crate) id: ContactId,
    pub(crate) display_name: String,
}

pub(crate) fn print_human(output: &ReminderOutputDto, random_picks: &[RandomContactPick]) {
    if output.is_empty() && random_picks.is_empty() {
        println!("no reminders");
        return;
    }

    print_bucket("overdue", &output.overdue);
    print_bucket("today", &output.today);
    print_bucket("soon", &output.soon);
    print_date_bucket("dates today", &output.dates_today);
    print_random_bucket("random contacts", random_picks);
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
        let tag_suffix = format_tag_suffix(&item.tags);
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

fn print_random_bucket(label: &str, items: &[RandomContactPick]) {
    if items.is_empty() {
        return;
    }

    println!("{label}:");
    for item in items {
        println!("  {}  {}", item.id, item.display_name);
    }
}

pub(crate) fn notification_body(
    output: &ReminderOutputDto,
    random_picks: &[RandomContactPick],
    max_names: usize,
) -> String {
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
    if !random_picks.is_empty() {
        lines.push(format!(
            "Random contacts ({}): {}",
            random_picks.len(),
            join_random_names(random_picks, max_names)
        ));
    }
    lines.join("\n")
}

#[cfg(feature = "email-notify")]
pub(crate) fn email_subject(
    output: &ReminderOutputDto,
    random_picks: &[RandomContactPick],
    prefix: &str,
) -> String {
    let total = output.overdue.len()
        + output.today.len()
        + output.soon.len()
        + output.dates_today.len()
        + random_picks.len();
    let trimmed = prefix.trim();
    if total == 0 {
        if trimmed.is_empty() {
            "knotter reminders".to_string()
        } else {
            trimmed.to_string()
        }
    } else if trimmed.is_empty() {
        format!(
            "knotter reminders (overdue {}, today {}, soon {}, dates {}, random {})",
            output.overdue.len(),
            output.today.len(),
            output.soon.len(),
            output.dates_today.len(),
            random_picks.len()
        )
    } else {
        format!(
            "{} (overdue {}, today {}, soon {}, dates {}, random {})",
            trimmed,
            output.overdue.len(),
            output.today.len(),
            output.soon.len(),
            output.dates_today.len(),
            random_picks.len()
        )
    }
}

#[cfg(feature = "email-notify")]
pub(crate) fn email_body(output: &ReminderOutputDto, random_picks: &[RandomContactPick]) -> String {
    let mut lines = Vec::new();
    push_email_bucket(&mut lines, "Overdue", &output.overdue);
    push_email_bucket(&mut lines, "Today", &output.today);
    push_email_bucket(&mut lines, "Soon", &output.soon);
    push_email_date_bucket(&mut lines, "Dates today", &output.dates_today);
    push_email_random_bucket(&mut lines, "Random contacts", random_picks);
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
        let tag_suffix = format_tag_suffix(&item.tags);
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

#[cfg(feature = "email-notify")]
fn push_email_random_bucket(lines: &mut Vec<String>, label: &str, items: &[RandomContactPick]) {
    if items.is_empty() {
        return;
    }
    lines.push(format!("{label} ({})", items.len()));
    for item in items {
        lines.push(format!("  {}", item.display_name));
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

fn join_random_names(items: &[RandomContactPick], max_names: usize) -> String {
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

fn format_tag_suffix(tags: &[String]) -> String {
    if tags.is_empty() {
        return String::new();
    }
    let tags = tags
        .iter()
        .map(|tag| format!("#{}", tag))
        .collect::<Vec<_>>()
        .join(" ");
    format!(" {}", tags)
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

#[cfg(test)]
mod tests;
