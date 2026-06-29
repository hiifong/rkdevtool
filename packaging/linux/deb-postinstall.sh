#!/bin/sh
set -e

# Reload udev after installing Rockchip USB rules (.deb postinst).
if [ -d /run/udev ] && command -v udevadm >/dev/null 2>&1; then
  udevadm control --reload-rules
  udevadm trigger --subsystem-match=usb
fi

exit 0
