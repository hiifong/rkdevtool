#!/bin/sh
set -e

# Re-apply udev after removing Rockchip USB rules (.deb postrm).
if [ -d /run/udev ] && command -v udevadm >/dev/null 2>&1; then
  udevadm control --reload-rules
  udevadm trigger --subsystem-match=usb
fi

exit 0
