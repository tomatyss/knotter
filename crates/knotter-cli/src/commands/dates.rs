use crate::commands::{print_json, Context};
use crate::error::{invalid_input, not_found};
use crate::util::{
    format_date_parts, now_utc, parse_contact_date_id, parse_contact_id, parse_date_parts,
};
use anyhow::Result;
use clap::{Args, Subcommand};
use knotter_core::domain::{ContactDateKind, ContactId};
use knotter_core::dto::ContactDateDto;
use knotter_store::repo::ContactDateNew;
use std::str::FromStr;

#[derive(Debug, Subcommand)]
pub enum DateCommand {
    Add(AddDateArgs),
    Ls(ListDatesArgs),
    Rm(RemoveDateArgs),
}

#[derive(Debug, Args)]
pub struct AddDateArgs {
    pub contact_id: String,
    #[arg(long, value_name = "KIND")]
    pub kind: String,
    #[arg(long)]
    pub label: Option<String>,
    #[arg(long, value_name = "DATE")]
    pub on: String,
}

#[derive(Debug, Args)]
pub struct ListDatesArgs {
    pub contact_id: String,
}

#[derive(Debug, Args)]
pub struct RemoveDateArgs {
    pub id: String,
}

pub fn add_date(ctx: &Context<'_>, args: AddDateArgs) -> Result<()> {
    let contact_id = parse_contact_id(&args.contact_id)?;
    ensure_contact_exists(ctx, contact_id)?;
    let kind = parse_contact_date_kind(&args.kind)?;
    let (month, day, year) =
        parse_date_parts(&args.on).map_err(|err| invalid_input(err.to_string()))?;
    if matches!(kind, ContactDateKind::Custom) && label_is_empty(args.label.as_deref()) {
        return Err(invalid_input("custom dates require --label"));
    }

    let now = now_utc();
    let created = ctx.store.contact_dates().upsert(
        now,
        ContactDateNew {
            contact_id,
            kind,
            label: args.label,
            month,
            day,
            year,
            source: Some("cli".to_string()),
        },
    )?;

    let dto = contact_date_to_dto(&created);
    if ctx.json {
        print_json(&dto)?;
    } else {
        let label = format_date_label(&dto);
        let date = format_date_parts(dto.month, dto.day, dto.year);
        println!("added {} {} {}", dto.id, label, date);
    }
    Ok(())
}

pub fn list_dates(ctx: &Context<'_>, args: ListDatesArgs) -> Result<()> {
    let contact_id = parse_contact_id(&args.contact_id)?;
    ensure_contact_exists(ctx, contact_id)?;
    let dates = ctx.store.contact_dates().list_for_contact(contact_id)?;
    let dtos: Vec<ContactDateDto> = dates.iter().map(contact_date_to_dto).collect();

    if ctx.json {
        print_json(&dtos)?;
        return Ok(());
    }

    if dtos.is_empty() {
        println!("no dates");
        return Ok(());
    }

    for date in dtos {
        let label = format_date_label(&date);
        let date_str = format_date_parts(date.month, date.day, date.year);
        println!("{}  {}  {}", date.id, label, date_str);
    }
    Ok(())
}

pub fn remove_date(ctx: &Context<'_>, args: RemoveDateArgs) -> Result<()> {
    let id = parse_contact_date_id(&args.id)?;
    ctx.store.contact_dates().delete(id)?;
    if ctx.json {
        print_json(&serde_json::json!({ "id": id }))?;
    } else {
        println!("removed {}", id);
    }
    Ok(())
}

fn parse_contact_date_kind(raw: &str) -> Result<ContactDateKind> {
    ContactDateKind::from_str(raw)
        .map_err(|_| invalid_input("invalid kind: expected birthday|name_day|custom"))
}

fn contact_date_to_dto(date: &knotter_core::domain::ContactDate) -> ContactDateDto {
    ContactDateDto {
        id: date.id,
        kind: date.kind,
        label: date.label.clone(),
        month: date.month,
        day: date.day,
        year: date.year,
    }
}

fn format_date_label(date: &ContactDateDto) -> String {
    match date.kind {
        ContactDateKind::Birthday => "Birthday".to_string(),
        ContactDateKind::NameDay => match date.label.as_deref() {
            Some(label) => format!("Name day ({})", label),
            None => "Name day".to_string(),
        },
        ContactDateKind::Custom => date.label.clone().unwrap_or_else(|| "Custom".to_string()),
    }
}

fn label_is_empty(label: Option<&str>) -> bool {
    label.map(|value| value.trim().is_empty()).unwrap_or(true)
}

pub fn ensure_contact_exists(ctx: &Context<'_>, contact_id: ContactId) -> Result<()> {
    if ctx.store.contacts().get(contact_id)?.is_none() {
        return Err(not_found("contact not found"));
    }
    Ok(())
}
