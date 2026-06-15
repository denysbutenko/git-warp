#!/usr/bin/env sh
set -eu

repo_url="${GIT_WARP_REPO_URL:-https://github.com/denysbutenko/git-warp}"
method="${GIT_WARP_INSTALL_METHOD:-binary}"

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
  - Windows x64: x86_64-pc-windows-msvc (install with install.ps1)
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

candidate_install_dirs() {
  printf '%s\n' \
    "${install_dir}" \
    "${HOME}/.local/bin" \
    "${HOME}/.cargo/bin" \
    "/usr/local/bin" \
    "/opt/homebrew/bin"
}

list_existing_warp_binaries() {
  printed=""
  candidate_install_dirs | awk 'NF && !seen[$0]++' | while IFS= read -r dir; do
    binary="${dir}/warp"
    [ -x "$binary" ] || continue
    case ":$printed:" in
      *":$binary:"*) continue ;;
    esac
    printed="${printed}:${binary}"
    version_line="$("$binary" --version 2>/dev/null | head -n1)"
    if [ -n "$version_line" ]; then
      printf '  - %s (%s)\n' "$binary" "$version_line"
    else
      printf '  - %s\n' "$binary"
    fi
  done
}

report_existing_installs() {
  existing="$(list_existing_warp_binaries)"
  if [ -n "$existing" ]; then
    echo "Existing Git-Warp installs detected:"
    printf '%s\n' "$existing"
    echo "The installer will replace ${install_dir}/warp; other locations are left as-is."
    echo "Run 'curl -fsSL ${repo_url}/raw/main/uninstall.sh | sh' to remove the default install,"
    echo "or 'cargo uninstall git-warp' to remove a Cargo install."
    echo
  fi
}

report_active_warp_shadowing() {
  active="$(command -v warp 2>/dev/null || true)"
  installed="${install_dir}/warp"
  [ -n "$active" ] || return 0
  [ "$active" = "$installed" ] && return 0
  echo
  echo "Note: 'warp' on PATH resolves to ${active}, not the binary just installed at ${installed}."
  echo "Reorder PATH to put ${install_dir} first, or remove the older binary at ${active}."
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

fail_version_lookup() {
  echo "error: $*" >&2
  echo "Pin a version explicitly to skip the GitHub API lookup:" >&2
  echo "  curl -fsSL ${repo_url}/raw/main/install.sh | GIT_WARP_VERSION=vX.Y.Z sh" >&2
  cargo_fallback_hint
  exit 1
}

resolve_latest_version() {
  case "$repo_url" in
    https://github.com/*) ;;
    *) fail_version_lookup "GIT_WARP_REPO_URL ($repo_url) is not on github.com; cannot auto-resolve the latest release" ;;
  esac

  api_url="https://api.github.com/repos/${repo_url#https://github.com/}/releases/latest"

  if command -v curl >/dev/null 2>&1; then
    body="$(curl -fsSL "$api_url" 2>/dev/null)" || fail_version_lookup "GitHub release lookup failed at $api_url"
  elif command -v wget >/dev/null 2>&1; then
    body="$(wget -qO- "$api_url" 2>/dev/null)" || fail_version_lookup "GitHub release lookup failed at $api_url"
  else
    fail_version_lookup "curl or wget is required to resolve the latest Git-Warp release"
  fi

  tag="$(printf '%s' "$body" | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n1)"

  if [ -z "$tag" ]; then
    fail_version_lookup "could not parse tag_name from $api_url response"
  fi

  printf '%s\n' "$tag"
}

if [ -n "${GIT_WARP_VERSION:-}" ]; then
  version="$GIT_WARP_VERSION"
else
  version="$(resolve_latest_version)"
  echo "Resolved latest Git-Warp release: ${version}"
fi

download_base="${GIT_WARP_DOWNLOAD_BASE:-${repo_url}/releases/download/${version}}"

verify_checksum() {
  dir="$1"
  archive_name="$2"

  if [ "${GIT_WARP_SKIP_CHECKSUM:-0}" = "1" ]; then
    echo "Skipping checksum verification (GIT_WARP_SKIP_CHECKSUM=1)"
    return 0
  fi

  if command -v shasum >/dev/null 2>&1; then
    checker="shasum -a 256 -c"
    digester="shasum -a 256"
  elif command -v sha256sum >/dev/null 2>&1; then
    checker="sha256sum -c"
    digester="sha256sum"
  else
    echo "error: neither shasum nor sha256sum is available to verify ${archive_name}" >&2
    cargo_fallback_hint
    exit 1
  fi

  if ( cd "$dir" && $checker "${archive_name}.sha256" >/dev/null 2>&1 ); then
    return 0
  fi

  expected="$(awk '{print $1; exit}' "${dir}/${archive_name}.sha256" 2>/dev/null)"
  actual="$( ( cd "$dir" && $digester "$archive_name" ) | awk '{print $1; exit}')"
  echo "error: checksum verification failed for ${archive_name}" >&2
  echo "  expected: ${expected:-<missing>}" >&2
  echo "  actual:   ${actual:-<unknown>}" >&2
  cargo_fallback_hint
  exit 1
}

target_triple() {
  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os" in
    Darwin) os_part="apple-darwin" ;;
    Linux) os_part="unknown-linux-gnu" ;;
    MINGW*|MSYS*|CYGWIN*)
      echo "error: install.sh does not run on Windows; use install.ps1 instead:" >&2
      echo "  irm ${repo_url}/raw/main/install.ps1 | iex" >&2
      cargo_fallback_hint
      exit 1
      ;;
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

  if [ "${GIT_WARP_SKIP_CHECKSUM:-0}" != "1" ]; then
    download "${url}.sha256" "${archive}.sha256"
  fi
  verify_checksum "$tmp_dir" "$asset"

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

report_existing_installs

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

report_active_warp_shadowing

echo "Run 'warp doctor' to check your setup."
