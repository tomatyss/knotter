use anyhow::Result;
use knotter_store::Store;
use serde::Serialize;

pub mod backup;
pub mod contacts;
pub mod interactions;
pub mod remind;
pub mod schedule;
pub mod sync;
pub mod tags;
pub mod tui;

pub const DEFAULT_SOON_DAYS: i64 = 7;
pub const DEFAULT_INTERACTION_LIMIT: i64 = 20;

pub struct Context<'a> {
    pub store: &'a Store,
    pub json: bool,
}

pub fn print_json<T: Serialize>(value: &T) -> Result<()> {
    let output = serde_json::to_string_pretty(value)?;
    println!("{}", output);
    Ok(())
}
