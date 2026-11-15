#!/bin/sh
# A21s ABOX audio bring-up: load modules, init Calliope DSP + SMA1303 amp,
# apply media-speaker routing, unmute speaker path.
# Deployed to /root/audio-up.sh on device; driven by abox-audio.service.
KVER=$(uname -r)
X=/usr/lib/modules/$KVER/extra
insmod $X/s2mpu12-regulator.ko
sleep 0.5
insmod $X/sma1303.ko
insmod $X/aud3004x-codec.ko
sleep 0.5
insmod $X/abox-snd.ko
sleep 1
insmod $X/exynos3830-aud3004x.ko
sleep 1
v=
for i in $(seq 1 20); do
  v=$(cat /sys/devices/platform/soc@0/14a50000.abox/calliope_version 2>/dev/null)
  [ -n "$v" ] && break
  sleep 0.5
done
echo "calliope_version=$v"
sh /root/abox_speaker.sh 1 >/dev/null 2>&1
# SMA1303 amp PLL: 1.536 MHz BCLK (16-bit, 48 kHz stereo)
echo 1536000 > /sys/bus/i2c/devices/0-001e/playback_init 2>/dev/null
# Real codec switch (UCM also toggles this on device enable)
amixer -c 1 cset name='SPK SPK On' on >/dev/null 2>&1
echo "=== audio-up done ==="
