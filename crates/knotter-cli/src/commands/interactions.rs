use crate::commands::{print_json, Context};
use crate::error::invalid_input;
use crate::util::{
    format_interaction_kind, now_utc, parse_contact_id, parse_interaction_kind,
    parse_local_timestamp,
};
use anyhow::Result;
use clap::Args;
use knotter_core::dto::InteractionDto;
use knotter_store::repo::InteractionNew;
use std::io::{self, Read};

#[derive(Debug, Args)]
pub struct AddNoteArgs {
    pub id: String,
    #[arg(long, default_value = "other:note")]
    pub kind: String,
    #[arg(long)]
    pub when: Option<String>,
    #[arg(long)]
    pub note: Option<String>,
    #[arg(long)]
    pub follow_up_at: Option<String>,
}

#[derive(Debug, Args)]
pub struct TouchArgs {
    pub id: String,
    #[arg(long)]
    pub reschedule: bool,
}

pub fn add_note(ctx: &Context<'_>, args: AddNoteArgs) -> Result<()> {
    let contact_id = parse_contact_id(&args.id)?;
    let kind = parse_interaction_kind(&args.kind)?;
    let occurred_at = match args.when {
        Some(value) => parse_local_timestamp(&value)?,
        None => now_utc(),
    };
    let follow_up_at = match args.follow_up_at {
        Some(value) => Some(parse_local_timestamp(&value)?),
        None => None,
    };

    let note = match args.note {
        Some(value) => value,
        None => read_note_from_stdin()?,
    };

    let interaction = ctx.store.interactions().add(InteractionNew {
        contact_id,
        occurred_at,
        created_at: now_utc(),
        kind,
        note,
        follow_up_at,
    })?;

    if ctx.json {
        let dto = InteractionDto {
            id: interaction.id,
            occurred_at: interaction.occurred_at,
            kind: format_interaction_kind(&interaction.kind),
            note: interaction.note,
            follow_up_at: interaction.follow_up_at,
        };
        print_json(&dto)?;
    } else {
        println!("added interaction {}", interaction.id);
    }
    Ok(())
}

pub fn touch_contact(ctx: &Context<'_>, args: TouchArgs) -> Result<()> {
    let contact_id = parse_contact_id(&args.id)?;
    let interaction =
        ctx.store
            .interactions()
            .touch_contact(now_utc(), contact_id, args.reschedule)?;

    if ctx.json {
        let dto = InteractionDto {
            id: interaction.id,
            occurred_at: interaction.occurred_at,
            kind: format_interaction_kind(&interaction.kind),
            note: interaction.note,
            follow_up_at: interaction.follow_up_at,
        };
        print_json(&dto)?;
    } else {
        println!("touched {}", contact_id);
    }
    Ok(())
}

fn read_note_from_stdin() -> Result<String> {
    let mut buffer = String::new();
    io::stdin().read_to_string(&mut buffer)?;
    if buffer.trim().is_empty() {
        return Err(invalid_input("note is empty (provide --note or stdin)"));
    }
    Ok(buffer.trim_end().to_string())
}
