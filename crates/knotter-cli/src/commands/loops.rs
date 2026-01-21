use crate::commands::{print_json, Context};
use crate::error::invalid_input;
use crate::util::{format_timestamp_date, local_offset, now_utc};
use anyhow::Result;
use clap::{ArgAction, Args, Subcommand};
use knotter_config::{AppConfig, LoopAnchor};
use knotter_core::domain::ContactId;
use knotter_core::filter::parse_filter;
use knotter_core::rules::schedule_next;
use knotter_store::query::ContactQuery;
use knotter_store::repo::{ContactUpdate, ContactsRepo, InteractionsRepo, TagsRepo};
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Subcommand)]
pub enum LoopCommand {
    Apply(LoopApplyArgs),
}

#[derive(Debug, Args)]
pub struct LoopApplyArgs {
    #[arg(long)]
    pub filter: Option<String>,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub force: bool,
    #[arg(long, action = ArgAction::SetTrue, conflicts_with = "no_schedule_missing")]
    pub schedule_missing: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub no_schedule_missing: bool,
    #[arg(long)]
    pub anchor: Option<String>,
}

#[derive(Debug, Serialize)]
struct LoopApplyChange {
    id: knotter_core::domain::ContactId,
    display_name: String,
    cadence_before: Option<i32>,
    cadence_after: Option<i32>,
    next_touchpoint_before: Option<i64>,
    next_touchpoint_after: Option<i64>,
    scheduled: bool,
}

#[derive(Debug, Serialize)]
struct LoopApplyReport {
    matched: usize,
    updated: usize,
    scheduled: usize,
    skipped: usize,
    dry_run: bool,
    changes: Vec<LoopApplyChange>,
}

pub fn apply_loops(ctx: &Context<'_>, args: LoopApplyArgs) -> Result<()> {
    let policy = &ctx.config.loops.policy;
    if policy.rules.is_empty() && policy.default_cadence_days.is_none() {
        return Err(invalid_input("no loops configured"));
    }

    let filter_text = args.filter.unwrap_or_default();
    let parsed = parse_filter(&filter_text)?;
    let query = ContactQuery::from_filter(&parsed)?;

    let now = now_utc();
    let offset = local_offset();
    let soon_days = ctx.config.due_soon_days;
    let contacts = ctx
        .store
        .contacts()
        .list_contacts(&query, now, soon_days, offset)?;

    if contacts.is_empty() {
        if ctx.json {
            print_json(&LoopApplyReport {
                matched: 0,
                updated: 0,
                scheduled: 0,
                skipped: 0,
                dry_run: args.dry_run,
                changes: Vec::new(),
            })?;
        } else {
            println!("no contacts matched");
        }
        return Ok(());
    }

    let anchor = match args.anchor {
        Some(value) => parse_anchor(&value)?,
        None => ctx.config.loops.anchor,
    };
    let schedule_missing = if args.schedule_missing {
        true
    } else if args.no_schedule_missing {
        false
    } else {
        ctx.config.loops.schedule_missing
    };
    let override_existing = args.force || ctx.config.loops.override_existing;

    let contact_ids = contacts
        .iter()
        .map(|contact| contact.id)
        .collect::<Vec<_>>();
    let tags_by_contact = ctx.store.tags().list_names_for_contacts(&contact_ids)?;
    let latest_interactions = if schedule_missing && anchor == LoopAnchor::LastInteraction {
        ctx.store
            .interactions()
            .latest_occurred_at_for_contacts(&contact_ids)?
    } else {
        HashMap::new()
    };

    let mut matched = 0;
    let mut updated = 0;
    let mut scheduled = 0;
    let mut skipped = 0;
    let mut changes = Vec::new();
    let mut planned_updates = Vec::new();

    for contact in contacts {
        if contact.archived_at.is_some() {
            skipped += 1;
            continue;
        }

        let tags = tags_by_contact
            .get(&contact.id)
            .cloned()
            .unwrap_or_default();
        let desired = match policy.resolve_cadence(tags.iter().map(|tag| tag.as_str())) {
            Some(value) => value,
            None => {
                skipped += 1;
                continue;
            }
        };
        matched += 1;

        let cadence_before = contact.cadence_days;
        let cadence_after = if cadence_before.is_some() && !override_existing {
            cadence_before
        } else {
            Some(desired)
        };
        let cadence_changed = cadence_before != cadence_after && cadence_after.is_some();

        let mut next_touchpoint_after = contact.next_touchpoint_at;
        let mut scheduled_now = false;
        if schedule_missing && contact.next_touchpoint_at.is_none() {
            if let Some(cadence_days) = cadence_after {
                if let Some(anchor_ts) = resolve_anchor(&contact, anchor, now, &latest_interactions)
                {
                    next_touchpoint_after = Some(schedule_next(anchor_ts, cadence_days)?);
                    scheduled_now = true;
                }
            }
        }

        if !cadence_changed && !scheduled_now {
            skipped += 1;
            continue;
        }

        if !args.dry_run {
            let mut update = ContactUpdate::default();
            if cadence_changed {
                update.cadence_days = Some(cadence_after);
            }
            if scheduled_now {
                update.next_touchpoint_at = Some(next_touchpoint_after);
            }
            planned_updates.push((contact.id, update));
        }

        updated += 1;
        if scheduled_now {
            scheduled += 1;
        }

        changes.push(LoopApplyChange {
            id: contact.id,
            display_name: contact.display_name,
            cadence_before,
            cadence_after,
            next_touchpoint_before: contact.next_touchpoint_at,
            next_touchpoint_after,
            scheduled: scheduled_now,
        });
    }

    if !args.dry_run && !planned_updates.is_empty() {
        let tx = ctx.store.connection().unchecked_transaction()?;
        let contacts = knotter_store::repo::ContactsRepo::new(&tx);
        for (contact_id, update) in planned_updates {
            contacts.update(now, contact_id, update)?;
        }
        tx.commit()?;
    }

    let report = LoopApplyReport {
        matched,
        updated,
        scheduled,
        skipped,
        dry_run: args.dry_run,
        changes,
    };

    if ctx.json {
        print_json(&report)?;
        return Ok(());
    }

    if report.changes.is_empty() {
        println!("no changes needed");
        println!(
            "matched {} | updated {} | scheduled {} | skipped {}",
            report.matched, report.updated, report.scheduled, report.skipped
        );
        return Ok(());
    }

    for change in &report.changes {
        let cadence_label = match (change.cadence_before, change.cadence_after) {
            (None, Some(after)) => format!("cadence set to {after}d"),
            (Some(before), Some(after)) if before != after => {
                format!("cadence {before}d -> {after}d")
            }
            _ => "cadence unchanged".to_string(),
        };
        let schedule_label = match (change.next_touchpoint_before, change.next_touchpoint_after) {
            (None, Some(after)) => format!("scheduled {}", format_timestamp_date(after)),
            _ => "schedule unchanged".to_string(),
        };
        let prefix = if args.dry_run {
            "would update"
        } else {
            "updated"
        };
        println!(
            "{prefix} {} {} ({}, {})",
            change.id, change.display_name, cadence_label, schedule_label
        );
    }

    println!(
        "matched {} | updated {} | scheduled {} | skipped {}",
        report.matched, report.updated, report.scheduled, report.skipped
    );

    Ok(())
}

pub(crate) fn apply_loops_for_contact_with_repos(
    contacts: &ContactsRepo<'_>,
    tags: &TagsRepo<'_>,
    interactions: &InteractionsRepo<'_>,
    config: &AppConfig,
    contact_id: ContactId,
) -> Result<()> {
    if !loops_configured(config) {
        return Err(invalid_input("no loops configured"));
    }
    let policy = &config.loops.policy;

    let Some(contact) = contacts.get(contact_id)? else {
        return Ok(());
    };
    if contact.archived_at.is_some() {
        return Ok(());
    }

    let tags = tags
        .list_for_contact(&contact.id.to_string())?
        .into_iter()
        .map(|tag| tag.name.as_str().to_string())
        .collect::<Vec<_>>();

    let desired = match policy.resolve_cadence(tags.iter().map(|tag| tag.as_str())) {
        Some(value) => value,
        None => return Ok(()),
    };

    let cadence_before = contact.cadence_days;
    let cadence_after = if cadence_before.is_some() && !config.loops.override_existing {
        cadence_before
    } else {
        Some(desired)
    };
    let cadence_changed = cadence_before != cadence_after && cadence_after.is_some();

    let mut next_touchpoint_after = contact.next_touchpoint_at;
    let mut scheduled_now = false;
    if config.loops.schedule_missing && contact.next_touchpoint_at.is_none() {
        if let Some(cadence_days) = cadence_after {
            let latest = if config.loops.anchor == LoopAnchor::LastInteraction {
                interactions.latest_occurred_at_for_contacts(&[contact.id])?
            } else {
                HashMap::new()
            };
            if let Some(anchor_ts) =
                resolve_anchor(&contact, config.loops.anchor, now_utc(), &latest)
            {
                next_touchpoint_after = Some(schedule_next(anchor_ts, cadence_days)?);
                scheduled_now = true;
            }
        }
    }

    if !cadence_changed && !scheduled_now {
        return Ok(());
    }

    let mut update = ContactUpdate::default();
    if cadence_changed {
        update.cadence_days = Some(cadence_after);
    }
    if scheduled_now {
        update.next_touchpoint_at = Some(next_touchpoint_after);
    }
    contacts.update(now_utc(), contact.id, update)?;

    Ok(())
}

pub(crate) fn loops_configured(config: &AppConfig) -> bool {
    let policy = &config.loops.policy;
    !(policy.rules.is_empty() && policy.default_cadence_days.is_none())
}

fn parse_anchor(raw: &str) -> Result<LoopAnchor> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "now" => Ok(LoopAnchor::Now),
        "created-at" | "created_at" => Ok(LoopAnchor::CreatedAt),
        "last-interaction" | "last_interaction" => Ok(LoopAnchor::LastInteraction),
        _ => Err(invalid_input(
            "invalid anchor: expected now|created-at|last-interaction",
        )),
    }
}

fn resolve_anchor(
    contact: &knotter_core::domain::Contact,
    anchor: LoopAnchor,
    now: i64,
    latest_interactions: &HashMap<knotter_core::domain::ContactId, i64>,
) -> Option<i64> {
    match anchor {
        LoopAnchor::Now => Some(now),
        LoopAnchor::CreatedAt => Some(contact.created_at),
        LoopAnchor::LastInteraction => latest_interactions.get(&contact.id).copied(),
    }
}
