#!/usr/bin/env bash
# Copy or document OpenVPN binaries into src-tauri/resources/openvpn.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LINUX_DIR="$ROOT/src-tauri/resources/openvpn/linux-x86_64"
mkdir -p "$LINUX_DIR"
if [[ -x /usr/sbin/openvpn ]]; then
  cp -a /usr/sbin/openvpn "$LINUX_DIR/openvpn"
  echo "Copied /usr/sbin/openvpn -> $LINUX_DIR/openvpn"
elif [[ -x /usr/bin/openvpn ]]; then
  cp -a /usr/bin/openvpn "$LINUX_DIR/openvpn"
  echo "Copied /usr/bin/openvpn -> $LINUX_DIR/openvpn"
else
  echo "No system openvpn found to copy. Place a static OpenVPN 2.6 binary at:"
  echo "  $LINUX_DIR/openvpn"
  exit 1
fi
# Windows/macOS: place official builds manually for release packaging.
mkdir -p "$ROOT/src-tauri/resources/openvpn/windows-x86_64"
mkdir -p "$ROOT/src-tauri/resources/openvpn/macos-aarch64"
mkdir -p "$ROOT/src-tauri/resources/openvpn/macos-x86_64"
echo "Done. Windows/macOS placeholders are empty until release binaries are added."
