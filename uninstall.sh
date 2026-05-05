#!/usr/bin/env sh
set -eu

repo_url="${GIT_WARP_REPO_URL:-https://github.com/denysbutenko/git-warp}"

if [ -n "${GIT_WARP_INSTALL_DIR:-}" ]; then
  install_dir="$GIT_WARP_INSTALL_DIR"
elif [ -n "${GIT_WARP_INSTALL_ROOT:-}" ]; then
  install_dir="${GIT_WARP_INSTALL_ROOT}/bin"
else
  install_dir="${HOME}/.local/bin"
fi

dry_run=0
for arg in "$@"; do
  case "$arg" in
    --dry-run|-n) dry_run=1 ;;
    -h|--help)
      cat <<EOF
Usage: uninstall.sh [--dry-run]

Removes the Git-Warp binary at \${GIT_WARP_INSTALL_DIR:-\$HOME/.local/bin}/warp.
Other detected installs (Cargo, Homebrew prefixes) are listed but not touched.

Environment:
  GIT_WARP_INSTALL_DIR   Override the install directory (default: \$HOME/.local/bin)
  GIT_WARP_INSTALL_ROOT  Use \$GIT_WARP_INSTALL_ROOT/bin as the install directory
EOF
      exit 0
      ;;
    *)
      echo "error: unknown argument: $arg" >&2
      exit 1
      ;;
  esac
done

target="${install_dir}/warp"

candidate_install_dirs() {
  printf '%s\n' \
    "${install_dir}" \
    "${HOME}/.local/bin" \
    "${HOME}/.cargo/bin" \
    "/usr/local/bin" \
    "/opt/homebrew/bin"
}

list_other_installs() {
  printed=""
  candidate_install_dirs | awk 'NF && !seen[$0]++' | while IFS= read -r dir; do
    binary="${dir}/warp"
    [ "$binary" = "$target" ] && continue
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

if [ ! -e "$target" ]; then
  echo "No Git-Warp binary found at ${target}; nothing to remove."
else
  if [ "$dry_run" -eq 1 ]; then
    echo "Would remove ${target}"
  else
    rm -f "$target" || {
      echo "error: failed to remove ${target}" >&2
      exit 1
    }
    echo "Removed ${target}"
  fi
fi

others="$(list_other_installs)"
if [ -n "$others" ]; then
  echo
  echo "Other Git-Warp installs detected (not removed):"
  printf '%s\n' "$others"
  echo "Remove them manually, or with the matching tool:"
  echo "  Cargo install:    cargo uninstall git-warp"
  echo "  Other directory:  rm <path>"
fi

active="$(command -v warp 2>/dev/null || true)"
if [ -n "$active" ]; then
  echo
  echo "'warp' is still on PATH at ${active}."
  echo "Remove it or adjust PATH if you intended a complete uninstall."
fi
