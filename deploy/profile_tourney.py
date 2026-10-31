#!/usr/bin/env python3
"""Collect a short private, process-only profile of one live tournament game."""

import argparse
import contextlib
import fcntl
import hashlib
import json
import os
import re
import resource
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path

PROC = Path("/proc")
REPORT_TIMEOUT = 30


def digest(path):
    value = hashlib.sha256()
    with Path(path).open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def read(path):
    return json.loads(Path(path).read_text())


def save(path, value):
    temporary = path.with_name(path.name + ".tmp")
    with temporary.open("w") as output:
        json.dump(value, output, indent=2)
        output.write("\n")
        output.flush()
        os.fsync(output.fileno())
    temporary.replace(path)


def process(pid):
    fields = (PROC / str(pid) / "stat").read_text().rsplit(") ", 1)[1].split()
    if fields[0] == "Z":
        raise ValueError(f"game PID {pid} is a zombie")
    ticks = int(fields[11]) + int(fields[12])
    status = dict(line.split(":", 1) for line in (PROC / str(pid) / "status").read_text().splitlines() if ":" in line)
    return dict(
        pid=pid,
        start_ticks=int(fields[19]),
        cpu_ticks=ticks,
        cpu_seconds=ticks / os.sysconf("SC_CLK_TCK"),
        cpus_allowed_list=status["Cpus_allowed_list"].strip(),
        monotonic=time.monotonic(),
        wall=time.time(),
    )


def absolute(path, cwd, *, strict=True):
    path = Path(path)
    return (path if path.is_absolute() else cwd / path).resolve(strict=strict)


def worker(run, pid):
    before = process(pid)
    directory = PROC / str(pid)
    argv = [part.decode() for part in (directory / "cmdline").read_bytes().split(b"\0") if part]
    if len(argv) != 4 or argv[1] != "tournament-game":
        raise ValueError(f"PID {pid} is not an exact tournament-game command")
    cwd = (directory / "cwd").resolve(strict=True)
    request = absolute(argv[2], cwd)
    match = re.fullmatch(r"slot-(\d+)\.request\.json", request.name)
    if request.parent != run / "analysis/game-workers" or not match:
        raise ValueError("game request is outside this run's game-workers")
    if absolute(argv[3], cwd, strict=False) != request.with_name(f"slot-{match[1]}.result.json"):
        raise ValueError("game result path does not match its request")
    raw = request.read_bytes()
    value = json.loads(raw)
    bundle = value["bundle"]
    engine = absolute(bundle["engine_bin"], cwd)
    if value["protocol"] != 1 or bundle["protocol"] != 1:
        raise ValueError("unsupported game worker protocol")
    if not (directory / "exe").samefile(engine) or not absolute(argv[0], cwd).samefile(engine):
        raise ValueError("request bundle and live executable disagree")
    if digest(directory / "exe") != bundle["engine_sha256"]:
        raise ValueError("live executable hash differs from request bundle")
    state = read(run / "state.json")
    slot = next((entry for entry in state["slots"] if entry["id"] == int(match[1])), None)
    if not slot or slot["status"] != "running":
        raise ValueError("request slot is not running")
    entrants = {entry["id"]: entry for entry in state["entrants"]}
    pair = sorted([slot["model_a"], slot["model_b"]])
    agents = [value["config"][side] for side in ("black", "white")]

    def model_path(agent):
        return str(absolute(agent["model"], cwd)) if agent.get("model") else None

    if sorted((model_path(agent) or "") for agent in agents) != sorted(
        (model_path(entrants[name]) or "") for name in pair
    ):
        raise ValueError("request model pair differs from running slot entrants")
    after = process(pid)
    if after["start_ticks"] != before["start_ticks"]:
        raise ValueError("game PID was reused during identity validation")
    return dict(
        pid=pid,
        process=after,
        argv=argv,
        request=str(request),
        request_sha256=hashlib.sha256(raw).hexdigest(),
        slot=int(match[1]),
        model_pair=pair,
        models=sorted(
            [
                dict(
                    name=agent["name"],
                    model=model_path(agent),
                    checkpoint_sha256=digest(model_path(agent)) if model_path(agent) else None,
                )
                for agent in agents
            ],
            key=lambda entry: (entry["model"] or "", entry["name"]),
        ),
        bundle=bundle,
    )


def select_worker(run, pid):
    if pid != "auto":
        return worker(run, int(pid))
    found = []
    for directory in PROC.iterdir():
        if directory.name.isdecimal():
            try:
                found.append(worker(run, int(directory.name)))
            except (OSError, ValueError, KeyError, IndexError, UnicodeError):
                continue  # Other processes and workers finishing naturally are not candidates.
    if not found:
        raise ValueError("no validated live tournament-game worker for this run")
    return max(found, key=lambda entry: (entry["process"]["cpu_ticks"], -entry["pid"]))


def stop_group(child):
    # Only children launched by this collector get a separate session/group.
    with contextlib.suppress(ProcessLookupError):
        os.killpg(child.pid, signal.SIGTERM)
    try:
        child.wait(timeout=2)
    except subprocess.TimeoutExpired:
        pass
    with contextlib.suppress(ProcessLookupError):
        os.killpg(child.pid, signal.SIGKILL)
    child.wait()


def command(argv, out, name, timeout, metadata):
    row = dict(argv=list(map(str, argv)), started=time.time(), timeout_seconds=timeout)
    cpu_before = cpu_usage()
    metadata["commands"].append(row)
    save(out / "metadata.json", metadata)
    env = dict(os.environ, DEBUGINFOD_URLS="", LC_ALL="C", PERF_PAGER="cat", PAGER="cat")
    with (out / (name + ".stdout")).open("wb") as stdout, (out / (name + ".stderr")).open("wb") as stderr:
        child = subprocess.Popen(row["argv"], stdout=stdout, stderr=stderr, env=env, start_new_session=True)
        try:
            child.wait(timeout=timeout)
        finally:
            stop_group(child)
            row.update(returncode=child.returncode, finished=time.time())
            row["cpu_seconds"] = {key: value - cpu_before[key] for key, value in cpu_usage().items()}
    if child.returncode:
        raise RuntimeError(f"{name} exited {child.returncode}; inspect {name}.stderr")


def cpu_usage():
    def total(who):
        value = resource.getrusage(who)
        return value.ru_utime + value.ru_stime

    return dict(collector_self=total(resource.RUSAGE_SELF), reaped_collector_children=total(resource.RUSAGE_CHILDREN))


def collect(args):
    run = args.run_dir.resolve(strict=True)
    out = args.out.absolute()
    previous_umask = os.umask(0o077)
    try:
        out.mkdir(mode=0o700, parents=True, exist_ok=False)
        metadata = dict(
            status="running",
            stage="preflight",
            run=str(run),
            output=str(out),
            started=time.time(),
            commands=[],
            frequency=args.frequency,
            seconds=args.seconds,
            call_graph=args.call_graph,
            boot_id=(PROC / "sys/kernel/random/boot_id").read_text().strip(),
            collector_affinity=sorted(os.sched_getaffinity(0)),
            event="cpu-clock:u",
            scope="One process, no inherited children. Model PAIR only; no side or phase attribution.",
            limits="Sampled self time is descriptive; collector overhead is not estimated by this tool.",
        )
        cpu_before = cpu_usage()
        try:
            save(out / "metadata.json", metadata)
            lock_path = run / "analysis/profile-tourney.lock"
            with lock_path.open("a+") as lock:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                perf = shutil.which("perf")
                if not perf:
                    raise RuntimeError("perf is not installed or is absent from PATH")
                selected = select_worker(run, args.pid)
                metadata.update(
                    worker=selected,
                    source=dict(
                        bundle_revision=selected["bundle"]["revision"],
                        note="Request bundle revision label, not an independent source-tree audit.",
                    ),
                    build_sha256=hashlib.sha256(json.dumps(selected["bundle"], sort_keys=True).encode()).hexdigest(),
                    perf=str(Path(perf).resolve()),
                    clock_ticks_per_second=os.sysconf("SC_CLK_TCK"),
                )
                (out / "request.json").write_bytes(Path(selected["request"]).read_bytes())
                if digest(out / "request.json") != selected["request_sha256"]:
                    raise ValueError("request changed before capture")
                command([perf, "--version"], out, "perf-version", 10, metadata)
                metadata["perf_version"] = (out / "perf-version.stdout").read_text().strip()
                metadata["stage"] = "record"
                before = worker(run, selected["pid"])
                if (
                    any(before[key] != selected[key] for key in ("argv", "request_sha256", "bundle", "models"))
                    or before["process"]["start_ticks"] != selected["process"]["start_ticks"]
                ):
                    raise ValueError("worker identity changed before recording")
                metadata["pid_before"] = before["process"]
                argv = [
                    perf,
                    "record",
                    "-e",
                    "cpu-clock:u",
                    "-F",
                    str(args.frequency),
                    "-p",
                    str(selected["pid"]),
                    "--no-inherit",
                    "--no-buildid-cache",
                    "-o",
                    out / "perf.data",
                ]
                if args.call_graph == "dwarf":
                    argv.extend(["--call-graph", "dwarf,8192"])
                command([*argv, "--", "sleep", str(args.seconds)], out, "record", args.seconds + 15, metadata)
                after = worker(run, selected["pid"])
                metadata["pid_after"] = after["process"]
                if after["process"]["start_ticks"] != before["process"]["start_ticks"]:
                    raise ValueError("game PID was reused during recording")
                if any(after[key] != before[key] for key in ("argv", "request_sha256", "bundle", "models")):
                    raise ValueError("worker identity changed during recording")
                metadata["stage"] = "report"
                command(
                    [
                        perf,
                        "report",
                        "-i",
                        out / "perf.data",
                        "--stdio",
                        "--no-children",
                        "--percent-limit",
                        "0.5",
                        "--sort",
                        "symbol",
                        "--no-call-graph",
                    ],
                    out,
                    "report",
                    REPORT_TIMEOUT,
                    metadata,
                )
                command(
                    [perf, "script", "-i", out / "perf.data", "--hide-call-graph", "-F", "pid"],
                    out,
                    "samples",
                    REPORT_TIMEOUT,
                    metadata,
                )
                samples = [line.strip() for line in (out / "samples.stdout").read_text().splitlines() if line.strip()]
                if not samples or any(line != str(selected["pid"]) for line in samples):
                    raise ValueError("zero samples or unexpected sampled PID; profile is not accepted")
                metadata.update(
                    status="complete",
                    stage="complete",
                    samples=len(samples),
                    process_cpu_seconds=after["process"]["cpu_seconds"] - before["process"]["cpu_seconds"],
                    elapsed_seconds=after["process"]["monotonic"] - before["process"]["monotonic"],
                )
        except BaseException as error:
            metadata.update(status="failed", error=str(error))
            raise
        finally:
            metadata.update(
                finished=time.time(),
                cpu_seconds={key: value - cpu_before[key] for key, value in cpu_usage().items()},
                artifacts={
                    str(path.relative_to(out)): dict(sha256=digest(path), bytes=path.stat().st_size)
                    for path in sorted(out.rglob("*"))
                    if path.is_file() and path.name != "metadata.json"
                },
            )
            save(out / "metadata.json", metadata)
        return metadata
    finally:
        os.umask(previous_umask)


def interrupted(signum, _frame):
    raise RuntimeError(f"profile collector interrupted by signal {signum}")


def parser():
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("--run-dir", required=True, type=Path)
    result.add_argument("--out", required=True, type=Path, help="new private output directory")
    result.add_argument("--pid", default="auto")
    result.add_argument("--seconds", type=int, default=30)
    result.add_argument("--frequency", type=int, default=19)
    result.add_argument("--call-graph", choices=("none", "dwarf"), default="none")
    return result


def main():
    cli = parser()
    args = cli.parse_args()
    if not 1 <= args.seconds <= 60 or not 1 <= args.frequency <= 99:
        cli.error("seconds must be in 1..60 and frequency in 1..99")
    if args.pid != "auto" and (not args.pid.isdecimal() or int(args.pid) <= 0):
        cli.error("pid must be auto or a positive process ID")
    for signum in (signal.SIGTERM, signal.SIGINT):
        signal.signal(signum, interrupted)
    try:
        result = collect(args)
        print(
            json.dumps(
                dict(
                    output=result["output"],
                    pid=result["worker"]["pid"],
                    samples=result["samples"],
                    status=result["status"],
                )
            )
        )
    except (OSError, ValueError, RuntimeError, KeyError, IndexError, subprocess.TimeoutExpired) as error:
        print(
            f"Tournament profile failed: {error}; evidence retained under {args.out}. No game was signalled or changed.",
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
