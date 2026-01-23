use std::collections::HashMap;

use anyhow::Result;
use chrono::FixedOffset;
use knotter_core::domain::{ContactId, TagName};
use knotter_core::dto::{ContactDetailDto, ContactListItemDto, InteractionDto};
use knotter_core::filter::ArchivedSelector;
use knotter_core::rules::compute_due_state;
use knotter_core::time::{local_offset, now_utc};
use knotter_store::repo::{ContactNew, ContactUpdate, InteractionNew};
use knotter_store::{query::ContactQuery, Store};

use crate::app::{App, Mode, TagChoice};
use crate::util::format_interaction_kind;

#[derive(Debug, Clone)]
pub enum Action {
    LoadList,
    LoadDetail(ContactId),
    LoadTags(ContactId),
    CreateContact(ContactNew),
    UpdateContact(ContactId, ContactUpdate),
    AddInteraction(InteractionNew),
    SetTags(ContactId, Vec<TagName>),
    ScheduleContact(ContactId, i64),
    ClearSchedule(ContactId),
    ArchiveContact(ContactId),
    UnarchiveContact(ContactId),
}

pub fn execute_action(app: &mut App, store: &Store, action: Action) -> Result<()> {
    match action {
        Action::LoadList => {
            let now = now_utc();
            let offset = local_offset();
            let query = if let Some(filter) = &app.filter {
                ContactQuery::from_filter(filter)?
            } else {
                ContactQuery::default()
            };
            let mut query = query;
            if !app.show_archived && query.archived.is_none() {
                query.archived = Some(ArchivedSelector::Active);
            }
            let contacts = store
                .contacts()
                .list_contacts(&query, now, app.soon_days, offset)?;
            let ids: Vec<ContactId> = contacts.iter().map(|c| c.id).collect();
            let tag_map = store.tags().list_names_for_contacts(&ids)?;
            let items = build_list_items(contacts, tag_map, now, app.soon_days, offset)?;
            app.apply_list(items);
            app.clear_error();
        }
        Action::LoadDetail(contact_id) => {
            if let Some(detail) = load_detail(store, contact_id)? {
                app.apply_detail(detail);
                app.clear_error();
            } else {
                app.detail = None;
                app.set_error("contact not found");
                if matches!(app.mode, Mode::Detail(id) if id == contact_id) {
                    app.mode = Mode::List;
                }
            }
        }
        Action::LoadTags(contact_id) => {
            let tags_with_counts = store.tags().list_with_counts()?;
            let attached = store.tags().list_for_contact(&contact_id.to_string())?;
            let attached_names: Vec<String> = attached
                .into_iter()
                .map(|tag| tag.name.as_str().to_string())
                .collect();
            let mut tag_choices = Vec::new();
            for (tag, count) in tags_with_counts {
                let name = tag.name.as_str().to_string();
                let selected = attached_names.contains(&name);
                tag_choices.push(TagChoice {
                    name,
                    count,
                    selected,
                });
            }
            if let crate::app::Mode::ModalEditTags(editor) = &mut app.mode {
                editor.set_tags(tag_choices);
            }
            app.clear_error();
        }
        Action::CreateContact(input) => {
            let now = now_utc();
            let contact = store.contacts().create(now, input)?;
            app.set_status(format!("Created {}", contact.display_name));
            app.pending_select = Some(contact.id);
            app.enqueue(Action::LoadList);
        }
        Action::UpdateContact(id, update) => {
            let now = now_utc();
            let contact = store.contacts().update(now, id, update)?;
            app.set_status(format!("Updated {}", contact.display_name));
            app.pending_select = Some(contact.id);
            app.enqueue(Action::LoadList);
            app.enqueue(Action::LoadDetail(contact.id));
        }
        Action::AddInteraction(input) => {
            let contact_id = input.contact_id;
            let now = now_utc();
            let interaction = if app.auto_reschedule_interactions {
                store.interactions().add_with_reschedule(now, input, true)?
            } else {
                store.interactions().add(input)?
            };
            app.set_status(format!(
                "Added interaction ({})",
                format_interaction_kind(&interaction.kind)
            ));
            app.enqueue(Action::LoadDetail(contact_id));
            app.enqueue(Action::LoadList);
        }
        Action::SetTags(contact_id, tags) => {
            let tag_names: Vec<TagName> = tags;
            store
                .tags()
                .set_contact_tags(&contact_id.to_string(), tag_names)?;
            app.set_status("Updated tags".to_string());
            app.enqueue(Action::LoadDetail(contact_id));
            app.enqueue(Action::LoadList);
        }
        Action::ScheduleContact(contact_id, timestamp) => {
            let update = ContactUpdate {
                display_name: None,
                email: None,
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: Some(Some(timestamp)),
                cadence_days: None,
                archived_at: None,
            };
            let now = now_utc();
            store.contacts().update(now, contact_id, update)?;
            app.set_status("Scheduled touchpoint".to_string());
            app.enqueue(Action::LoadDetail(contact_id));
            app.enqueue(Action::LoadList);
        }
        Action::ClearSchedule(contact_id) => {
            let update = ContactUpdate {
                display_name: None,
                email: None,
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: Some(None),
                cadence_days: None,
                archived_at: None,
            };
            let now = now_utc();
            store.contacts().update(now, contact_id, update)?;
            app.set_status("Cleared schedule".to_string());
            app.enqueue(Action::LoadDetail(contact_id));
            app.enqueue(Action::LoadList);
        }
        Action::ArchiveContact(contact_id) => {
            let now = now_utc();
            let contact = store.contacts().archive(now, contact_id)?;
            app.set_status(format!("Archived {}", contact.display_name));
            app.enqueue(Action::LoadDetail(contact_id));
            app.enqueue(Action::LoadList);
        }
        Action::UnarchiveContact(contact_id) => {
            let now = now_utc();
            let contact = store.contacts().unarchive(now, contact_id)?;
            app.set_status(format!("Unarchived {}", contact.display_name));
            app.enqueue(Action::LoadDetail(contact_id));
            app.enqueue(Action::LoadList);
        }
    }

    Ok(())
}

fn build_list_items(
    contacts: Vec<knotter_core::domain::Contact>,
    tags: HashMap<ContactId, Vec<String>>,
    now: i64,
    soon_days: i64,
    offset: FixedOffset,
) -> Result<Vec<ContactListItemDto>> {
    let mut items = Vec::with_capacity(contacts.len());
    for contact in contacts {
        let due_state = compute_due_state(now, contact.next_touchpoint_at, soon_days, offset)?;
        let tags = tags.get(&contact.id).cloned().unwrap_or_default();
        items.push(ContactListItemDto {
            id: contact.id,
            display_name: contact.display_name,
            due_state,
            next_touchpoint_at: contact.next_touchpoint_at,
            archived_at: contact.archived_at,
            tags,
        });
    }
    Ok(items)
}

fn load_detail(store: &Store, contact_id: ContactId) -> Result<Option<ContactDetailDto>> {
    let contact = match store.contacts().get(contact_id)? {
        Some(contact) => contact,
        None => return Ok(None),
    };
    let tags = store.tags().list_for_contact(&contact_id.to_string())?;
    let interactions = store.interactions().list_for_contact(contact_id, 50, 0)?;
    let recent_interactions = interactions
        .into_iter()
        .map(|interaction| InteractionDto {
            id: interaction.id,
            occurred_at: interaction.occurred_at,
            kind: format_interaction_kind(&interaction.kind),
            note: interaction.note,
            follow_up_at: interaction.follow_up_at,
        })
        .collect();
    let tags = tags
        .into_iter()
        .map(|tag| tag.name.as_str().to_string())
        .collect();
    Ok(Some(ContactDetailDto {
        id: contact.id,
        display_name: contact.display_name,
        email: contact.email,
        phone: contact.phone,
        handle: contact.handle,
        timezone: contact.timezone,
        next_touchpoint_at: contact.next_touchpoint_at,
        cadence_days: contact.cadence_days,
        created_at: contact.created_at,
        updated_at: contact.updated_at,
        archived_at: contact.archived_at,
        tags,
        recent_interactions,
    }))
}
