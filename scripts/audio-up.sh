#!/bin/sh
# A21s ABOX audio bring-up: load modules, init Calliope DSP + SMA1303 amp,
# apply media-speaker routing, unmute speaker path.
# Deployed to /root/audio-up.sh on device; driven by abox-audio.service.
set -eu
KVER=$(uname -r)
X=/usr/lib/modules/$KVER/extra
load_module() {
  name=$(printf '%s' "$1" | tr '-' '_')
  [ -d "/sys/module/$name" ] || insmod "$X/$1.ko"
}
load_module s2mpu12-regulator
sleep 0.5
load_module sma1303
load_module aud3004x-codec
sleep 0.5
load_module abox-snd
sleep 1
load_module exynos3830-aud3004x
sleep 1
v=
for i in $(seq 1 20); do
  v=$(cat /sys/devices/platform/soc@0/14a50000.abox/calliope_version 2>/dev/null)
  [ -n "$v" ] && break
  sleep 0.5
done
echo "calliope_version=$v"
[ -n "$v" ] || { echo 'ABOX DSP did not become ready' >&2; exit 1; }
card=
for id in /sys/class/sound/card*/id; do
  case "$(cat "$id")" in
    Exynos3830*) card=${id%/id}; card=${card##*/card}; break ;;
  esac
done
[ -n "$card" ] || { echo 'Exynos3830 sound card missing' >&2; exit 1; }
sh /root/abox_speaker.sh "$card"
# SMA1303 amp PLL: 1.536 MHz BCLK (16-bit, 48 kHz stereo)
amp=
for init in /sys/bus/i2c/drivers/sma1303/*/playback_init; do
  [ -f "$init" ] || continue
  [ -z "$amp" ] || { echo 'Multiple SMA1303 amps found' >&2; exit 1; }
  amp=$init
done
[ -n "$amp" ] || { echo 'SMA1303 playback_init missing' >&2; exit 1; }
echo 1536000 > "$amp"
# Real codec switch (UCM also toggles this on device enable)
amixer -c "$card" cset name='SPK SPK On' on
echo "=== audio-up done ==="
