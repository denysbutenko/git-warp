#!/usr/bin/env sh
set -eu

repo_url="${GIT_WARP_REPO_URL:-https://github.com/denysbutenko/git-warp}"
version="${GIT_WARP_VERSION:-v0.2.0}"
install_root="${GIT_WARP_INSTALL_ROOT:-}"

if ! command -v cargo >/dev/null 2>&1; then
  echo "Rust/Cargo is required to install Git-Warp."
  echo "Install Rust from https://rustup.rs, then rerun this script."
  exit 1
fi

echo "Installing Git-Warp ${version} from ${repo_url}"

set -- cargo install --locked --force --git "$repo_url" --tag "$version" --bin warp

if [ -n "$install_root" ]; then
  set -- "$@" --root "$install_root"
fi

set -- "$@" git-warp

"$@"

echo

if [ -n "$install_root" ]; then
  "$install_root/bin/warp" --version
elif command -v warp >/dev/null 2>&1; then
  warp --version
else
  echo "Git-Warp installed, but 'warp' is not on PATH yet."
fi

echo "Run 'warp doctor' to check your setup."
