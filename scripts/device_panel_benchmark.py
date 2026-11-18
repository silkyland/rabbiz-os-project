#!/usr/bin/env python3
"""Run ON DEVICE under device_session.sh; compare identical panel gestures.

Usage: device_panel_benchmark.py baseline|candidate [--touch /root/touch_path.py]
Writes only /tmp/rabbiz-panel-perf, takes screenshots as the session user,
and never prints journal lines other than the opt-in panel timing trace.
"""
import argparse
import json
import math
import os
from pathlib import Path
import re
import subprocess
import time


def run(*args):
    return subprocess.run(args, check=True, text=True, capture_output=True).stdout


def percentile(values, q):
    values = sorted(values)
    return values[max(0, math.ceil(len(values) * q) - 1)] if values else None


def main():
    parser = argparse.ArgumentParser(__doc__)
    parser.add_argument('label', choices=('baseline', 'candidate'))
    parser.add_argument('--touch', default='/root/touch_path.py')
    args = parser.parse_args()
    out = Path('/tmp/rabbiz-panel-perf')
    out.mkdir(exist_ok=True)
    import pwd
    account = pwd.getpwnam('rabbiz')
    os.chown(out, account.pw_uid, account.pw_gid)
    pid = int(run('pgrep', '-x', 'rabbiz-de').splitlines()[0])
    stat = Path(f'/proc/{pid}/stat')

    def ticks():
        fields = stat.read_text().rsplit(')', 1)[1].split()
        return int(fields[11]) + int(fields[12])

    def touch(path):
        run('python3', args.touch, path)

    # Start from Home; discard preparation from the sample.
    touch('360,1400,0;360,140,900')
    touch('360,1590,0;360,950,150')
    cursor = run('journalctl', '-n', '0', '--show-cursor').strip()
    cursor = next(line.removeprefix('-- cursor: ') for line in cursor.splitlines() if line.startswith('-- cursor: '))
    start, cpu = time.monotonic(), ticks()
    for i, x in enumerate((540, 180, 540)):
        touch(f'{x},12,0;{x},1440,1100')
        time.sleep(.4)
        run('runuser', '-u', 'rabbiz', '--', 'env',
            f'XDG_RUNTIME_DIR=/run/user/{account.pw_uid}', 'WAYLAND_DISPLAY=wayland-0',
            'grim', '-o', 'DSI-1', str(out / f'{args.label}-{i}.png'))
        touch('360,1400,0;360,140,900')
        time.sleep(.4)
    elapsed = time.monotonic() - start
    cpu_seconds = (ticks() - cpu) / os.sysconf('SC_CLK_TCK')
    if int(run('pgrep', '-x', 'rabbiz-de').splitlines()[0]) != pid:
        raise RuntimeError('Compositor changed during measurement; discard sample')
    journal = run('journalctl', '-b', f'_PID={pid}', f'--after-cursor={cursor}', '-o', 'cat')
    trace = '\n'.join(line for line in journal.splitlines() if line.startswith('panel-'))
    (out / f'{args.label}-trace.txt').write_text(trace + '\n')
    def values(pattern):
        return [float(n) for n in re.findall(pattern, trace)]
    frame = values(r'panel-frame cpu_ms=([\d.]+)')
    gap = [v for v in values(r'queue_gap_ms=([\d.]+)') if v > 0]
    result = dict(kernel=run('uname', '-r').strip(), pid=pid, elapsed_s=elapsed,
        cpu_seconds=cpu_seconds, one_core_percent=100*cpu_seconds/elapsed, cycles=3,
        frames=len(frame), frame_cpu_p50_ms=percentile(frame, .5),
        frame_cpu_p95_ms=percentile(frame, .95), queue_gap_p95_ms=percentile(gap, .95),
        blur_ms=values(r'panel-backdrop blur_ms=([\d.]+)'),
        raster_ms=values(r'panel-raster cpu_ms=([\d.]+)'))
    (out / f'{args.label}.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
