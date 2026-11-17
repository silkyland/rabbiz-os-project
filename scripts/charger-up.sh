#!/bin/sh
# A21s S2MU106 charging bring-up: load charger driver, instantiate the
# i2c device (no DT node yet), verify the charge switch is on.
# Deployed to /root/charger-up.sh on device; driven by s2mu106-charger.service.
set -eu
KVER=$(uname -r)
X=/usr/lib/modules/$KVER/extra
[ -d /sys/module/s2mu106_charger ] || insmod $X/s2mu106-charger.ko
sleep 0.2
[ -e /sys/bus/i2c/devices/0-003d ] || \
  echo s2mu106-charger 0x3d > /sys/bus/i2c/devices/i2c-0/new_device
sleep 0.5
echo "online=$(cat /sys/class/power_supply/s2mu106-charger/online 2>/dev/null) \
status=$(cat /sys/class/power_supply/s2mu106-charger/status 2>/dev/null)"
echo "=== charger-up done ==="
