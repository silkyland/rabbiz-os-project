#!/usr/bin/env python3
"""Check the CHUB scan bitmap and optionally sample live IIO motion."""
import argparse
import re
import sys
import time
from pathlib import Path

SCAN_REPLY = re.compile(
    r"a21s-chub: rx cmd=4 type=255 sub=27 plen=(\d+) data=(.*)"
)
SENSORS = (
    ("accelerometer", 1, "a21s-accel", "accel"),
    ("gyroscope", 4, "a21s-gyro", "anglvel"),
)


def scan_bitmap(log):
    replies = SCAN_REPLY.findall(log)
    if not replies:
        raise ValueError("No SENSOR_SCAN_RESULT reply in the log")
    length, payload = replies[-1]
    data = bytes.fromhex(payload)
    if int(length) < 8 or len(data) < 8:
        raise ValueError("SENSOR_SCAN_RESULT lacks its 8-byte legacy bitmap")
    return int.from_bytes(data[:8], "little")


def sample_motion():
    devices = {
        name.read_text().strip(): name.parent
        for name in Path("/sys/bus/iio/devices").glob("iio:device*/name")
    }
    axes = {}
    for label, _, device, channel in SENSORS:
        if device not in devices:
            raise ValueError(f"Missing IIO device {device}")
        axes[label] = [
            devices[device] / f"in_{channel}_{axis}_raw" for axis in "xyz"
        ]

    def snapshot():
        return {
            label: tuple(int(path.read_text()) for path in paths)
            for label, paths in axes.items()
        }

    print("Hold the phone still for 3 seconds.", flush=True)
    time.sleep(3)
    initial = snapshot()
    print("Rotate the phone slowly through multiple axes for 8 seconds.", flush=True)
    observed = {label: {sample} for label, sample in initial.items()}
    deadline = time.monotonic() + 8
    while time.monotonic() < deadline:
        for label, sample in snapshot().items():
            observed[label].add(sample)
        time.sleep(0.05)

    passed = True
    for label, samples in observed.items():
        changed = len(samples) > 1 and any(any(sample) for sample in samples)
        print(f"{label}: {len(samples)} distinct XYZ samples ({'PASS' if changed else 'FAIL'})")
        passed &= changed
    return passed


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("log", help="Kernel log file, or - for stdin")
    parser.add_argument("--motion", action="store_true", help="Also sample IIO on the A21s")
    args = parser.parse_args()

    try:
        log = sys.stdin.read() if args.log == "-" else Path(args.log).read_text()
        bitmap = scan_bitmap(log)
        missing = [label for label, bit, _, _ in SENSORS if not bitmap & (1 << bit)]
        print(f"CHUB legacy sensor bitmap: 0x{bitmap:016x}", flush=True)
        if missing:
            print(f"FAIL: missing from discovery: {', '.join(missing)}", file=sys.stderr)
            return 1
        print("PASS: accelerometer and gyroscope are both advertised", flush=True)
        return 0 if not args.motion or sample_motion() else 1
    except (OSError, ValueError) as error:
        print(f"INCONCLUSIVE: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
