#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IMAGE="${MICLAW_LINUX_IMAGE:-miclaw-api-bridge-linux-appimage:ubuntu22}"
OUT_DIR="${MICLAW_OUT_DIR:-$ROOT/target-local/linux-appimage}"
CACHE_DIR="${MICLAW_CACHE_DIR:-$ROOT/.cache/local-build}"

if ! command -v docker >/dev/null 2>&1; then
  echo "docker is required for local Linux AppImage builds." >&2
  exit 1
fi

if ! docker info >/dev/null 2>&1; then
  echo "docker is installed, but the Docker daemon is not running." >&2
  echo "Start Docker Desktop, OrbStack, Colima, or another Docker backend, then retry." >&2
  exit 1
fi

mkdir -p "$OUT_DIR" "$CACHE_DIR/cargo-registry" "$CACHE_DIR/cargo-git" "$CACHE_DIR/pnpm-store"

docker build -f "$ROOT/docker/linux-appimage.Dockerfile" -t "$IMAGE" "$ROOT"

docker run --rm -t \
  -v "$ROOT:/src:ro" \
  -v "$OUT_DIR:/out" \
  -v "$CACHE_DIR/cargo-registry:/root/.cargo/registry" \
  -v "$CACHE_DIR/cargo-git:/root/.cargo/git" \
  -v "$CACHE_DIR/pnpm-store:/root/.local/share/pnpm/store" \
  "$IMAGE" \
  bash -lc '
    set -euo pipefail
    rsync -a --delete \
      --exclude .cache \
      --exclude .git \
      --exclude dist \
      --exclude node_modules \
      --exclude src-tauri/target \
      --exclude target-local \
      /src/ /work/
    cd /work
    pnpm config set store-dir /root/.local/share/pnpm/store
    pnpm install --frozen-lockfile
    pnpm tauri build --bundles appimage
    mkdir -p /out
    find src-tauri/target/release/bundle/appimage -maxdepth 1 -type f -name "*.AppImage" -print -exec cp -f {} /out/ \;
  '

echo "Linux AppImage output: $OUT_DIR"
