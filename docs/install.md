# Install Git-Warp

## Quick Install

Install Git-Warp with one command. Rust and Cargo are not required.

```bash
curl -fsSL https://raw.githubusercontent.com/denysbutenko/git-warp/main/install.sh | sh
```

Then check that your shell can find `warp`:

```bash
warp --version
warp doctor
```

The installer downloads a prebuilt release archive for your platform and places
the `warp` binary in `~/.local/bin`.

## PATH Setup

If `warp --version` is not found after installation, add `~/.local/bin` to your
shell path:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

For Zsh, make that permanent with:

```bash
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.zshrc
```

Open a new terminal, then run:

```bash
warp --version
```

## Custom Install Location

Install into another writable directory with `GIT_WARP_INSTALL_DIR`:

```bash
curl -fsSL https://raw.githubusercontent.com/denysbutenko/git-warp/main/install.sh | GIT_WARP_INSTALL_DIR=/usr/local/bin sh
```

## Install A Specific Version

The installer defaults to the latest documented release. Pin a version with
`GIT_WARP_VERSION`:

```bash
curl -fsSL https://raw.githubusercontent.com/denysbutenko/git-warp/main/install.sh | GIT_WARP_VERSION=v0.3.0 sh
```

## Supported Prebuilt Binaries

Release binaries are published for:

- macOS Apple Silicon: `aarch64-apple-darwin`
- macOS Intel: `x86_64-apple-darwin`
- Linux arm64: `aarch64-unknown-linux-gnu`
- Linux x64: `x86_64-unknown-linux-gnu`

## Upgrade

Re-running the installer overwrites the binary at the same install directory:

```bash
curl -fsSL https://raw.githubusercontent.com/denysbutenko/git-warp/main/install.sh | sh
```

The installer prints any other `warp` binaries it sees (Cargo, Homebrew prefixes,
custom directories). After the upgrade it warns when `warp` on `PATH` resolves
to a different binary than the one just installed, and prints the fix:

- Reorder `PATH` so the install directory comes first, or
- Remove the older binary at the path the warning prints.

Run `warp doctor` to confirm only one `warp` binary is active. The Install check
lists every `warp` it finds in `PATH` and known install directories, marks the
active one, and warns when more than one exists.

## Uninstall

Remove the default install with the bundled uninstaller:

```bash
curl -fsSL https://raw.githubusercontent.com/denysbutenko/git-warp/main/uninstall.sh | sh
```

The uninstaller removes `~/.local/bin/warp` (or `$GIT_WARP_INSTALL_DIR/warp`),
lists any other detected `warp` installs without touching them, and warns if
`warp` is still on `PATH` after the removal. Use `--dry-run` to preview the
removal without changing anything.

If you installed Git-Warp with Cargo, also run:

```bash
cargo uninstall git-warp
```

## Cargo Fallback

Use Cargo only if you want to build during installation:

```bash
curl -fsSL https://raw.githubusercontent.com/denysbutenko/git-warp/main/install.sh | GIT_WARP_INSTALL_METHOD=cargo sh
```

## Build From Source

Use this path when contributing or testing local changes:

```bash
git clone https://github.com/denysbutenko/git-warp
cd git-warp
cargo build --release
cargo install --path .
warp --version
```
