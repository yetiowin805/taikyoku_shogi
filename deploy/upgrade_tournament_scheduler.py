"""Upgrade a legacy scheduler after games finish, holding its admission gate throughout.

New schedulers support live model admission and do not need this migration for
training completions. This utility never changes the rolling game-engine pointer.
"""

import argparse
import fcntl
import json
import os
import signal
import subprocess
import time
from contextlib import ExitStack
from pathlib import Path

import tournament_tasks as tasks
import tourney_analysis as a


def service(action, name):
    subprocess.run(["systemctl", action, name], check=True)


def clock_transition(config, initial_ms, increment_ms):
    """Prepare an explicit clock change; publish only after active games finish."""
    if initial_ms is None and increment_ms is None:
        return config
    if initial_ms is None or increment_ms is None:
        raise ValueError("initial time and increment must be supplied together")
    if not 1000 < initial_ms <= 86_400_000 or not 0 < increment_ms <= 3_600_000:
        raise ValueError("invalid Fischer clock: initial >1s, <=1 day; increment >0, <=1 hour")
    command = list(config["command"])
    for flag, value in [("--initial-time-ms", initial_ms), ("--increment-ms", increment_ms)]:
        if command.count(flag) > 1:
            raise ValueError("duplicate clock option: " + flag)
        if flag in command:
            index = command.index(flag)
            if index + 1 == len(command) or command[index + 1].startswith("--"):
                raise ValueError("missing clock value: " + flag)
            command[index + 1] = str(value)
        else:
            command += [flag, str(value)]
    if "--max-time-ms" in command:
        raise ValueError("cannot combine Fischer clocks with a fixed move budget")
    return dict(config, command=command, time_control=dict(initial_ms=initial_ms, increment_ms=increment_ms))


def external_leases(control, services, count):
    pids = {
        int(subprocess.check_output(["systemctl", "show", name, "-p", "MainPID", "--value"], text=True).strip() or 0)
        for name in services
    } - {0}
    held = set()
    for line in Path("/proc/locks").read_text().splitlines():
        fields = line.split()
        if len(fields) >= 8 and fields[1:4] == ["FLOCK", "ADVISORY", "WRITE"] and int(fields[4]) in pids:
            major, minor, inode = fields[5].split(":")
            held.add((int(major, 16), int(minor, 16), int(inode)))
    result = set()
    for index in range(count):
        path = control / f"cpu-{index}.lock"
        if path.exists():
            info = path.stat()
            if (os.major(info.st_dev), os.minor(info.st_dev), info.st_ino) in held:
                result.add(index)
    return result


def drain(control, stack, count, services, status):
    # The caller holds allocation.lock continuously. Releasing it between polls
    # permits old game workers to reclaim CPUs ahead of a waiting maintenance task.
    leases = {}
    while True:
        for index in range(count):
            if index in leases:
                continue
            lease = (control / f"cpu-{index}.lock").open("a+")
            try:
                fcntl.flock(lease, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                lease.close()
            else:
                leases[index] = stack.enter_context(lease)
        external = external_leases(control, services, count)
        running = [s["id"] for s in a.read(control.parent / "state.json")["slots"] if s["status"] == "running"]
        status("draining", running_slots=running, held_cpus=sorted(leases), external_cpus=sorted(external))
        if set(leases) | external == set(range(count)):
            if running:
                raise RuntimeError("All leases accounted for but game slots remain running; refusing restart")
            return
        time.sleep(2)


def stop_idle_coordinator(control, coordinator):
    # Called only with the gate held and every CPU lease accounted for. Legacy
    # worker threads block in flock and cannot join on SIGTERM until gate release.
    meta = a.read(control / "supervisor.json")
    if not a.alive(meta):
        service("stop", coordinator)
        return
    pid = meta["tournament_pid"]
    fields = Path(f"/proc/{pid}/stat").read_text().split(") ", 1)[1].split()
    if int(fields[1]) != meta["pid"]:
        raise ValueError("Tournament child identity changed; refusing restart")
    for task in Path(f"/proc/{pid}/task").iterdir():
        if (task / "children").read_text().strip():
            raise ValueError("Tournament still owns child processes; refusing restart")
    fd = os.pidfd_open(pid)
    try:
        current = Path(f"/proc/{pid}/stat").read_text().split(") ", 1)[1].split()
        if current[19] != fields[19] or int(current[1]) != meta["pid"]:
            raise ValueError("Tournament child changed during restart")
        subprocess.run(["systemctl", "stop", "--no-block", coordinator], check=True)
        signal.pidfd_send_signal(fd, signal.SIGKILL)
        service("stop", coordinator)
    finally:
        os.close(fd)


def upgrade(args):
    run, binary = args.run.resolve(), args.binary.resolve()
    control = run / "analysis"
    if subprocess.check_output([str(binary), "tournament-task-protocol"], text=True).strip() != "1":
        raise ValueError("Replacement scheduler lacks the required task protocol")
    expected = a.file_digest(binary)
    pointer = (control / "active-engine.json").read_bytes()
    cfg = a.read(control / "config.json")
    # Validate before touching any service or taking the game-admission gate.
    clock_transition(cfg, args.initial_time_ms, args.increment_ms)
    if len(cfg["cpus"]) != 8 or cfg.get("sidecar") != "none":
        raise ValueError("Legacy migration requires an eight-CPU external-sidecar scheduler")
    report = control / "scheduler-upgrade.json"

    def status(state, **extra):
        tasks.durable(
            report,
            json.dumps(dict(state=state, binary=str(binary), sha256=expected, updated=time.time(), **extra)).encode(),
        )

    with ExitStack() as stack:
        own = stack.enter_context((control / "scheduler-upgrade.lock").open("a+"))
        fcntl.flock(own, fcntl.LOCK_EX | fcntl.LOCK_NB)
        try:
            gate = stack.enter_context((control / "allocation.lock").open("a+"))
            fcntl.flock(gate, fcntl.LOCK_EX)
            for name in args.replace_service:
                service("stop", name)
            drain(control, stack, 8, args.external_service, status)
            update = stack.enter_context((control / "update.lock").open("a+"))
            fcntl.flock(update, fcntl.LOCK_EX)
            if a.file_digest(binary) != expected or (control / "active-engine.json").read_bytes() != pointer:
                raise ValueError("Scheduler or game-engine pointer changed during drain")
            status("upgrading")
            stop_idle_coordinator(control, args.coordinator)
            cfg = clock_transition(a.read(control / "config.json"), args.initial_time_ms, args.increment_ms)
            backup = control / f"before-scheduler-upgrade-{time.time_ns()}"
            backup.mkdir()
            for path in [run / "state.json", control / "config.json", control / "manifest.json"]:
                tasks.durable(backup / path.name, path.read_bytes())
            cfg["command"][0] = str(binary)
            tasks.durable(control / "config.json", json.dumps(cfg, indent=2).encode())
            service("start", args.coordinator)
            while True:
                try:
                    tasks.ready(run)
                    break
                except (ValueError, FileNotFoundError):
                    if subprocess.run(["systemctl", "is-active", "--quiet", args.coordinator]).returncode:
                        raise RuntimeError(
                            "Scheduler did not start; inspect tournament.log; backup: " + str(backup)
                        ) from None
                    time.sleep(1)
            if args.initial_time_ms is not None:
                applied = a.read(run / "state.json").get("time_control")
                if applied != cfg["time_control"]:
                    raise RuntimeError("Restarted scheduler did not apply the requested clock")
            if (control / "active-engine.json").read_bytes() != pointer:
                raise RuntimeError("Game engine pointer unexpectedly changed")
            status("ready", backup=str(backup))
        except BaseException as error:
            status("failed", error=str(error))
            raise
    # Admission no longer needs any idle CPUs or scheduler restart.
    for name in args.replace_service:
        service("start", name)
    status("completed")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("run", type=Path)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--coordinator", default="taikyoku-coordinator.service")
    parser.add_argument("--replace-service", action="append", default=[])
    parser.add_argument("--external-service", action="append", default=[])
    parser.add_argument("--initial-time-ms", type=int, help="Apply after current games finish; 60000 for 1+1")
    parser.add_argument("--increment-ms", type=int, help="Supply with initial time; 1000 for 1+1")
    upgrade(parser.parse_args())


if __name__ == "__main__":
    main()
