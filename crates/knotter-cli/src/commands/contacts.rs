use crate::commands::{print_json, Context, DEFAULT_INTERACTION_LIMIT, DEFAULT_SOON_DAYS};
use crate::util::{
    due_state_label, format_interaction_kind, format_timestamp_date, format_timestamp_datetime,
    local_offset, now_utc, parse_contact_id, parse_local_timestamp,
};
use anyhow::{anyhow, Result};
use clap::Args;
use knotter_core::dto::{ContactDetailDto, ContactListItemDto, InteractionDto};
use knotter_core::filter::parse_filter;
use knotter_core::rules::compute_due_state;
use knotter_store::query::ContactQuery;
use knotter_store::repo::{ContactNew, ContactUpdate};

#[derive(Debug, Args)]
pub struct AddContactArgs {
    #[arg(long)]
    pub name: String,
    #[arg(long)]
    pub email: Option<String>,
    #[arg(long)]
    pub phone: Option<String>,
    #[arg(long)]
    pub handle: Option<String>,
    #[arg(long)]
    pub timezone: Option<String>,
    #[arg(long)]
    pub cadence_days: Option<i32>,
    #[arg(long)]
    pub next_touchpoint_at: Option<String>,
}

#[derive(Debug, Args)]
pub struct EditContactArgs {
    pub id: String,
    #[arg(long)]
    pub name: Option<String>,
    #[arg(long)]
    pub email: Option<String>,
    #[arg(long)]
    pub phone: Option<String>,
    #[arg(long)]
    pub handle: Option<String>,
    #[arg(long)]
    pub timezone: Option<String>,
    #[arg(long)]
    pub cadence_days: Option<i32>,
    #[arg(long)]
    pub next_touchpoint_at: Option<String>,
}

#[derive(Debug, Args)]
pub struct ShowArgs {
    pub id: String,
}

#[derive(Debug, Args)]
pub struct ListArgs {
    #[arg(long)]
    pub filter: Option<String>,
}

#[derive(Debug, Args)]
pub struct DeleteArgs {
    pub id: String,
}

pub fn add_contact(ctx: &Context<'_>, args: AddContactArgs) -> Result<()> {
    let now = now_utc();
    let next_touchpoint_at = match args.next_touchpoint_at {
        Some(value) => Some(parse_local_timestamp(&value)?),
        None => None,
    };

    let contact = ctx.store.contacts().create(
        now,
        ContactNew {
            display_name: args.name,
            email: args.email,
            phone: args.phone,
            handle: args.handle,
            timezone: args.timezone,
            next_touchpoint_at,
            cadence_days: args.cadence_days,
            archived_at: None,
        },
    )?;

    if ctx.json {
        print_json(&contact)?;
    } else {
        println!("created {} {}", contact.id, contact.display_name);
    }
    Ok(())
}

pub fn edit_contact(ctx: &Context<'_>, args: EditContactArgs) -> Result<()> {
    let now = now_utc();
    let id = parse_contact_id(&args.id)?;

    let mut update = ContactUpdate::default();
    if let Some(name) = args.name {
        update.display_name = Some(name);
    }
    if let Some(email) = args.email {
        update.email = Some(normalize_optional_value(email));
    }
    if let Some(phone) = args.phone {
        update.phone = Some(normalize_optional_value(phone));
    }
    if let Some(handle) = args.handle {
        update.handle = Some(normalize_optional_value(handle));
    }
    if let Some(timezone) = args.timezone {
        update.timezone = Some(normalize_optional_value(timezone));
    }
    if let Some(cadence) = args.cadence_days {
        update.cadence_days = Some(Some(cadence));
    }
    if let Some(value) = args.next_touchpoint_at {
        let parsed = parse_local_timestamp(&value)?;
        update.next_touchpoint_at = Some(Some(parsed));
    }

    if update_is_empty(&update) {
        return Err(anyhow!("no updates provided"));
    }

    let contact = ctx.store.contacts().update(now, id, update)?;
    if ctx.json {
        print_json(&contact)?;
    } else {
        println!("updated {} {}", contact.id, contact.display_name);
    }
    Ok(())
}

pub fn show_contact(ctx: &Context<'_>, args: ShowArgs) -> Result<()> {
    let id = parse_contact_id(&args.id)?;
    let contact = ctx
        .store
        .contacts()
        .get(id)?
        .ok_or_else(|| anyhow!("contact not found"))?;

    let tags = ctx.store.tags().list_for_contact(&contact.id.to_string())?;
    let tag_names: Vec<String> = tags
        .iter()
        .map(|tag| tag.name.as_str().to_string())
        .collect();

    let interactions =
        ctx.store
            .interactions()
            .list_for_contact(contact.id, DEFAULT_INTERACTION_LIMIT, 0)?;
    let interaction_dtos: Vec<InteractionDto> = interactions
        .iter()
        .map(|interaction| InteractionDto {
            id: interaction.id,
            occurred_at: interaction.occurred_at,
            kind: format_interaction_kind(&interaction.kind),
            note: interaction.note.clone(),
            follow_up_at: interaction.follow_up_at,
        })
        .collect();

    let detail = ContactDetailDto {
        id: contact.id,
        display_name: contact.display_name.clone(),
        email: contact.email.clone(),
        phone: contact.phone.clone(),
        handle: contact.handle.clone(),
        timezone: contact.timezone.clone(),
        next_touchpoint_at: contact.next_touchpoint_at,
        cadence_days: contact.cadence_days,
        created_at: contact.created_at,
        updated_at: contact.updated_at,
        archived_at: contact.archived_at,
        tags: tag_names.clone(),
        recent_interactions: interaction_dtos,
    };

    if ctx.json {
        print_json(&detail)?;
        return Ok(());
    }

    println!("id: {}", detail.id);
    println!("name: {}", detail.display_name);
    if let Some(email) = detail.email.as_deref() {
        println!("email: {}", email);
    }
    if let Some(phone) = detail.phone.as_deref() {
        println!("phone: {}", phone);
    }
    if let Some(handle) = detail.handle.as_deref() {
        println!("handle: {}", handle);
    }
    if let Some(timezone) = detail.timezone.as_deref() {
        println!("timezone: {}", timezone);
    }
    if let Some(next) = detail.next_touchpoint_at {
        println!("next_touchpoint_at: {}", format_timestamp_datetime(next));
    }
    if let Some(cadence) = detail.cadence_days {
        println!("cadence_days: {}", cadence);
    }
    println!(
        "created_at: {}",
        format_timestamp_datetime(detail.created_at)
    );
    println!(
        "updated_at: {}",
        format_timestamp_datetime(detail.updated_at)
    );
    if let Some(archived) = detail.archived_at {
        println!("archived_at: {}", format_timestamp_datetime(archived));
    }

    if !tag_names.is_empty() {
        let tag_line = tag_names
            .iter()
            .map(|tag| format!("#{}", tag))
            .collect::<Vec<_>>()
            .join(" ");
        println!("tags: {}", tag_line);
    }

    if detail.recent_interactions.is_empty() {
        println!("interactions: none");
    } else {
        println!("interactions:");
        for interaction in detail.recent_interactions {
            let when = format_timestamp_datetime(interaction.occurred_at);
            let kind = interaction.kind;
            let note = if interaction.note.trim().is_empty() {
                "(no note)"
            } else {
                &interaction.note
            };
            println!("  {} [{}] {}", when, kind, note);
        }
    }

    Ok(())
}

pub fn list_contacts(ctx: &Context<'_>, args: ListArgs) -> Result<()> {
    let filter_text = args.filter.unwrap_or_default();
    let parsed = parse_filter(&filter_text)?;
    let query = ContactQuery::from_filter(&parsed)?;

    let now = now_utc();
    let offset = local_offset();
    let contacts = ctx
        .store
        .contacts()
        .list_contacts(&query, now, DEFAULT_SOON_DAYS, offset)?;

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
        let due_state =
            compute_due_state(now, contact.next_touchpoint_at, DEFAULT_SOON_DAYS, offset)?;
        items.push(ContactListItemDto {
            id: contact.id,
            display_name: contact.display_name,
            due_state,
            next_touchpoint_at: contact.next_touchpoint_at,
            tags: tag_names,
        });
    }

    if ctx.json {
        print_json(&items)?;
        return Ok(());
    }

    if items.is_empty() {
        println!("no contacts");
        return Ok(());
    }

    for item in items {
        let due = due_state_label(item.due_state);
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
            "{}  {}  [{}]  {}{}",
            item.id, item.display_name, due, date, tag_suffix
        );
    }

    Ok(())
}

pub fn delete_contact(ctx: &Context<'_>, args: DeleteArgs) -> Result<()> {
    let id = parse_contact_id(&args.id)?;
    ctx.store.contacts().delete(id)?;
    if ctx.json {
        print_json(&serde_json::json!({ "id": id }))?;
    } else {
        println!("deleted {}", id);
    }
    Ok(())
}

fn normalize_optional_value(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn update_is_empty(update: &ContactUpdate) -> bool {
    update.display_name.is_none()
        && update.email.is_none()
        && update.phone.is_none()
        && update.handle.is_none()
        && update.timezone.is_none()
        && update.next_touchpoint_at.is_none()
        && update.cadence_days.is_none()
        && update.archived_at.is_none()
}
