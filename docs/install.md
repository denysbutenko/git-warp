# Install Git-Warp

## Quick Install

Install Git-Warp with one command. Rust and Cargo are not required.

### macOS / Linux

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

### Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/denysbutenko/git-warp/main/install.ps1 | iex
```

Then verify the binary is reachable:

```powershell
warp --version
warp doctor
```

The PowerShell installer downloads the `x86_64-pc-windows-msvc` `.zip`, verifies
its SHA256, and places `warp.exe` in `%LOCALAPPDATA%\Programs\git-warp\bin`. You
may need to open a new PowerShell session for the PATH update to take effect.

The Bash installer (`install.sh`) detects MSYS / Git Bash / Cygwin shells and
points you back at `install.ps1`; there is no Bash-on-Windows install path.

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

On Windows, pass `-InstallDir` (or set `$env:GIT_WARP_INSTALL_DIR` before
piping):

```powershell
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/denysbutenko/git-warp/main/install.ps1))) -InstallDir 'C:\tools\git-warp'
```

## Checksum Verification

`install.sh` downloads the release archive's companion `.sha256` file from the
same release and verifies the digest with `shasum -a 256 -c` (falling back to
`sha256sum -c`) before extracting. A mismatch aborts the install with both the
expected and actual digests printed on stderr.

If you install from a private mirror that does not publish digests, opt out
with:

```bash
curl -fsSL https://raw.githubusercontent.com/denysbutenko/git-warp/main/install.sh | GIT_WARP_SKIP_CHECKSUM=1 sh
```

## Install A Specific Version

With no env override, the installer resolves the latest published release from
the GitHub releases API
(`https://api.github.com/repos/denysbutenko/git-warp/releases/latest`) and
installs that tag. The resolved tag is printed before the download starts.

Pin a specific version with `GIT_WARP_VERSION` to skip the API lookup:

```bash
curl -fsSL https://raw.githubusercontent.com/denysbutenko/git-warp/main/install.sh | GIT_WARP_VERSION=v0.3.0 sh
```

If the API lookup fails (no network, rate-limited, custom non-`github.com`
`GIT_WARP_REPO_URL`), the installer aborts and points at the `GIT_WARP_VERSION`
pin or the Cargo fallback.

## Copy-on-Write (CoW) Support

Git-Warp uses Copy-on-Write (CoW) to create worktrees almost instantaneously. This
feature is supported on:

- **macOS**: APFS filesystems.
- **Linux**: Filesystems that implement the FICLONE ioctl (btrfs, xfs with `reflink=1`, bcachefs, OCFS2, etc.).

Git-Warp automatically detects CoW support. If your filesystem does not support
it, Git-Warp falls back to traditional Git worktree creation. Run `warp doctor`
to check CoW status for your repository.

## Supported Prebuilt Binaries

Release binaries are published for:

- macOS Apple Silicon: `aarch64-apple-darwin`
- macOS Intel: `x86_64-apple-darwin`
- Linux arm64: `aarch64-unknown-linux-gnu`
- Linux x64: `x86_64-unknown-linux-gnu`
- Windows x64: `x86_64-pc-windows-msvc`

## Windows Manual Download

If you prefer to bypass `install.ps1`, grab the archive yourself:

```powershell
$tag = 'v0.3.0'
$asset = "git-warp-$tag-x86_64-pc-windows-msvc.zip"
$base = "https://github.com/denysbutenko/git-warp/releases/download/$tag"
irm "$base/$asset" -OutFile $asset
irm "$base/$asset.sha256" -OutFile "$asset.sha256"
$expected = (Get-Content "$asset.sha256" -TotalCount 1).Trim().Split(' ')[0]
$actual = (Get-FileHash $asset -Algorithm SHA256).Hash.ToLowerInvariant()
if ($expected -ne $actual) { throw "SHA256 mismatch" }
Unblock-File $asset
Expand-Archive -Force $asset -DestinationPath "$env:LOCALAPPDATA\Programs\git-warp\bin"
```

Then add `%LOCALAPPDATA%\Programs\git-warp\bin` to your `PATH`.

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

### macOS / Linux

Remove the default install with the bundled uninstaller:

```bash
curl -fsSL https://raw.githubusercontent.com/denysbutenko/git-warp/main/uninstall.sh | sh
```

The uninstaller:
- Removes agent hook entries from Claude/Codex settings using the `warp` binary before its deletion (opt out with `GIT_WARP_KEEP_HOOKS=1`).
- Removes `~/.local/bin/warp` (or `$GIT_WARP_INSTALL_DIR/warp`).
- Scans `~/.bashrc`, `~/.zshrc`, and `~/.config/fish/config.fish` for leftover shell-config snippets and prints cleanup instructions if found.
- Lists any other detected `warp` installs without touching them.

Use `--dry-run` to preview the changes without modifying your system.

### Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/denysbutenko/git-warp/main/uninstall.ps1 | iex
```

The PowerShell uninstaller mirrors the Bash one:
- Removes agent hook entries from Claude/Codex settings using `warp.exe` before its deletion (opt out with `$env:GIT_WARP_KEEP_HOOKS = '1'`).
- Removes `%LOCALAPPDATA%\Programs\git-warp\bin\warp.exe` (or `$env:GIT_WARP_INSTALL_DIR\warp.exe`). Drops the install directory if it ends up empty.
- Scans the four `$PROFILE.*` PowerShell-profile paths for leftover `warp_cd` / `warp __complete` snippets and prints cleanup hints.
- Lists any other detected `warp.exe` installs (Cargo, custom directories) without touching them.

Preview the actions without modifying anything:

```powershell
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/denysbutenko/git-warp/main/uninstall.ps1))) -DryRun
```

### Cargo

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
