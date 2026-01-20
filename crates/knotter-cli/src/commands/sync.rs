use crate::commands::{print_json, Context};
use crate::error::{invalid_input, not_found};
use crate::util::{format_interaction_kind, now_utc};
use anyhow::{Context as _, Result};
use clap::{Args, Subcommand};
use knotter_config::{ContactSourceKind, MacosSourceConfig};
use knotter_core::domain::{ContactId, TagName};
use knotter_core::dto::{
    ExportContactDto, ExportInteractionDto, ExportMetadataDto, ExportSnapshotDto,
};
use knotter_store::error::StoreErrorKind;
use knotter_store::repo::contacts::{ContactNew, ContactUpdate};
use knotter_sync::carddav::CardDavSource;
use knotter_sync::ics::{self, IcsExportOptions};
use knotter_sync::macos::MacosContactsSource;
use knotter_sync::source::VcfSource;
use knotter_sync::vcf;
use serde::Serialize;
use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Debug, Subcommand)]
pub enum ImportCommand {
    Vcf(ImportVcfArgs),
    Macos(ImportMacosArgs),
    #[command(name = "carddav", alias = "gmail")]
    Carddav(ImportCarddavArgs),
    Source(ImportSourceArgs),
}

#[derive(Debug, Args)]
pub struct ImportCommonArgs {
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub limit: Option<usize>,
    #[arg(long, value_name = "TAG")]
    pub tag: Vec<String>,
}

#[derive(Debug, Args)]
pub struct ImportVcfArgs {
    pub file: PathBuf,
    #[command(flatten)]
    pub common: ImportCommonArgs,
}

#[derive(Debug, Args)]
pub struct ImportMacosArgs {
    #[arg(long)]
    pub group: Option<String>,
    #[command(flatten)]
    pub common: ImportCommonArgs,
}

#[derive(Debug, Args)]
pub struct ImportCarddavArgs {
    #[arg(long)]
    pub url: String,
    #[arg(long)]
    pub username: String,
    #[arg(long, value_name = "ENV", conflicts_with = "password_stdin")]
    pub password_env: Option<String>,
    #[arg(long, conflicts_with = "password_env")]
    pub password_stdin: bool,
    #[arg(long)]
    pub user_agent: Option<String>,
    #[command(flatten)]
    pub common: ImportCommonArgs,
}

#[derive(Debug, Args)]
pub struct ImportSourceArgs {
    pub name: String,
    #[arg(long, value_name = "ENV", conflicts_with = "password_stdin")]
    pub password_env: Option<String>,
    #[arg(long, conflicts_with = "password_env")]
    pub password_stdin: bool,
    #[command(flatten)]
    pub common: ImportCommonArgs,
}

#[derive(Debug, Subcommand)]
pub enum ExportCommand {
    Vcf(ExportVcfArgs),
    Ics(ExportIcsArgs),
    Json(ExportJsonArgs),
}

#[derive(Debug, Args)]
pub struct ExportVcfArgs {
    #[arg(long)]
    pub out: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct ExportIcsArgs {
    #[arg(long)]
    pub out: Option<PathBuf>,
    #[arg(long)]
    pub window_days: Option<i64>,
}

#[derive(Debug, Args)]
pub struct ExportJsonArgs {
    #[arg(long)]
    pub out: Option<PathBuf>,
    #[arg(long)]
    pub exclude_archived: bool,
}

#[derive(Debug, Serialize)]
struct ExportReport {
    format: String,
    count: usize,
    output: Option<String>,
}

#[derive(Debug, Clone)]
struct ImportOptions {
    dry_run: bool,
    limit: Option<usize>,
    extra_tags: Vec<TagName>,
}

pub fn import_vcf(ctx: &Context<'_>, args: ImportVcfArgs) -> Result<()> {
    let data = fs::read_to_string(&args.file)
        .with_context(|| format!("read vcf file {}", args.file.display()))?;
    let options = build_import_options(&args.common, None)?;
    import_from_vcf_data(ctx, "vcard", data, options)
}

pub fn import_macos(ctx: &Context<'_>, args: ImportMacosArgs) -> Result<()> {
    let options = build_import_options(&args.common, None)?;
    let source = MacosContactsSource::new(args.group);
    import_from_source(ctx, &source, source.source_name(), options)
}

pub fn import_carddav(ctx: &Context<'_>, args: ImportCarddavArgs) -> Result<()> {
    let password = resolve_password(args.password_env.as_deref(), args.password_stdin, None)?;
    let user_agent = args
        .user_agent
        .clone()
        .or_else(|| Some(default_user_agent()));
    let source = CardDavSource::new(args.url, args.username, password, user_agent);
    let options = build_import_options(&args.common, None)?;
    import_from_source(ctx, &source, source.source_name(), options)
}

pub fn import_source(ctx: &Context<'_>, args: ImportSourceArgs) -> Result<()> {
    let source = ctx
        .config
        .contacts
        .source(&args.name)
        .ok_or_else(|| not_found(format!("contact source {} not found", args.name)))?;
    let source_label = source.name.clone();

    match &source.kind {
        ContactSourceKind::Carddav(cfg) => {
            let username = cfg.username.as_ref().ok_or_else(|| {
                invalid_input(format!("carddav source {source_label} missing username"))
            })?;
            let password = resolve_password(
                args.password_env.as_deref(),
                args.password_stdin,
                cfg.password_env.as_deref(),
            )?;
            let user_agent = Some(default_user_agent());
            let source =
                CardDavSource::new(cfg.url.clone(), username.to_string(), password, user_agent);
            let options = build_import_options(&args.common, cfg.tag.as_deref())?;
            import_from_source(ctx, &source, &source_label, options)
        }
        ContactSourceKind::Macos(MacosSourceConfig { group, tag }) => {
            let source = MacosContactsSource::new(group.clone());
            let options = build_import_options(&args.common, tag.as_deref())?;
            import_from_source(ctx, &source, &source_label, options)
        }
    }
}

pub fn export_vcf(ctx: &Context<'_>, args: ExportVcfArgs) -> Result<()> {
    let contacts = load_export_contacts(ctx, false)?;
    let tags = load_tags(ctx, &contacts)?;
    let data = vcf::export_vcf(&contacts, &tags)?;
    write_export(
        ctx,
        ExportReport {
            format: "vcf".to_string(),
            count: contacts.len(),
            output: args.out.as_ref().map(|path| path.display().to_string()),
        },
        args.out.as_deref(),
        &data,
    )
}

pub fn export_ics(ctx: &Context<'_>, args: ExportIcsArgs) -> Result<()> {
    if let Some(days) = args.window_days {
        if days <= 0 {
            return Err(invalid_input("--window-days must be positive"));
        }
    }

    let contacts = load_export_contacts(ctx, false)?;
    let tags = load_tags(ctx, &contacts)?;
    let export = ics::export_ics(
        &contacts,
        &tags,
        IcsExportOptions {
            now_utc: now_utc(),
            window_days: args.window_days,
        },
    )?;

    write_export(
        ctx,
        ExportReport {
            format: "ics".to_string(),
            count: export.count,
            output: args.out.as_ref().map(|path| path.display().to_string()),
        },
        args.out.as_deref(),
        &export.data,
    )
}

pub fn export_json(ctx: &Context<'_>, args: ExportJsonArgs) -> Result<()> {
    let include_archived = !args.exclude_archived;
    let contacts = load_export_contacts(ctx, include_archived)?;
    let ids: Vec<ContactId> = contacts.iter().map(|contact| contact.id).collect();
    let mut tags = load_tags(ctx, &contacts)?;
    let mut interactions = ctx.store.interactions().list_for_contacts(&ids)?;

    let export_contacts: Vec<ExportContactDto> = contacts
        .into_iter()
        .map(|contact| {
            let tags = tags.remove(&contact.id).unwrap_or_default();
            let interactions = interactions.remove(&contact.id).unwrap_or_default();
            let interactions = interactions
                .into_iter()
                .map(|interaction| ExportInteractionDto {
                    id: interaction.id,
                    occurred_at: interaction.occurred_at,
                    created_at: interaction.created_at,
                    kind: format_interaction_kind(&interaction.kind),
                    note: interaction.note,
                    follow_up_at: interaction.follow_up_at,
                })
                .collect();

            ExportContactDto {
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
                interactions,
            }
        })
        .collect();

    let metadata = ExportMetadataDto {
        exported_at: now_utc(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        schema_version: ctx.store.schema_version()?,
        format_version: 1,
    };

    let snapshot = ExportSnapshotDto {
        metadata,
        contacts: export_contacts,
    };

    let data = serde_json::to_string_pretty(&snapshot)?;
    write_json_export(
        ctx,
        ExportReport {
            format: "json".to_string(),
            count: snapshot.contacts.len(),
            output: args.out.as_ref().map(|path| path.display().to_string()),
        },
        args.out.as_deref(),
        &data,
    )
}

fn load_export_contacts(
    ctx: &Context<'_>,
    include_archived: bool,
) -> Result<Vec<knotter_core::domain::Contact>> {
    let mut contacts = ctx.store.contacts().list_all()?;
    if !include_archived {
        contacts.retain(|contact| contact.archived_at.is_none());
    }
    Ok(contacts)
}

fn load_tags(
    ctx: &Context<'_>,
    contacts: &[knotter_core::domain::Contact],
) -> Result<std::collections::HashMap<knotter_core::domain::ContactId, Vec<String>>> {
    let ids: Vec<knotter_core::domain::ContactId> =
        contacts.iter().map(|contact| contact.id).collect();
    ctx.store
        .tags()
        .list_names_for_contacts(&ids)
        .map_err(Into::into)
}

fn write_export(
    ctx: &Context<'_>,
    report: ExportReport,
    out: Option<&Path>,
    data: &str,
) -> Result<()> {
    if ctx.json && out.is_none() {
        return Err(invalid_input("--json requires --out for export commands"));
    }

    match out {
        Some(path) => {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent)
                        .with_context(|| format!("create export directory {}", parent.display()))?;
                }
            }
            fs::write(path, data)
                .with_context(|| format!("write export file {}", path.display()))?;
            if ctx.json {
                print_json(&report)?;
            } else {
                println!("Exported {} contacts to {}", report.count, path.display());
            }
            Ok(())
        }
        None => {
            print!("{}", data);
            Ok(())
        }
    }
}

fn write_json_export(
    ctx: &Context<'_>,
    report: ExportReport,
    out: Option<&Path>,
    data: &str,
) -> Result<()> {
    match out {
        Some(path) => {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent)
                        .with_context(|| format!("create export directory {}", parent.display()))?;
                }
            }
            fs::write(path, data)
                .with_context(|| format!("write export file {}", path.display()))?;
            if ctx.json {
                print_json(&report)?;
            } else {
                println!("Exported {} contacts to {}", report.count, path.display());
            }
            Ok(())
        }
        None => {
            print!("{}", data);
            Ok(())
        }
    }
}

fn import_from_source(
    ctx: &Context<'_>,
    source: &impl VcfSource,
    source_label: &str,
    options: ImportOptions,
) -> Result<()> {
    let data = source.fetch_vcf()?;
    import_from_vcf_data(ctx, source_label, data, options)
}

fn import_from_vcf_data(
    ctx: &Context<'_>,
    source_name: &str,
    data: String,
    options: ImportOptions,
) -> Result<()> {
    let parsed = vcf::parse_vcf(&data)?;
    let report = import_contacts(ctx, parsed, options)?;
    emit_import_report(ctx, source_name, report)
}

fn import_contacts(
    ctx: &Context<'_>,
    parsed: vcf::ParsedVcf,
    options: ImportOptions,
) -> Result<vcf::ImportReport> {
    let mut report = vcf::ImportReport {
        created: 0,
        updated: 0,
        skipped: parsed.skipped,
        warnings: parsed.warnings,
        dry_run: options.dry_run,
    };
    let now = now_utc();

    let mut contacts = parsed.contacts;
    if let Some(limit) = options.limit {
        if contacts.len() > limit {
            let skipped = contacts.len() - limit;
            report.skipped += skipped;
            report.warnings.push(format!(
                "limit reached; skipped {skipped} remaining contacts"
            ));
            contacts.truncate(limit);
        }
    }

    let mode = if options.dry_run {
        ImportMode::DryRun
    } else {
        ImportMode::Apply
    };

    for contact in contacts {
        let contact = apply_extra_tags(contact, &options.extra_tags);
        match apply_vcf_contact(ctx, now, contact, mode) {
            Ok(ImportOutcome::Created) => report.created += 1,
            Ok(ImportOutcome::Updated) => report.updated += 1,
            Ok(ImportOutcome::Skipped(warning)) => {
                report.skipped += 1;
                report.warnings.push(warning);
            }
            Err(err) => {
                if let Some(store_err) = err.downcast_ref::<knotter_store::error::StoreError>() {
                    match store_err.kind() {
                        StoreErrorKind::Core | StoreErrorKind::InvalidId => {
                            report.skipped += 1;
                            report
                                .warnings
                                .push(format!("skipping contact: {store_err}"));
                            continue;
                        }
                        _ => {}
                    }
                }
                return Err(err);
            }
        }
    }

    Ok(report)
}

fn emit_import_report(
    ctx: &Context<'_>,
    source_name: &str,
    report: vcf::ImportReport,
) -> Result<()> {
    if ctx.json {
        return print_json(&report);
    }

    let suffix = if report.dry_run { " (dry run)" } else { "" };
    println!(
        "Imported {} contacts{}: created {}, updated {}, skipped {}",
        source_name, suffix, report.created, report.updated, report.skipped
    );
    if report.dry_run {
        println!("Dry run: no changes were applied.");
    }
    if !report.warnings.is_empty() {
        println!("Warnings:");
        for warning in report.warnings {
            println!("- {}", warning);
        }
    }
    Ok(())
}

fn build_import_options(
    common: &ImportCommonArgs,
    config_tag: Option<&str>,
) -> Result<ImportOptions> {
    if let Some(limit) = common.limit {
        if limit == 0 {
            return Err(invalid_input("--limit must be greater than zero"));
        }
    }

    let mut tags = parse_tags(&common.tag)?;
    if let Some(tag) = config_tag {
        let tag = TagName::new(tag).map_err(|_| invalid_input(format!("invalid tag: {tag}")))?;
        tags.push(tag);
    }
    let extra_tags = dedupe_tags(tags);

    Ok(ImportOptions {
        dry_run: common.dry_run,
        limit: common.limit,
        extra_tags,
    })
}

fn parse_tags(values: &[String]) -> Result<Vec<TagName>> {
    let mut tags = Vec::with_capacity(values.len());
    for value in values {
        let tag =
            TagName::new(value).map_err(|_| invalid_input(format!("invalid tag: {value}")))?;
        tags.push(tag);
    }
    Ok(tags)
}

fn dedupe_tags(tags: Vec<TagName>) -> Vec<TagName> {
    if tags.is_empty() {
        return tags;
    }
    let mut set: HashSet<TagName> = HashSet::new();
    for tag in tags {
        set.insert(tag);
    }
    let mut merged: Vec<TagName> = set.into_iter().collect();
    merged.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    merged
}

fn apply_extra_tags(mut contact: vcf::VcfContact, extra_tags: &[TagName]) -> vcf::VcfContact {
    if extra_tags.is_empty() {
        return contact;
    }
    let mut tags = contact.tags;
    tags.extend(extra_tags.iter().cloned());
    contact.tags = dedupe_tags(tags);
    contact
}

fn resolve_password(
    password_env: Option<&str>,
    password_stdin: bool,
    fallback_env: Option<&str>,
) -> Result<String> {
    if password_stdin {
        let mut buffer = String::new();
        std::io::stdin()
            .read_to_string(&mut buffer)
            .context("read password from stdin")?;
        let password = buffer.trim().to_string();
        if password.is_empty() {
            return Err(invalid_input("stdin password is empty"));
        }
        return Ok(password);
    }

    let var = password_env
        .or(fallback_env)
        .ok_or_else(|| invalid_input("missing password; use --password-env or --password-stdin"))?;
    let password = std::env::var(var)
        .map_err(|_| invalid_input(format!("environment variable {var} is not set")))?;
    let trimmed = password.trim();
    if trimmed.is_empty() {
        return Err(invalid_input(format!(
            "environment variable {var} is empty"
        )));
    }
    Ok(trimmed.to_string())
}

fn default_user_agent() -> String {
    format!("knotter/{}", env!("CARGO_PKG_VERSION"))
}

#[derive(Debug)]
enum ImportOutcome {
    Created,
    Updated,
    Skipped(String),
}

#[derive(Debug, Clone, Copy)]
enum ImportMode {
    Apply,
    DryRun,
}

fn apply_vcf_contact(
    ctx: &Context<'_>,
    now_utc: i64,
    contact: vcf::VcfContact,
    mode: ImportMode,
) -> Result<ImportOutcome> {
    let email = contact.email.clone();
    if let Some(email) = email.as_ref() {
        let matches = ctx.store.contacts().list_by_email(email)?;
        if matches.len() > 1 {
            return Ok(ImportOutcome::Skipped(format!(
                "multiple contacts share email {email}; skipping"
            )));
        }
        if let Some(existing) = matches.first() {
            if existing.archived_at.is_some() {
                return Ok(ImportOutcome::Skipped(format!(
                    "email {email} belongs to archived contact {}; skipping",
                    existing.id
                )));
            }

            if matches!(mode, ImportMode::DryRun) {
                return Ok(ImportOutcome::Updated);
            }

            let update = ContactUpdate {
                display_name: Some(contact.display_name),
                email: Some(Some(email.to_string())),
                phone: contact.phone.map(Some),
                handle: None,
                timezone: None,
                next_touchpoint_at: contact.next_touchpoint_at.map(Some),
                cadence_days: contact.cadence_days.map(Some),
                archived_at: None,
            };
            let updated = ctx.store.contacts().update(now_utc, existing.id, update)?;
            merge_tags(ctx, &updated.id, contact.tags)?;
            return Ok(ImportOutcome::Updated);
        }
    }

    if matches!(mode, ImportMode::DryRun) {
        return Ok(ImportOutcome::Created);
    }

    let new_contact = ContactNew {
        display_name: contact.display_name,
        email: contact.email,
        phone: contact.phone,
        handle: None,
        timezone: None,
        next_touchpoint_at: contact.next_touchpoint_at,
        cadence_days: contact.cadence_days,
        archived_at: None,
    };
    let created = ctx.store.contacts().create(now_utc, new_contact)?;
    if !contact.tags.is_empty() {
        ctx.store
            .tags()
            .set_contact_tags(&created.id.to_string(), contact.tags)?;
    }
    Ok(ImportOutcome::Created)
}

fn merge_tags(ctx: &Context<'_>, contact_id: &ContactId, incoming: Vec<TagName>) -> Result<()> {
    if incoming.is_empty() {
        return Ok(());
    }

    let mut set: std::collections::HashSet<TagName> = std::collections::HashSet::new();
    for tag in ctx.store.tags().list_for_contact(&contact_id.to_string())? {
        set.insert(tag.name);
    }
    for tag in incoming {
        set.insert(tag);
    }

    let mut tags: Vec<TagName> = set.into_iter().collect();
    tags.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    ctx.store
        .tags()
        .set_contact_tags(&contact_id.to_string(), tags)?;
    Ok(())
}
