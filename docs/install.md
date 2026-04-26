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
curl -fsSL https://raw.githubusercontent.com/denysbutenko/git-warp/main/install.sh | GIT_WARP_VERSION=v0.2.0 sh
```

## Supported Prebuilt Binaries

Release binaries are published for:

- macOS Apple Silicon: `aarch64-apple-darwin`
- macOS Intel: `x86_64-apple-darwin`
- Linux arm64: `aarch64-unknown-linux-gnu`
- Linux x64: `x86_64-unknown-linux-gnu`

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
