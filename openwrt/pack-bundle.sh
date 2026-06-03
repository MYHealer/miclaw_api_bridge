#!/usr/bin/env bash
# Assemble a deployable OpenWrt bundle (binary + init + config + LuCI app +
# installer) into a single tar.gz. Reused by build-openwrt.sh and CI.
#
# Usage:
#   openwrt/pack-bundle.sh <path-to-binary> [output.tar.gz]
#
# Default output: openwrt/out/miclaw_api_bridge_openwrt_aarch64.tar.gz
set -euo pipefail

BIN="${1:?usage: pack-bundle.sh <binary> [output.tar.gz]}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OWRT="$ROOT/openwrt"
OUT="${2:-$OWRT/out/miclaw_api_bridge_openwrt_aarch64.tar.gz}"

test -f "$BIN" || { echo "!! binary not found: $BIN" >&2; exit 1; }

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
PKG="$STAGE/miclaw_api_bridge_openwrt"
mkdir -p "$PKG"

# binary
cp -f "$BIN" "$PKG/miclaw_api_bridge"
chmod +x "$PKG/miclaw_api_bridge"

# init + config tree
mkdir -p "$PKG/files"
cp -R "$OWRT/files/etc" "$PKG/files/etc"

# LuCI app tree
mkdir -p "$PKG/luci"
cp -R "$OWRT/luci/htdocs" "$PKG/luci/htdocs"
cp -R "$OWRT/luci/root"   "$PKG/luci/root"

# installer scripts + docs
cp -f "$OWRT/install.sh"   "$PKG/install.sh"
cp -f "$OWRT/uninstall.sh" "$PKG/uninstall.sh"
cp -f "$OWRT/README.md"    "$PKG/README.md"
chmod +x "$PKG/install.sh" "$PKG/uninstall.sh" \
	"$PKG/files/etc/init.d/miclaw_api_bridge"

mkdir -p "$(dirname "$OUT")"
tar -czf "$OUT" -C "$STAGE" miclaw_api_bridge_openwrt

echo "==> bundle: $OUT"
ls -lh "$OUT"
