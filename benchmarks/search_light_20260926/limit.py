"""Run one experiment on one low-priority CPU with a duty-cycle cap (Linux)."""
import argparse
import json
import os
from pathlib import Path
import resource
import signal
import subprocess
import time

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--cpu', type=int, default=max(os.sched_getaffinity(0)))
p.add_argument('--duty', type=float, default=.25)
p.add_argument('--timeout', type=float, default=900)
p.add_argument('--stats', type=Path, required=True)
p.add_argument('command', nargs=argparse.REMAINDER)
a = p.parse_args()
if not 0 < a.duty <= 1 or a.timeout <= 0:
    p.error('duty must be in (0,1], timeout must be positive')
command = a.command[1:] if a.command[:1] == ['--'] else a.command
if not command:
    p.error('missing command')
start = time.monotonic()
child = subprocess.Popen(['taskset', '-c', str(a.cpu), 'nice', '-n', '19', *command], start_new_session=True)
timed_out = False
interrupted = False

def interrupted_signal(signum, frame):
    raise KeyboardInterrupt

signal.signal(signal.SIGTERM, interrupted_signal)
signal.signal(signal.SIGINT, interrupted_signal)

def send(sig):
    try:
        os.killpg(child.pid, sig)
    except ProcessLookupError:
        pass

try:
    while child.poll() is None:
        if time.monotonic() - start > a.timeout:
            timed_out = True
            break
        send(signal.SIGCONT)
        time.sleep(.2 * a.duty)
        send(signal.SIGSTOP)
        time.sleep(.2 * (1 - a.duty))
except KeyboardInterrupt:
    interrupted = True
finally:
    send(signal.SIGCONT)
    if child.poll() is None:
        send(signal.SIGTERM)
        try:
            child.wait(timeout=2)
        except subprocess.TimeoutExpired:
            send(signal.SIGKILL)
            child.wait()
    # No orphan experiment descendants after a timeout/interruption/early exit.
    send(signal.SIGCONT)
    send(signal.SIGTERM)
r = resource.getrusage(resource.RUSAGE_CHILDREN)
a.stats.parent.mkdir(parents=True, exist_ok=True)
a.stats.write_text(json.dumps(dict(command=command, cpu=a.cpu, nice=19, duty=a.duty,
    timeout=timed_out, interrupted=interrupted, exit_code=child.returncode, wall_seconds=time.monotonic()-start,
    child_cpu_seconds=r.ru_utime+r.ru_stime, peak_child_rss_kib=r.ru_maxrss), indent=2)+'\n')
raise SystemExit(130 if interrupted else 124 if timed_out else child.returncode)
