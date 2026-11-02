"""Run every NNUE unittest in balanced processes with disjoint CPU affinity."""

import argparse
import json
import os
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from collections import Counter
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]

# Approximate relative costs from the November 2026 suite profile, used only for
# scheduling. Unlisted/new modules are always discovered and get a default cost.
MODULE_COST = {
    "test_continuation": 20,
    "test_benchmark_training": 14,
    "test_finetune": 13,
    "test_v5": 11,
    "test_training_efficiency": 8,
}


def cases(suite):
    for test in suite:
        if isinstance(test, unittest.TestSuite):
            yield from cases(test)
        else:
            yield test


def discover(directory):
    loader = unittest.TestLoader()
    suite = loader.discover(str(directory), pattern="test_*.py")
    if loader.errors:
        raise RuntimeError("Test discovery failed:\n" + "\n".join(loader.errors))
    tests = list(cases(suite))
    if not tests:
        raise ValueError(f"No tests discovered in {directory}")
    ids = [test.id() for test in tests]
    if len(ids) != len(set(ids)):
        raise ValueError("Duplicate test IDs discovered")
    return tests


def plan(modules, cpus, jobs):
    count = min(jobs, len(modules), len(cpus))
    if count < 1:
        raise ValueError("Need tests, available CPUs, and a positive job count")
    groups = [[] for _ in range(count)]
    costs = [0] * count
    for module in sorted(modules, key=lambda name: (-MODULE_COST.get(name, 1), name)):
        index = min(range(count), key=costs.__getitem__)
        groups[index].append(module)
        costs[index] += MODULE_COST.get(module, 1)
    return [(sorted(group), cpus[index::count]) for index, group in enumerate(groups)]


def verify(expected, reports):
    actual = [ident for report in reports for ident in report["ids"]]
    if Counter(actual) != Counter(expected):
        raise ValueError("Worker test IDs differ from discovery (missing, duplicated, or unexpected tests)")
    if any(report["count"] != len(report["ids"]) for report in reports):
        raise ValueError("A worker did not run its complete discovered suite")


def worker(directory, modules, cpus, result_path):
    if hasattr(os, "sched_setaffinity"):
        os.sched_setaffinity(0, set(cpus))
    tests = [test for test in discover(directory) if type(test).__module__ in modules]
    result = unittest.TextTestRunner(verbosity=2).run(unittest.TestSuite(tests))
    result_path.write_text(
        json.dumps(
            dict(
                ids=[test.id() for test in tests],
                count=result.testsRun,
                failures=len(result.failures),
                errors=len(result.errors),
                skipped=len(result.skipped),
                successful=result.wasSuccessful(),
            )
        )
    )
    return 0 if result.wasSuccessful() else 1


def run(directory, jobs):
    started = time.monotonic()
    tests = discover(directory)
    cpus = sorted(os.sched_getaffinity(0)) if hasattr(os, "sched_getaffinity") else list(range(os.cpu_count() or 1))
    groups = plan({type(test).__module__ for test in tests}, cpus, jobs)
    processes = []
    reports = []
    with tempfile.TemporaryDirectory(prefix="nnue-tests-") as temp:
        try:
            for index, (modules, affinity) in enumerate(groups):
                log = Path(temp) / f"worker-{index}.log"
                result = log.with_suffix(".json")
                command = [
                    sys.executable,
                    str(Path(__file__).resolve()),
                    "--directory",
                    str(directory),
                    "--worker",
                    *modules,
                    "--cpus",
                    *map(str, affinity),
                    "--result",
                    str(result),
                ]
                with log.open("w") as output:
                    process = subprocess.Popen(command, stdout=output, stderr=subprocess.STDOUT, start_new_session=True)
                processes.append((process, log, result))
                print(f"Worker {index + 1}: CPUs {affinity}; {', '.join(modules)}", flush=True)
            for process, log, result in processes:
                process.wait()
                print(log.read_text(), end="", flush=True)
                if not result.exists():
                    raise RuntimeError(f"Test worker exited {process.returncode} without a result")
                report = json.loads(result.read_text())
                reports.append(report)
            verify([test.id() for test in tests], reports)
            successful = all(process.returncode == 0 for process, _, _ in processes) and all(
                report["successful"] for report in reports
            )
            print(
                f"Ran {sum(r['count'] for r in reports)} tests in {time.monotonic() - started:.3f}s "
                f"across {len(groups)} workers; "
                f"failures={sum(r['failures'] for r in reports)}, errors={sum(r['errors'] for r in reports)}, "
                f"skipped={sum(r['skipped'] for r in reports)}; coverage verified",
                flush=True,
            )
            return 0 if successful else 1
        finally:
            for process, _, _ in processes:
                if process.poll() is None:
                    if os.name == "posix":
                        try:
                            os.killpg(process.pid, signal.SIGKILL)
                        except ProcessLookupError:
                            pass
                    else:
                        process.kill()
                    process.wait()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--jobs", type=int, default=4, help="Maximum test processes (default: 4; capped by available CPUs)"
    )
    parser.add_argument("--directory", type=Path, default=HERE, help=argparse.SUPPRESS)
    parser.add_argument("--worker", nargs="+", help=argparse.SUPPRESS)
    parser.add_argument("--cpus", type=int, nargs="+", help=argparse.SUPPRESS)
    parser.add_argument("--result", type=Path, help=argparse.SUPPRESS)
    args = parser.parse_args()
    if args.jobs < 1:
        parser.error("--jobs must be positive")
    # Benchmark workers inspect source paths; use the same cwd as CI's existing
    # unittest discovery command, including when invoked from another directory.
    os.chdir(ROOT)
    try:
        if args.worker:
            if args.cpus is None or args.result is None:
                parser.error("workers require --cpus and --result")
            return worker(args.directory.resolve(), args.worker, args.cpus, args.result)
        if os.name == "posix":
            signal.signal(signal.SIGTERM, stop)
        return run(args.directory.resolve(), args.jobs)
    except (OSError, ValueError, RuntimeError) as error:
        parser.exit(1, f"Tests failed: {error}\n")


def stop(signum, _frame):
    # Raising through run() executes its worker cleanup on CI cancellation.
    raise SystemExit(128 + signum)


if __name__ == "__main__":
    sys.exit(main())
