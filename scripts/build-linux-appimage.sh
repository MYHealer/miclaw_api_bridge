#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IMAGE="${MICLAW_LINUX_IMAGE:-miclaw-api-bridge-linux-appimage:ubuntu22}"
OUT_DIR="${MICLAW_OUT_DIR:-$ROOT/target-local/linux-appimage}"
CACHE_DIR="${MICLAW_CACHE_DIR:-$ROOT/.cache/local-build}"
DOCKER_PLATFORM="${MICLAW_DOCKER_PLATFORM:-linux/amd64}"
PLATFORM_CACHE="${DOCKER_PLATFORM//\//-}"
TARGET_CACHE_DIR="$CACHE_DIR/$PLATFORM_CACHE-target"
TAURI_CACHE_DIR="$CACHE_DIR/tauri-cache"

if ! command -v docker >/dev/null 2>&1; then
  echo "docker is required for local Linux AppImage builds." >&2
  exit 1
fi

if ! docker info >/dev/null 2>&1; then
  echo "docker is installed, but the Docker daemon is not running." >&2
  echo "Start Docker Desktop, OrbStack, Colima, or another Docker backend, then retry." >&2
  exit 1
fi

mkdir -p \
  "$OUT_DIR" \
  "$CACHE_DIR/cargo-registry" \
  "$CACHE_DIR/cargo-git" \
  "$CACHE_DIR/pnpm-store" \
  "$TARGET_CACHE_DIR" \
  "$TAURI_CACHE_DIR"
rm -f "$OUT_DIR"/*.AppImage

docker build --platform "$DOCKER_PLATFORM" -f "$ROOT/docker/linux-appimage.Dockerfile" -t "$IMAGE" "$ROOT"

docker run --rm -t \
  --platform "$DOCKER_PLATFORM" \
  -e APPIMAGE_EXTRACT_AND_RUN=1 \
  -v "$ROOT:/src:ro" \
  -v "$OUT_DIR:/out" \
  -v "$CACHE_DIR/cargo-registry:/root/.cargo/registry" \
  -v "$CACHE_DIR/cargo-git:/root/.cargo/git" \
  -v "$CACHE_DIR/pnpm-store:/root/.local/share/pnpm/store" \
  -v "$TAURI_CACHE_DIR:/root/.cache/tauri" \
  -v "$TARGET_CACHE_DIR:/work/src-tauri/target" \
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

    mkdir -p /root/.cache/tauri
    plugin_real=/root/.cache/tauri/linuxdeploy-plugin-appimage.real.AppImage
    plugin_wrapper=/root/.cache/tauri/linuxdeploy-plugin-appimage.AppImage
    if [ ! -f "$plugin_real" ]; then
      curl -fL --retry 5 --retry-delay 2 \
        https://github.com/linuxdeploy/linuxdeploy-plugin-appimage/releases/download/continuous/linuxdeploy-plugin-appimage-x86_64.AppImage \
        -o "$plugin_real"
      chmod +x "$plugin_real"
    fi
    cat > "$plugin_wrapper" <<'"'"'EOF'"'"'
#!/usr/bin/env bash
set -euo pipefail
export APPIMAGE_EXTRACT_AND_RUN=1
script_path="$(readlink -f "${BASH_SOURCE[0]}")"
real="$(dirname "$script_path")/linuxdeploy-plugin-appimage.real.AppImage"
if command -v qemu-x86_64 >/dev/null 2>&1; then
  exec qemu-x86_64 "$real" "$@"
fi
exec "$real" "$@"
EOF
    chmod +x "$plugin_wrapper"
    ln -sf "$plugin_wrapper" /root/.cache/tauri/linuxdeploy-plugin-appimage

    rm -rf src-tauri/target/release/bundle/appimage

    if ! pnpm tauri build --bundles appimage; then
      echo "Tauri AppImage bundling failed; trying local linuxdeploy fallback..." >&2
    fi

    appdir=src-tauri/target/release/bundle/appimage/miclaw_api_bridge.AppDir
    bundle_dir=src-tauri/target/release/bundle/appimage
    version="$(sed -n "s/.*\"version\"[[:space:]]*:[[:space:]]*\"\\([^\"]*\\)\".*/\\1/p" package.json | head -n 1)"

    if ! find "$bundle_dir" -maxdepth 1 -type f -name "*.AppImage" | grep -q .; then
      if [ ! -d "$appdir" ]; then
        echo "Tauri did not leave an AppDir to package: $appdir" >&2
        exit 1
      fi

      linuxdeploy_noop=/tmp/linuxdeploy-copy-libs
      cat > "$linuxdeploy_noop" <<'"'"'EOF'"'"'
#!/usr/bin/env bash
set -euo pipefail
appdir=
for arg in "$@"; do
  case "$arg" in
    --appdir=*) appdir="${arg#--appdir=}" ;;
    --appdir) shift; appdir="${1:-}" ;;
  esac
done
for arg in "$@"; do
  case "$arg" in
    --library=*)
      lib="${arg#--library=}"
      if [ -n "$appdir" ] && [ -e "$lib" ]; then
        mkdir -p "$appdir/$(dirname "$lib")"
        cp -a --parents "$lib" "$appdir/"
      fi
      ;;
  esac
done
EOF
      chmod +x "$linuxdeploy_noop"

      LINUXDEPLOY="$linuxdeploy_noop" /root/.cache/tauri/linuxdeploy-plugin-gtk.sh --appdir "$appdir"
      gtk_hook="$appdir/apprun-hooks/linuxdeploy-plugin-gtk.sh"
      if [ -f "$gtk_hook" ]; then
        sed -i \
          -e "s/export GDK_BACKEND=x11/export GDK_BACKEND=\"\${MICLAW_GDK_BACKEND:-wayland,x11}\"/" \
          "$gtk_hook"
        cat >> "$gtk_hook" <<'"'"'EOF'"'"'
export NO_AT_BRIDGE="${NO_AT_BRIDGE:-1}"
export WEBKIT_DISABLE_DMABUF_RENDERER="${WEBKIT_DISABLE_DMABUF_RENDERER:-1}"
export WEBKIT_DISABLE_COMPOSITING_MODE="${WEBKIT_DISABLE_COMPOSITING_MODE:-1}"
EOF
      fi

      collect_deps() {
        local appdir="$1"
        local idx=0
        local file dep dest
        mapfile -d "" queue < <(find "$appdir" -type f -print0)
        while [ "$idx" -lt "${#queue[@]}" ]; do
          file="${queue[$idx]}"
          idx=$((idx + 1))
          file "$file" | grep -q "ELF" || continue
          while IFS= read -r dep; do
            [ -n "$dep" ] || continue
            [ -e "$dep" ] || continue
            case "$(basename "$dep")" in
              ld-linux-*|libBrokenLocale.so*|libSegFault.so*|libanl.so*|libc.so*|libdl.so*|libgcc_s.so*|libm.so*|libmemusage.so*|libmvec.so*|libnsl.so*|libnss_*.so*|libpthread.so*|libresolv.so*|librt.so*|libstdc++.so*|libthread_db.so*|libutil.so*)
                continue
                ;;
              libEGL.so*|libGL.so*|libGLES*.so*|libGLX.so*|libGLdispatch.so*|libOpenGL.so*|libdrm.so*|libgbm.so*|libglapi.so*|libwayland-*.so*|libX11*.so*|libXau.so*|libxcb*.so*|libXcomposite.so*|libXcursor.so*|libXdamage.so*|libXdmcp.so*|libXext.so*|libXfixes.so*|libXi.so*|libXinerama.so*|libXrandr.so*|libXrender.so*|libxkbcommon.so*)
                continue
                ;;
            esac
            dest="$appdir/usr/lib/$(basename "$dep")"
            if [ ! -e "$dest" ]; then
              mkdir -p "$(dirname "$dest")"
              cp -L "$dep" "$dest"
              queue+=("$dest")
            fi
          done < <(ldd "$file" 2>/dev/null | sed -n "s/.*=> \\(\\/[^ ]*\\).*/\\1/p; s/^[[:space:]]*\\(\\/[^ ]*\\) .*/\\1/p")
        done
      }

      collect_deps "$appdir"
      if [ -d "$appdir/usr/lib/x86_64-linux-gnu/webkit2gtk-4.1" ]; then
        mkdir -p "$appdir/lib/x86_64-linux-gnu"
        cp -a "$appdir/usr/lib/x86_64-linux-gnu/webkit2gtk-4.1" "$appdir/lib/x86_64-linux-gnu/"
      fi
      find "$appdir/usr/lib" "$appdir/lib" -type f -name "libwebkit*" \
        -exec sed -i -e "s|/usr|././|g" {} \; 2>/dev/null || true

      rm -f "$bundle_dir"/*.AppImage
      (
        cd "$bundle_dir"
        ARCH=x86_64 LINUXDEPLOY_OUTPUT_VERSION="$version" \
          /root/.cache/tauri/linuxdeploy-plugin-appimage.AppImage --appdir "$(basename "$appdir")"
      )
      produced="$(find "$bundle_dir" -maxdepth 1 -type f -name "*.AppImage" | head -n 1)"
      if [ -z "$produced" ]; then
        echo "Fallback AppImage packaging did not produce an AppImage." >&2
        exit 1
      fi
      mv -f "$produced" "$bundle_dir/miclaw_api_bridge_${version}_amd64.AppImage"
    fi

    mkdir -p /out
    find src-tauri/target/release/bundle/appimage -maxdepth 1 -type f -name "*.AppImage" -print -exec cp -f {} /out/ \;
    tar -C src-tauri/target/release/bundle/appimage -czf "/out/miclaw_api_bridge_${version}_amd64.AppDir.tar.gz" miclaw_api_bridge.AppDir
  '

file "$OUT_DIR"/*.AppImage
cat > "$OUT_DIR/run-steamos-debug.sh" <<'EOF'
#!/usr/bin/env bash
set -u

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP="${1:-$HERE/miclaw_api_bridge_0.1.0_amd64.AppImage}"
LOG="$HERE/steamos-appimage-debug-$(date +%Y%m%d-%H%M%S).log"

{
  echo "== system =="
  date
  uname -a
  echo "shell: $SHELL"
  echo "pwd: $(pwd)"
  echo

  echo "== display env =="
  env | grep -E '^(DISPLAY|WAYLAND_DISPLAY|XDG_SESSION_TYPE|XDG_CURRENT_DESKTOP|DESKTOP_SESSION|DBUS_SESSION_BUS_ADDRESS|GDK_BACKEND|WEBKIT|NO_AT_BRIDGE)=' | sort || true
  echo

  echo "== appimage =="
  ls -l "$APP"
  file "$APP" || true
  chmod +x "$APP" 2>/dev/null || true
  ls -l /dev/fuse 2>/dev/null || echo "/dev/fuse missing"
  echo

  echo "== run appimage =="
  before="$(date +%s)"
  RUST_LOG="${RUST_LOG:-debug}" \
  WEBKIT_DISABLE_DMABUF_RENDERER="${WEBKIT_DISABLE_DMABUF_RENDERER:-1}" \
  WEBKIT_DISABLE_COMPOSITING_MODE="${WEBKIT_DISABLE_COMPOSITING_MODE:-1}" \
  NO_AT_BRIDGE="${NO_AT_BRIDGE:-1}" \
  MICLAW_GDK_BACKEND="${MICLAW_GDK_BACKEND:-wayland,x11}" \
    "$APP"
  status=$?
  echo "appimage exit status: $status"

  if [ "$status" -ne 0 ]; then
    echo
    echo "== extract and run fallback =="
    rm -rf "$HERE/squashfs-root"
    "$APP" --appimage-extract >/dev/null
    RUST_LOG="${RUST_LOG:-debug}" \
    WEBKIT_DISABLE_DMABUF_RENDERER="${WEBKIT_DISABLE_DMABUF_RENDERER:-1}" \
    WEBKIT_DISABLE_COMPOSITING_MODE="${WEBKIT_DISABLE_COMPOSITING_MODE:-1}" \
    NO_AT_BRIDGE="${NO_AT_BRIDGE:-1}" \
    MICLAW_GDK_BACKEND="${MICLAW_GDK_BACKEND:-wayland,x11}" \
      "$HERE/squashfs-root/AppRun"
    echo "extracted AppRun exit status: $?"

    echo
    echo "== direct binary fallback =="
    env -u LD_LIBRARY_PATH -u APPDIR \
      RUST_LOG="${RUST_LOG:-debug}" \
      WEBKIT_DISABLE_DMABUF_RENDERER="${WEBKIT_DISABLE_DMABUF_RENDERER:-1}" \
      WEBKIT_DISABLE_COMPOSITING_MODE="${WEBKIT_DISABLE_COMPOSITING_MODE:-1}" \
      NO_AT_BRIDGE="${NO_AT_BRIDGE:-1}" \
      GDK_BACKEND="${MICLAW_GDK_BACKEND:-x11}" \
      "$HERE/squashfs-root/usr/bin/miclaw_api_bridge"
    echo "direct binary exit status: $?"
  fi

  echo
  echo "== recent coredumps =="
  if command -v coredumpctl >/dev/null 2>&1; then
    coredumpctl --no-pager --since "@$before" info 2>/dev/null || true
  else
    echo "coredumpctl not found"
  fi
} 2>&1 | tee "$LOG"

echo "debug log: $LOG"
EOF
chmod +x "$OUT_DIR/run-steamos-debug.sh"
echo "Linux AppImage output: $OUT_DIR"
