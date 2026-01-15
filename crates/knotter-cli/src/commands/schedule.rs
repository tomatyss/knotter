use crate::commands::{print_json, Context};
use crate::util::{format_timestamp_datetime, now_utc, parse_contact_id, parse_local_date_time};
use anyhow::Result;
use clap::Args;
use knotter_store::repo::ContactUpdate;

#[derive(Debug, Args)]
pub struct ScheduleArgs {
    pub id: String,
    #[arg(long = "at")]
    pub date: String,
    #[arg(long)]
    pub time: Option<String>,
}

#[derive(Debug, Args)]
pub struct ClearScheduleArgs {
    pub id: String,
}

pub fn schedule_contact(ctx: &Context<'_>, args: ScheduleArgs) -> Result<()> {
    let contact_id = parse_contact_id(&args.id)?;
    let timestamp = parse_local_date_time(&args.date, args.time.as_deref())?;

    let update = ContactUpdate {
        next_touchpoint_at: Some(Some(timestamp)),
        ..Default::default()
    };

    let contact = ctx.store.contacts().update(now_utc(), contact_id, update)?;

    if ctx.json {
        print_json(&contact)?;
    } else {
        println!(
            "scheduled {} at {}",
            contact.id,
            format_timestamp_datetime(timestamp)
        );
    }
    Ok(())
}

pub fn clear_schedule(ctx: &Context<'_>, args: ClearScheduleArgs) -> Result<()> {
    let contact_id = parse_contact_id(&args.id)?;
    let update = ContactUpdate {
        next_touchpoint_at: Some(None),
        ..Default::default()
    };

    let contact = ctx.store.contacts().update(now_utc(), contact_id, update)?;

    if ctx.json {
        print_json(&contact)?;
    } else {
        println!("cleared schedule for {}", contact.id);
    }
    Ok(())
}
