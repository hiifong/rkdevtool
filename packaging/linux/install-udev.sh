#!/bin/sh
set -e

# One-time udev setup for AppImage / manual installs (requires root).
RULE_NAME="99-rkdevtool-rockchip.rules"
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
SRC="$SCRIPT_DIR/$RULE_NAME"
DEST="/lib/udev/rules.d/$RULE_NAME"

if [ "$(id -u)" -ne 0 ]; then
  echo "Run as root: sudo $0" >&2
  exit 1
fi

if [ ! -f "$SRC" ]; then
  echo "Rule file not found: $SRC" >&2
  exit 1
fi

install -Dm644 "$SRC" "$DEST"
echo "Installed $DEST"

if [ -d /run/udev ] && command -v udevadm >/dev/null 2>&1; then
  udevadm control --reload-rules
  udevadm trigger --subsystem-match=usb
  echo "udev reloaded. Unplug/replug the device or re-enter Maskrom."
fi
