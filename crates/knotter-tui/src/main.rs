mod actions;
mod app;
mod ui;
mod util;

use std::fs;
use std::io::{self, Stdout};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result};
use clap::Parser;
use crossterm::event::{self, Event};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::actions::execute_action;
use crate::app::App;
use knotter_core::rules::validate_soon_days;
use knotter_store::{paths, Store};

#[derive(Debug, Parser)]
#[command(name = "knotter-tui", version, about = "knotter TUI")]
struct Args {
    #[arg(long)]
    db_path: Option<PathBuf>,
    #[arg(long, default_value_t = 7)]
    soon_days: i64,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let db_path = match args.db_path {
        Some(path) => {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    let created = !parent.exists();
                    fs::create_dir_all(parent)
                        .with_context(|| format!("create db directory {}", parent.display()))?;
                    if created {
                        restrict_dir_permissions(parent)?;
                    }
                }
            }
            path
        }
        None => paths::db_path()?,
    };

    let store = Store::open(&db_path)?;
    store.migrate()?;

    let mut app = App::new();
    app.soon_days = validate_soon_days(args.soon_days)?;

    let mut terminal = TerminalGuard::new()?;
    run_app(&mut terminal, &store, &mut app)
}

fn run_app(terminal: &mut TerminalGuard, store: &Store, app: &mut App) -> Result<()> {
    let tick_rate = Duration::from_millis(200);
    let mut last_tick = Instant::now();

    loop {
        while let Some(action) = app.next_action() {
            if let Err(err) = execute_action(app, store, action) {
                app.set_error(err.to_string());
            }
        }

        terminal.terminal_mut().draw(|frame| ui::draw(frame, app))?;

        if app.should_quit {
            break;
        }

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or(Duration::from_secs(0));
        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(key) => app.handle_key(key),
                Event::Resize(_, _) => {}
                _ => {}
            }
        }

        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }
    }

    Ok(())
}

struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalGuard {
    fn new() -> Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;

        let original_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let _ = restore_terminal();
            original_hook(info);
        }));

        Ok(Self { terminal })
    }

    fn terminal_mut(&mut self) -> &mut Terminal<CrosstermBackend<Stdout>> {
        &mut self.terminal
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = restore_terminal();
    }
}

fn restore_terminal() -> Result<()> {
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;
    Ok(())
}

#[cfg(unix)]
fn restrict_dir_permissions(dir: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let perms = fs::Permissions::from_mode(0o700);
    fs::set_permissions(dir, perms)
        .with_context(|| format!("restrict permissions for {}", dir.display()))?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict_dir_permissions(_dir: &std::path::Path) -> Result<()> {
    Ok(())
}
