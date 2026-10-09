#!/usr/bin/env bash
# Place or document gpclient / gpauth binaries under src-tauri/resources/gpclient.
# Prefer building from a pinned GlobalProtect-openconnect tag, or copy local builds.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LINUX_DIR="$ROOT/src-tauri/resources/gpclient/linux-x86_64"
mkdir -p "$LINUX_DIR"
mkdir -p "$ROOT/src-tauri/resources/gpclient/linux-aarch64"
mkdir -p "$ROOT/src-tauri/resources/gpclient/macos-aarch64"
mkdir -p "$ROOT/src-tauri/resources/gpclient/macos-x86_64"
mkdir -p "$ROOT/src-tauri/resources/gpclient/windows-x86_64"

copy_if_present() {
  local src="$1"
  local dest="$2"
  if [[ -x "$src" ]]; then
    cp -a "$src" "$dest"
    echo "Copied $src -> $dest"
    return 0
  fi
  return 1
}

found=0
if copy_if_present "$(command -v gpclient 2>/dev/null || true)" "$LINUX_DIR/gpclient"; then
  found=1
fi
if copy_if_present "$(command -v gpauth 2>/dev/null || true)" "$LINUX_DIR/gpauth"; then
  found=1
fi

# Optional: build from a local checkout
if [[ "${PORTAL_GPOC_SRC:-}" != "" && -d "${PORTAL_GPOC_SRC}" ]]; then
  echo "Building GlobalProtect-openconnect from PORTAL_GPOC_SRC=${PORTAL_GPOC_SRC}"
  (
    cd "${PORTAL_GPOC_SRC}"
    make build BUILD_GUI_HELPER=0 || true
  )
  copy_if_present "${PORTAL_GPOC_SRC}/target/release/gpclient" "$LINUX_DIR/gpclient" && found=1
  copy_if_present "${PORTAL_GPOC_SRC}/target/release/gpauth" "$LINUX_DIR/gpauth" && found=1
fi

if [[ "$found" -eq 0 ]]; then
  echo "No gpclient/gpauth found on PATH."
  echo "Place binaries at:"
  echo "  $LINUX_DIR/gpclient"
  echo "  $LINUX_DIR/gpauth"
  echo "Or set PORTAL_GPOC_SRC to a GlobalProtect-openconnect checkout and re-run."
  echo "Dev overrides: PORTAL_GPCLIENT_BIN / PORTAL_GPAUTH_BIN"
  # Keep placeholders so the resource tree exists for packaging.
  : >"$LINUX_DIR/.gitkeep"
  exit 0
fi

echo "Done. macOS/Windows placeholders remain empty until release binaries are added."
