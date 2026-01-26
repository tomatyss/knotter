use crate::commands::{print_json, Context};
use crate::error::{invalid_input, not_found};
use crate::util::{format_interaction_kind, now_utc};
use anyhow::{Context as _, Result};
use clap::{ArgAction, Args, Subcommand};
use knotter_config::{ContactSourceKind, EmailAccountTls, EmailMergePolicy, MacosSourceConfig};
use knotter_core::domain::{normalize_email, Contact, ContactId, InteractionKind, TagName};
use knotter_core::dto::{
    ExportContactDto, ExportInteractionDto, ExportMetadataDto, ExportSnapshotDto,
};
use knotter_store::error::StoreErrorKind;
use knotter_store::repo::contacts::{ContactNew, ContactUpdate};
use knotter_store::repo::EmailMessageRecord;
use knotter_store::repo::EmailOps;
use knotter_sync::carddav::CardDavSource;
use knotter_sync::email::{fetch_mailbox_headers, EmailAccount, EmailHeader, EmailTls};
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
    Email(ImportEmailArgs),
    Source(ImportSourceArgs),
}

#[derive(Debug, Args)]
pub struct ImportCommonArgs {
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub limit: Option<usize>,
    #[arg(
        long,
        help = "Stop when a message is skipped so it can be retried later"
    )]
    pub retry_skipped: bool,
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

#[derive(Debug, Args)]
pub struct ImportEmailArgs {
    #[arg(long, value_name = "ACCOUNT", action = ArgAction::Append)]
    pub account: Vec<String>,
    #[arg(
        long,
        help = "Force a full resync on UIDVALIDITY changes (may duplicate touches when Message-ID is missing)"
    )]
    pub force_uidvalidity_resync: bool,
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
    retry_skipped: bool,
    extra_tags: Vec<TagName>,
}

#[derive(Debug, Serialize)]
struct EmailImportReport {
    accounts: usize,
    mailboxes: usize,
    messages_seen: usize,
    messages_imported: usize,
    contacts_created: usize,
    contacts_merged: usize,
    contacts_matched: usize,
    touches_recorded: usize,
    warnings: Vec<String>,
    dry_run: bool,
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

pub fn import_email(ctx: &Context<'_>, args: ImportEmailArgs) -> Result<()> {
    let accounts = if args.account.is_empty() {
        ctx.config.contacts.email_accounts.clone()
    } else {
        let mut selected = Vec::new();
        for name in &args.account {
            let account = ctx
                .config
                .contacts
                .email_account(name)
                .ok_or_else(|| not_found(format!("email account {} not found", name)))?;
            selected.push(account.clone());
        }
        selected
    };

    if accounts.is_empty() {
        return Err(invalid_input("no email accounts configured"));
    }

    let mut report = EmailImportReport {
        accounts: 0,
        mailboxes: 0,
        messages_seen: 0,
        messages_imported: 0,
        contacts_created: 0,
        contacts_merged: 0,
        contacts_matched: 0,
        touches_recorded: 0,
        warnings: Vec::new(),
        dry_run: args.common.dry_run,
    };

    let mut remaining = args.common.limit;

    let mut stop_all = false;
    for account_cfg in accounts {
        report.accounts += 1;
        let password =
            resolve_password(Some(&account_cfg.password_env), false, None).map_err(|err| {
                invalid_input(format!(
                    "email account {} password error: {err}",
                    account_cfg.name
                ))
            })?;
        let tls = match account_cfg.tls {
            EmailAccountTls::Tls => EmailTls::Tls,
            EmailAccountTls::StartTls => EmailTls::StartTls,
            EmailAccountTls::None => EmailTls::None,
        };
        let account = EmailAccount {
            host: account_cfg.host.clone(),
            port: account_cfg.port,
            username: account_cfg.username.clone(),
            password,
            tls,
            mailboxes: account_cfg.mailboxes.clone(),
        };
        let identities = normalize_identities(&account_cfg.identities, &account_cfg.username);
        if identities.is_empty() {
            return Err(invalid_input(format!(
                "email account {} identities are empty; set identities or a valid username email",
                account_cfg.name
            )));
        }
        let options = build_import_options(&args.common, account_cfg.tag.as_deref())?;

        for mailbox in &account.mailboxes {
            if matches!(remaining, Some(0)) {
                break;
            }
            if stop_all {
                break;
            }
            report.mailboxes += 1;
            let state = ctx
                .store
                .email_sync()
                .load_state(&account_cfg.name, mailbox)?;
            let mut last_uid = state.as_ref().map(|s| s.last_uid).unwrap_or(0);
            let fetch_limit = match remaining {
                Some(0) => None,
                Some(value) => Some(value),
                None => None,
            };
            let mut result = fetch_mailbox_headers(&account, mailbox, last_uid, fetch_limit)?;
            let mut skip_mailbox = false;
            if let Some(prev) = state.as_ref().and_then(|s| s.uidvalidity) {
                if let Some(current) = result.uidvalidity {
                    if current != prev {
                        let has_missing_message_id = ctx
                            .store
                            .email_sync()
                            .has_null_message_id(&account_cfg.name, mailbox)?;
                        if has_missing_message_id {
                            if args.force_uidvalidity_resync {
                                report.warnings.push(format!(
                                    "mailbox {mailbox} uidvalidity changed; forcing resync (missing Message-ID may duplicate touches)"
                                ));
                                last_uid = 0;
                                result = fetch_mailbox_headers(
                                    &account,
                                    mailbox,
                                    last_uid,
                                    fetch_limit,
                                )?;
                            } else {
                                report.warnings.push(format!(
                                    "mailbox {mailbox} uidvalidity changed; skipping resync to avoid duplicate touches without Message-ID (run with --force-uidvalidity-resync to override)"
                                ));
                                skip_mailbox = true;
                            }
                        } else {
                            last_uid = 0;
                            result =
                                fetch_mailbox_headers(&account, mailbox, last_uid, fetch_limit)?;
                        }
                    }
                }
            }
            if skip_mailbox {
                continue;
            }

            let email_ctx = EmailImportContext {
                ctx,
                account_name: &account_cfg.name,
                merge_policy: &account_cfg.merge_policy,
                options: &options,
                identities: &identities,
                now_utc: now_utc(),
            };
            let mut headers = result.headers;
            headers.sort_by_key(|header| header.uid);
            let mut new_last_uid = last_uid;
            let mut processed_all = true;
            for header in headers {
                if let Some(limit) = remaining.as_mut() {
                    if *limit == 0 {
                        processed_all = false;
                        break;
                    }
                    *limit -= 1;
                }
                report.messages_seen += 1;
                if let Some(contact_id) = handle_email_header(&email_ctx, &header, &mut report)? {
                    if options.dry_run {
                        continue;
                    }
                    let record = EmailMessageRecord {
                        account: account_cfg.name.clone(),
                        mailbox: mailbox.to_string(),
                        uidvalidity: result.uidvalidity.unwrap_or(0),
                        uid: header.uid as i64,
                        message_id: header.message_id.clone(),
                        contact_id,
                        occurred_at: header.occurred_at,
                        direction: direction_for_header(&identities, &header),
                        subject: header.subject.clone(),
                        created_at: now_utc(),
                    };
                    let tx = ctx.store.connection().unchecked_transaction()?;
                    let email_sync = knotter_store::repo::EmailSyncRepo::new(&tx);
                    let interactions = knotter_store::repo::InteractionsRepo::new(&tx);
                    let mut inserted = false;
                    if email_sync.record_message(&record)? {
                        let note = format_email_note(&record.direction, record.subject.as_deref());
                        let interaction = knotter_store::repo::InteractionNew {
                            contact_id,
                            occurred_at: record.occurred_at,
                            created_at: record.created_at,
                            kind: InteractionKind::Email,
                            note,
                            follow_up_at: None,
                        };
                        interactions.add_with_reschedule_in_tx(
                            record.created_at,
                            interaction,
                            ctx.config.interactions.auto_reschedule,
                        )?;
                        inserted = true;
                    }
                    tx.commit()?;
                    if inserted {
                        report.messages_imported += 1;
                        report.touches_recorded += 1;
                    }
                } else if options.retry_skipped {
                    report.warnings.push(format!(
                        "email {} skipped; stopping due to --retry-skipped",
                        header.uid
                    ));
                    processed_all = false;
                    stop_all = true;
                    break;
                }
                new_last_uid = header.uid as i64;
            }
            if processed_all {
                new_last_uid = new_last_uid.max(result.last_uid);
            }

            if !options.dry_run && !stop_all {
                let uidvalidity = result.uidvalidity;
                let state = knotter_store::repo::EmailSyncState {
                    account: account_cfg.name.clone(),
                    mailbox: mailbox.to_string(),
                    uidvalidity,
                    last_uid: new_last_uid,
                    last_seen_at: Some(now_utc()),
                };
                ctx.store.email_sync().upsert_state(&state)?;
            }

            if matches!(remaining, Some(0)) {
                break;
            }
            if stop_all {
                break;
            }
        }

        if matches!(remaining, Some(0)) {
            break;
        }
        if stop_all {
            break;
        }
    }

    if ctx.json {
        print_json(&report)?;
    } else {
        println!(
            "email import: {} account(s), {} mailbox(es), {} message(s), {} touch(es)",
            report.accounts, report.mailboxes, report.messages_seen, report.touches_recorded
        );
        if !report.warnings.is_empty() {
            println!("warnings:");
            for warning in report.warnings {
                println!("  - {}", warning);
            }
        }
    }

    Ok(())
}

pub fn export_vcf(ctx: &Context<'_>, args: ExportVcfArgs) -> Result<()> {
    let contacts = load_export_contacts(ctx, false)?;
    let tags = load_tags(ctx, &contacts)?;
    let emails = load_emails(ctx, &contacts)?;
    let data = vcf::export_vcf(&contacts, &tags, &emails)?;
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
    let mut emails = load_emails(ctx, &contacts)?;
    let mut interactions = ctx.store.interactions().list_for_contacts(&ids)?;

    let export_contacts: Vec<ExportContactDto> = contacts
        .into_iter()
        .map(|contact| {
            let tags = tags.remove(&contact.id).unwrap_or_default();
            let emails = emails.remove(&contact.id).unwrap_or_default();
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
                emails,
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

fn load_emails(
    ctx: &Context<'_>,
    contacts: &[knotter_core::domain::Contact],
) -> Result<std::collections::HashMap<knotter_core::domain::ContactId, Vec<String>>> {
    let ids: Vec<knotter_core::domain::ContactId> =
        contacts.iter().map(|contact| contact.id).collect();
    ctx.store
        .emails()
        .list_emails_for_contacts(&ids)
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
                        StoreErrorKind::Core
                        | StoreErrorKind::InvalidId
                        | StoreErrorKind::DuplicateEmail => {
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
        retry_skipped: common.retry_skipped,
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

fn handle_email_header(
    email_ctx: &EmailImportContext<'_>,
    header: &EmailHeader,
    report: &mut EmailImportReport,
) -> Result<Option<ContactId>> {
    let direction = direction_for_header(email_ctx.identities, header);
    let counterparty = select_counterparty(email_ctx.identities, header, &direction);
    let Some(counterparty) = counterparty else {
        report
            .warnings
            .push(format!("email {} missing counterparty", header.uid));
        return Ok(None);
    };
    let Some(email) = normalize_email(&counterparty.email) else {
        report
            .warnings
            .push(format!("email {} has empty address", header.uid));
        return Ok(None);
    };
    let display_name = counterparty
        .name
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .unwrap_or_else(|| email.clone());

    if let Some(contact_id) = email_ctx
        .ctx
        .store
        .emails()
        .find_contact_id_by_email(&email)?
    {
        let contact = email_ctx
            .ctx
            .store
            .contacts()
            .get(contact_id)?
            .ok_or_else(|| not_found("contact not found"))?;
        if contact.archived_at.is_some() {
            report
                .warnings
                .push(format!("email {email} belongs to archived contact"));
            return Ok(None);
        }
        report.contacts_matched += 1;
        if !email_ctx.options.dry_run {
            merge_tags(
                email_ctx.ctx,
                &contact_id,
                email_ctx.options.extra_tags.clone(),
            )?;
        }
        return Ok(Some(contact_id));
    }

    if matches!(email_ctx.merge_policy, EmailMergePolicy::NameOrEmail)
        && !display_name.trim().is_empty()
    {
        let matches = email_ctx
            .ctx
            .store
            .contacts()
            .list_by_display_name(&display_name)?;
        let active_matches: Vec<Contact> = matches
            .iter()
            .filter(|contact| contact.archived_at.is_none())
            .cloned()
            .collect();
        if active_matches.len() == 1 {
            let contact = &active_matches[0];
            if email_ctx.options.dry_run {
                report.contacts_merged += 1;
                return Ok(Some(contact.id));
            }
            match email_ctx.ctx.store.contacts().update_with_email_ops(
                email_ctx.now_utc,
                contact.id,
                ContactUpdate::default(),
                EmailOps::Mutate {
                    clear: false,
                    add: vec![email.clone()],
                    remove: Vec::new(),
                    source: Some(email_ctx.account_name.to_string()),
                },
            ) {
                Ok(_) => {}
                Err(err) => {
                    if err.kind() == StoreErrorKind::DuplicateEmail {
                        report
                            .warnings
                            .push(format!("email {email} already belongs to another contact"));
                        return Ok(None);
                    }
                    return Err(err.into());
                }
            }
            merge_tags(
                email_ctx.ctx,
                &contact.id,
                email_ctx.options.extra_tags.clone(),
            )?;
            report.contacts_merged += 1;
            return Ok(Some(contact.id));
        }
        if active_matches.is_empty() && !matches.is_empty() {
            report
                .warnings
                .push(format!("email {email} matches archived contact"));
            return Ok(None);
        }
        if active_matches.len() > 1 {
            report
                .warnings
                .push(format!("email {email} matches multiple contacts by name"));
            return Ok(None);
        }
    }

    report.contacts_created += 1;
    if email_ctx.options.dry_run {
        return Ok(None);
    }

    let new_contact = ContactNew {
        display_name,
        email: Some(email.clone()),
        phone: None,
        handle: None,
        timezone: None,
        next_touchpoint_at: None,
        cadence_days: None,
        archived_at: None,
    };
    let created = email_ctx.ctx.store.contacts().create_with_tags(
        email_ctx.now_utc,
        new_contact,
        email_ctx.options.extra_tags.clone(),
    )?;
    email_ctx.ctx.store.emails().add_email(
        email_ctx.now_utc,
        &created.id,
        &email,
        Some(email_ctx.account_name),
        true,
    )?;
    Ok(Some(created.id))
}

fn direction_for_header(
    identities: &std::collections::HashSet<String>,
    header: &EmailHeader,
) -> String {
    let from_is_identity = header.from.iter().any(|addr| {
        normalize_email(&addr.email)
            .map(|value| identities.contains(&value))
            .unwrap_or(false)
    });
    if from_is_identity {
        "outbound".to_string()
    } else {
        "inbound".to_string()
    }
}

fn select_counterparty(
    identities: &std::collections::HashSet<String>,
    header: &EmailHeader,
    direction: &str,
) -> Option<knotter_sync::email::EmailAddress> {
    let mut candidates = if direction == "outbound" {
        header.to.clone()
    } else {
        header.from.clone()
    };
    if candidates.is_empty() {
        return None;
    }
    candidates.sort_by_key(|addr| addr.email.to_ascii_lowercase());
    for candidate in candidates {
        let Some(normalized) = normalize_email(&candidate.email) else {
            continue;
        };
        if !identities.contains(&normalized) {
            return Some(candidate);
        }
    }
    None
}

fn normalize_identities(values: &[String], username: &str) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    for value in values {
        if let Some(email) = normalize_email(value) {
            out.insert(email);
        }
    }
    if out.is_empty() {
        if let Some(email) = normalize_email(username) {
            out.insert(email);
        }
    }
    out
}

fn format_email_note(direction: &str, subject: Option<&str>) -> String {
    let base = if direction == "outbound" {
        "Sent email"
    } else {
        "Email"
    };
    match subject {
        Some(value) if !value.trim().is_empty() => format!("{base}: {}", value.trim()),
        _ => base.to_string(),
    }
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

struct EmailImportContext<'a> {
    ctx: &'a Context<'a>,
    account_name: &'a str,
    merge_policy: &'a EmailMergePolicy,
    options: &'a ImportOptions,
    identities: &'a HashSet<String>,
    now_utc: i64,
}

fn apply_vcf_contact(
    ctx: &Context<'_>,
    now_utc: i64,
    contact: vcf::VcfContact,
    mode: ImportMode,
) -> Result<ImportOutcome> {
    let mut matched: Option<Contact> = None;
    for email in &contact.emails {
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
            if let Some(current) = matched.as_ref() {
                if current.id != existing.id {
                    return Ok(ImportOutcome::Skipped(
                        "emails map to multiple contacts; skipping".to_string(),
                    ));
                }
            } else {
                matched = Some(existing.clone());
            }
        }
    }

    if let Some(existing) = matched {
        if matches!(mode, ImportMode::DryRun) {
            return Ok(ImportOutcome::Updated);
        }

        let primary = contact.emails.first().cloned();
        let update = ContactUpdate {
            display_name: Some(contact.display_name),
            email: primary.clone().map(Some),
            email_source: Some("vcf".to_string()),
            phone: contact.phone.map(Some),
            handle: None,
            timezone: None,
            next_touchpoint_at: contact.next_touchpoint_at.map(Some),
            cadence_days: contact.cadence_days.map(Some),
            archived_at: None,
        };
        let email_ops = if contact.emails.is_empty() {
            EmailOps::None
        } else {
            EmailOps::Mutate {
                clear: false,
                add: contact.emails.clone(),
                remove: Vec::new(),
                source: Some("vcf".to_string()),
            }
        };
        let updated =
            ctx.store
                .contacts()
                .update_with_email_ops(now_utc, existing.id, update, email_ops)?;
        merge_tags(ctx, &updated.id, contact.tags)?;
        return Ok(ImportOutcome::Updated);
    }

    if matches!(mode, ImportMode::DryRun) {
        return Ok(ImportOutcome::Created);
    }

    let primary = contact.emails.first().cloned();
    let new_contact = ContactNew {
        display_name: contact.display_name,
        email: primary.clone(),
        phone: contact.phone,
        handle: None,
        timezone: None,
        next_touchpoint_at: contact.next_touchpoint_at,
        cadence_days: contact.cadence_days,
        archived_at: None,
    };
    ctx.store.contacts().create_with_emails_and_tags(
        now_utc,
        new_contact,
        contact.tags,
        contact.emails,
        Some("vcf"),
    )?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use knotter_config::AppConfig;
    use knotter_store::repo::ContactNew;
    use knotter_store::Store;
    use knotter_sync::email::{EmailAddress, EmailHeader};

    #[test]
    fn email_import_skips_ambiguous_name_matches() {
        let store = Store::open_in_memory().expect("open store");
        store.migrate().expect("migrate");
        let now = 1_700_000_000;
        for idx in 0..2 {
            store
                .contacts()
                .create(
                    now,
                    ContactNew {
                        display_name: "Ada".to_string(),
                        email: Some(format!("ada{idx}@example.com")),
                        phone: None,
                        handle: None,
                        timezone: None,
                        next_touchpoint_at: None,
                        cadence_days: None,
                        archived_at: None,
                    },
                )
                .expect("create contact");
        }

        let config = AppConfig::default();
        let ctx = Context {
            store: &store,
            json: false,
            config: &config,
        };
        let identities = std::collections::HashSet::from(["me@example.com".to_string()]);
        let options = ImportOptions {
            dry_run: false,
            limit: None,
            retry_skipped: false,
            extra_tags: Vec::new(),
        };
        let email_ctx = EmailImportContext {
            ctx: &ctx,
            account_name: "test",
            merge_policy: &EmailMergePolicy::NameOrEmail,
            options: &options,
            identities: &identities,
            now_utc: now,
        };
        let header = EmailHeader {
            mailbox: "INBOX".to_string(),
            uid: 1,
            message_id: None,
            occurred_at: now,
            from: vec![EmailAddress {
                name: Some("Ada".to_string()),
                email: "ada@example.com".to_string(),
            }],
            to: vec![EmailAddress {
                name: None,
                email: "me@example.com".to_string(),
            }],
            subject: None,
        };
        let mut report = EmailImportReport {
            accounts: 0,
            mailboxes: 0,
            messages_seen: 0,
            messages_imported: 0,
            contacts_created: 0,
            contacts_merged: 0,
            contacts_matched: 0,
            touches_recorded: 0,
            warnings: Vec::new(),
            dry_run: false,
        };

        let result = handle_email_header(&email_ctx, &header, &mut report).expect("handle header");
        assert!(result.is_none());
        assert_eq!(report.contacts_created, 0);
        assert_eq!(report.contacts_merged, 0);
        assert!(report
            .warnings
            .iter()
            .any(|warning| warning.contains("matches multiple contacts by name")));
    }
}
