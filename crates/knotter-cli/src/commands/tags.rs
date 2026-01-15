use crate::commands::{print_json, Context};
use crate::util::parse_contact_id;
use anyhow::Result;
use clap::{Args, Subcommand};
use knotter_core::domain::TagName;
use serde::Serialize;

#[derive(Debug, Subcommand)]
pub enum TagCommand {
    Add(TagAddArgs),
    Rm(TagRemoveArgs),
    Ls(TagListArgs),
}

#[derive(Debug, Args)]
pub struct TagAddArgs {
    pub id: String,
    pub tag: String,
}

#[derive(Debug, Args)]
pub struct TagRemoveArgs {
    pub id: String,
    pub tag: String,
}

#[derive(Debug, Args)]
pub struct TagListArgs {}

#[derive(Debug, Serialize)]
struct TagCountDto {
    name: String,
    count: i64,
}

pub fn add_tag(ctx: &Context<'_>, args: TagAddArgs) -> Result<()> {
    let id = parse_contact_id(&args.id)?;
    let tag = TagName::new(&args.tag)?;
    let normalized = tag.as_str().to_string();
    ctx.store.tags().add_tag_to_contact(&id.to_string(), tag)?;

    if ctx.json {
        print_json(&serde_json::json!({ "id": id, "tag": normalized }))?;
    } else {
        println!("tag added to {}", id);
    }
    Ok(())
}

pub fn remove_tag(ctx: &Context<'_>, args: TagRemoveArgs) -> Result<()> {
    let id = parse_contact_id(&args.id)?;
    let tag = TagName::new(&args.tag)?;
    let normalized = tag.as_str().to_string();
    ctx.store
        .tags()
        .remove_tag_from_contact(&id.to_string(), tag)?;

    if ctx.json {
        print_json(&serde_json::json!({ "id": id, "tag": normalized }))?;
    } else {
        println!("tag removed from {}", id);
    }
    Ok(())
}

pub fn list_tags(ctx: &Context<'_>, _args: TagListArgs) -> Result<()> {
    let tags = ctx.store.tags().list_with_counts()?;
    let items: Vec<TagCountDto> = tags
        .into_iter()
        .map(|(tag, count)| TagCountDto {
            name: tag.name.as_str().to_string(),
            count,
        })
        .collect();

    if ctx.json {
        print_json(&items)?;
        return Ok(());
    }

    if items.is_empty() {
        println!("no tags");
        return Ok(());
    }

    for item in items {
        println!("{} ({})", item.name, item.count);
    }
    Ok(())
}
