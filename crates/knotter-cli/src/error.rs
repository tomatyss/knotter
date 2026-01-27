use anyhow::Error;
use knotter_config::ConfigError;
use knotter_core::filter::FilterParseError;
use knotter_core::time::TimeParseError;
use knotter_core::CoreError;
use knotter_store::error::{StoreError, StoreErrorKind};
use knotter_sync::error::SyncError;
use std::process::ExitCode;
use thiserror::Error as ThisError;

pub const EXIT_FAILURE: u8 = 1;
pub const EXIT_NOT_FOUND: u8 = 2;
pub const EXIT_INVALID_INPUT: u8 = 3;

#[derive(Debug, ThisError)]
pub enum CliError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("not found: {0}")]
    NotFound(String),
}

pub fn invalid_input(message: impl Into<String>) -> Error {
    CliError::InvalidInput(message.into()).into()
}

pub fn not_found(message: impl Into<String>) -> Error {
    CliError::NotFound(message.into()).into()
}

pub fn report_error(err: &Error, verbose: bool) {
    if verbose {
        eprintln!("error: {:#}", err);
    } else {
        eprintln!("error: {}", err);
    }
}

pub fn exit_code_for(err: &Error) -> ExitCode {
    for cause in err.chain() {
        if let Some(cli_err) = cause.downcast_ref::<CliError>() {
            return ExitCode::from(match cli_err {
                CliError::InvalidInput(_) => EXIT_INVALID_INPUT,
                CliError::NotFound(_) => EXIT_NOT_FOUND,
            });
        }
        if let Some(store_err) = cause.downcast_ref::<StoreError>() {
            return ExitCode::from(store_exit_code(store_err));
        }
        if let Some(config_err) = cause.downcast_ref::<ConfigError>() {
            return ExitCode::from(config_exit_code(config_err));
        }
        if let Some(sync_err) = cause.downcast_ref::<SyncError>() {
            return ExitCode::from(sync_exit_code(sync_err));
        }
        if let Some(_core_err) = cause.downcast_ref::<CoreError>() {
            return ExitCode::from(EXIT_INVALID_INPUT);
        }
        if let Some(_parse_err) = cause.downcast_ref::<FilterParseError>() {
            return ExitCode::from(EXIT_INVALID_INPUT);
        }
        if let Some(_parse_err) = cause.downcast_ref::<TimeParseError>() {
            return ExitCode::from(EXIT_INVALID_INPUT);
        }
    }
    ExitCode::from(EXIT_FAILURE)
}

fn store_exit_code(err: &StoreError) -> u8 {
    match err.kind() {
        StoreErrorKind::NotFound => EXIT_NOT_FOUND,
        StoreErrorKind::InvalidId
        | StoreErrorKind::InvalidFilter
        | StoreErrorKind::InvalidBackupPath
        | StoreErrorKind::InvalidInteractionKind
        | StoreErrorKind::InvalidDataPath
        | StoreErrorKind::DuplicateEmail
        | StoreErrorKind::InvalidMerge
        | StoreErrorKind::Core => EXIT_INVALID_INPUT,
        StoreErrorKind::MissingHomeDir
        | StoreErrorKind::Migration
        | StoreErrorKind::Sql
        | StoreErrorKind::Io => EXIT_FAILURE,
    }
}

fn config_exit_code(err: &ConfigError) -> u8 {
    match err {
        ConfigError::MissingHomeDir => EXIT_FAILURE,
        ConfigError::InvalidConfigPath(_)
        | ConfigError::MissingConfigFile(_)
        | ConfigError::InsecurePermissions(_)
        | ConfigError::InvalidSoonDays(_)
        | ConfigError::InvalidCadenceDays(_)
        | ConfigError::InvalidLoopDefaultCadence(_)
        | ConfigError::InvalidLoopCadenceDays(_)
        | ConfigError::InvalidLoopTag(_)
        | ConfigError::DuplicateLoopTag(_)
        | ConfigError::InvalidContactSourceName(_)
        | ConfigError::DuplicateContactSourceName(_)
        | ConfigError::InvalidContactSourceField { .. }
        | ConfigError::InvalidEmailAccountName(_)
        | ConfigError::DuplicateEmailAccountName(_)
        | ConfigError::InvalidEmailAccountField { .. }
        | ConfigError::InvalidNotificationsEmailField { .. }
        | ConfigError::Read { .. }
        | ConfigError::Parse { .. } => EXIT_INVALID_INPUT,
    }
}

fn sync_exit_code(err: &SyncError) -> u8 {
    match err {
        SyncError::Unavailable(_) => EXIT_INVALID_INPUT,
        SyncError::Command(_) | SyncError::Io(_) => EXIT_FAILURE,
        SyncError::Core(_) | SyncError::Parse(_) => EXIT_INVALID_INPUT,
        #[cfg(feature = "dav-sync")]
        SyncError::Http(_) => EXIT_FAILURE,
        #[cfg(feature = "dav-sync")]
        SyncError::Url(_) => EXIT_INVALID_INPUT,
    }
}
