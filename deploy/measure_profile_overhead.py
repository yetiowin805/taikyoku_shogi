#!/usr/bin/env python3
"""Measure perf observer cost with paired identical warm search batches."""

import argparse
import contextlib
import json
import math
import os
import random
import resource
import select
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "benchmarks/nnue_followup"))
import profile_workload as workload

PARITY_FIELDS = ("signature", "best", "score", "nodes", "q_nodes", "completed_depth", "hash", "pieces", "legal", "nnue")


def indices(value):
    try:
        return [int(part) for part in value.split(",")]
    except ValueError as error:
        raise argparse.ArgumentTypeError("use comma-separated integer indices") from error


def schedule(positions, repeats, seed):
    rng = random.Random(seed)
    starts = {position: bool(rng.getrandbits(1)) for position in positions}
    for repetition in range(repeats):
        order = list(positions)
        rng.shuffle(order)
        for position in order:
            arms = ["detached", "attached"]
            if starts[position] ^ bool(repetition % 2):
                arms.reverse()
            yield repetition, position, arms


def summarize(rows, seed=87231, draws=3000):
    result = dict(pairs=len(rows), metrics={})
    if not rows:
        raise ValueError("no completed paired batches")
    for metric in ("cpu_ms", "ms"):
        logs = [math.log(row["attached"][metric] / row["detached"][metric]) for row in rows]
        point = math.exp(sum(logs) / len(logs))
        item = dict(
            attached_over_detached=point,
            overhead_percent=100 * (point - 1),
            total_ms={arm: sum(row[arm][metric] for row in rows) for arm in ("detached", "attached")},
        )
        if len(logs) >= 2:
            rng = random.Random(seed)
            samples = sorted(math.exp(sum(rng.choices(logs, k=len(logs))) / len(logs)) for _ in range(draws))
            low, high = samples[int(draws * 0.025)], samples[min(draws - 1, int(draws * 0.975))]
            item.update(paired_bootstrap_95=[low, high], overhead_percent_95=[100 * (low - 1), 100 * (high - 1)])
        result["metrics"][metric] = item
    result.update(
        method="Equal-pair geometric mean attached/detached batch ratios; deterministic percentile bootstrap of whole paired batches.",
        uncertainty="Serial pairs share models/roots and host load; this conditional paired interval is not a broad deployment guarantee.",
        interpretation="An interval spanning 1.01 does not establish overhead below 1%; neither a noisy reduction nor exact search parity proves zero observer cost.",
        profiler_cpu_seconds=sum(row["attached"]["recorder"]["cpu_seconds"] for row in rows),
        sampling_wall_seconds=sum(row["attached"]["recorder"]["sampling_wall_seconds"] for row in rows),
        target_samples=sum(row["attached"]["recorder"].get("target_samples", 0) for row in rows),
    )
    return result


def target_cpu(pid):
    # /proc's comm may contain spaces/parentheses; fields after the final ')' start at state (field 3).
    fields = Path(f"/proc/{pid}/stat").read_text().rsplit(") ", 1)[1].split()
    return dict(start_ticks=int(fields[19]), cpu_ticks=int(fields[11]) + int(fields[12]))


def validate_ack(ack):
    # Some perf versions include the C string's trailing NUL in the acknowledgement.
    if ack not in (b"ack\n", b"ack\n\0"):
        raise RuntimeError("unexpected perf acknowledgement: " + repr(ack))


class Recorder:
    """Attach disabled, acknowledge enable, then stop only the perf process."""

    def __init__(self, pid, path, args, env):
        self.path = path
        self.args = args
        self.env = env
        self.pid = pid
        self.result = None
        self.proc = None
        self.fds = []
        self.error = None
        self.command = None

    def __enter__(self):
        started = time.monotonic()
        self.before = resource.getrusage(resource.RUSAGE_CHILDREN)
        control_read, control_write = os.pipe()
        ack_read, ack_write = os.pipe()
        self.fds = [control_read, control_write, ack_read, ack_write]
        self.command = [
            getattr(self.args, "perf_path", "perf"),
            "record",
            "-e",
            "cpu-clock:u",
            "-F",
            str(self.args.frequency),
            "--no-inherit",
            "-p",
            str(self.pid),
            "--delay=-1",
            "--control",
            f"fd:{control_read},{ack_write}",
            "-o",
            str(self.path),
        ]
        if self.args.call_graph == "dwarf":
            self.command.extend(["--call-graph", "dwarf,8192"])
        self.error = self.path.with_suffix(".stderr").open("wb")
        try:
            self.proc = subprocess.Popen(
                self.command,
                env=self.env,
                stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL,
                stderr=self.error,
                pass_fds=(control_read, ack_write),
            )
            for fd in (control_read, ack_write):
                os.close(fd)
                self.fds.remove(fd)
            os.write(control_write, b"enable\n")
            deadline = time.monotonic() + 10
            ack = b""
            while b"\n" not in ack:
                if self.proc.poll() is not None:
                    raise RuntimeError("perf exited before enable acknowledgement; inspect " + str(self.error.name))
                if time.monotonic() >= deadline:
                    raise TimeoutError("perf enable acknowledgement timed out")
                if select.select([ack_read], [], [], min(0.1, max(0, deadline - time.monotonic())))[0]:
                    chunk = os.read(ack_read, 64)
                    if not chunk:
                        raise RuntimeError("perf acknowledgement pipe closed")
                    ack += chunk
            validate_ack(ack)
            self.enabled = time.monotonic()
            self.registration_seconds = self.enabled - started
            return self
        except BaseException:
            self.__exit__(*sys.exc_info())
            raise

    def __exit__(self, exc_type, _exc, _traceback):
        stopped = time.monotonic()
        error = None
        if self.proc is not None:
            was_running = self.proc.poll() is None
            if was_running:
                # Never signal the benchmark child or its process group here.
                try:
                    self.proc.send_signal(signal.SIGINT)
                except ProcessLookupError:
                    pass
            try:
                self.proc.wait(timeout=10)
            except subprocess.TimeoutExpired:
                self.proc.kill()
                self.proc.wait()
                error = RuntimeError("perf did not stop within ten seconds of SIGINT")
            after = resource.getrusage(resource.RUSAGE_CHILDREN)
            self.result = dict(
                command=self.command,
                data_path=str(self.path),
                target_pid=self.pid,
                returncode=self.proc.returncode,
                cpu_seconds=(after.ru_utime - self.before.ru_utime) + (after.ru_stime - self.before.ru_stime),
                registration_seconds=getattr(self, "registration_seconds", None),
                sampling_wall_seconds=stopped - getattr(self, "enabled", stopped),
                perf_data_bytes=self.path.stat().st_size if self.path.exists() else 0,
                cpu_accounting="RUSAGE_CHILDREN delta around perf; the sole benchmark child remains alive/unreaped during this interval.",
            )
            if not was_running or self.proc.returncode not in (0, -signal.SIGINT, 128 + signal.SIGINT):
                error = RuntimeError("perf failed/exited unexpectedly; inspect " + str(self.error.name))
            if not self.result["perf_data_bytes"]:
                error = RuntimeError("perf produced no nonempty recording")
            workload.atomic_json(self.path.with_suffix(".json"), self.result)
        for fd in self.fds:
            os.close(fd)
        self.fds.clear()
        if self.error is not None:
            self.error.close()
        if error is not None and exc_type is None:
            raise error


def checked_response(worker, query, corpus, expected):
    reply = worker.request(query)
    workload.validate_result(reply, query, corpus)
    for metric in ("cpu_ms", "ms"):
        value = reply.get(metric)
        if type(value) not in (int, float) or not math.isfinite(value) or value <= 0:
            raise ValueError("invalid response timing: " + metric)
    for field in PARITY_FIELDS:
        if field not in reply:
            raise ValueError("missing response field: " + field)
    if (
        type(reply["nodes"]) is not int
        or reply["nodes"] <= 0
        or type(reply["q_nodes"]) is not int
        or reply["q_nodes"] < 0
    ):
        raise ValueError("invalid response node counts")
    identity = {field: reply[field] for field in PARITY_FIELDS}
    key = query["model"], query["position"], query["depth"]
    if expected.setdefault(key, identity) != identity:
        raise ValueError("search signature/nodes changed with repeated work: " + str(query))
    return reply


def batch(worker, query, arm, pair_index, args, corpus, expected, responses, env):
    recorder = (
        Recorder(worker.proc.pid, args.out / f"pair-{pair_index:04d}.perf.data", args, env)
        if arm == "attached"
        else None
    )
    with recorder if recorder is not None else contextlib.nullcontext():
        before = target_cpu(worker.proc.pid)
        started = time.monotonic()
        replies = []
        for index in range(args.batch_repeats):
            reply = checked_response(worker, query, corpus, expected)
            replies.append(reply)
            responses.write(
                json.dumps(dict(pair=pair_index, arm=arm, batch_index=index, query=query, response=reply)) + "\n"
            )
        elapsed = time.monotonic() - started
        after = target_cpu(worker.proc.pid)
        if before["start_ticks"] != after["start_ticks"] or after["cpu_ticks"] < before["cpu_ticks"]:
            raise ValueError("benchmark process identity changed")
    responses.flush()
    return dict(
        searches=len(replies),
        cpu_ms=sum(reply["cpu_ms"] for reply in replies),
        ms=sum(reply["ms"] for reply in replies),
        batch_wall_ms=elapsed * 1000,
        target_cpu_ticks=after["cpu_ticks"] - before["cpu_ticks"],
        target_start_ticks=before["start_ticks"],
        recorder=recorder.result if recorder is not None else None,
    )


def audit_samples(rows, env, perf_path):
    """Decode outside every measured batch; verify samples belong to the target."""
    for row in rows:
        recorder = row["attached"]["recorder"]
        command = [perf_path, "script", "--no-demangle", "--hide-call-graph", "-i", recorder["data_path"], "-F", "pid"]
        result = subprocess.run(command, env=env, check=True, capture_output=True, text=True, timeout=30)
        samples = [int(line.strip()) for line in result.stdout.splitlines() if line.strip()]
        if any(pid != recorder["target_pid"] for pid in samples):
            raise ValueError("perf recording contains samples outside its benchmark target")
        recorder.update(target_samples=len(samples), decode_command=command)
        workload.atomic_json(Path(recorder["data_path"]).with_suffix(".json"), recorder)
    if not sum(row["attached"]["recorder"]["target_samples"] for row in rows):
        raise ValueError("perf recorded no target CPU samples; observer-cost result is inconclusive")


def measure(args):
    corpus_path = args.corpus.resolve(strict=True)
    binary = args.binary.resolve(strict=True)
    corpus = json.loads(corpus_path.read_text())
    workload.validate_indices(args.models, corpus["models"], "model")
    workload.validate_indices(args.positions, corpus["positions"], "position")
    if args.cpu not in os.sched_getaffinity(0):
        raise ValueError("CPU outside available affinity")
    inputs = workload.audit_inputs(corpus, args.models)
    args.out = args.out.resolve()
    args.out.mkdir(mode=0o700, parents=True, exist_ok=False)
    env = {key: value for key, value in os.environ.items() if not key.startswith("TAIKYOKU_AB_")}
    env.pop("LLVM_PROFILE_FILE", None)
    env["DEBUGINFOD_URLS"] = ""
    env["LC_ALL"] = "C"
    env["PERF_BUILDID_DIR"] = str(args.out / "buildid-cache")
    args.perf_path = shutil.which("perf")
    helper = Path(workload.__file__).resolve()
    plan = dict(
        status="running",
        binary=str(binary),
        binary_sha256=workload.file_digest(binary),
        corpus_path=str(corpus_path),
        corpus_sha256=workload.file_digest(corpus_path),
        inputs=inputs,
        cpu=args.cpu,
        models=args.models,
        positions=args.positions,
        depth=args.depth,
        repeats=args.repeats,
        batch_repeats=args.batch_repeats,
        frequency=args.frequency,
        call_graph=args.call_graph,
        seed=args.seed,
        cpu_ticks_per_second=os.sysconf("SC_CLK_TCK"),
        perf_path=args.perf_path,
        perf_version=None,
        workload_helper=dict(path=str(helper), sha256=workload.file_digest(helper)),
        runner_sha256=workload.file_digest(__file__),
        environment_overrides={"DEBUGINFOD_URLS": "", "LC_ALL": "C", "PERF_BUILDID_DIR": env["PERF_BUILDID_DIR"]},
        removed_environment="TAIKYOKU_AB_* and LLVM_PROFILE_FILE",
        readiness="perf --delay=-1 --control fd pipe; wait for bounded enable acknowledgement before each attached batch",
        timing="Response cpu_ms/ms sum only search(); registration excluded. /proc CPU ticks and batch wall include request/validation overhead.",
        limits="Warm fixed-work replay while the tournament remains active; host contention/cgroup quotas and a small model/root sample limit generalization. No real-game strength or clock behavior measurement.",
        commands=[],
    )
    workload.atomic_json(args.out / "plan.json", plan)
    rows = []
    expected = {}
    try:
        if args.perf_path is None:
            raise RuntimeError("perf executable not found on PATH; install a matching Linux perf tool")
        plan["perf_version"] = subprocess.run(
            [args.perf_path, "--version"], env=env, check=True, capture_output=True, text=True, timeout=5
        ).stdout.strip()
        workload.atomic_json(args.out / "plan.json", plan)
        with (
            (args.out / "responses.jsonl").open("w") as responses,
            (args.out / "pairs.jsonl").open("w") as pairs,
            (args.out / "warmups.jsonl").open("w") as warm,
        ):
            for model in args.models:
                command = ["taskset", "-c", str(args.cpu), str(binary), str(corpus_path)]
                plan["commands"].append(dict(model=model, argv=command))
                workload.atomic_json(args.out / "plan.json", plan)
                with (
                    (args.out / f"model-{model}.stderr").open("wb") as error,
                    workload.ProfileWorker(
                        command, env=env, stderr=error, timeout_seconds=args.request_timeout_seconds
                    ) as worker,
                ):
                    for position in args.positions:
                        query = dict(position=position, model=model, depth=args.depth)
                        warm.write(json.dumps(checked_response(worker, query, corpus, expected)) + "\n")
                        warm.flush()
                    for repetition, position, order in schedule(args.positions, args.repeats, args.seed + model):
                        query = dict(position=position, model=model, depth=args.depth)
                        values = {
                            arm: batch(worker, query, arm, len(rows), args, corpus, expected, responses, env)
                            for arm in order
                        }
                        row = dict(query, repetition=repetition, order=order, **values)
                        rows.append(row)
                        pairs.write(json.dumps(row) + "\n")
                        pairs.flush()
                        print(
                            f"paired model {model} root {position} repetition {repetition + 1}/{args.repeats}",
                            flush=True,
                        )
        audit_samples(rows, env, args.perf_path)
        # Add final decoded sample counts to the paired audit records.
        (args.out / "pairs.jsonl").write_text("".join(json.dumps(row) + "\n" for row in rows))
        if (
            workload.file_digest(binary) != plan["binary_sha256"]
            or workload.file_digest(corpus_path) != plan["corpus_sha256"]
            or workload.audit_inputs(corpus, args.models) != inputs
            or workload.file_digest(helper) != plan["workload_helper"]["sha256"]
        ):
            raise ValueError("binary, corpus or inputs changed during observer-cost measurement")
        summary = summarize(rows, args.seed)
        summary.update(
            all_search_signatures_match=True,
            measured_searches=len(rows) * args.batch_repeats * 2,
            limits=plan["limits"],
            by_model={
                corpus["models"][model]["name"]: summarize([row for row in rows if row["model"] == model], args.seed)
                for model in args.models
            },
        )
        workload.atomic_json(args.out / "summary.json", summary)
        plan["status"] = "complete"
        workload.atomic_json(args.out / "plan.json", plan)
        return summary
    except BaseException as error:
        plan.update(status="failed", error=str(error))
        workload.atomic_json(args.out / "plan.json", plan)
        raise


def run(args):
    previous = os.umask(0o077)
    try:
        return measure(args)
    finally:
        os.umask(previous)


def interrupted(signum, _frame):
    raise RuntimeError(f"observer-cost measurement interrupted by signal {signum}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("binary", "corpus", "out"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--cpu", type=int, required=True)
    parser.add_argument("--models", type=indices, default=[0, 1])
    parser.add_argument("--positions", type=indices, default=[0, 1])
    parser.add_argument("--depth", type=int, default=2)
    parser.add_argument("--repeats", type=int, default=8)
    parser.add_argument("--batch-repeats", type=int, default=15)
    parser.add_argument("--frequency", type=int, default=19)
    parser.add_argument("--call-graph", choices=("none", "dwarf"), default="none")
    parser.add_argument("--request-timeout-seconds", type=float, default=120)
    parser.add_argument("--seed", type=int, default=87231)
    args = parser.parse_args()
    if min(args.depth, args.repeats, args.batch_repeats, args.frequency, args.request_timeout_seconds) <= 0:
        parser.error("depth, repeats, batch size, frequency and timeout must be positive")
    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    try:
        print(json.dumps(run(args), indent=2))
    except BaseException as error:
        print(
            f"Observer-cost experiment failed: {error}; inspect {args.out}; no tournament selection was changed.",
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
