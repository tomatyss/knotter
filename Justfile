# Common developer commands for knotter.

set shell := ["zsh", "-cu"]

@default:
	just --list

# Format code in-place.
format:
	cargo fmt

# Check formatting without writing changes.
format-check:
	cargo fmt --check

# Run clippy lints for all targets and features.
lint:
	cargo clippy --all-targets --all-features -- -D warnings

# Run the test suite.
test:
	cargo test

# Build the workspace.
build:
	cargo build

# Local pre-commit gate.
precommit: format-check lint test
