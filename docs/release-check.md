# Release Check

Run the release check before tagging a Git-Warp release.

```bash
warp release-check --version v0.3.0
```

The command validates release metadata first, then runs the release verification
commands. It stops at the first failing command.

## Metadata Checks

`warp release-check --version v0.3.0` verifies that:

- `Cargo.toml` has package version `0.3.0`.
- `CHANGELOG.md` has a `v0.3.0` release heading.
- `docs/releases/v0.3.0.md` exists.
- `docs/install.md` mentions `GIT_WARP_VERSION=v0.3.0`.
- `install.sh` defaults `GIT_WARP_VERSION` to `v0.3.0`.

Use the fast metadata-only mode while preparing notes:

```bash
warp release-check --metadata-only --version v0.3.0
```

## Full Verification

After metadata passes, the full command runs:

```bash
cargo fmt --check
cargo test
cargo build --release --bin warp
./target/release/warp --version
./target/release/warp --help
./target/release/warp switch --help
./target/release/warp cleanup --help
./target/release/warp doctor
```

For source builds, this covers the contributor path. For public installs, the
metadata checks cover the install script default, pinned install docs, release
notes, and changelog before the tag is created.
