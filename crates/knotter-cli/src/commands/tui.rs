use anyhow::{anyhow, Context as _, Result};
use clap::Args;
use knotter_core::rules::validate_soon_days;
use knotter_store::paths;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Args)]
pub struct TuiArgs {
    #[arg(long)]
    pub soon_days: Option<i64>,
}

pub fn launch(
    db_path: Option<PathBuf>,
    config_path: Option<PathBuf>,
    args: TuiArgs,
    verbose: bool,
) -> Result<()> {
    let db_path = paths::resolve_db_path(db_path).with_context(|| "resolve database path")?;
    if verbose {
        eprintln!("db: {}", db_path.display());
    }
    let mut command = build_command(&db_path, config_path, args.soon_days)?;

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = command.exec();
        Err(exec_error(err))
    }

    #[cfg(not(unix))]
    {
        let status = command.status().with_context(|| "launch knotter-tui")?;
        std::process::exit(status.code().unwrap_or(1));
    }
}

fn build_command(
    db_path: &Path,
    config_path: Option<PathBuf>,
    soon_days: Option<i64>,
) -> Result<Command> {
    let binary = find_tui_binary();
    let mut command = Command::new(binary);
    command.arg("--db-path").arg(db_path);
    if let Some(path) = config_path {
        command.arg("--config").arg(path);
    }
    if let Some(value) = soon_days {
        let soon_days = validate_soon_days(value)?;
        command.arg("--soon-days").arg(soon_days.to_string());
    }
    Ok(command)
}

fn find_tui_binary() -> PathBuf {
    let name = format!("knotter-tui{}", env::consts::EXE_SUFFIX);
    if let Ok(current) = env::current_exe() {
        if let Some(dir) = current.parent() {
            let candidate = dir.join(&name);
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    PathBuf::from(name)
}

fn exec_error(err: std::io::Error) -> anyhow::Error {
    if err.kind() == std::io::ErrorKind::NotFound {
        return anyhow!(
            "knotter-tui binary not found; build it with `cargo build -p knotter-tui` or install the package"
        );
    }
    anyhow!("launch knotter-tui failed: {}", err)
}
