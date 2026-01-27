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

#[derive(Debug, Args, Clone)]
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

#[derive(Debug, Args)]
pub struct SyncArgs {
    #[command(flatten)]
    pub common: ImportCommonArgs,
    #[arg(
        long,
        help = "Force a full resync on UIDVALIDITY changes (may duplicate touches when Message-ID is missing)"
    )]
    pub force_uidvalidity_resync: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub no_loops: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub no_remind: bool,
}

trait SyncRunner {
    fn import_source(
        &self,
        ctx: &Context<'_>,
        source_name: &str,
        common: &ImportCommonArgs,
    ) -> Result<()>;
    fn import_email(
        &self,
        ctx: &Context<'_>,
        common: &ImportCommonArgs,
        force_uidvalidity_resync: bool,
    ) -> Result<()>;
    fn apply_loops(&self, ctx: &Context<'_>, dry_run: bool) -> Result<()>;
    fn remind(&self, ctx: &Context<'_>, dry_run: bool) -> Result<()>;
}

struct DefaultSyncRunner;

impl SyncRunner for DefaultSyncRunner {
    fn import_source(
        &self,
        ctx: &Context<'_>,
        source_name: &str,
        common: &ImportCommonArgs,
    ) -> Result<()> {
        let args = ImportSourceArgs {
            name: source_name.to_string(),
            password_env: None,
            password_stdin: false,
            common: common.clone(),
        };
        import_source(ctx, args)
    }

    fn import_email(
        &self,
        ctx: &Context<'_>,
        common: &ImportCommonArgs,
        force_uidvalidity_resync: bool,
    ) -> Result<()> {
        let args = ImportEmailArgs {
            account: Vec::new(),
            force_uidvalidity_resync,
            common: common.clone(),
        };
        import_email(ctx, args)
    }

    fn apply_loops(&self, ctx: &Context<'_>, dry_run: bool) -> Result<()> {
        let args = crate::commands::loops::LoopApplyArgs {
            filter: None,
            dry_run,
            force: false,
            schedule_missing: false,
            no_schedule_missing: false,
            anchor: None,
        };
        crate::commands::loops::apply_loops(ctx, args)
    }

    fn remind(&self, ctx: &Context<'_>, dry_run: bool) -> Result<()> {
        let args = crate::commands::remind::RemindArgs {
            soon_days: None,
            notify: false,
            no_notify: dry_run,
        };
        crate::commands::remind::remind(ctx, args)
    }
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
    merge_candidates_created: usize,
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
        merge_candidates_created: 0,
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
            "email import: {} account(s), {} mailbox(es), {} message(s), {} touch(es), {} merge candidate(s)",
            report.accounts,
            report.mailboxes,
            report.messages_seen,
            report.touches_recorded,
            report.merge_candidates_created
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

pub fn sync_all(ctx: &Context<'_>, args: SyncArgs) -> Result<()> {
    sync_all_with_runner(ctx, args, &DefaultSyncRunner)
}

fn sync_all_with_runner(ctx: &Context<'_>, args: SyncArgs, runner: &dyn SyncRunner) -> Result<()> {
    if ctx.json {
        return Err(invalid_input(
            "sync does not support --json; run import/loops/remind separately",
        ));
    }

    let mut ran_any = false;
    let mut errors: Vec<String> = Vec::new();

    if ctx.config.contacts.sources.is_empty() {
        println!("no contact sources configured; skipping contact import");
    } else {
        for source in &ctx.config.contacts.sources {
            ran_any = true;
            record_sync_result(
                format!("contact source {}", source.name),
                runner.import_source(ctx, &source.name, &args.common),
                &mut errors,
            );
        }
    }

    if ctx.config.contacts.email_accounts.is_empty() {
        println!("no email accounts configured; skipping email import");
    } else {
        ran_any = true;
        record_sync_result(
            "email import".to_string(),
            runner.import_email(ctx, &args.common, args.force_uidvalidity_resync),
            &mut errors,
        );
    }

    if !ran_any {
        return Err(invalid_input(
            "no contact sources or email accounts configured",
        ));
    }

    if !args.no_loops {
        if crate::commands::loops::loops_configured(ctx.config) {
            record_sync_result(
                "loops apply".to_string(),
                runner.apply_loops(ctx, args.common.dry_run),
                &mut errors,
            );
        } else {
            println!("no loops configured; skipping loop apply");
        }
    }

    if !args.no_remind {
        record_sync_result(
            "remind".to_string(),
            runner.remind(ctx, args.common.dry_run),
            &mut errors,
        );
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(anyhow::anyhow!(format!(
            "sync completed with {} error(s)",
            errors.len()
        )))
    }
}

fn record_sync_result(label: String, result: Result<()>, errors: &mut Vec<String>) {
    if let Err(err) = result {
        let message = format!("{label}: {err}");
        eprintln!("warning: {message}");
        errors.push(message);
    }
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
    let report = import_contacts(ctx, source_name, parsed, options)?;
    emit_import_report(ctx, source_name, report)
}

fn import_contacts(
    ctx: &Context<'_>,
    source_name: &str,
    parsed: vcf::ParsedVcf,
    options: ImportOptions,
) -> Result<vcf::ImportReport> {
    let mut report = vcf::ImportReport {
        created: 0,
        updated: 0,
        skipped: parsed.skipped,
        merge_candidates_created: 0,
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
        match apply_vcf_contact(ctx, source_name, now, contact, mode) {
            Ok(ImportOutcome::Created) => report.created += 1,
            Ok(ImportOutcome::Updated) => report.updated += 1,
            Ok(ImportOutcome::Staged {
                candidates_created,
                warning,
                contact_created,
            }) => {
                if contact_created {
                    report.created += 1;
                }
                report.merge_candidates_created += candidates_created;
                report.warnings.push(warning);
            }
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
        "Imported {} contacts{}: created {}, updated {}, skipped {}, merge candidates {}",
        source_name,
        suffix,
        report.created,
        report.updated,
        report.skipped,
        report.merge_candidates_created
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
        if contact.archived_at.is_some()
            && !email_ctx
                .ctx
                .store
                .merge_candidates()
                .has_open_for_contact(contact_id)?
        {
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
                        if let Some(owner_id) =
                            handle_duplicate_email_match(email_ctx, report, contact.id, &email)?
                        {
                            report.contacts_matched += 1;
                            if !email_ctx.options.dry_run {
                                merge_tags(
                                    email_ctx.ctx,
                                    &owner_id,
                                    email_ctx.options.extra_tags.clone(),
                                )?;
                            }
                            return Ok(Some(owner_id));
                        }
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
            return stage_email_merge_candidates(
                email_ctx,
                report,
                email,
                display_name,
                active_matches,
            );
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

fn stage_email_merge_candidates(
    email_ctx: &EmailImportContext<'_>,
    report: &mut EmailImportReport,
    email: String,
    display_name: String,
    matches: Vec<Contact>,
) -> Result<Option<ContactId>> {
    if email_ctx.options.dry_run {
        report.contacts_created += 1;
        report.merge_candidates_created += matches.len();
        report.warnings.push(format!(
            "email {email} matches multiple contacts by name; dry-run would stage contact"
        ));
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
        archived_at: Some(email_ctx.now_utc),
    };
    let tx = email_ctx.ctx.store.connection().unchecked_transaction()?;
    let created = knotter_store::repo::ContactsRepo::new(&tx).create_with_emails_and_tags(
        email_ctx.now_utc,
        new_contact,
        email_ctx.options.extra_tags.clone(),
        vec![email.clone()],
        Some(email_ctx.account_name),
    )?;

    let mut candidates_created = 0;
    for existing in matches {
        let result = knotter_store::repo::MergeCandidatesRepo::new(&tx).create(
            email_ctx.now_utc,
            created.id,
            existing.id,
            knotter_store::repo::MergeCandidateCreate {
                reason: "email-name-ambiguous".to_string(),
                source: Some(email_ctx.account_name.to_string()),
                preferred_contact_id: Some(existing.id),
            },
        )?;
        if result.created {
            candidates_created += 1;
        }
    }
    tx.commit()?;

    report.contacts_created += 1;
    report.merge_candidates_created += candidates_created;
    report.warnings.push(format!(
        "email {email} matches multiple contacts by name; staged contact {} for merge",
        created.id
    ));

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
    Staged {
        candidates_created: usize,
        warning: String,
        contact_created: bool,
    },
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
    source_name: &str,
    now_utc: i64,
    contact: vcf::VcfContact,
    mode: ImportMode,
) -> Result<ImportOutcome> {
    let mut matched_contacts: Vec<Contact> = Vec::new();
    let mut active_matches: Vec<Contact> = Vec::new();
    let mut archived_found = false;
    let mut duplicate_email_matches = false;
    for email in &contact.emails {
        let matches = ctx.store.contacts().list_by_email(email)?;
        if matches.len() > 1 {
            duplicate_email_matches = true;
        }
        for existing in matches {
            if existing.archived_at.is_some() {
                archived_found = true;
            } else if !active_matches.iter().any(|c| c.id == existing.id) {
                active_matches.push(existing.clone());
            }
            if !matched_contacts.iter().any(|c| c.id == existing.id) {
                matched_contacts.push(existing);
            }
        }
    }

    if active_matches.is_empty() && archived_found && !contact.emails.is_empty() {
        return Ok(ImportOutcome::Skipped(
            "emails only match archived contacts; skipping".to_string(),
        ));
    }

    if duplicate_email_matches || active_matches.len() > 1 {
        return stage_merge_candidate(
            ctx,
            source_name,
            now_utc,
            contact,
            matched_contacts,
            mode,
            "vcf-ambiguous-email",
        );
    }

    if let Some(existing) = active_matches.first().cloned() {
        if matches!(mode, ImportMode::DryRun) {
            return Ok(ImportOutcome::Updated);
        }

        let mut filtered_emails = Vec::new();
        for email in &contact.emails {
            if filtered_emails.contains(email) {
                continue;
            }
            if let Some(owner_id) = ctx.store.emails().find_contact_id_by_email(email)? {
                if owner_id != existing.id {
                    continue;
                }
            }
            filtered_emails.push(email.clone());
        }
        let primary = filtered_emails.first().cloned();
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
        let email_ops = if filtered_emails.is_empty() {
            EmailOps::None
        } else {
            EmailOps::Mutate {
                clear: false,
                add: filtered_emails,
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

fn stage_merge_candidate(
    ctx: &Context<'_>,
    source_name: &str,
    now_utc: i64,
    contact: vcf::VcfContact,
    matches: Vec<Contact>,
    mode: ImportMode,
    reason: &str,
) -> Result<ImportOutcome> {
    let vcf::VcfContact {
        display_name,
        emails,
        phone,
        tags,
        next_touchpoint_at,
        cadence_days,
    } = contact;

    let emails_repo = knotter_store::repo::EmailsRepo::new(ctx.store.connection());
    let mut staged_emails = Vec::new();
    for email in emails {
        if staged_emails.contains(&email) {
            continue;
        }
        if emails_repo.find_contact_id_by_email(&email)?.is_none() {
            staged_emails.push(email);
        }
    }
    let contact_created = !staged_emails.is_empty();

    if matches!(mode, ImportMode::DryRun) {
        let candidates_created = if contact_created {
            matches.len()
        } else {
            matches.len().saturating_sub(1)
        };
        let warning = if contact_created {
            format!(
                "dry-run would stage a contact and create {} merge candidate(s)",
                candidates_created
            )
        } else {
            format!(
                "dry-run would create {} merge candidate(s) between existing contacts",
                candidates_created
            )
        };
        return Ok(ImportOutcome::Staged {
            candidates_created,
            warning,
            contact_created,
        });
    }

    if !contact_created {
        if matches.len() < 2 {
            return Ok(ImportOutcome::Skipped(
                "emails already belong to existing contacts; no merge candidates created"
                    .to_string(),
            ));
        }
        let preferred_id = matches
            .iter()
            .find(|contact| contact.archived_at.is_none())
            .map(|contact| contact.id)
            .unwrap_or(matches[0].id);
        let tx = ctx.store.connection().unchecked_transaction()?;
        let mut candidates_created = 0;
        for existing in matches {
            if existing.id == preferred_id {
                continue;
            }
            let result = knotter_store::repo::MergeCandidatesRepo::new(&tx).create(
                now_utc,
                preferred_id,
                existing.id,
                knotter_store::repo::MergeCandidateCreate {
                    reason: reason.to_string(),
                    source: Some(source_name.to_string()),
                    preferred_contact_id: Some(preferred_id),
                },
            )?;
            if result.created {
                candidates_created += 1;
            }
        }
        tx.commit()?;
        let warning = format!(
            "emails already belong to existing contacts; {} merge candidate(s) created",
            candidates_created
        );
        return Ok(ImportOutcome::Staged {
            candidates_created,
            warning,
            contact_created: false,
        });
    }

    let tx = ctx.store.connection().unchecked_transaction()?;
    let primary = staged_emails.first().cloned();
    let new_contact = ContactNew {
        display_name,
        email: primary,
        phone,
        handle: None,
        timezone: None,
        next_touchpoint_at,
        cadence_days,
        archived_at: Some(now_utc),
    };
    let created = knotter_store::repo::ContactsRepo::new(&tx).create_with_emails_and_tags(
        now_utc,
        new_contact,
        tags,
        staged_emails,
        Some("vcf"),
    )?;

    let mut candidates_created = 0;
    for existing in matches {
        let result = knotter_store::repo::MergeCandidatesRepo::new(&tx).create(
            now_utc,
            created.id,
            existing.id,
            knotter_store::repo::MergeCandidateCreate {
                reason: reason.to_string(),
                source: Some(source_name.to_string()),
                preferred_contact_id: Some(existing.id),
            },
        )?;
        if result.created {
            candidates_created += 1;
        }
    }
    tx.commit()?;

    let warning = format!(
        "staged contact {} for merge; {} candidate(s) created",
        created.id, candidates_created
    );
    Ok(ImportOutcome::Staged {
        candidates_created,
        warning,
        contact_created: true,
    })
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

fn handle_duplicate_email_match(
    email_ctx: &EmailImportContext<'_>,
    report: &mut EmailImportReport,
    contact_id: ContactId,
    email: &str,
) -> Result<Option<ContactId>> {
    let mut created_candidates = 0;
    let owner_id = email_ctx
        .ctx
        .store
        .emails()
        .find_contact_id_by_email(email)?;
    if let Some(found_owner_id) = owner_id {
        let result = email_ctx.ctx.store.merge_candidates().create(
            email_ctx.now_utc,
            contact_id,
            found_owner_id,
            knotter_store::repo::MergeCandidateCreate {
                reason: "email-duplicate".to_string(),
                source: Some(email_ctx.account_name.to_string()),
                preferred_contact_id: Some(found_owner_id),
            },
        )?;
        if result.created {
            created_candidates = 1;
            report.merge_candidates_created += 1;
        }
    }
    if created_candidates == 0 {
        report
            .warnings
            .push(format!("email {email} already belongs to another contact"));
    } else {
        report.warnings.push(format!(
            "email {email} already belongs to another contact; merge candidate created"
        ));
    }
    Ok(owner_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use knotter_config::{
        AppConfig, ContactSourceConfig, ContactSourceKind, EmailAccountConfig, EmailAccountTls,
        EmailMergePolicy, MacosSourceConfig,
    };
    use knotter_store::repo::ContactNew;
    use knotter_store::Store;
    use knotter_sync::email::{EmailAddress, EmailHeader};
    use knotter_sync::vcf;
    use std::cell::{Cell, RefCell};
    use std::collections::HashSet;
    use tempfile::TempDir;

    #[test]
    fn email_import_stages_ambiguous_name_matches() {
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
            merge_candidates_created: 0,
            touches_recorded: 0,
            warnings: Vec::new(),
            dry_run: false,
        };

        let result = handle_email_header(&email_ctx, &header, &mut report).expect("handle header");
        let staged_id = result.expect("staged contact");
        assert_eq!(report.contacts_created, 1);
        assert_eq!(report.contacts_merged, 0);
        assert_eq!(report.merge_candidates_created, 2);
        assert!(report
            .warnings
            .iter()
            .any(|warning| warning.contains("staged contact")));
        let staged = store.contacts().get(staged_id).expect("fetch staged");
        assert!(staged.expect("contact").archived_at.is_some());
    }

    #[test]
    fn email_import_dry_run_reports_staged_counts() {
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
            dry_run: true,
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
            merge_candidates_created: 0,
            touches_recorded: 0,
            warnings: Vec::new(),
            dry_run: true,
        };

        let result = handle_email_header(&email_ctx, &header, &mut report).expect("handle header");
        assert!(result.is_none());
        assert_eq!(report.contacts_created, 1);
        assert_eq!(report.merge_candidates_created, 2);
        assert!(report
            .warnings
            .iter()
            .any(|warning| warning.contains("dry-run")));
    }

    #[test]
    fn vcf_import_updates_active_even_with_archived_match() {
        let store = Store::open_in_memory().expect("open store");
        store.migrate().expect("migrate");
        let now = 1_700_000_000;

        let active = store
            .contacts()
            .create(
                now,
                ContactNew {
                    display_name: "Active".to_string(),
                    email: Some("active@example.com".to_string()),
                    phone: None,
                    handle: None,
                    timezone: None,
                    next_touchpoint_at: None,
                    cadence_days: None,
                    archived_at: None,
                },
            )
            .expect("create active");
        store
            .contacts()
            .create(
                now,
                ContactNew {
                    display_name: "Archived".to_string(),
                    email: Some("archived@example.com".to_string()),
                    phone: None,
                    handle: None,
                    timezone: None,
                    next_touchpoint_at: None,
                    cadence_days: None,
                    archived_at: Some(now),
                },
            )
            .expect("create archived");

        let config = AppConfig::default();
        let ctx = Context {
            store: &store,
            json: false,
            config: &config,
        };
        let contact = vcf::VcfContact {
            display_name: "Updated".to_string(),
            emails: vec![
                "active@example.com".to_string(),
                "archived@example.com".to_string(),
            ],
            phone: None,
            tags: Vec::new(),
            next_touchpoint_at: None,
            cadence_days: None,
        };

        let outcome = apply_vcf_contact(&ctx, "test", now + 10, contact, ImportMode::Apply)
            .expect("apply vcf");
        assert!(matches!(outcome, ImportOutcome::Updated));

        let updated = store
            .contacts()
            .get(active.id)
            .expect("get active")
            .expect("active exists");
        assert_eq!(updated.display_name, "Updated");
        let candidates = store
            .merge_candidates()
            .list(None)
            .expect("list candidates");
        assert!(candidates.is_empty());
    }

    #[test]
    fn vcf_dry_run_reports_staged_counts() {
        let store = Store::open_in_memory().expect("open store");
        store.migrate().expect("migrate");
        let now = 1_700_000_000;

        store
            .contacts()
            .create(
                now,
                ContactNew {
                    display_name: "Ada".to_string(),
                    email: Some("ada@example.com".to_string()),
                    phone: None,
                    handle: None,
                    timezone: None,
                    next_touchpoint_at: None,
                    cadence_days: None,
                    archived_at: None,
                },
            )
            .expect("create a");
        store
            .contacts()
            .create(
                now,
                ContactNew {
                    display_name: "Ada Two".to_string(),
                    email: Some("ada2@example.com".to_string()),
                    phone: None,
                    handle: None,
                    timezone: None,
                    next_touchpoint_at: None,
                    cadence_days: None,
                    archived_at: None,
                },
            )
            .expect("create b");

        let config = AppConfig::default();
        let ctx = Context {
            store: &store,
            json: false,
            config: &config,
        };
        let contact = vcf::VcfContact {
            display_name: "Ada".to_string(),
            emails: vec![
                "ada@example.com".to_string(),
                "ada2@example.com".to_string(),
            ],
            phone: None,
            tags: Vec::new(),
            next_touchpoint_at: None,
            cadence_days: None,
        };

        let outcome = apply_vcf_contact(&ctx, "test", now + 10, contact, ImportMode::DryRun)
            .expect("apply vcf");
        match outcome {
            ImportOutcome::Staged {
                candidates_created,
                warning,
                contact_created,
            } => {
                assert!(!contact_created);
                assert_eq!(candidates_created, 1);
                assert!(warning.contains("dry-run"));
            }
            _ => panic!("expected staged"),
        }
    }

    #[test]
    fn email_import_duplicate_email_creates_merge_candidate() {
        let store = Store::open_in_memory().expect("open store");
        store.migrate().expect("migrate");
        let now = 1_700_000_000;

        let contact = store
            .contacts()
            .create(
                now,
                ContactNew {
                    display_name: "Ada".to_string(),
                    email: Some("ada@example.com".to_string()),
                    phone: None,
                    handle: None,
                    timezone: None,
                    next_touchpoint_at: None,
                    cadence_days: None,
                    archived_at: None,
                },
            )
            .expect("create contact");
        let owner = store
            .contacts()
            .create(
                now,
                ContactNew {
                    display_name: "Grace".to_string(),
                    email: Some("dup@example.com".to_string()),
                    phone: None,
                    handle: None,
                    timezone: None,
                    next_touchpoint_at: None,
                    cadence_days: None,
                    archived_at: None,
                },
            )
            .expect("create owner");

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
        let mut report = EmailImportReport {
            accounts: 0,
            mailboxes: 0,
            messages_seen: 0,
            messages_imported: 0,
            contacts_created: 0,
            contacts_merged: 0,
            contacts_matched: 0,
            merge_candidates_created: 0,
            touches_recorded: 0,
            warnings: Vec::new(),
            dry_run: false,
        };

        handle_duplicate_email_match(&email_ctx, &mut report, contact.id, "dup@example.com")
            .expect("handle duplicate");

        assert_eq!(report.merge_candidates_created, 1);
        assert!(report
            .warnings
            .iter()
            .any(|warning| warning.contains("merge candidate created")));
        let candidates = store
            .merge_candidates()
            .list(None)
            .expect("list candidates");
        assert_eq!(candidates.len(), 1);
        let candidate = &candidates[0];
        assert!(candidate.contact_a_id == contact.id || candidate.contact_b_id == contact.id);
        assert!(candidate.contact_a_id == owner.id || candidate.contact_b_id == owner.id);
        assert_eq!(candidate.preferred_contact_id, Some(owner.id));
    }

    #[test]
    fn email_import_matches_archived_with_open_merge_candidate() {
        let store = Store::open_in_memory().expect("open store");
        store.migrate().expect("migrate");
        let now = 1_700_000_000;

        let archived = store
            .contacts()
            .create(
                now,
                ContactNew {
                    display_name: "Staged".to_string(),
                    email: Some("staged@example.com".to_string()),
                    phone: None,
                    handle: None,
                    timezone: None,
                    next_touchpoint_at: None,
                    cadence_days: None,
                    archived_at: Some(now),
                },
            )
            .expect("create staged");
        let other = store
            .contacts()
            .create(
                now,
                ContactNew {
                    display_name: "Other".to_string(),
                    email: Some("other@example.com".to_string()),
                    phone: None,
                    handle: None,
                    timezone: None,
                    next_touchpoint_at: None,
                    cadence_days: None,
                    archived_at: None,
                },
            )
            .expect("create other");
        store
            .merge_candidates()
            .create(
                now,
                archived.id,
                other.id,
                knotter_store::repo::MergeCandidateCreate {
                    reason: "email-name-ambiguous".to_string(),
                    source: Some("test".to_string()),
                    preferred_contact_id: Some(other.id),
                },
            )
            .expect("create candidate");

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
                name: Some("Staged".to_string()),
                email: "staged@example.com".to_string(),
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
            merge_candidates_created: 0,
            touches_recorded: 0,
            warnings: Vec::new(),
            dry_run: false,
        };

        let result = handle_email_header(&email_ctx, &header, &mut report).expect("handle header");
        assert_eq!(result, Some(archived.id));
        assert_eq!(report.contacts_matched, 1);
        assert!(report
            .warnings
            .iter()
            .all(|warning| !warning.contains("archived contact")));
    }

    #[derive(Default)]
    struct TestRunner {
        calls: RefCell<Vec<String>>,
        fail_on: RefCell<HashSet<String>>,
        last_force_uidvalidity: Cell<Option<bool>>,
    }

    impl TestRunner {
        fn fail_step(&self, label: &str) {
            self.fail_on.borrow_mut().insert(label.to_string());
        }

        fn record(&self, label: &str) -> Result<()> {
            self.calls.borrow_mut().push(label.to_string());
            if self.fail_on.borrow().contains(label) {
                return Err(anyhow::anyhow!("boom"));
            }
            Ok(())
        }
    }

    impl SyncRunner for TestRunner {
        fn import_source(
            &self,
            _ctx: &Context<'_>,
            source_name: &str,
            _common: &ImportCommonArgs,
        ) -> Result<()> {
            self.record(&format!("source:{source_name}"))
        }

        fn import_email(
            &self,
            _ctx: &Context<'_>,
            _common: &ImportCommonArgs,
            force_uidvalidity_resync: bool,
        ) -> Result<()> {
            self.last_force_uidvalidity
                .set(Some(force_uidvalidity_resync));
            self.record("email")
        }

        fn apply_loops(&self, _ctx: &Context<'_>, _dry_run: bool) -> Result<()> {
            self.record("loops")
        }

        fn remind(&self, _ctx: &Context<'_>, _dry_run: bool) -> Result<()> {
            self.record("remind")
        }
    }

    fn base_sync_args() -> SyncArgs {
        SyncArgs {
            common: ImportCommonArgs {
                dry_run: false,
                limit: None,
                retry_skipped: false,
                tag: Vec::new(),
            },
            force_uidvalidity_resync: false,
            no_loops: false,
            no_remind: false,
        }
    }

    #[test]
    fn sync_best_effort_continues_after_errors() {
        let mut config = AppConfig::default();
        config.contacts.sources = vec![
            ContactSourceConfig {
                name: "alpha".to_string(),
                kind: ContactSourceKind::Macos(MacosSourceConfig {
                    group: None,
                    tag: None,
                }),
            },
            ContactSourceConfig {
                name: "beta".to_string(),
                kind: ContactSourceKind::Macos(MacosSourceConfig {
                    group: None,
                    tag: None,
                }),
            },
        ];
        config.contacts.email_accounts = vec![EmailAccountConfig {
            name: "work".to_string(),
            host: "example.test".to_string(),
            port: 993,
            username: "user@example.test".to_string(),
            password_env: "KNOTTER_EMAIL_PASSWORD".to_string(),
            mailboxes: vec!["INBOX".to_string()],
            identities: vec!["user@example.test".to_string()],
            tag: None,
            merge_policy: EmailMergePolicy::EmailOnly,
            tls: EmailAccountTls::Tls,
        }];
        config.loops.policy.default_cadence_days = Some(14);

        let temp = TempDir::new().expect("temp dir");
        let db_path = temp.path().join("knotter.sqlite3");
        let store = Store::open(&db_path).expect("open store");
        store.migrate().expect("migrate");
        let ctx = Context {
            store: &store,
            json: false,
            config: &config,
        };
        let runner = TestRunner::default();
        runner.fail_step("source:alpha");

        let result = sync_all_with_runner(&ctx, base_sync_args(), &runner);
        assert!(result.is_err());

        let calls = runner.calls.borrow();
        assert!(calls.contains(&"source:alpha".to_string()));
        assert!(calls.contains(&"source:beta".to_string()));
        assert!(calls.contains(&"email".to_string()));
        assert!(calls.contains(&"loops".to_string()));
        assert!(calls.contains(&"remind".to_string()));
    }

    #[test]
    fn sync_respects_no_loops_and_no_remind() {
        let mut config = AppConfig::default();
        config.contacts.sources = vec![ContactSourceConfig {
            name: "alpha".to_string(),
            kind: ContactSourceKind::Macos(MacosSourceConfig {
                group: None,
                tag: None,
            }),
        }];
        config.contacts.email_accounts = vec![EmailAccountConfig {
            name: "work".to_string(),
            host: "example.test".to_string(),
            port: 993,
            username: "user@example.test".to_string(),
            password_env: "KNOTTER_EMAIL_PASSWORD".to_string(),
            mailboxes: vec!["INBOX".to_string()],
            identities: vec!["user@example.test".to_string()],
            tag: None,
            merge_policy: EmailMergePolicy::EmailOnly,
            tls: EmailAccountTls::Tls,
        }];
        config.loops.policy.default_cadence_days = Some(14);

        let temp = TempDir::new().expect("temp dir");
        let db_path = temp.path().join("knotter.sqlite3");
        let store = Store::open(&db_path).expect("open store");
        store.migrate().expect("migrate");
        let ctx = Context {
            store: &store,
            json: false,
            config: &config,
        };
        let runner = TestRunner::default();
        let mut args = base_sync_args();
        args.no_loops = true;
        args.no_remind = true;

        let result = sync_all_with_runner(&ctx, args, &runner);
        assert!(result.is_ok());

        let calls = runner.calls.borrow();
        assert!(calls.contains(&"source:alpha".to_string()));
        assert!(calls.contains(&"email".to_string()));
        assert!(!calls.contains(&"loops".to_string()));
        assert!(!calls.contains(&"remind".to_string()));
    }

    #[test]
    fn sync_forwards_force_uidvalidity_resync() {
        let mut config = AppConfig::default();
        config.contacts.sources = vec![ContactSourceConfig {
            name: "alpha".to_string(),
            kind: ContactSourceKind::Macos(MacosSourceConfig {
                group: None,
                tag: None,
            }),
        }];
        config.contacts.email_accounts = vec![EmailAccountConfig {
            name: "work".to_string(),
            host: "example.test".to_string(),
            port: 993,
            username: "user@example.test".to_string(),
            password_env: "KNOTTER_EMAIL_PASSWORD".to_string(),
            mailboxes: vec!["INBOX".to_string()],
            identities: vec!["user@example.test".to_string()],
            tag: None,
            merge_policy: EmailMergePolicy::EmailOnly,
            tls: EmailAccountTls::Tls,
        }];

        let temp = TempDir::new().expect("temp dir");
        let db_path = temp.path().join("knotter.sqlite3");
        let store = Store::open(&db_path).expect("open store");
        store.migrate().expect("migrate");
        let ctx = Context {
            store: &store,
            json: false,
            config: &config,
        };
        let runner = TestRunner::default();
        let mut args = base_sync_args();
        args.force_uidvalidity_resync = true;

        let result = sync_all_with_runner(&ctx, args, &runner);
        assert!(result.is_ok());
        assert_eq!(runner.last_force_uidvalidity.get(), Some(true));
    }
}
