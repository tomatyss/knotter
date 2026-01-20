# Shell Completions

knotter can generate shell completion scripts using:

```
knotter completions <shell>
```

Supported shells: `bash`, `zsh`, `fish`, `powershell`, `elvish`.

## Bash

User install (no root required):

```
mkdir -p ~/.local/share/bash-completion/completions
knotter completions bash > ~/.local/share/bash-completion/completions/knotter
```

Quick one-off session:

```
source <(knotter completions bash)
```

## Zsh

User install (ensure the directory is on your `fpath`):

```
mkdir -p ~/.zsh/completions
knotter completions zsh > ~/.zsh/completions/_knotter
```

Then add to your `~/.zshrc` if needed:

```
fpath=(~/.zsh/completions $fpath)
autoload -Uz compinit
compinit
```

## Fish

```
mkdir -p ~/.config/fish/completions
knotter completions fish > ~/.config/fish/completions/knotter.fish
```

## PowerShell

Add to your PowerShell profile:

```
knotter completions powershell | Out-String | Invoke-Expression
```

Persist by adding the line to `$PROFILE`.

## Elvish

```
mkdir -p ~/.elvish/lib
knotter completions elvish > ~/.elvish/lib/knotter.elv
```

Then load it in `~/.elvish/rc.elv`:

```
use knotter
```

## Notes

- Completions are generated from the current CLI, so re-run after upgrades.
- Use `knotter completions --help` for the full list of supported shells.
