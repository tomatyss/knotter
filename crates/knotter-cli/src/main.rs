mod commands;
mod notify;
mod util;

use anyhow::{Context as _, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

use crate::commands::{backup, contacts, interactions, remind, schedule, sync, tags, tui, Context};
use knotter_config as config;
use knotter_core::{filter::FilterParseError, CoreError};
use knotter_store::error::{StoreError, StoreErrorKind};
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
    #[command(name = "add-contact")]
    AddContact(contacts::AddContactArgs),
    #[command(name = "edit-contact")]
    EditContact(contacts::EditContactArgs),
    Show(contacts::ShowArgs),
    List(contacts::ListArgs),
    Delete(contacts::DeleteArgs),
    #[command(subcommand)]
    Tag(tags::TagCommand),
    #[command(name = "add-note")]
    AddNote(interactions::AddNoteArgs),
    Touch(interactions::TouchArgs),
    Schedule(schedule::ScheduleArgs),
    #[command(name = "clear-schedule")]
    ClearSchedule(schedule::ClearScheduleArgs),
    Remind(remind::RemindArgs),
    Tui(tui::TuiArgs),
    #[command(subcommand)]
    Import(sync::ImportCommand),
    #[command(subcommand)]
    Export(sync::ExportCommand),
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{:#}", err);
            exit_code_for(&err)
        }
    }
}

fn run() -> Result<()> {
    let Cli {
        db_path,
        config: config_path,
        json,
        verbose,
        command,
    } = Cli::parse();

    match command {
        Command::Tui(args) => tui::launch(db_path, config_path, args, verbose),
        command => {
            let app_config = config::load(config_path.clone()).with_context(|| "load config")?;
            if verbose {
                match config::resolve_config_path(config_path.clone()) {
                    Ok(path) => {
                        if path.exists() {
                            eprintln!("config: {}", path.display());
                        } else {
                            eprintln!("config: {} (missing, using defaults)", path.display());
                        }
                    }
                    Err(err) => {
                        eprintln!("config: unavailable ({err})");
                    }
                }
            }
            let db_path =
                paths::resolve_db_path(db_path).with_context(|| "resolve database path")?;

            if verbose {
                eprintln!("db: {}", db_path.display());
            }

            let store = Store::open(&db_path)?;
            store.migrate()?;

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
                Command::Tag(cmd) => match cmd {
                    tags::TagCommand::Add(args) => tags::add_tag(&ctx, args),
                    tags::TagCommand::Rm(args) => tags::remove_tag(&ctx, args),
                    tags::TagCommand::Ls(args) => tags::list_tags(&ctx, args),
                },
                Command::AddNote(args) => interactions::add_note(&ctx, args),
                Command::Touch(args) => interactions::touch_contact(&ctx, args),
                Command::Schedule(args) => schedule::schedule_contact(&ctx, args),
                Command::ClearSchedule(args) => schedule::clear_schedule(&ctx, args),
                Command::Remind(args) => remind::remind(&ctx, args),
                Command::Tui(_) => unreachable!("tui command handled before store initialization"),
                Command::Import(cmd) => match cmd {
                    sync::ImportCommand::Vcf(args) => sync::import_vcf(&ctx, args),
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

fn exit_code_for(err: &anyhow::Error) -> ExitCode {
    for cause in err.chain() {
        if let Some(store_err) = cause.downcast_ref::<StoreError>() {
            return ExitCode::from(store_exit_code(store_err));
        }
        if let Some(_core_err) = cause.downcast_ref::<CoreError>() {
            return ExitCode::from(4);
        }
        if let Some(_parse_err) = cause.downcast_ref::<FilterParseError>() {
            return ExitCode::from(3);
        }
    }
    ExitCode::from(1)
}

fn store_exit_code(err: &StoreError) -> u8 {
    match err.kind() {
        StoreErrorKind::NotFound => 2,
        StoreErrorKind::InvalidId
        | StoreErrorKind::InvalidFilter
        | StoreErrorKind::InvalidBackupPath => 3,
        StoreErrorKind::Core => 4,
        _ => 1,
    }
}
