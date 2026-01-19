use crate::commands::{print_json, Context};
use crate::notify::{Notifier, StdoutNotifier};
use crate::util::{format_timestamp_date, local_offset, now_utc};
use anyhow::Result;
use clap::Args;
use knotter_config::NotificationBackend;
use knotter_core::dto::{ContactListItemDto, ReminderOutputDto};
use knotter_core::rules::{compute_due_state, validate_soon_days};

#[cfg(feature = "desktop-notify")]
use crate::notify::DesktopNotifier;
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
            tags: tag_names,
        });
    }

    let output = ReminderOutputDto::from_items(items);

    if ctx.json {
        print_json(&output)?;
    } else if !notify_requested {
        print_human(&output);
    }

    if notify_requested {
        notify(&output, ctx.json, backend)?;
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

fn notify(output: &ReminderOutputDto, json_mode: bool, backend: NotificationBackend) -> Result<()> {
    if output.is_empty() {
        return Ok(());
    }

    let title = "knotter reminders";
    let body = notification_body(output, 5);

    if backend == NotificationBackend::Stdout {
        if json_mode {
            return Err(anyhow::anyhow!(
                "stdout notifications are unavailable in --json mode; drop --json or use desktop backend"
            ));
        }
        print_human(output);
        return Ok(());
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
                eprintln!("desktop notification failed, falling back to stdout: {err}");
            }
        }
    }

    #[cfg(not(feature = "desktop-notify"))]
    {
        if json_mode {
            return Err(anyhow::anyhow!(
                "desktop notifications unavailable (build with desktop-notify feature)"
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
    lines.join("\n")
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
