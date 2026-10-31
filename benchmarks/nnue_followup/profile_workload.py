#!/usr/bin/env python3
"""Exercise a PGO-instrumented benchmark; this does not train NNUE weights."""

import argparse
import hashlib
import json
import math
import os
import re
import selectors
import signal
import subprocess
import sys
import time
from pathlib import Path

MAX_LINE_BYTES = 16 * 1024 * 1024


def file_digest(path):
    digest = hashlib.sha256()
    with Path(path).open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def atomic_json(path, value):
    path = Path(path)
    temporary = path.with_name(path.name + ".tmp")
    with temporary.open("w") as output:
        output.write(json.dumps(value, indent=2) + "\n")
        output.flush()
        os.fsync(output.fileno())
    temporary.replace(path)
    fd = os.open(path.parent, os.O_DIRECTORY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def validate_indices(indices, entries, name):
    if len(set(indices)) != len(indices):
        raise ValueError(f"duplicate {name} indices; use --repeats instead")
    if not indices or any(index < 0 or index >= len(entries) for index in indices):
        raise ValueError(f"{name} index outside [0, {len(entries)})")


def audit_inputs(corpus, models):
    """The benchmark eagerly replays every root, even roots not requested."""
    inputs = {}

    def check(path, expected, kind):
        path = Path(path).resolve(strict=True)
        if not isinstance(expected, str) or not re.fullmatch(r"[0-9a-f]{64}", expected):
            raise ValueError(f"invalid content hash: {path}")
        previous = inputs.get(str(path))
        if previous and previous["sha256"] != expected:
            raise ValueError(f"conflicting input hashes: {path}")
        if file_digest(path) != expected:
            raise ValueError(f"profile input changed: {path}")
        inputs[str(path)] = dict(sha256=expected, bytes=path.stat().st_size, kind=kind)
        return path

    for entry in corpus["positions"]:
        if type(entry["ply"]) is not int or entry["ply"] < 0:
            raise ValueError("position ply must be a nonnegative integer")
        check(entry["game"], entry["sha256"], "game")
    for index in models:
        entry = corpus["models"][index]
        model_path = Path(entry["path"])
        checkpoint_path = check(entry["path"], entry["sha256"], "model")
        checkpoint = json.loads(checkpoint_path.read_text())
        descriptor = checkpoint.get("weights", {}).get("nnue")
        if descriptor:
            check(model_path.parent / descriptor["file"], descriptor["sha256"], "nnue")
    return inputs


class LineReader:
    """Read bounded JSON records without trusting first-byte readiness."""

    def __init__(self, stream):
        self.stream = stream
        self.pending = bytearray()
        self.selector = selectors.DefaultSelector()
        os.set_blocking(stream.fileno(), False)
        self.selector.register(stream, selectors.EVENT_READ)

    def close(self):
        self.selector.close()

    def read(self, timeout):
        deadline = time.monotonic() + timeout
        while True:
            end = self.pending.find(b"\n")
            if end >= 0:
                if end > MAX_LINE_BYTES:
                    raise ValueError("profile response exceeds line size limit")
                line = bytes(self.pending[:end])
                del self.pending[: end + 1]
                return line
            if len(self.pending) > MAX_LINE_BYTES:
                raise ValueError("profile response exceeds line size limit")
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError("profile response exceeded request deadline")
            if not self.selector.select(min(remaining, 0.2)):
                continue
            try:
                chunk = os.read(self.stream.fileno(), 64 * 1024)
            except BlockingIOError:
                continue
            if not chunk:
                raise RuntimeError("instrumented engine exited before a complete response; inspect stderr.log")
            self.pending.extend(chunk)


class ProfileWorker:
    """One JSON-lines child, with bounded requests and unconditional cleanup.

    Callers supply argv (including any CPU affinity wrapper), environment and a
    stderr file. Responses are decoded here; validate_result checks their search
    identity against the caller's corpus and request.
    """

    def __init__(self, command, *, env=None, stderr=None, timeout_seconds=120):
        self.command = command
        self.env = env
        self.stderr = stderr
        self.timeout = timeout_seconds
        self.proc = None
        self.reader = None

    def __enter__(self):
        self.proc = subprocess.Popen(
            self.command, env=self.env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.stderr
        )
        try:
            self.reader = LineReader(self.proc.stdout)
        except BaseException:
            self.__exit__(*sys.exc_info())
            raise
        return self

    def request(self, request):
        self.proc.stdin.write((json.dumps(request) + "\n").encode())
        self.proc.stdin.flush()
        return json.loads(self.reader.read(self.timeout))

    def __exit__(self, exc_type, _exc, _traceback):
        try:
            if exc_type is None:
                self.proc.stdin.close()
                if self.proc.wait(timeout=self.timeout):
                    raise RuntimeError("instrumented engine failed; inspect stderr.log")
        finally:
            if self.proc.poll() is None:
                self.proc.kill()
            self.proc.wait()
            self.proc.stdin.close()
            self.proc.stdout.close()
            if self.reader is not None:
                self.reader.close()


def validate_result(result, request, corpus):
    for field in ("position", "model", "depth", "completed_depth"):
        expected = request["depth"] if field == "completed_depth" else request[field]
        if type(result.get(field)) is not int or result[field] != expected:
            raise ValueError(f"profile response {field} mismatch: {request}")
    if result.get("aborted") is not False:
        raise ValueError(f"profile search aborted: {request}")
    if result.get("name") != corpus["positions"][request["position"]]["name"]:
        raise ValueError("profile response position name mismatch")
    if result.get("agent") != corpus["models"][request["model"]]["name"]:
        raise ValueError("profile response model name mismatch")
    if not isinstance(result.get("signature"), str) or not re.fullmatch(r"[0-9a-f]{64}", result["signature"]):
        raise ValueError("profile response has no valid search signature")


def interrupted(signum, _frame):
    # selectors retries InterruptedError internally; use an exception that must
    # escape the I/O wait and enter the worker's cleanup block.
    raise RuntimeError(f"profile interrupted by signal {signum}")


def run(args):
    corpus_path = args.corpus.resolve(strict=True)
    corpus_hash = file_digest(corpus_path)
    corpus = json.loads(corpus_path.read_text())
    validate_indices(args.positions, corpus["positions"], "position")
    validate_indices(args.models, corpus["models"], "model")
    inputs = audit_inputs(corpus, args.models)
    binary = args.binary.resolve(strict=True)
    binary_hash = file_digest(binary)
    args.out.mkdir(parents=True, exist_ok=False)
    profiles = args.out.resolve() / "profiles"
    profiles.mkdir()
    requests = [
        dict(position=p, model=m, depth=args.depth)
        for m in args.models
        for _ in range(args.repeats)
        for p in args.positions
    ]
    plan = dict(
        requests=requests,
        cpu=args.cpu,
        corpus=corpus,
        corpus_path=str(corpus_path),
        corpus_sha256=corpus_hash,
        inputs=inputs,
        binary=str(binary),
        binary_sha256=binary_hash,
        request_timeout_seconds=args.request_timeout_seconds,
        commands=[],
        status="running",
    )
    plan_path = args.out / "plan.json"
    atomic_json(plan_path, plan)
    env = {key: value for key, value in os.environ.items() if not key.startswith("TAIKYOKU_AB_")}
    env["LLVM_PROFILE_FILE"] = str(profiles / "train-%p-%m.profraw")
    signatures = {}
    try:
        with (args.out / "stderr.log").open("wb") as err, (args.out / "searches.jsonl").open("w") as out:
            # Benchmark players retain network allocations until process exit.
            for model in args.models:
                command = ["taskset", "-c", str(args.cpu), str(binary), str(corpus_path)]
                plan["commands"].append(dict(argv=command, model=model))
                atomic_json(plan_path, plan)
                with ProfileWorker(
                    command, env=env, stderr=err, timeout_seconds=args.request_timeout_seconds
                ) as worker:
                    for request in (request for request in requests if request["model"] == model):
                        result = worker.request(request)
                        validate_result(result, request, corpus)
                        key = (request["position"], model, args.depth)
                        if signatures.setdefault(key, result["signature"]) != result["signature"]:
                            raise ValueError(f"repeated search signature changed: {request}")
                        out.write(json.dumps(result) + "\n")
                        out.flush()
                model_profiles = list(profiles.glob(f"train-{worker.proc.pid}-*.profraw"))
                if not model_profiles or any(path.stat().st_size == 0 for path in model_profiles):
                    raise RuntimeError(f"no nonempty profiles for model {model}: build with -C profile-generate first")
        if not list(profiles.glob("*.profraw")):
            raise RuntimeError("no profiles: build with -C profile-generate first")
        if file_digest(corpus_path) != corpus_hash or file_digest(binary) != binary_hash:
            raise ValueError("profile corpus or instrumented binary changed during workload")
        if audit_inputs(corpus, args.models) != inputs:
            raise ValueError("profile inputs changed during workload")
        plan.update(
            status="complete", profiles={str(path): file_digest(path) for path in sorted(profiles.glob("*.profraw"))}
        )
        atomic_json(plan_path, plan)
    except BaseException as exc:
        plan.update(status="failed", error=str(exc))
        atomic_json(plan_path, plan)
        raise
    print(f"{len(requests)} searches; profiles in {profiles}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("binary", "corpus", "out"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--positions", nargs="+", type=int, required=True)
    parser.add_argument("--models", nargs="+", type=int, required=True)
    parser.add_argument("--repeats", type=int, default=20)
    parser.add_argument("--depth", type=int, default=2)
    parser.add_argument("--cpu", type=int, default=min(os.sched_getaffinity(0)))
    parser.add_argument("--request-timeout-seconds", type=float, default=120)
    args = parser.parse_args()
    if min(args.repeats, args.depth) < 1:
        parser.error("repeats and depth must be positive")
    if not math.isfinite(args.request_timeout_seconds) or args.request_timeout_seconds <= 0:
        parser.error("request timeout must be finite and positive")
    if args.cpu not in os.sched_getaffinity(0):
        parser.error("selected CPU is outside the allowed affinity mask")
    for signum in (signal.SIGTERM, signal.SIGINT):
        signal.signal(signum, interrupted)
    run(args)


if __name__ == "__main__":
    main()
