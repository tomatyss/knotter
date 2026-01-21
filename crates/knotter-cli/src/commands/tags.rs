use crate::commands::{loops, print_json, Context};
use crate::error::invalid_input;
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
    #[arg(long)]
    pub apply_loop: bool,
}

#[derive(Debug, Args)]
pub struct TagRemoveArgs {
    pub id: String,
    pub tag: String,
    #[arg(long)]
    pub apply_loop: bool,
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
    let apply_loop = args.apply_loop || ctx.config.loops.apply_on_tag_change;
    if apply_loop && !loops::loops_configured(ctx.config) {
        return Err(invalid_input("no loops configured"));
    }
    if apply_loop {
        let tx = ctx.store.connection().unchecked_transaction()?;
        let tags = knotter_store::repo::TagsRepo::new(&tx);
        let contacts = knotter_store::repo::ContactsRepo::new(&tx);
        let interactions = knotter_store::repo::InteractionsRepo::new(&tx);
        tags.add_tag_to_contact(&id.to_string(), tag)?;
        loops::apply_loops_for_contact_with_repos(&contacts, &tags, &interactions, ctx.config, id)?;
        tx.commit()?;
    } else {
        ctx.store.tags().add_tag_to_contact(&id.to_string(), tag)?;
    }

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
    let apply_loop = args.apply_loop || ctx.config.loops.apply_on_tag_change;
    if apply_loop && !loops::loops_configured(ctx.config) {
        return Err(invalid_input("no loops configured"));
    }
    if apply_loop {
        let tx = ctx.store.connection().unchecked_transaction()?;
        let tags = knotter_store::repo::TagsRepo::new(&tx);
        let contacts = knotter_store::repo::ContactsRepo::new(&tx);
        let interactions = knotter_store::repo::InteractionsRepo::new(&tx);
        tags.remove_tag_from_contact(&id.to_string(), tag)?;
        loops::apply_loops_for_contact_with_repos(&contacts, &tags, &interactions, ctx.config, id)?;
        tx.commit()?;
    } else {
        ctx.store
            .tags()
            .remove_tag_from_contact(&id.to_string(), tag)?;
    }

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
