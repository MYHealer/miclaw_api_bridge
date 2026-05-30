#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_ARGS=()

if [ -n "${MICLAW_RUST_TARGET:-}" ]; then
  rustup target add "$MICLAW_RUST_TARGET"
  TARGET_ARGS=(--target "$MICLAW_RUST_TARGET")
fi

if [ -n "${MICLAW_OUT_DIR:-}" ]; then
  OUT_DIR="$MICLAW_OUT_DIR"
elif [ -n "${MICLAW_RUST_TARGET:-}" ]; then
  OUT_DIR="$ROOT/target-local/binaries/$MICLAW_RUST_TARGET"
else
  OUT_DIR="$ROOT/target-local/binaries"
fi

mkdir -p "$OUT_DIR"
cd "$ROOT"
pnpm build

cd "$ROOT/src-tauri"
if [ "${#TARGET_ARGS[@]}" -gt 0 ]; then
  cargo build --release "${TARGET_ARGS[@]}" --bin miclaw_api_bridge
  cargo build --release "${TARGET_ARGS[@]}" --features desktop --bin miclaw_api_bridge_desktop
else
  cargo build --release --bin miclaw_api_bridge
  cargo build --release --features desktop --bin miclaw_api_bridge_desktop
fi

target_dir="$ROOT/src-tauri/target"
if [ -n "${MICLAW_RUST_TARGET:-}" ]; then
  target_dir="$target_dir/$MICLAW_RUST_TARGET"
fi
target_dir="$target_dir/release"

server_name="miclaw_api_bridge"
desktop_name="miclaw_api_bridge_desktop"
if [[ "${MICLAW_RUST_TARGET:-}" == *windows* ]]; then
  server_name="$server_name.exe"
  desktop_name="$desktop_name.exe"
fi

cp -f "$target_dir/$server_name" "$OUT_DIR/"
cp -f "$target_dir/$desktop_name" "$OUT_DIR/"

echo "Binary output: $OUT_DIR"
