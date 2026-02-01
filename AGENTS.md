# Repository Guidelines

## Project Structure & Module Organization
- Current root: `docs/ARCHITECTURE.md` (design/spec) and `TODO.md` (implementation plan).
- Planned layout (per `docs/ARCHITECTURE.md`): Rust workspace with `crates/` containing `knotter-core`, `knotter-store`, `knotter-sync`, `knotter-cli`, `knotter-tui`, plus an optional `knotter` bin crate. Keep dependencies one-way: core -> store/sync -> CLI/TUI.

## Build, Test, and Development Commands
No build system is checked in yet. Once the Cargo workspace is created, use standard Rust commands:
- `cargo build` — compile the workspace.
- `cargo test` — run unit/integration tests.
- `cargo fmt` / `cargo fmt --check` — format code.
- `cargo clippy --all-targets --all-features -D warnings` — lint.
- After any update (code or docs), run `just precommit` and fix any issues it reports.
- Default CLI builds include all sync features (`dav-sync`, `email-sync`, `telegram-sync`) so imports work out of the box; use `--no-default-features` for a no-sync build.

## Coding Style & Naming Conventions
- Language: Rust. Prefer `rustfmt` defaults (4-space indentation; wrap by formatter).
- Crates follow the `knotter-*` naming pattern; modules and files use `snake_case`.
- Keep UI crates (`knotter-cli`, `knotter-tui`) thin; domain rules live in `knotter-core`.
- Favor idiomatic Rust patterns (ownership-first APIs, explicit errors, minimal `unwrap`).
- Keep code files under 300 lines; split modules when they grow.
- Avoid unnecessary abstractions; prefer simple, readable implementations.
- Document public APIs and complex logic with clear comments/docstrings.

## Maintainability & Technical Debt
- Refactor incrementally to keep complexity down and avoid monolith modules.
- Prefer small, testable units over deep inheritance or generic layers.

## Testing Guidelines
- Use Rust’s built-in test harness (`#[test]`).
- Targeted coverage (per architecture spec):
  - `knotter-core`: tag normalization, due-state logic, filter parser.
  - `knotter-store`: migrations + CRUD + filter queries.
  - `knotter-sync`: vCard/ICS import-export round-trips.
- Run all tests with `cargo test`. No coverage threshold is defined yet.
- Add or update tests for new features and bug fixes.

## Commit & Pull Request Guidelines
- Git history is empty; no commit-message convention is established.
- Suggested commit style: short, imperative summary that describes intent (e.g., “Add store migrations”).
- PRs should include: concise summary, testing performed, and notes on schema or data-model changes. Link related issues if applicable.
- When bumping versions and tagging a release, update `Formula/knotter.rb` to match the new tag and SHA before pushing.

## Documentation Updates
- Review docs after any behavior or API change and update relevant files in `docs/` and `README.md`.
- Keep `docs/ARCHITECTURE.md`, `docs/DB_SCHEMA.md`, and `docs/cli-output.md` aligned with the codebase.

## Work Log & Context
- Maintain a lightweight development log (e.g., `docs/LOG.md` or similar) and keep it current.
- Include a timestamp on each entry; keep the log out of version control.
- Before starting work, read the log to restore context; update it after meaningful changes or decisions.
- Use this template so decisions and progress are traceable:

```text
Timestamp:
Goal (incl. success criteria):
Constraints/Assumptions:
Key decisions:
State:
Done:
Now:
Next:
Open questions (UNCONFIRMED if needed):
Working set (files/ids/commands):
```

## Security & Configuration Tips
- The app stores personal contact data locally; avoid logging sensitive fields by default.
- Data paths follow XDG conventions (see `docs/ARCHITECTURE.md`).

## Dependency Hygiene
- Keep dependencies up to date and remove unused ones.
- Prefer fewer, well-supported crates over bespoke helpers unless necessary.
