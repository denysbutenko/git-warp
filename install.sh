#!/usr/bin/env sh
set -eu

repo_url="${GIT_WARP_REPO_URL:-https://github.com/denysbutenko/git-warp}"
version="${GIT_WARP_VERSION:-v0.2.0}"
method="${GIT_WARP_INSTALL_METHOD:-binary}"
download_base="${GIT_WARP_DOWNLOAD_BASE:-${repo_url}/releases/download/${version}}"

if [ -n "${GIT_WARP_INSTALL_DIR:-}" ]; then
  install_dir="$GIT_WARP_INSTALL_DIR"
elif [ -n "${GIT_WARP_INSTALL_ROOT:-}" ]; then
  install_dir="${GIT_WARP_INSTALL_ROOT}/bin"
else
  install_dir="${HOME}/.local/bin"
fi

fail() {
  echo "error: $*" >&2
  exit 1
}

supported_targets() {
  cat >&2 <<'EOF'
Supported prebuilt targets:
  - macOS Apple Silicon: aarch64-apple-darwin
  - macOS Intel: x86_64-apple-darwin
  - Linux arm64: aarch64-unknown-linux-gnu
  - Linux x64: x86_64-unknown-linux-gnu
EOF
}

cargo_fallback_hint() {
  cat >&2 <<EOF
If you have Rust and Cargo installed, retry with:
  curl -fsSL ${repo_url}/raw/main/install.sh | GIT_WARP_INSTALL_METHOD=cargo sh
EOF
}

fail_unsupported_target() {
  echo "error: unsupported $1: $2" >&2
  supported_targets
  cargo_fallback_hint
  exit 1
}

fail_download() {
  echo "error: failed to download $1" >&2
  echo "The release asset may not exist yet for this platform or version." >&2
  cargo_fallback_hint
  exit 1
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || fail "$1 is required for Git-Warp installation"
}

download() {
  url="$1"
  output="$2"

  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$url" -o "$output" || fail_download "$url"
  elif command -v wget >/dev/null 2>&1; then
    wget -q "$url" -O "$output" || fail_download "$url"
  else
    echo "error: curl or wget is required to download Git-Warp release assets" >&2
    cargo_fallback_hint
    exit 1
  fi
}

target_triple() {
  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os" in
    Darwin) os_part="apple-darwin" ;;
    Linux) os_part="unknown-linux-gnu" ;;
    *) fail_unsupported_target "operating system" "$os" ;;
  esac

  case "$arch" in
    arm64 | aarch64) arch_part="aarch64" ;;
    x86_64 | amd64) arch_part="x86_64" ;;
    *) fail_unsupported_target "CPU architecture" "$arch" ;;
  esac

  printf '%s-%s\n' "$arch_part" "$os_part"
}

install_from_binary() {
  need_cmd uname
  need_cmd tar

  target="$(target_triple)"
  asset="git-warp-${version}-${target}.tar.gz"
  url="${download_base}/${asset}"
  tmp_dir="$(mktemp -d 2>/dev/null || mktemp -d -t git-warp-install)"
  archive="${tmp_dir}/${asset}"

  trap 'rm -rf "$tmp_dir"' EXIT HUP INT TERM

  echo "Downloading Git-Warp ${version} for ${target}"
  download "$url" "$archive"

  tar -xzf "$archive" -C "$tmp_dir" || fail "failed to extract ${archive}; the download may be incomplete or corrupt"

  if [ ! -f "${tmp_dir}/warp" ]; then
    echo "error: release archive did not contain a warp binary" >&2
    echo "Check that ${asset} is the expected Git-Warp release asset." >&2
    cargo_fallback_hint
    exit 1
  fi

  mkdir -p "$install_dir"
  cp "${tmp_dir}/warp" "${install_dir}/warp"
  chmod 755 "${install_dir}/warp"
}

install_from_cargo() {
  need_cmd cargo

  echo "Installing Git-Warp ${version} from ${repo_url} with Cargo"

  set -- cargo install --locked --force --git "$repo_url" --tag "$version" --bin warp

  if [ -n "${GIT_WARP_INSTALL_ROOT:-}" ]; then
    set -- "$@" --root "$GIT_WARP_INSTALL_ROOT"
  fi

  set -- "$@" git-warp

  "$@" || {
    echo "error: Cargo install failed for Git-Warp ${version}." >&2
    echo "Check the Cargo output above, then retry after fixing the reported build or network issue." >&2
    exit 1
  }
}

case "$method" in
  binary) install_from_binary ;;
  cargo) install_from_cargo ;;
  *) fail "unsupported install method: ${method}; use 'binary' or 'cargo'" ;;
esac

echo

if [ -x "${install_dir}/warp" ]; then
  "${install_dir}/warp" --version
elif command -v warp >/dev/null 2>&1; then
  warp --version
else
  echo "Git-Warp installed, but 'warp' is not on PATH yet."
fi

case ":${PATH}:" in
  *":${install_dir}:"*) ;;
  *)
    echo "Add ${install_dir} to PATH so your shell can find 'warp':"
    echo "  export PATH=\"${install_dir}:\$PATH\""
    echo "Open a new terminal or run 'warp doctor' after updating PATH."
    ;;
esac

echo "Run 'warp doctor' to check your setup."
