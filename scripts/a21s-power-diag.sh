#!/bin/sh
# Read-only power / charging diagnostics for the Galaxy A21s (RabbizOS).
#
# Nothing here writes to hardware: no register pokes, no governor changes,
# no service restarts. It answers three questions at once:
#
#   1. What does the charger path look like right now? (battery status,
#      current, voltage, USB gadget state, cable detection)
#   2. How much power is the SoC drawing while idle? (governor, cpuidle
#      residency, per-IRQ wake rate, top talkers, enabled clock gates)
#   3. Which register values did the bootloader leave in the S2MU106
#      (MUIC @0x3d / charger / fuel gauge @0x3b) — the block that has no
#      mainline driver yet, so nobody re-programs the input current limit.
#
# Run it on the device as root, plugged in AND unplugged, and diff the two
# outputs:
#   ssh root@172.16.42.1 'sh -s' < scripts/a21s-power-diag.sh > plugged.log
#   (unplug, wait 10 s)
#   ssh root@172.16.42.1 'sh -s' < scripts/a21s-power-diag.sh > unplugged.log
#
# Timing: roughly 15 seconds of sampling.

set -u

say() { printf '\n===== %s =====\n' "$*"; }

say "kernel"
uname -a
echo "cmdline: $(cat /proc/cmdline)"

say "uptime / load"
uptime

say "power_supply"
if [ -d /sys/class/power_supply ] && [ -n "$(ls -A /sys/class/power_supply 2>/dev/null)" ]; then
	for ps in /sys/class/power_supply/*; do
		echo "-- $(basename "$ps")"
		cat "$ps/uevent" 2>/dev/null
		echo "   files: $(ls "$ps" | tr '\n' ' ')"
	done
else
	echo "EMPTY: /sys/class/power_supply has no devices"
fi

say "usb gadget (UDC)"
for u in /sys/class/udc/*; do
	[ -e "$u" ] || continue
	echo "-- $u"
	for a in state current_speed maximum_speed is_otg role; do
		[ -r "$u/$a" ] && echo "   $a: $(cat "$u/$a")"
	done
done
command -v lsusb >/dev/null 2>&1 && lsusb 2>/dev/null | head -5

say "usb controller / phy"
for d in /sys/bus/platform/drivers/dwc3* /sys/bus/platform/devices/*usb*; do
	[ -d "$d" ] && echo "$d"
done 2>/dev/null | head -20

say "cpufreq"
for p in /sys/devices/system/cpu/cpufreq/policy*; do
	[ -d "$p" ] || continue
	echo "-- $(basename "$p")"
	echo "   governor:      $(cat "$p/scaling_governor" 2>/dev/null)"
	echo "   cur:           $(cat "$p/scaling_cur_freq" 2>/dev/null)"
	echo "   min/max:       $(cat "$p/scaling_min_freq" 2>/dev/null) / $(cat "$p/scaling_max_freq" 2>/dev/null)"
	echo "   available gov: $(cat "$p/scaling_available_governors" 2>/dev/null)"
	echo "   freqs:         $(cat "$p/scaling_available_frequencies" 2>/dev/null)"
done
[ -r /sys/devices/system/cpu/cpufreq/boost ] && echo "boost: $(cat /sys/devices/system/cpu/cpufreq/boost)"
[ -r /sys/devices/system/cpu/online ] && echo "online: $(cat /sys/devices/system/cpu/online)"

say "cpuidle (residency: name usage time_us)"
for c in /sys/devices/system/cpu/cpu0/cpuidle/state*; do
	[ -d "$c" ] || continue
	echo "   $(basename "$c"): $(cat "$c/name" 2>/dev/null) usage=$(cat "$c/usage" 2>/dev/null) time_us=$(cat "$c/time" 2>/dev/null) latency_us=$(cat "$c/latency" 2>/dev/null) disabled=$(cat "$c/disable" 2>/dev/null)"
done
[ -r /sys/devices/system/cpu/cpuidle/current_driver ] && echo "driver: $(cat /sys/devices/system/cpu/cpuidle/current_driver)"

say "suspend capability"
echo "state:      $(cat /sys/power/state 2>/dev/null)"
echo "mem_sleep:  $(cat /sys/power/mem_sleep 2>/dev/null)"

say "interrupt / context switch rate (vmstat 1s x5)"
vmstat 1 5 2>/dev/null

say "top CPU consumers"
top -bn1 2>/dev/null | head -18

say "running services (count: $(systemctl list-units --type=service --state=running --no-legend --no-pager 2>/dev/null | wc -l))"
systemctl list-units --type=service --state=running --no-legend --no-pager 2>/dev/null | awk '{print "   " $1}'

say "radios"
iw dev 2>/dev/null | grep -E 'Interface|channel|txpower' | head -6
for i in /sys/class/net/wlan0 /sys/class/net/eth0; do
	[ -d "$i" ] || continue
	echo "-- $i"
	echo "   operstate: $(cat "$i/operstate" 2>/dev/null)"
	[ -r "$i/statistics/tx_bytes" ] && echo "   tx/rx bytes: $(cat "$i/statistics/tx_bytes") / $(cat "$i/statistics/rx_bytes")"
done
iw dev wlan0 get power_save 2>/dev/null
for h in /sys/class/bluetooth/hci*; do
	[ -e "$h" ] && echo "bt: $(basename "$h") present"
done
hciconfig 2>/dev/null | head -4

say "backlight"
for b in /sys/class/backlight/*; do
	[ -d "$b" ] || continue
	echo "-- $(basename "$b")"
	echo "   brightness/max: $(cat "$b/brightness" 2>/dev/null) / $(cat "$b/max_brightness" 2>/dev/null)"
	echo "   actual_brightness: $(cat "$b/actual_brightness" 2>/dev/null)"
done

say "thermal"
if [ -d /sys/class/thermal ] && [ -n "$(ls -A /sys/class/thermal 2>/dev/null)" ]; then
	for z in /sys/class/thermal/thermal_zone*; do
		[ -e "$z/type" ] && echo "   $(cat "$z/type"): $(cat "$z/temp" 2>/dev/null)"
	done
else
	echo "EMPTY: no thermal zones"
fi

say "i2c buses"
ls -l /dev/i2c-* 2>/dev/null || echo "no /dev/i2c-* (i2c-dev not loaded)"
if command -v i2cdetect >/dev/null 2>&1; then
	for dev in /dev/i2c-*; do
		[ -e "$dev" ] || continue
		echo "-- $dev"
		i2cdetect -y -r "$(basename "$dev" | tr -cd 0-9)" 2>&1 | grep -vE '^\s*$' | head -12
	done
fi

say "s2mu106 registers (plain i2c write+read, never SMBus ioctl)"
# AGENTS.md: I2C_SMBUS byte reads on 0x3d return garbage; i2ctransfer issues a
# plain I2C_RDWR transfer, which is what the kernel driver itself uses.
i2crd() {
	# bus addr reg
	i2ctransfer -y "$1" "w1@$2" "$3" r1 2>/dev/null | awk '{print $NF}'
}
dec_inlim() { echo $(( ($1 & 0x7f) * 25 + 50 )); }        # mA
dec_cv()    { echo $(( ($1 & 0x7f) * 5 + 3900 )); }        # mV
dec_fast()  { echo $(( (($1 & 0x3f) + 1) * 50 )); }        # mA
dec_mode()  { case $(( $1 & 0x0f )) in
		0) echo "OFF";; 1) echo "BUCK(!)";; 3) echo "CHG";;
		2) echo "BST";; 6) echo "OTG_BST";; *) echo "mode$(( $1 & 0x0f ))";; esac; }

CHG_BUS=""; CHG_ADDR=""
# The charger driver binds the chip through i2c-0/new_device, so the adapter
# number is not stable across DT changes (see HANDOFF 0.6). Find it by driver.
for d in /sys/bus/i2c/drivers/s2mu106-charger/*; do
	[ -e "$d/name" ] || continue
	CHG_BUS=$(basename "$(dirname "$(readlink -f "$d")")" | tr -cd 0-9)
	CHG_ADDR=$(basename "$d" | cut -d- -f2)
done
echo "charger i2c device: bus=${CHG_BUS:-?} addr=${CHG_ADDR:-?}"
if [ -n "$CHG_BUS" ] && command -v i2ctransfer >/dev/null 2>&1; then
	A="0x$CHG_ADDR"
	c0=$(i2crd "$CHG_BUS" "$A" 0x18); c1=$(i2crd "$CHG_BUS" "$A" 0x19)
	c5=$(i2crd "$CHG_BUS" "$A" 0x1d); c7=$(i2crd "$CHG_BUS" "$A" 0x1f)
	c12=$(i2crd "$CHG_BUS" "$A" 0x24)
	s0=$(i2crd "$CHG_BUS" "$A" 0x0e); s1=$(i2crd "$CHG_BUS" "$A" 0x0f)
	echo "  CTRL0  (0x18) = $c0  mode=$(dec_mode ${c0:-0})   <- must be CHG"
	echo "  CTRL1  (0x19) = $c1  input_limit=$(dec_inlim ${c1:-0}) mA"
	echo "  CTRL5  (0x1d) = $c5  CV=$(dec_cv ${c5:-0}) mV"
	echo "  CTRL7  (0x1f) = $c7  fast_charge=$(dec_fast ${c7:-0}) mA"
	echo "  CTRL12 (0x24) = $c12  wdt_en=$(( (0x${c12#0x} >> 4) & 1 )) wdt_time=$(( (0x${c12#0x} >> 1) & 7 ))"
	echo "  STATUS0(0x0e) = $s0  CHGIN_STATUS=$(( 0x${s0#0x} & 7 ))  (3 or 5 = VBUS good)"
	echo "  STATUS1(0x0f) = $s1  fault=$(( (0x${s1#0x} >> 4) & 0xf )) done=$(( 0x${s1#0x} & 1 ))"
else
	echo "  charger driver not bound (module missing or service failed)"
fi

say "MUIC raw page (0x3d regs 0x00-0x0f, plain transfers)"
if [ -n "$CHG_BUS" ] && command -v i2ctransfer >/dev/null 2>&1; then
	for r in 0 1 2 3 4 5 6 7 8 9 a b c d e f; do
		printf '   0x%02x=%s' "0x$r" "$(i2crd "$CHG_BUS" "0x$CHG_ADDR" "0x$r")"
		[ "$r" = 7 ] || [ "$r" = f ] && printf '\n'
	done
	echo
fi

say "charge time-series (6 x 10 s) — expected rise with cable attached"
GAUGE=""
for b in /sys/class/power_supply/*; do
	[ "$(cat "$b/type" 2>/dev/null)" = "Battery" ] && GAUGE=$b
done
for i in 1 2 3 4 5 6; do
	cap=$(cat "$GAUGE/capacity" 2>/dev/null)
	st=$(cat "$GAUGE/status" 2>/dev/null)
	ci=$(cat "$GAUGE/current_now" 2>/dev/null)
	ca=$(cat "$GAUGE/current_avg" 2>/dev/null)
	v=$(cat "$GAUGE/voltage_now" 2>/dev/null)
	ch=$(cat /sys/class/power_supply/s2mu106-charger/online 2>/dev/null)
	lim=$(cat /sys/class/power_supply/s2mu106-charger/input_current_limit 2>/dev/null)
	printf '   t=%02ds cap=%s%% status=%s I_now=%s I_avg=%s V=%s charger_online=%s in_limit=%s\n' \
		"$(( (i - 1) * 10 ))" "${cap:-?}" "${st:-?}" "${ci:-?}" "${ca:-?}" "${v:-?}" "${ch:-?}" "${lim:-?}"
	[ "$i" = 6 ] || sleep 10
done

say "kernel messages (power related)"
dmesg 2>/dev/null | grep -iE 's2mu|charg|batt|vbus|extcon|power_supply|cpuidle|schedutil' | tail -25

say "done"
