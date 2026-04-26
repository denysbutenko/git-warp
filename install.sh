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

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || fail "$1 is required"
}

download() {
  url="$1"
  output="$2"

  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$url" -o "$output"
  elif command -v wget >/dev/null 2>&1; then
    wget -q "$url" -O "$output"
  else
    fail "curl or wget is required to download Git-Warp"
  fi
}

target_triple() {
  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os" in
    Darwin) os_part="apple-darwin" ;;
    Linux) os_part="unknown-linux-gnu" ;;
    *) fail "unsupported operating system: $os" ;;
  esac

  case "$arch" in
    arm64 | aarch64) arch_part="aarch64" ;;
    x86_64 | amd64) arch_part="x86_64" ;;
    *) fail "unsupported CPU architecture: $arch" ;;
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

  tar -xzf "$archive" -C "$tmp_dir"

  if [ ! -f "${tmp_dir}/warp" ]; then
    fail "release archive did not contain a warp binary"
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

  "$@"
}

case "$method" in
  binary) install_from_binary ;;
  cargo) install_from_cargo ;;
  *) fail "unsupported install method: $method" ;;
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
  *) echo "Add ${install_dir} to PATH if your shell cannot find 'warp'." ;;
esac

echo "Run 'warp doctor' to check your setup."
