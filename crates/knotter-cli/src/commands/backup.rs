use crate::commands::{print_json, Context};
use anyhow::{Context as _, Result};
use clap::Args;
use knotter_store::error::StoreError;
use knotter_store::paths;
use serde::Serialize;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Args)]
pub struct BackupArgs {
    #[arg(long)]
    pub out: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
struct BackupReport {
    output: String,
    size_bytes: u64,
}

pub fn backup(ctx: &Context<'_>, args: BackupArgs) -> Result<()> {
    let out = match args.out {
        Some(path) => path,
        None => paths::backup_path()?,
    };

    if let Err(err) = ctx.store.backup_to(&out) {
        if matches!(err, StoreError::InvalidBackupPath(_)) {
            return Err(err)
                .with_context(|| format!("backup path matches database: {}", out.display()));
        }
        return Err(err).with_context(|| format!("backup database to {}", out.display()));
    }

    let size = fs::metadata(&out)
        .with_context(|| format!("stat backup file {}", out.display()))?
        .len();

    if ctx.json {
        let report = BackupReport {
            output: out.display().to_string(),
            size_bytes: size,
        };
        return print_json(&report);
    }

    println!("Backup written to {}", out.display());
    Ok(())
}
