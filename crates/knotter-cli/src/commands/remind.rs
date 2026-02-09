use crate::commands::remind_fmt::{notification_body, print_human, RandomContactPick};
use crate::commands::{print_json, Context};
use crate::error::invalid_input;
use crate::notify::{Notifier, StdoutNotifier};
use crate::util::{local_offset, now_utc};
use anyhow::Result;
use clap::Args;
use knotter_config::{NotificationBackend, NotificationsEmailConfig};
use knotter_core::dto::{ContactListItemDto, DateReminderItemDto, ReminderOutputDto};
use knotter_core::rules::{compute_due_state, validate_soon_days};

#[cfg(feature = "desktop-notify")]
use crate::notify::DesktopNotifier;
#[cfg(feature = "desktop-notify")]
use anyhow::Context as _;
#[cfg(feature = "desktop-notify")]
use tracing::warn;

#[cfg(feature = "email-notify")]
use crate::commands::remind_fmt::{email_body, email_subject};
#[cfg(feature = "email-notify")]
use crate::notify::EmailNotifier;

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

    let random_picks = if notify_requested
        && output.is_empty()
        && ctx.config.notifications.random_contacts_if_no_reminders > 0
    {
        ctx.store
            .contacts()
            .list_random_active(
                ctx.config.notifications.random_contacts_if_no_reminders,
                &[],
            )?
            .into_iter()
            .map(|contact| RandomContactPick {
                id: contact.id,
                display_name: contact.display_name,
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    if ctx.json {
        print_json(&output)?;
    } else if !notify_requested {
        print_human(&output, &random_picks);
    }

    if notify_requested {
        notify(&output, &random_picks, ctx.json, backend, email_config)?;
    }

    Ok(())
}

fn notify(
    output: &ReminderOutputDto,
    random_picks: &[RandomContactPick],
    json_mode: bool,
    backend: NotificationBackend,
    email_config: Option<&NotificationsEmailConfig>,
) -> Result<()> {
    #[cfg(not(feature = "email-notify"))]
    let _ = email_config;

    if output.is_empty() && random_picks.is_empty() {
        return Ok(());
    }

    let title = "knotter reminders";
    let body = notification_body(output, random_picks, 5);

    if backend == NotificationBackend::Stdout {
        if json_mode {
            return Err(invalid_input(
                "stdout notifications are unavailable in --json mode; drop --json or use desktop backend",
            ));
        }
        print_human(output, random_picks);
        return Ok(());
    }

    if backend == NotificationBackend::Email {
        #[cfg(feature = "email-notify")]
        {
            let email_config = email_config.ok_or_else(|| {
                invalid_input("notifications.email config is required for email backend")
            })?;
            let subject = email_subject(output, random_picks, &email_config.subject_prefix);
            let body = email_body(output, random_picks);
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
