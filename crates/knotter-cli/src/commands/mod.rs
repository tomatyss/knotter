use anyhow::Result;
use knotter_config::AppConfig;
use knotter_store::Store;
use serde::Serialize;
use std::io::{self, Write};

pub mod backup;
pub mod completions;
pub mod contacts;
pub mod dates;
pub mod interactions;
pub mod loops;
pub mod merge;
pub mod remind;
pub mod schedule;
pub mod sync;
pub mod tags;
pub mod tui;

pub const DEFAULT_INTERACTION_LIMIT: i64 = 20;

pub struct Context<'a> {
    pub store: &'a Store,
    pub json: bool,
    pub config: &'a AppConfig,
}

pub fn print_json<T: Serialize>(value: &T) -> Result<()> {
    let mut stdout = io::stdout().lock();
    serde_json::to_writer_pretty(&mut stdout, value)?;
    writeln!(stdout)?;
    Ok(())
}
