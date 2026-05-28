#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET="${MICLAW_WINDOWS_TARGET:-x86_64-pc-windows-msvc}"
XWIN_CACHE_DIR="${XWIN_CACHE_DIR:-$ROOT/.cache/xwin}"
export XWIN_CACHE_DIR

if ! command -v cargo-xwin >/dev/null 2>&1; then
  echo "cargo-xwin is required. Install it with:" >&2
  echo "  cargo install --locked cargo-xwin" >&2
  exit 1
fi

if ! command -v makensis >/dev/null 2>&1; then
  echo "NSIS is required for cross-compiled Windows installers." >&2
  echo "On macOS, install it with:" >&2
  echo "  brew install nsis" >&2
  exit 1
fi

if [[ "$(uname -s)" == "Darwin" ]] && ! command -v llvm-rc >/dev/null 2>&1; then
  LLVM_BIN="$(brew --prefix llvm 2>/dev/null)/bin"
  if [[ -x "$LLVM_BIN/llvm-rc" ]]; then
    export PATH="$LLVM_BIN:$PATH"
  else
    echo "llvm-rc is required. On macOS, install it with:" >&2
    echo "  brew install llvm" >&2
    echo "Then ensure llvm's bin directory is on PATH." >&2
    exit 1
  fi
fi

rustup target add "$TARGET"
cd "$ROOT"
pnpm install --frozen-lockfile
pnpm tauri build --runner cargo-xwin --target "$TARGET" --bundles nsis

echo "Windows NSIS output: $ROOT/src-tauri/target/$TARGET/release/bundle/nsis"
