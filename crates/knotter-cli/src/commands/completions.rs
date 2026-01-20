use anyhow::Result;
use clap::CommandFactory;
use clap_complete::generate;
use std::io;

#[derive(Debug, clap::Args)]
pub struct CompletionsArgs {
    #[arg(value_enum)]
    pub shell: CompletionShell,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum CompletionShell {
    Bash,
    Zsh,
    Fish,
    #[value(name = "powershell")]
    PowerShell,
    Elvish,
}

impl From<CompletionShell> for clap_complete::Shell {
    fn from(shell: CompletionShell) -> Self {
        match shell {
            CompletionShell::Bash => clap_complete::Shell::Bash,
            CompletionShell::Zsh => clap_complete::Shell::Zsh,
            CompletionShell::Fish => clap_complete::Shell::Fish,
            CompletionShell::PowerShell => clap_complete::Shell::PowerShell,
            CompletionShell::Elvish => clap_complete::Shell::Elvish,
        }
    }
}

pub fn emit(args: CompletionsArgs) -> Result<()> {
    let mut cmd = crate::Cli::command();
    let name = cmd.get_name().to_string();
    let mut stdout = io::stdout().lock();
    let shell: clap_complete::Shell = args.shell.into();
    generate(shell, &mut cmd, name, &mut stdout);
    Ok(())
}
