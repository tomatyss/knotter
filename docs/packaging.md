# Packaging

Releases publish macOS/Linux tarballs plus Linux .deb packages. These notes
describe local builds for macOS Homebrew (single-repo tap), Debian/Ubuntu, and
generic Unix systems.

## Homebrew (macOS, single-repo tap)

The formula lives at `Formula/knotter.rb`. Install from this repo:

```
brew tap tomatyss/knotter https://github.com/tomatyss/knotter
brew install tomatyss/knotter/knotter
```

Formula files must point at a tagged release tarball and include a SHA256.

## Release tagging and assets

Releases are tag-driven. Push a tag like `v0.1.0` to trigger the release
workflow, which builds multi-arch Linux (gnu + musl) and macOS tarballs, Linux
.deb packages for x86_64, and publishes checksums to GitHub Releases.

## Debian/Ubuntu (.deb)

We rely on `cargo-deb` for local package builds.

Install the tool (one-time):

```
cargo install cargo-deb
```

Build packages:

```
cargo deb -p knotter-cli
cargo deb -p knotter-tui
```

Packages are written under `target/debian/`. Install them with:

```
sudo dpkg -i target/debian/knotter-cli_*.deb
sudo dpkg -i target/debian/knotter-tui_*.deb
```

## Generic Unix install

Build release binaries and copy them into your PATH:

```
cargo build --release -p knotter-cli -p knotter-tui
install -m 755 target/release/knotter /usr/local/bin/knotter
install -m 755 target/release/knotter-tui /usr/local/bin/knotter-tui
```

## Linux musl (static) local build

For a static binary suitable for minimal distros or containers, use musl with a
cross build tool like `cross`:

```
cross build --release -p knotter-cli -p knotter-tui --target x86_64-unknown-linux-musl
```
