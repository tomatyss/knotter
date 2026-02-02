use crate::commands::{print_json, Context};
use crate::error::{invalid_input, not_found};
use crate::util::{format_interaction_kind, now_utc};
use anyhow::{Context as _, Result};
use clap::{ArgAction, Args, Subcommand};
use knotter_config::{
    ContactSourceKind, EmailAccountTls, EmailMergePolicy, MacosSourceConfig, TelegramMergePolicy,
};
use knotter_core::domain::{
    normalize_email, normalize_phone_for_match, Contact, ContactId, InteractionKind, TagName,
};
use knotter_core::dto::{
    ContactDateDto, ExportContactDto, ExportInteractionDto, ExportMetadataDto, ExportSnapshotDto,
};
use knotter_store::error::StoreErrorKind;
use knotter_store::repo::contacts::{ContactNew, ContactUpdate};
use knotter_store::repo::ContactDateNew;
use knotter_store::repo::EmailMessageRecord;
use knotter_store::repo::{EmailOps, TelegramAccountNew, TelegramMessageRecord, TelegramSyncState};
use knotter_sync::carddav::CardDavSource;
use knotter_sync::email::{fetch_mailbox_headers, EmailAccount, EmailHeader, EmailTls};
use knotter_sync::ics::{self, IcsExportOptions};
use knotter_sync::macos::MacosContactsSource;
use knotter_sync::source::VcfSource;
use knotter_sync::telegram::{self, TelegramAccount as SyncTelegramAccount, TelegramUser};
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
    Telegram(ImportTelegramArgs),
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
    #[arg(
        long,
        help = "Match existing contacts by display name + phone when no email match is found"
    )]
    pub match_phone_name: bool,
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
pub struct ImportTelegramArgs {
    #[arg(long, value_name = "ACCOUNT", action = ArgAction::Append)]
    pub account: Vec<String>,
    #[arg(long, conflicts_with = "messages_only")]
    pub contacts_only: bool,
    #[arg(long, conflicts_with = "contacts_only")]
    pub messages_only: bool,
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
    pub no_telegram: bool,
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
    fn import_telegram(&self, ctx: &Context<'_>, common: &ImportCommonArgs) -> Result<()>;
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

    fn import_telegram(&self, ctx: &Context<'_>, common: &ImportCommonArgs) -> Result<()> {
        let args = ImportTelegramArgs {
            account: Vec::new(),
            contacts_only: false,
            messages_only: false,
            common: common.clone(),
        };
        import_telegram(ctx, args)
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
    match_phone_name: bool,
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

#[derive(Debug, Serialize)]
struct TelegramImportReport {
    accounts: usize,
    users_seen: usize,
    contacts_created: usize,
    contacts_matched: usize,
    contacts_merged: usize,
    merge_candidates_created: usize,
    messages_seen: usize,
    messages_imported: usize,
    touches_recorded: usize,
    warnings: Vec<String>,
    dry_run: bool,
}

pub fn import_vcf(ctx: &Context<'_>, args: ImportVcfArgs) -> Result<()> {
    let data = fs::read_to_string(&args.file)
        .with_context(|| format!("read vcf file {}", args.file.display()))?;
    let options = build_import_options(&args.common, None, args.match_phone_name)?;
    import_from_vcf_data(ctx, "vcard", data, options)
}

pub fn import_macos(ctx: &Context<'_>, args: ImportMacosArgs) -> Result<()> {
    let options = build_import_options(&args.common, None, true)?;
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
    let options = build_import_options(&args.common, None, false)?;
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
            let options = build_import_options(&args.common, cfg.tag.as_deref(), false)?;
            import_from_source(ctx, &source, &source_label, options)
        }
        ContactSourceKind::Macos(MacosSourceConfig { group, tag }) => {
            let source = MacosContactsSource::new(group.clone());
            let options = build_import_options(&args.common, tag.as_deref(), true)?;
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
        let options = build_import_options(&args.common, account_cfg.tag.as_deref(), false)?;

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

fn import_telegram_account(
    ctx: &Context<'_>,
    account_cfg: &knotter_config::TelegramAccountConfig,
    options: &ImportOptions,
    contacts_only: bool,
    messages_only: bool,
    report: &mut TelegramImportReport,
) -> Result<bool> {
    let now_utc = now_utc();
    let api_hash = resolve_required_env(&account_cfg.api_hash_env, "telegram api hash")?;
    let session_path = match &account_cfg.session_path {
        Some(path) => path.clone(),
        None => default_telegram_session_path(&account_cfg.name)?,
    };
    let account = SyncTelegramAccount {
        name: account_cfg.name.clone(),
        api_id: account_cfg.api_id,
        api_hash,
        phone: account_cfg.phone.clone(),
        session_path,
    };

    let mut client = telegram::connect(account)?;
    client.ensure_authorized()?;

    import_telegram_account_with_client(
        ctx,
        account_cfg,
        options,
        contacts_only,
        messages_only,
        report,
        &mut *client,
        now_utc,
    )
}

#[allow(clippy::too_many_arguments)]
fn import_telegram_account_with_client(
    ctx: &Context<'_>,
    account_cfg: &knotter_config::TelegramAccountConfig,
    options: &ImportOptions,
    contacts_only: bool,
    messages_only: bool,
    report: &mut TelegramImportReport,
    client: &mut dyn telegram::TelegramClient,
    now_utc: i64,
) -> Result<bool> {
    let ctx = TelegramImportContext {
        ctx,
        options,
        now_utc,
        account_name: &account_cfg.name,
        merge_policy: account_cfg.merge_policy,
        allowlist_user_ids: &account_cfg.allowlist_user_ids,
        snippet_len: account_cfg.snippet_len,
        messages_only,
    };

    let mut stop_all = false;
    let users = client.list_users()?;
    report.users_seen += users.len();

    for user in users {
        if user.is_bot {
            continue;
        }
        if !ctx.allowlist_user_ids.is_empty() && !ctx.allowlist_user_ids.contains(&user.id) {
            continue;
        }

        let contact_id = resolve_telegram_contact(&ctx, &user, report)?;
        let Some(contact_id) = contact_id else {
            if ctx.options.retry_skipped {
                stop_all = true;
                break;
            }
            continue;
        };

        if contacts_only {
            continue;
        }

        let stop_messages = import_telegram_messages(&ctx, client, &user, contact_id, report)?;
        if stop_messages {
            stop_all = true;
            break;
        }
    }

    Ok(stop_all)
}

pub fn import_telegram(ctx: &Context<'_>, args: ImportTelegramArgs) -> Result<()> {
    if args.contacts_only && args.messages_only {
        return Err(invalid_input(
            "telegram import: --contacts-only and --messages-only are mutually exclusive",
        ));
    }

    let accounts = if args.account.is_empty() {
        ctx.config.contacts.telegram_accounts.clone()
    } else {
        let mut selected = Vec::new();
        for name in &args.account {
            let account = ctx
                .config
                .contacts
                .telegram_account(name)
                .ok_or_else(|| not_found(format!("telegram account {} not found", name)))?;
            selected.push(account.clone());
        }
        selected
    };

    if accounts.is_empty() {
        return Err(invalid_input("no telegram accounts configured"));
    }

    let mut report = TelegramImportReport {
        accounts: 0,
        users_seen: 0,
        contacts_created: 0,
        contacts_matched: 0,
        contacts_merged: 0,
        merge_candidates_created: 0,
        messages_seen: 0,
        messages_imported: 0,
        touches_recorded: 0,
        warnings: Vec::new(),
        dry_run: args.common.dry_run,
    };

    let mut stop_all = false;
    let mut first_error: Option<anyhow::Error> = None;
    for account_cfg in &accounts {
        if stop_all {
            break;
        }
        let options = build_import_options(&args.common, account_cfg.tag.as_deref(), false)?;
        report.accounts += 1;
        let result = import_telegram_account(
            ctx,
            account_cfg,
            &options,
            args.contacts_only,
            args.messages_only,
            &mut report,
        );
        match result {
            Ok(stop) => stop_all = stop,
            Err(err) => {
                report.warnings.push(format!(
                    "telegram account {} failed: {}",
                    account_cfg.name, err
                ));
                if first_error.is_none() {
                    first_error = Some(err);
                }
                stop_all = true;
            }
        }
    }

    if ctx.json {
        print_json(&report)?;
    } else {
        println!(
            "telegram import: {} account(s), {} user(s), {} message(s), {} touch(es), {} merge candidate(s)",
            report.accounts,
            report.users_seen,
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

    if let Some(err) = first_error {
        Err(err)
    } else {
        Ok(())
    }
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

    if !args.no_telegram {
        if ctx.config.contacts.telegram_accounts.is_empty() {
            println!("no telegram accounts configured; skipping telegram import");
        } else {
            ran_any = true;
            record_sync_result(
                "telegram import".to_string(),
                runner.import_telegram(ctx, &args.common),
                &mut errors,
            );
        }
    }

    if !ran_any {
        return Err(invalid_input(
            "no contact sources, email accounts, or telegram accounts configured",
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
    let dates = load_contact_dates(ctx, &contacts)?;
    let data = vcf::export_vcf(&contacts, &tags, &emails, &dates)?;
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
    let mut dates = load_contact_dates(ctx, &contacts)?;
    let mut interactions = ctx.store.interactions().list_for_contacts(&ids)?;

    let export_contacts: Vec<ExportContactDto> = contacts
        .into_iter()
        .map(|contact| {
            let tags = tags.remove(&contact.id).unwrap_or_default();
            let emails = emails.remove(&contact.id).unwrap_or_default();
            let dates = dates.remove(&contact.id).unwrap_or_default();
            let dates = dates
                .into_iter()
                .map(|date| ContactDateDto {
                    id: date.id,
                    kind: date.kind,
                    label: date.label,
                    month: date.month,
                    day: date.day,
                    year: date.year,
                })
                .collect();
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
                dates,
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

fn load_contact_dates(
    ctx: &Context<'_>,
    contacts: &[knotter_core::domain::Contact],
) -> Result<
    std::collections::HashMap<
        knotter_core::domain::ContactId,
        Vec<knotter_core::domain::ContactDate>,
    >,
> {
    let ids: Vec<knotter_core::domain::ContactId> =
        contacts.iter().map(|contact| contact.id).collect();
    ctx.store
        .contact_dates()
        .list_for_contacts(&ids)
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
        match apply_vcf_contact(ctx, source_name, now, contact, mode, &options) {
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
    match_phone_name: bool,
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
        match_phone_name,
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

struct TelegramImportContext<'a> {
    ctx: &'a Context<'a>,
    options: &'a ImportOptions,
    now_utc: i64,
    account_name: &'a str,
    merge_policy: TelegramMergePolicy,
    allowlist_user_ids: &'a [i64],
    snippet_len: usize,
    messages_only: bool,
}

fn resolve_telegram_contact(
    telegram_ctx: &TelegramImportContext<'_>,
    user: &TelegramUser,
    report: &mut TelegramImportReport,
) -> Result<Option<ContactId>> {
    let username = normalize_telegram_username(user.username.as_deref());
    let phone = normalize_optional_string(user.phone.as_deref());
    let display_name = user.display_name();
    let warn_messages_only_ambiguous = |report: &mut TelegramImportReport, label: &str| {
        report.warnings.push(format!(
            "telegram user {} matches multiple contacts by {label}; messages-only skips staging",
            user.id
        ));
    };

    if let Some(contact_id) = telegram_ctx
        .ctx
        .store
        .telegram_accounts()
        .find_contact_id_by_user_id(user.id)?
    {
        return attach_telegram_account(
            telegram_ctx,
            contact_id,
            user,
            username,
            phone,
            report,
            true,
        );
    }

    if let Some(username) = username.as_deref() {
        let ids = telegram_ctx
            .ctx
            .store
            .telegram_accounts()
            .list_contact_ids_by_username(username)?;
        if !ids.is_empty() {
            let mut matches = Vec::new();
            for id in ids {
                if let Some(contact) = telegram_ctx.ctx.store.contacts().get(id)? {
                    matches.push(contact);
                }
            }
            let active_matches: Vec<Contact> = matches
                .iter()
                .filter(|contact| contact.archived_at.is_none())
                .cloned()
                .collect();
            if active_matches.len() == 1 {
                let contact = &active_matches[0];
                return attach_telegram_account(
                    telegram_ctx,
                    contact.id,
                    user,
                    Some(username.to_string()),
                    phone,
                    report,
                    true,
                );
            }
            if active_matches.is_empty() && !matches.is_empty() {
                report.warnings.push(format!(
                    "telegram username {username} matches archived contact"
                ));
                return Ok(None);
            }
            if active_matches.len() > 1 {
                if telegram_ctx.messages_only {
                    warn_messages_only_ambiguous(report, "username");
                    return Ok(None);
                }
                return stage_telegram_merge_candidates(
                    telegram_ctx,
                    report,
                    user,
                    Some(username.to_string()),
                    phone,
                    display_name.clone(),
                    active_matches,
                    "telegram-username-ambiguous",
                    "username",
                );
            }
        }
    }

    if let Some(username) = username.as_deref() {
        let mut matches = Vec::new();
        matches.extend(telegram_ctx.ctx.store.contacts().list_by_handle(username)?);
        let at_username = format!("@{username}");
        if at_username != username {
            matches.extend(
                telegram_ctx
                    .ctx
                    .store
                    .contacts()
                    .list_by_handle(&at_username)?,
            );
        }
        if !matches.is_empty() {
            let mut seen = HashSet::new();
            let mut unique_matches = Vec::new();
            for contact in matches {
                if seen.insert(contact.id) {
                    unique_matches.push(contact);
                }
            }
            let active_matches: Vec<Contact> = unique_matches
                .iter()
                .filter(|contact| contact.archived_at.is_none())
                .cloned()
                .collect();
            if active_matches.len() == 1 {
                let contact = &active_matches[0];
                return attach_telegram_account(
                    telegram_ctx,
                    contact.id,
                    user,
                    Some(username.to_string()),
                    phone,
                    report,
                    true,
                );
            }
            if active_matches.is_empty() && !unique_matches.is_empty() {
                report.warnings.push(format!(
                    "telegram handle {username} matches archived contact"
                ));
                return Ok(None);
            }
            if active_matches.len() > 1 {
                if telegram_ctx.messages_only {
                    warn_messages_only_ambiguous(report, "handle");
                    return Ok(None);
                }
                return stage_telegram_merge_candidates(
                    telegram_ctx,
                    report,
                    user,
                    Some(username.to_string()),
                    phone,
                    display_name.clone(),
                    active_matches,
                    "telegram-handle-ambiguous",
                    "handle",
                );
            }
        }
    }

    if matches!(
        telegram_ctx.merge_policy,
        TelegramMergePolicy::NameOrUsername
    ) && !display_name.trim().is_empty()
    {
        let matches = telegram_ctx
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
            if telegram_ctx.options.dry_run {
                report.contacts_merged += 1;
                return Ok(Some(contact.id));
            }
            let result = attach_telegram_account(
                telegram_ctx,
                contact.id,
                user,
                username,
                phone,
                report,
                false,
            );
            if matches!(result, Ok(Some(_))) {
                report.contacts_merged += 1;
            }
            return result;
        }
        if active_matches.is_empty() && !matches.is_empty() {
            report.warnings.push(format!(
                "telegram user {} matches archived contact",
                user.id
            ));
            return Ok(None);
        }
        if active_matches.len() > 1 {
            if telegram_ctx.messages_only {
                warn_messages_only_ambiguous(report, "name");
                return Ok(None);
            }
            return stage_telegram_merge_candidates(
                telegram_ctx,
                report,
                user,
                username,
                phone,
                display_name,
                active_matches,
                "telegram-name-ambiguous",
                "name",
            );
        }
    }

    if telegram_ctx.messages_only {
        report.warnings.push(format!(
            "telegram user {} not linked to a contact; skipping messages",
            user.id
        ));
        return Ok(None);
    }

    report.contacts_created += 1;
    if telegram_ctx.options.dry_run {
        return Ok(None);
    }

    let new_contact = ContactNew {
        display_name,
        email: None,
        phone: None,
        handle: None,
        timezone: None,
        next_touchpoint_at: None,
        cadence_days: None,
        archived_at: None,
    };
    let created = telegram_ctx.ctx.store.contacts().create_with_tags(
        telegram_ctx.now_utc,
        new_contact,
        telegram_ctx.options.extra_tags.clone(),
    )?;
    telegram_ctx.ctx.store.telegram_accounts().upsert(
        telegram_ctx.now_utc,
        TelegramAccountNew {
            contact_id: created.id,
            telegram_user_id: user.id,
            username,
            phone,
            first_name: normalize_optional_string(user.first_name.as_deref()),
            last_name: normalize_optional_string(user.last_name.as_deref()),
            source: Some(format!("telegram:{}", telegram_ctx.account_name)),
        },
    )?;
    Ok(Some(created.id))
}

fn attach_telegram_account(
    telegram_ctx: &TelegramImportContext<'_>,
    contact_id: ContactId,
    user: &TelegramUser,
    username: Option<String>,
    phone: Option<String>,
    report: &mut TelegramImportReport,
    matched: bool,
) -> Result<Option<ContactId>> {
    let contact = telegram_ctx
        .ctx
        .store
        .contacts()
        .get(contact_id)?
        .ok_or_else(|| not_found("contact not found"))?;
    if contact.archived_at.is_some()
        && !telegram_ctx
            .ctx
            .store
            .merge_candidates()
            .has_open_for_contact(contact_id)?
    {
        report.warnings.push(format!(
            "telegram user {} belongs to archived contact",
            user.id
        ));
        return Ok(None);
    }

    if telegram_ctx.options.dry_run {
        if matched {
            report.contacts_matched += 1;
        }
        return Ok(Some(contact_id));
    }

    if let Err(err) = telegram_ctx.ctx.store.telegram_accounts().upsert(
        telegram_ctx.now_utc,
        TelegramAccountNew {
            contact_id,
            telegram_user_id: user.id,
            username,
            phone,
            first_name: normalize_optional_string(user.first_name.as_deref()),
            last_name: normalize_optional_string(user.last_name.as_deref()),
            source: Some(format!("telegram:{}", telegram_ctx.account_name)),
        },
    ) {
        if err.kind() == StoreErrorKind::DuplicateTelegramUser {
            report.warnings.push(format!(
                "telegram user {} already linked to another contact",
                user.id
            ));
            return Ok(None);
        }
        return Err(err.into());
    }
    merge_tags(
        telegram_ctx.ctx,
        &contact_id,
        telegram_ctx.options.extra_tags.clone(),
    )?;
    if matched {
        report.contacts_matched += 1;
    }
    Ok(Some(contact_id))
}

#[allow(clippy::too_many_arguments)]
fn stage_telegram_merge_candidates(
    telegram_ctx: &TelegramImportContext<'_>,
    report: &mut TelegramImportReport,
    user: &TelegramUser,
    username: Option<String>,
    phone: Option<String>,
    display_name: String,
    matches: Vec<Contact>,
    reason: &str,
    match_label: &str,
) -> Result<Option<ContactId>> {
    if telegram_ctx.options.dry_run {
        report.contacts_created += 1;
        report.merge_candidates_created += matches.len();
        report.warnings.push(format!(
            "telegram user {} matches multiple contacts by {match_label}; dry-run would stage contact",
            user.id
        ));
        return Ok(None);
    }

    let new_contact = ContactNew {
        display_name,
        email: None,
        phone: None,
        handle: None,
        timezone: None,
        next_touchpoint_at: None,
        cadence_days: None,
        archived_at: Some(telegram_ctx.now_utc),
    };
    let tx = telegram_ctx
        .ctx
        .store
        .connection()
        .unchecked_transaction()?;
    let created = knotter_store::repo::ContactsRepo::new(&tx).create_with_tags(
        telegram_ctx.now_utc,
        new_contact,
        telegram_ctx.options.extra_tags.clone(),
    )?;
    knotter_store::repo::TelegramAccountsRepo::new(&tx).upsert(
        telegram_ctx.now_utc,
        TelegramAccountNew {
            contact_id: created.id,
            telegram_user_id: user.id,
            username,
            phone,
            first_name: normalize_optional_string(user.first_name.as_deref()),
            last_name: normalize_optional_string(user.last_name.as_deref()),
            source: Some(format!("telegram:{}", telegram_ctx.account_name)),
        },
    )?;

    let mut candidates_created = 0;
    for existing in matches {
        let result = knotter_store::repo::MergeCandidatesRepo::new(&tx).create(
            telegram_ctx.now_utc,
            created.id,
            existing.id,
            knotter_store::repo::MergeCandidateCreate {
                reason: reason.to_string(),
                source: Some(telegram_ctx.account_name.to_string()),
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
        "telegram user {} matches multiple contacts by {match_label}; staged contact {} for merge",
        user.id, created.id
    ));
    Ok(Some(created.id))
}

fn import_telegram_messages(
    telegram_ctx: &TelegramImportContext<'_>,
    client: &mut dyn telegram::TelegramClient,
    user: &TelegramUser,
    contact_id: ContactId,
    report: &mut TelegramImportReport,
) -> Result<bool> {
    let existing = telegram_ctx
        .ctx
        .store
        .telegram_sync()
        .load_state(telegram_ctx.account_name, user.id)?;
    let last_message_id = existing.map(|state| state.last_message_id).unwrap_or(0);

    let batch = client.fetch_messages(user.id, last_message_id, telegram_ctx.options.limit)?;
    let mut messages = batch.messages;
    messages.sort_by_key(|message| message.id);

    let mut new_last_message_id = last_message_id;
    for message in messages {
        report.messages_seen += 1;
        new_last_message_id = new_last_message_id.max(message.id);

        if telegram_ctx.options.dry_run {
            continue;
        }

        let direction = if message.outgoing {
            "outbound".to_string()
        } else {
            "inbound".to_string()
        };
        let snippet = snippet_from_text(message.text.as_deref(), telegram_ctx.snippet_len);
        let record = TelegramMessageRecord {
            account: telegram_ctx.account_name.to_string(),
            peer_id: message.peer_id,
            message_id: message.id,
            contact_id,
            occurred_at: message.occurred_at,
            direction: direction.clone(),
            snippet: snippet.clone(),
            created_at: telegram_ctx.now_utc,
        };

        let tx = telegram_ctx
            .ctx
            .store
            .connection()
            .unchecked_transaction()?;
        let sync_repo = knotter_store::repo::TelegramSyncRepo::new(&tx);
        let interactions = knotter_store::repo::InteractionsRepo::new(&tx);
        let mut inserted = false;
        if sync_repo.record_message(&record)? {
            let note = format_telegram_note(&direction, snippet.as_deref());
            let interaction = knotter_store::repo::InteractionNew {
                contact_id,
                occurred_at: record.occurred_at,
                created_at: record.created_at,
                kind: InteractionKind::Telegram,
                note,
                follow_up_at: None,
            };
            interactions.add_with_reschedule_in_tx(
                record.created_at,
                interaction,
                telegram_ctx.ctx.config.interactions.auto_reschedule,
            )?;
            inserted = true;
        }
        tx.commit()?;
        if inserted {
            report.messages_imported += 1;
            report.touches_recorded += 1;
        }
    }

    if !telegram_ctx.options.dry_run {
        if !batch.complete {
            report.warnings.push(format!(
                "telegram account {} hit --limit for user {}; sync state not advanced",
                telegram_ctx.account_name, user.id
            ));
            return Ok(false);
        }
        let state = TelegramSyncState {
            account: telegram_ctx.account_name.to_string(),
            peer_id: user.id,
            last_message_id: new_last_message_id,
            last_seen_at: Some(telegram_ctx.now_utc),
        };
        telegram_ctx
            .ctx
            .store
            .telegram_sync()
            .upsert_state(&state)?;
    }

    Ok(false)
}

fn normalize_telegram_username(raw: Option<&str>) -> Option<String> {
    let value = raw?;
    let trimmed = value.trim().trim_start_matches('@');
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_ascii_lowercase())
}

fn normalize_optional_string(raw: Option<&str>) -> Option<String> {
    let value = raw?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn snippet_from_text(text: Option<&str>, max_len: usize) -> Option<String> {
    let raw = text?;
    let collapsed = collapse_whitespace(raw);
    if collapsed.is_empty() {
        return None;
    }
    Some(truncate_with_ellipsis(&collapsed, max_len))
}

fn collapse_whitespace(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut last_was_space = false;
    for ch in value.chars() {
        if ch.is_whitespace() {
            if !last_was_space {
                out.push(' ');
                last_was_space = true;
            }
        } else {
            out.push(ch);
            last_was_space = false;
        }
    }
    out.trim().to_string()
}

fn truncate_with_ellipsis(value: &str, max_len: usize) -> String {
    if max_len == 0 {
        return String::new();
    }
    let total_len = value.chars().count();
    if total_len <= max_len {
        return value.to_string();
    }
    if max_len <= 3 {
        return value.chars().take(max_len).collect();
    }
    let mut out: String = value.chars().take(max_len - 3).collect();
    out.push_str("...");
    out
}

fn format_telegram_note(direction: &str, snippet: Option<&str>) -> String {
    let base = if direction == "outbound" {
        "Sent Telegram message"
    } else {
        "Telegram message"
    };
    match snippet {
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

fn apply_contact_dates(
    ctx: &Context<'_>,
    now_utc: i64,
    contact_id: ContactId,
    dates: Vec<vcf::ContactDateInput>,
) -> Result<()> {
    apply_contact_dates_repo(ctx.store.contact_dates(), now_utc, contact_id, dates)
}

fn apply_contact_dates_repo(
    repo: knotter_store::repo::ContactDatesRepo<'_>,
    now_utc: i64,
    contact_id: ContactId,
    dates: Vec<vcf::ContactDateInput>,
) -> Result<()> {
    if dates.is_empty() {
        return Ok(());
    }
    for date in dates {
        repo.upsert_preserve_year(
            now_utc,
            ContactDateNew {
                contact_id,
                kind: date.kind,
                label: date.label,
                month: date.month,
                day: date.day,
                year: date.year,
                source: Some("vcf".to_string()),
            },
        )?;
    }
    Ok(())
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

fn resolve_required_env(var: &str, label: &str) -> Result<String> {
    let value = std::env::var(var).map_err(|_| {
        invalid_input(format!(
            "environment variable {var} is not set (required for {label})"
        ))
    })?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(invalid_input(format!(
            "environment variable {var} is empty (required for {label})"
        )));
    }
    Ok(trimmed.to_string())
}

fn default_telegram_session_path(account_name: &str) -> Result<PathBuf> {
    ensure_safe_telegram_account_name(account_name)?;
    let dir = knotter_store::paths::ensure_data_subdir("telegram")?;
    Ok(dir.join(format!("{account_name}.session")))
}

fn ensure_safe_telegram_account_name(account_name: &str) -> Result<()> {
    let mut components = Path::new(account_name).components();
    match components.next() {
        Some(std::path::Component::Normal(_)) if components.next().is_none() => Ok(()),
        _ => Err(invalid_input(
            "telegram account name must be a single path segment",
        )),
    }
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
    options: &ImportOptions,
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

    let email_archived_only =
        active_matches.is_empty() && archived_found && !contact.emails.is_empty();

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
        apply_vcf_update(ctx, now_utc, existing.id, contact)?;
        return Ok(ImportOutcome::Updated);
    }

    if options.match_phone_name {
        if let Some(phone) = contact.phone.as_deref() {
            let matches = match_contacts_by_phone_name(ctx, &contact.display_name, phone)?;
            if matches.active_matches.is_empty() && matches.archived_found {
                return Ok(ImportOutcome::Skipped(
                    "phone + name only match archived contacts; skipping".to_string(),
                ));
            }
            if matches.active_matches.len() > 1 {
                return stage_existing_matches(
                    ctx,
                    source_name,
                    now_utc,
                    matches.matched_contacts,
                    mode,
                    "vcf-ambiguous-phone-name",
                    "phone + name match multiple contacts",
                );
            }
            if let Some(existing) = matches.active_matches.first().cloned() {
                if matches!(mode, ImportMode::DryRun) {
                    return Ok(ImportOutcome::Updated);
                }
                apply_vcf_update(ctx, now_utc, existing.id, contact)?;
                return Ok(ImportOutcome::Updated);
            }
        }
    }

    if email_archived_only {
        return Ok(ImportOutcome::Skipped(
            "emails only match archived contacts; skipping".to_string(),
        ));
    }

    if matches!(mode, ImportMode::DryRun) {
        return Ok(ImportOutcome::Created);
    }

    let vcf::VcfContact {
        display_name,
        emails,
        phone,
        tags,
        next_touchpoint_at,
        cadence_days,
        dates,
    } = contact;
    let primary = emails.first().cloned();
    let new_contact = ContactNew {
        display_name,
        email: primary.clone(),
        phone,
        handle: None,
        timezone: None,
        next_touchpoint_at,
        cadence_days,
        archived_at: None,
    };
    let created = ctx.store.contacts().create_with_emails_and_tags(
        now_utc,
        new_contact,
        tags,
        emails,
        Some("vcf"),
    )?;
    apply_contact_dates(ctx, now_utc, created.id, dates)?;
    Ok(ImportOutcome::Created)
}

struct PhoneNameMatches {
    matched_contacts: Vec<Contact>,
    active_matches: Vec<Contact>,
    archived_found: bool,
}

fn match_contacts_by_phone_name(
    ctx: &Context<'_>,
    display_name: &str,
    phone: &str,
) -> Result<PhoneNameMatches> {
    let Some(normalized_phone) = normalize_phone_for_match(phone) else {
        return Ok(PhoneNameMatches {
            matched_contacts: Vec::new(),
            active_matches: Vec::new(),
            archived_found: false,
        });
    };

    let candidates = ctx.store.contacts().list_by_display_name(display_name)?;
    let mut matched_contacts = Vec::new();
    let mut active_matches = Vec::new();
    let mut archived_found = false;

    for contact in candidates {
        let Some(contact_phone) = contact.phone.as_deref() else {
            continue;
        };
        let Some(contact_normalized) = normalize_phone_for_match(contact_phone) else {
            continue;
        };
        if !phones_equivalent(&contact_normalized, &normalized_phone) {
            continue;
        }

        if contact.archived_at.is_some() {
            archived_found = true;
        } else {
            active_matches.push(contact.clone());
        }
        matched_contacts.push(contact);
    }

    Ok(PhoneNameMatches {
        matched_contacts,
        active_matches,
        archived_found,
    })
}

fn phones_equivalent(left: &str, right: &str) -> bool {
    if left == right {
        return true;
    }
    let left_stripped = strip_us_country_code(left);
    let right_stripped = strip_us_country_code(right);
    if let (Some(left_value), Some(right_value)) = (left_stripped, right_stripped) {
        if left_value == right_value {
            return true;
        }
    }
    if let Some(stripped) = left_stripped {
        if stripped == right {
            return true;
        }
    }
    if let Some(stripped) = right_stripped {
        if stripped == left {
            return true;
        }
    }
    false
}

fn strip_us_country_code(value: &str) -> Option<&str> {
    if let Some(stripped) = value.strip_prefix("+1") {
        return Some(stripped);
    }
    if value.len() == 11 && value.starts_with('1') {
        return Some(&value[1..]);
    }
    None
}

fn apply_vcf_update(
    ctx: &Context<'_>,
    now_utc: i64,
    existing_id: ContactId,
    contact: vcf::VcfContact,
) -> Result<()> {
    let vcf::VcfContact {
        display_name,
        emails,
        phone,
        tags,
        next_touchpoint_at,
        cadence_days,
        dates,
    } = contact;

    let mut filtered_emails = Vec::new();
    for email in &emails {
        if filtered_emails.contains(email) {
            continue;
        }
        if let Some(owner_id) = ctx.store.emails().find_contact_id_by_email(email)? {
            if owner_id != existing_id {
                continue;
            }
        }
        filtered_emails.push(email.clone());
    }
    let primary = filtered_emails.first().cloned();
    let update = ContactUpdate {
        display_name: Some(display_name),
        email: primary.clone().map(Some),
        email_source: Some("vcf".to_string()),
        phone: phone.map(Some),
        handle: None,
        timezone: None,
        next_touchpoint_at: next_touchpoint_at.map(Some),
        cadence_days: cadence_days.map(Some),
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
            .update_with_email_ops(now_utc, existing_id, update, email_ops)?;
    merge_tags(ctx, &updated.id, tags)?;
    apply_contact_dates(ctx, now_utc, updated.id, dates)?;
    Ok(())
}

fn stage_existing_matches(
    ctx: &Context<'_>,
    source_name: &str,
    now_utc: i64,
    matches: Vec<Contact>,
    mode: ImportMode,
    reason: &str,
    warning_label: &str,
) -> Result<ImportOutcome> {
    if matches.len() < 2 {
        return Ok(ImportOutcome::Skipped(format!(
            "{warning_label}; no merge candidates created"
        )));
    }

    if matches!(mode, ImportMode::DryRun) {
        let candidates_created = matches.len().saturating_sub(1);
        let warning = format!(
            "dry-run would create {candidates_created} merge candidate(s) for {warning_label}"
        );
        return Ok(ImportOutcome::Staged {
            candidates_created,
            warning,
            contact_created: false,
        });
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
        "{warning_label}; {} merge candidate(s) created",
        candidates_created
    );
    Ok(ImportOutcome::Staged {
        candidates_created,
        warning,
        contact_created: false,
    })
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
        dates,
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
    apply_contact_dates_repo(
        knotter_store::repo::ContactDatesRepo::new(&tx),
        now_utc,
        created.id,
        dates,
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
        EmailMergePolicy, MacosSourceConfig, TelegramAccountConfig, TelegramMergePolicy,
        DEFAULT_TELEGRAM_SNIPPET_LEN,
    };
    use knotter_store::repo::ContactNew;
    use knotter_store::Store;
    use knotter_sync::email::{EmailAddress, EmailHeader};
    use knotter_sync::telegram::{TelegramMessage, TelegramMessageBatch};
    use knotter_sync::vcf;
    use std::cell::{Cell, RefCell};
    use std::collections::{HashMap, HashSet};
    use tempfile::TempDir;

    type TelegramResult<T> = std::result::Result<T, knotter_sync::SyncError>;

    fn telegram_account_config(name: &str) -> TelegramAccountConfig {
        TelegramAccountConfig {
            name: name.to_string(),
            api_id: 123,
            api_hash_env: "KNOTTER_TELEGRAM_HASH".to_string(),
            phone: "+15551234567".to_string(),
            session_path: None,
            tag: None,
            merge_policy: TelegramMergePolicy::NameOrUsername,
            allowlist_user_ids: Vec::new(),
            snippet_len: DEFAULT_TELEGRAM_SNIPPET_LEN,
        }
    }

    fn empty_telegram_report(dry_run: bool) -> TelegramImportReport {
        TelegramImportReport {
            accounts: 0,
            users_seen: 0,
            contacts_created: 0,
            contacts_matched: 0,
            contacts_merged: 0,
            merge_candidates_created: 0,
            messages_seen: 0,
            messages_imported: 0,
            touches_recorded: 0,
            warnings: Vec::new(),
            dry_run,
        }
    }

    fn telegram_user(id: i64, username: Option<&str>, first_name: Option<&str>) -> TelegramUser {
        TelegramUser {
            id,
            username: username.map(|value| value.to_string()),
            phone: None,
            first_name: first_name.map(|value| value.to_string()),
            last_name: None,
            is_bot: false,
        }
    }

    #[derive(Clone)]
    struct FakeTelegramClient {
        account_name: String,
        users: Vec<TelegramUser>,
        batches: HashMap<i64, TelegramMessageBatch>,
    }

    impl FakeTelegramClient {
        fn new(account_name: &str, users: Vec<TelegramUser>) -> Self {
            Self {
                account_name: account_name.to_string(),
                users,
                batches: HashMap::new(),
            }
        }

        fn with_batch(mut self, peer_id: i64, batch: TelegramMessageBatch) -> Self {
            self.batches.insert(peer_id, batch);
            self
        }
    }

    impl telegram::TelegramClient for FakeTelegramClient {
        fn account_name(&self) -> &str {
            &self.account_name
        }

        fn list_users(&mut self) -> TelegramResult<Vec<TelegramUser>> {
            Ok(self.users.clone())
        }

        fn fetch_messages(
            &mut self,
            peer_id: i64,
            _since_message_id: i64,
            _limit: Option<usize>,
        ) -> TelegramResult<TelegramMessageBatch> {
            Ok(self
                .batches
                .get(&peer_id)
                .cloned()
                .unwrap_or(TelegramMessageBatch {
                    messages: Vec::new(),
                    complete: true,
                }))
        }

        fn ensure_authorized(&mut self) -> TelegramResult<()> {
            Ok(())
        }
    }

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
            match_phone_name: false,
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
            match_phone_name: false,
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
        let options = ImportOptions {
            dry_run: false,
            limit: None,
            retry_skipped: false,
            extra_tags: Vec::new(),
            match_phone_name: false,
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
            dates: Vec::new(),
        };

        let outcome =
            apply_vcf_contact(&ctx, "test", now + 10, contact, ImportMode::Apply, &options)
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
    fn vcf_import_matches_by_phone_and_name_when_enabled() {
        let store = Store::open_in_memory().expect("open store");
        store.migrate().expect("migrate");
        let now = 1_700_000_000;

        let existing = store
            .contacts()
            .create(
                now,
                ContactNew {
                    display_name: "Ada Lovelace".to_string(),
                    email: None,
                    phone: Some("+1 (415) 555-1212".to_string()),
                    handle: None,
                    timezone: None,
                    next_touchpoint_at: None,
                    cadence_days: None,
                    archived_at: None,
                },
            )
            .expect("create contact");

        let config = AppConfig::default();
        let ctx = Context {
            store: &store,
            json: false,
            config: &config,
        };
        let options = ImportOptions {
            dry_run: false,
            limit: None,
            retry_skipped: false,
            extra_tags: Vec::new(),
            match_phone_name: true,
        };
        let contact = vcf::VcfContact {
            display_name: "Ada Lovelace".to_string(),
            emails: Vec::new(),
            phone: Some("415-555-1212".to_string()),
            tags: Vec::new(),
            next_touchpoint_at: None,
            cadence_days: None,
            dates: Vec::new(),
        };

        let outcome =
            apply_vcf_contact(&ctx, "test", now + 10, contact, ImportMode::Apply, &options)
                .expect("apply vcf");
        assert!(matches!(outcome, ImportOutcome::Updated));

        let updated = store
            .contacts()
            .get(existing.id)
            .expect("get contact")
            .expect("contact exists");
        assert_eq!(updated.display_name, "Ada Lovelace");
        let contacts = store.contacts().list_all().expect("list contacts");
        assert_eq!(contacts.len(), 1);
    }

    #[test]
    fn vcf_import_matches_plus_one_and_eleven_digit_us_numbers() {
        let store = Store::open_in_memory().expect("open store");
        store.migrate().expect("migrate");
        let now = 1_700_000_000;

        let existing = store
            .contacts()
            .create(
                now,
                ContactNew {
                    display_name: "Grace Hopper".to_string(),
                    email: None,
                    phone: Some("+1 (212) 555-0100".to_string()),
                    handle: None,
                    timezone: None,
                    next_touchpoint_at: None,
                    cadence_days: None,
                    archived_at: None,
                },
            )
            .expect("create contact");

        let config = AppConfig::default();
        let ctx = Context {
            store: &store,
            json: false,
            config: &config,
        };
        let options = ImportOptions {
            dry_run: false,
            limit: None,
            retry_skipped: false,
            extra_tags: Vec::new(),
            match_phone_name: true,
        };
        let contact = vcf::VcfContact {
            display_name: "Grace Hopper".to_string(),
            emails: Vec::new(),
            phone: Some("12125550100".to_string()),
            tags: Vec::new(),
            next_touchpoint_at: None,
            cadence_days: None,
            dates: Vec::new(),
        };

        let outcome =
            apply_vcf_contact(&ctx, "test", now + 10, contact, ImportMode::Apply, &options)
                .expect("apply vcf");
        assert!(matches!(outcome, ImportOutcome::Updated));

        let updated = store
            .contacts()
            .get(existing.id)
            .expect("get contact")
            .expect("contact exists");
        assert_eq!(updated.display_name, "Grace Hopper");
        let contacts = store.contacts().list_all().expect("list contacts");
        assert_eq!(contacts.len(), 1);
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
        let options = ImportOptions {
            dry_run: true,
            limit: None,
            retry_skipped: false,
            extra_tags: Vec::new(),
            match_phone_name: false,
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
            dates: Vec::new(),
        };

        let outcome = apply_vcf_contact(
            &ctx,
            "test",
            now + 10,
            contact,
            ImportMode::DryRun,
            &options,
        )
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
            match_phone_name: false,
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
            match_phone_name: false,
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

    #[test]
    fn telegram_import_matches_handle() {
        let store = Store::open_in_memory().expect("open store");
        store.migrate().expect("migrate");
        let now = 1_700_000_000;
        let contact = store
            .contacts()
            .create(
                now,
                ContactNew {
                    display_name: "Alice".to_string(),
                    email: None,
                    phone: None,
                    handle: Some("@alice".to_string()),
                    timezone: None,
                    next_touchpoint_at: None,
                    cadence_days: None,
                    archived_at: None,
                },
            )
            .expect("create contact");

        let config = AppConfig::default();
        let ctx = Context {
            store: &store,
            json: false,
            config: &config,
        };
        let options = ImportOptions {
            dry_run: false,
            limit: None,
            retry_skipped: false,
            extra_tags: Vec::new(),
            match_phone_name: false,
        };
        let account_cfg = telegram_account_config("primary");
        let mut report = empty_telegram_report(false);
        let mut client = FakeTelegramClient::new(
            "primary",
            vec![telegram_user(7, Some("alice"), Some("Alice"))],
        );

        let stop = import_telegram_account_with_client(
            &ctx,
            &account_cfg,
            &options,
            true,
            false,
            &mut report,
            &mut client,
            now,
        )
        .expect("import");

        assert!(!stop);
        assert_eq!(report.contacts_matched, 1);
        let linked = store
            .telegram_accounts()
            .list_for_contact(contact.id)
            .expect("list telegram accounts");
        assert_eq!(linked.len(), 1);
    }

    #[test]
    fn telegram_messages_only_skips_contact_creation() {
        let store = Store::open_in_memory().expect("open store");
        store.migrate().expect("migrate");
        let now = 1_700_000_000;

        let config = AppConfig::default();
        let ctx = Context {
            store: &store,
            json: false,
            config: &config,
        };
        let options = ImportOptions {
            dry_run: false,
            limit: None,
            retry_skipped: false,
            extra_tags: Vec::new(),
            match_phone_name: false,
        };
        let account_cfg = telegram_account_config("primary");
        let mut report = empty_telegram_report(false);
        let mut client = FakeTelegramClient::new(
            "primary",
            vec![telegram_user(11, Some("alice"), Some("Alice"))],
        );

        let stop = import_telegram_account_with_client(
            &ctx,
            &account_cfg,
            &options,
            false,
            true,
            &mut report,
            &mut client,
            now,
        )
        .expect("import");

        assert!(!stop);
        let matches = store
            .contacts()
            .list_by_display_name("Alice")
            .expect("list contacts");
        assert!(matches.is_empty());
        assert_eq!(report.contacts_created, 0);
        assert!(report
            .warnings
            .iter()
            .any(|warning| warning.contains("not linked")));
    }

    #[test]
    fn telegram_messages_only_matches_existing_handle() {
        let store = Store::open_in_memory().expect("open store");
        store.migrate().expect("migrate");
        let now = 1_700_000_000;

        let contact = store
            .contacts()
            .create(
                now,
                ContactNew {
                    display_name: "Dora".to_string(),
                    email: None,
                    phone: None,
                    handle: Some("@dora".to_string()),
                    timezone: None,
                    next_touchpoint_at: None,
                    cadence_days: None,
                    archived_at: None,
                },
            )
            .expect("create contact");

        let config = AppConfig::default();
        let ctx = Context {
            store: &store,
            json: false,
            config: &config,
        };
        let options = ImportOptions {
            dry_run: false,
            limit: None,
            retry_skipped: false,
            extra_tags: Vec::new(),
            match_phone_name: false,
        };
        let account_cfg = telegram_account_config("primary");
        let mut report = empty_telegram_report(false);
        let user = telegram_user(55, Some("dora"), Some("Dora"));
        let batch = TelegramMessageBatch {
            messages: vec![TelegramMessage {
                id: 1,
                peer_id: user.id,
                sender_id: Some(user.id),
                occurred_at: now - 5,
                outgoing: false,
                text: Some("hey".to_string()),
            }],
            complete: true,
        };
        let mut client =
            FakeTelegramClient::new("primary", vec![user.clone()]).with_batch(user.id, batch);

        let stop = import_telegram_account_with_client(
            &ctx,
            &account_cfg,
            &options,
            false,
            true,
            &mut report,
            &mut client,
            now,
        )
        .expect("import");

        assert!(!stop);
        let linked = store
            .telegram_accounts()
            .list_for_contact(contact.id)
            .expect("list telegram accounts");
        assert_eq!(linked.len(), 1);
        let state = store
            .telegram_sync()
            .load_state("primary", user.id)
            .expect("load state");
        assert!(state.is_some());
    }

    #[test]
    fn telegram_contacts_only_skips_messages_and_state() {
        let store = Store::open_in_memory().expect("open store");
        store.migrate().expect("migrate");
        let now = 1_700_000_000;

        let config = AppConfig::default();
        let ctx = Context {
            store: &store,
            json: false,
            config: &config,
        };
        let options = ImportOptions {
            dry_run: false,
            limit: None,
            retry_skipped: false,
            extra_tags: Vec::new(),
            match_phone_name: false,
        };
        let account_cfg = telegram_account_config("primary");
        let mut report = empty_telegram_report(false);
        let user = telegram_user(21, Some("bob"), Some("Bob"));
        let message = TelegramMessage {
            id: 1,
            peer_id: user.id,
            sender_id: Some(user.id),
            occurred_at: now - 10,
            outgoing: false,
            text: Some("hi".to_string()),
        };
        let batch = TelegramMessageBatch {
            messages: vec![message],
            complete: true,
        };
        let mut client =
            FakeTelegramClient::new("primary", vec![user.clone()]).with_batch(user.id, batch);

        let stop = import_telegram_account_with_client(
            &ctx,
            &account_cfg,
            &options,
            true,
            false,
            &mut report,
            &mut client,
            now,
        )
        .expect("import");

        assert!(!stop);
        let contacts = store
            .contacts()
            .list_by_display_name("Bob")
            .expect("list contacts");
        assert_eq!(contacts.len(), 1);
        let linked = store
            .telegram_accounts()
            .list_for_contact(contacts[0].id)
            .expect("list telegram accounts");
        assert_eq!(linked.len(), 1);
        let state = store
            .telegram_sync()
            .load_state("primary", user.id)
            .expect("load state");
        assert!(state.is_none());
    }

    #[test]
    fn telegram_limit_incomplete_does_not_advance_state() {
        let store = Store::open_in_memory().expect("open store");
        store.migrate().expect("migrate");
        let now = 1_700_000_000;

        let contact = store
            .contacts()
            .create(
                now,
                ContactNew {
                    display_name: "Cara".to_string(),
                    email: None,
                    phone: None,
                    handle: None,
                    timezone: None,
                    next_touchpoint_at: None,
                    cadence_days: None,
                    archived_at: None,
                },
            )
            .expect("create contact");

        let config = AppConfig::default();
        let ctx = Context {
            store: &store,
            json: false,
            config: &config,
        };
        let options = ImportOptions {
            dry_run: false,
            limit: Some(1),
            retry_skipped: false,
            extra_tags: Vec::new(),
            match_phone_name: false,
        };
        let telegram_ctx = TelegramImportContext {
            ctx: &ctx,
            options: &options,
            now_utc: now,
            account_name: "primary",
            merge_policy: TelegramMergePolicy::NameOrUsername,
            allowlist_user_ids: &[],
            snippet_len: DEFAULT_TELEGRAM_SNIPPET_LEN,
            messages_only: false,
        };
        let user = telegram_user(42, Some("cara"), Some("Cara"));
        let batch = TelegramMessageBatch {
            messages: vec![TelegramMessage {
                id: 10,
                peer_id: user.id,
                sender_id: Some(user.id),
                occurred_at: now - 5,
                outgoing: false,
                text: Some("hello".to_string()),
            }],
            complete: false,
        };
        let mut client = FakeTelegramClient::new("primary", Vec::new()).with_batch(user.id, batch);
        let mut report = empty_telegram_report(false);

        let stop =
            import_telegram_messages(&telegram_ctx, &mut client, &user, contact.id, &mut report)
                .expect("import messages");
        assert!(!stop);
        let state = store
            .telegram_sync()
            .load_state("primary", user.id)
            .expect("load state");
        assert!(state.is_none());
        assert!(report
            .warnings
            .iter()
            .any(|warning| warning.contains("hit --limit")));
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

        fn import_telegram(&self, _ctx: &Context<'_>, _common: &ImportCommonArgs) -> Result<()> {
            self.record("telegram")
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
            no_telegram: false,
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
