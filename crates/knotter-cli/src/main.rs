mod commands;
mod error;
mod notify;
mod util;

use anyhow::{Context as _, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;
use tracing::debug;

use crate::commands::{
    backup, completions, contacts, dates, interactions, loops, merge, remind, schedule, sync, tags,
    tui, Context,
};
use crate::error::{exit_code_for, report_error};
use knotter_config as config;
use knotter_store::{paths, Store};

#[derive(Debug, Parser)]
#[command(name = "knotter", version, about = "knotter CLI")]
struct Cli {
    #[arg(long, global = true)]
    db_path: Option<PathBuf>,
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[arg(long, global = true)]
    json: bool,
    #[arg(long, short, global = true)]
    verbose: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Backup(backup::BackupArgs),
    /// Generate shell completions
    Completions(completions::CompletionsArgs),
    #[command(name = "add-contact")]
    AddContact(contacts::AddContactArgs),
    #[command(name = "edit-contact")]
    EditContact(contacts::EditContactArgs),
    Show(contacts::ShowArgs),
    List(contacts::ListArgs),
    Delete(contacts::DeleteArgs),
    #[command(name = "archive-contact")]
    ArchiveContact(contacts::ArchiveArgs),
    #[command(name = "unarchive-contact")]
    UnarchiveContact(contacts::UnarchiveArgs),
    #[command(subcommand)]
    Tag(tags::TagCommand),
    #[command(subcommand)]
    Date(dates::DateCommand),
    #[command(subcommand)]
    Loops(loops::LoopCommand),
    #[command(subcommand)]
    Merge(merge::MergeCommand),
    #[command(name = "add-note")]
    AddNote(interactions::AddNoteArgs),
    Touch(interactions::TouchArgs),
    Schedule(schedule::ScheduleArgs),
    #[command(name = "clear-schedule")]
    ClearSchedule(schedule::ClearScheduleArgs),
    Remind(remind::RemindArgs),
    Sync(sync::SyncArgs),
    Tui(tui::TuiArgs),
    #[command(subcommand)]
    Import(sync::ImportCommand),
    #[command(subcommand)]
    Export(sync::ExportCommand),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let verbose = cli.verbose;
    init_logging(verbose);
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            report_error(&err, verbose);
            exit_code_for(&err)
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    let Cli {
        db_path,
        config: config_path,
        json,
        verbose,
        command,
    } = cli;

    match command {
        Command::Tui(args) => tui::launch(db_path, config_path, args, verbose),
        Command::Completions(args) => completions::emit(args),
        command => {
            let app_config = config::load(config_path.clone()).with_context(|| "load config")?;
            if verbose {
                match config::resolve_config_path(config_path.clone()) {
                    Ok(path) => {
                        if path.exists() {
                            debug!(path = %path.display(), "config resolved");
                        } else {
                            debug!(path = %path.display(), "config missing, using defaults");
                        }
                    }
                    Err(err) => {
                        debug!(error = %err, "config unavailable");
                    }
                }
            }
            let db_path =
                paths::resolve_db_path(db_path).with_context(|| "resolve database path")?;

            if verbose {
                debug!(path = %db_path.display(), "database path resolved");
            }

            let store = Store::open(&db_path)
                .with_context(|| format!("open database {}", db_path.display()))?;
            store.migrate().with_context(|| "run migrations")?;

            let ctx = Context {
                store: &store,
                json,
                config: &app_config,
            };

            match command {
                Command::AddContact(args) => contacts::add_contact(&ctx, args),
                Command::Backup(args) => backup::backup(&ctx, args),
                Command::EditContact(args) => contacts::edit_contact(&ctx, args),
                Command::Show(args) => contacts::show_contact(&ctx, args),
                Command::List(args) => contacts::list_contacts(&ctx, args),
                Command::Delete(args) => contacts::delete_contact(&ctx, args),
                Command::ArchiveContact(args) => contacts::archive_contact(&ctx, args),
                Command::UnarchiveContact(args) => contacts::unarchive_contact(&ctx, args),
                Command::Tag(cmd) => match cmd {
                    tags::TagCommand::Add(args) => tags::add_tag(&ctx, args),
                    tags::TagCommand::Rm(args) => tags::remove_tag(&ctx, args),
                    tags::TagCommand::Ls(args) => tags::list_tags(&ctx, args),
                },
                Command::Date(cmd) => match cmd {
                    dates::DateCommand::Add(args) => dates::add_date(&ctx, args),
                    dates::DateCommand::Ls(args) => dates::list_dates(&ctx, args),
                    dates::DateCommand::Rm(args) => dates::remove_date(&ctx, args),
                },
                Command::Loops(cmd) => match cmd {
                    loops::LoopCommand::Apply(args) => loops::apply_loops(&ctx, args),
                },
                Command::Merge(cmd) => match cmd {
                    merge::MergeCommand::List(args) => merge::list_merges(&ctx, args),
                    merge::MergeCommand::Show(args) => merge::show_merge(&ctx, args),
                    merge::MergeCommand::Apply(args) => merge::apply_merge(&ctx, args),
                    merge::MergeCommand::ApplyAll(args) => merge::apply_all_merges(&ctx, args),
                    merge::MergeCommand::Dismiss(args) => merge::dismiss_merge(&ctx, args),
                    merge::MergeCommand::Contacts(args) => merge::merge_contacts(&ctx, args),
                },
                Command::AddNote(args) => interactions::add_note(&ctx, args),
                Command::Touch(args) => interactions::touch_contact(&ctx, args),
                Command::Schedule(args) => schedule::schedule_contact(&ctx, args),
                Command::ClearSchedule(args) => schedule::clear_schedule(&ctx, args),
                Command::Remind(args) => remind::remind(&ctx, args),
                Command::Sync(args) => sync::sync_all(&ctx, args),
                Command::Tui(_) => unreachable!("tui command handled before store initialization"),
                Command::Completions(_) => {
                    unreachable!("completions command handled before store initialization")
                }
                Command::Import(cmd) => match cmd {
                    sync::ImportCommand::Vcf(args) => sync::import_vcf(&ctx, args),
                    sync::ImportCommand::Macos(args) => sync::import_macos(&ctx, args),
                    sync::ImportCommand::Carddav(args) => sync::import_carddav(&ctx, args),
                    sync::ImportCommand::Email(args) => sync::import_email(&ctx, args),
                    sync::ImportCommand::Telegram(args) => sync::import_telegram(&ctx, args),
                    sync::ImportCommand::Source(args) => sync::import_source(&ctx, args),
                },
                Command::Export(cmd) => match cmd {
                    sync::ExportCommand::Vcf(args) => sync::export_vcf(&ctx, args),
                    sync::ExportCommand::Ics(args) => sync::export_ics(&ctx, args),
                    sync::ExportCommand::Json(args) => sync::export_json(&ctx, args),
                },
            }
        }
    }
}

fn init_logging(verbose: bool) {
    use tracing_subscriber::{fmt, EnvFilter};
    let default_level = if verbose { "debug" } else { "warn" };
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level));
    let _ = fmt()
        .with_env_filter(filter)
        .with_target(false)
        .without_time()
        .try_init();
}
