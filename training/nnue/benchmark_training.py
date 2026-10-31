"""Paired CPU NNUE benchmark, with separate optional profiling processes.

Inputs stay immutable. A numerical mismatch is saved before failing; allowing a
mismatch explicitly is useful for screening, not evidence of training quality.
"""

import argparse
import hashlib
import io
import json
import math
import os
import platform
import statistics
import subprocess
import sys
import tarfile
import tempfile
import time
from pathlib import Path

import dataset_storage

REPO = Path(__file__).resolve().parents[2]


def digest(path):
    value = hashlib.sha256()
    with Path(path).open("rb") as source:
        for chunk in iter(lambda: source.read(1 << 20), b""):
            value.update(chunk)
    return value.hexdigest()


def cpu_model():
    info = Path("/proc/cpuinfo")
    if info.exists():
        for line in info.read_text().splitlines():
            if line.startswith("model name"):
                return line.split(":", 1)[1].strip()
    return platform.processor()


def distribution(values):
    return dict(
        n=len(values),
        min=min(values),
        median=statistics.median(values),
        max=max(values),
        mean=statistics.mean(values),
        sample_sd=statistics.stdev(values) if len(values) > 1 else None,
    )


def numerical_difference(left, right):
    """Report structural mismatches as well as errors on common numeric leaves."""
    errors, changed = [], []

    def visit(a, b, path):
        if isinstance(a, dict) and isinstance(b, dict):
            for key in sorted(a.keys() | b.keys()):
                if key not in a or key not in b:
                    changed.append(f"{path}.{key}")
                else:
                    visit(a[key], b[key], f"{path}.{key}")
        elif isinstance(a, list) and isinstance(b, list):
            if len(a) != len(b):
                changed.append(f"{path}.length")
            for i, (x, y) in enumerate(zip(a, b, strict=False)):
                visit(x, y, f"{path}[{i}]")
        elif type(a) in (int, float) and type(b) in (int, float):
            if not math.isfinite(a) or not math.isfinite(b):
                raise ValueError(f"Nonfinite numerical result: {path}")
            errors.append(abs(a - b))
        elif a != b:
            changed.append(path)

    visit(left, right, "result")
    return dict(
        exact=left == right,
        compared_values=len(errors),
        max_abs_error=max(errors, default=0),
        mean_abs_error=statistics.mean(errors) if errors else None,
        changed_structure_or_values=changed,
    )


def compare_results(baseline, candidate):
    comparison = {
        key: numerical_difference(baseline[key], candidate[key]) for key in ("losses", "metrics", "validation_scores")
    }
    comparison["weights_exact"] = baseline["weights_sha256"] == candidate["weights_sha256"]
    comparison["workload_exact"] = (
        baseline["sample_ids_sha256"] == candidate["sample_ids_sha256"]
        and baseline["dataset_manifest_sha256"] == candidate["dataset_manifest_sha256"]
    )
    comparison["exact"] = (
        comparison["weights_exact"]
        and comparison["workload_exact"]
        and all(comparison[key]["exact"] for key in ("losses", "metrics", "validation_scores"))
    )
    return comparison


def summarize(rows):
    summary = {}
    for width in sorted({row["width"] for row in rows}):
        selected = [row for row in rows if row["width"] == width]
        metrics = {}
        for key in ("train_s", "validation_s", "workload_s", "process_s", "peak_rss_kib", "export_s", "checkpoint_s"):
            if not all(key in row["results"][arm] for row in selected for arm in ("baseline", "candidate")):
                continue
            baseline = [row["results"]["baseline"][key] for row in selected]
            candidate = [row["results"]["candidate"][key] for row in selected]
            metrics[key] = dict(
                baseline=distribution(baseline),
                candidate=distribution(candidate),
                paired_ratio=distribution([b / a for a, b in zip(baseline, candidate, strict=True)]),
            )
        summary[str(width)] = dict(
            pairs=len(selected), exact_pairs=sum(row["comparison"]["exact"] for row in selected), metrics=metrics
        )
    return summary


def run_worker(a, directory, width, seed, microbatch, profiled=False):
    command = [
        sys.executable,
        str(Path(__file__).resolve()),
        str(a.dataset.resolve()),
        "--module-dir",
        str(directory),
        "--width",
        str(width),
        "--steps",
        str(a.steps),
        "--seed",
        str(seed),
        "--cpu",
        str(a.cpu),
        "--validation-samples",
        str(a.validation_samples),
        "--batch",
        str(a.batch),
        "--microbatch",
        str(microbatch),
    ]
    if profiled:
        command.append("--profile-worker")
    if a.artifacts:
        command.append("--artifacts")
    started = time.perf_counter()
    child = subprocess.run(command, check=True, capture_output=True, text=True, timeout=a.timeout)
    result = json.loads(child.stdout)
    result["process_s"] = time.perf_counter() - started
    return result


def write_report(path, report):
    # The caller reserves a new output path before starting any worker. Publish
    # complete JSON after each pair, including the first failing comparison.
    report["summary"] = summarize(report["raw"])
    report["median_candidate_over_baseline"] = {
        width: {metric: stats["paired_ratio"]["median"] for metric, stats in row["metrics"].items()}
        for width, row in report["summary"].items()
    }
    with tempfile.NamedTemporaryFile(mode="w", dir=path.parent, prefix=f".{path.name}.", delete=False) as target:
        temporary = Path(target.name)
        try:
            target.write(json.dumps(report, indent=2, allow_nan=False) + "\n")
            target.close()
            temporary.replace(path)
        finally:
            temporary.unlink(missing_ok=True)


def benchmark(a):
    baseline = subprocess.check_output(["git", "rev-parse", a.baseline_rev + "^{commit}"], cwd=REPO, text=True).strip()
    archive = subprocess.check_output(["git", "archive", baseline, "training/nnue"], cwd=REPO)
    report = dict(
        version=2,
        status="running",
        baseline_revision=baseline,
        candidate_revision=subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=REPO, text=True).strip(),
        candidate_source_dirty=bool(
            subprocess.check_output(["git", "status", "--porcelain", "--", "training/nnue"], cwd=REPO, text=True)
        ),
        candidate_source_sha256={p.name: digest(p) for p in sorted(Path(__file__).parent.glob("*.py"))},
        dataset=json.loads((a.dataset / "dataset.json").read_text()),
        dataset_manifest_sha256=digest(a.dataset / "dataset.json"),
        seed=a.seed,
        steps=a.steps,
        batch=a.batch,
        microbatch=a.microbatch,
        candidate_microbatch=a.candidate_microbatch,
        validation_samples=a.validation_samples,
        cpu=a.cpu,
        cpu_model=cpu_model(),
        torch_threads=1,
        artifact_temporary_root=tempfile.gettempdir(),
        python=sys.version,
        platform=platform.platform(),
        artifacts=a.artifacts,
        allow_numerical_differences=a.allow_numerical_differences,
        timing_scope="One warmup update; timed fixed update count, validation, and optional private export/checkpoint. Setup and output hashing are outside workload_s; process_s includes them. Not a full epoch or production resume benchmark.",
        profile_scope="Separate cProfile processes; cumulative callable times may overlap and must not be summed. Profiled runs never enter normal timing summaries.",
        raw=[],
        profiles=[],
    )
    a.out.parent.mkdir(parents=True, exist_ok=True)
    with a.out.open("x") as target:
        target.write(json.dumps(report, indent=2) + "\n")
    try:
        with tempfile.TemporaryDirectory(prefix="nnue-cpu-benchmark-") as temp:
            with tarfile.open(fileobj=io.BytesIO(archive)) as source:
                source.extractall(temp, filter="data")
            directories = dict(baseline=Path(temp) / "training/nnue", candidate=Path(__file__).parent)
            for width in a.widths:
                for pair in range(a.pairs):
                    variants = ["baseline", "candidate"] if pair % 2 == 0 else ["candidate", "baseline"]
                    results = {
                        arm: run_worker(
                            a,
                            directories[arm],
                            width,
                            a.seed + pair,
                            a.microbatch if arm == "baseline" else a.candidate_microbatch,
                        )
                        for arm in variants
                    }
                    comparison = compare_results(results["baseline"], results["candidate"])
                    report["raw"].append(dict(width=width, pair=pair, results=results, comparison=comparison))
                    write_report(a.out, report)
                    print(
                        json.dumps(
                            dict(
                                width=width,
                                pair=pair,
                                exact=comparison["exact"],
                                train_ratio=results["candidate"]["train_s"] / results["baseline"]["train_s"],
                            )
                        ),
                        flush=True,
                    )
                    if not comparison["workload_exact"]:
                        raise ValueError("Sampled IDs or dataset identity changed between variants")
                    if any(
                        result["dataset_manifest_sha256"] != report["dataset_manifest_sha256"]
                        for result in results.values()
                    ):
                        raise ValueError("Dataset identity changed during benchmark")
                    if not comparison["exact"] and not a.allow_numerical_differences:
                        raise ValueError(
                            "Numerical mismatch; inspect saved diagnostics before explicitly allowing differences"
                        )
                if a.profile:
                    first = next(row for row in report["raw"] if row["width"] == width)
                    for arm in ("baseline", "candidate"):
                        profiled = run_worker(
                            a,
                            directories[arm],
                            width,
                            a.seed,
                            a.microbatch if arm == "baseline" else a.candidate_microbatch,
                            profiled=True,
                        )
                        parity = compare_results(first["results"][arm], profiled)
                        report["profiles"].append(dict(width=width, variant=arm, result=profiled, parity=parity))
                        if not parity["exact"]:
                            raise ValueError(f"Profiling changed {arm} outputs at width {width}")
            report["status"] = (
                "exact" if all(row["comparison"]["exact"] for row in report["raw"]) else "numerical_differences_allowed"
            )
    except KeyboardInterrupt:
        report.update(status="interrupted", error="Benchmark interrupted")
        raise
    except Exception as error:
        report.update(status="failed", error=str(error))
        if isinstance(error, subprocess.CalledProcessError):
            report["worker_stderr"] = error.stderr
        raise
    finally:
        write_report(a.out, report)
    print(
        json.dumps(
            dict(
                status=report["status"],
                report=str(a.out),
                median_candidate_over_baseline=report["median_candidate_over_baseline"],
            )
        )
    )


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("dataset", type=Path)
    p.add_argument("--baseline-rev", default="6b3397c0409a48bd1fa2c82deb4888b4a26c39f6")
    p.add_argument("--widths", type=int, nargs="+", default=[512, 1536])
    p.add_argument("--pairs", type=int, default=3)
    p.add_argument("--steps", type=int, default=8)
    p.add_argument("--batch", type=int, default=8)
    p.add_argument("--microbatch", type=int, default=1)
    p.add_argument(
        "--candidate-microbatch", type=int, help="Defaults to --microbatch; keep effective --batch unchanged"
    )
    p.add_argument("--validation-samples", type=int, default=64)
    p.add_argument("--seed", type=int, default=20260925)
    p.add_argument("--cpu", type=int, default=min(os.sched_getaffinity(0)))
    p.add_argument("--timeout", type=float, default=300, help="Per-worker wall-clock deadline (seconds)")
    p.add_argument("--profile", action="store_true", help="Additional instrumented processes after normal pairs")
    p.add_argument(
        "--artifacts", action="store_true", help="Time export and benchmark checkpoint in a private temporary directory"
    )
    p.add_argument(
        "--allow-numerical-differences",
        action="store_true",
        help="Save differences and continue; does not approve numerical changes",
    )
    p.add_argument("--out", type=Path)
    p.add_argument("--module-dir", type=Path, help=argparse.SUPPRESS)
    p.add_argument("--width", type=int, help=argparse.SUPPRESS)
    p.add_argument("--profile-worker", action="store_true", help=argparse.SUPPRESS)
    a = p.parse_args()
    if a.candidate_microbatch is None:
        a.candidate_microbatch = a.microbatch
    if min(a.pairs, a.steps, a.batch, a.microbatch, a.candidate_microbatch, a.validation_samples, *a.widths) < 1:
        p.error("pairs, steps, batch sizes, validation samples and widths must be positive")
    if not math.isfinite(a.timeout) or a.timeout <= 0:
        p.error("timeout must be finite and positive")
    if a.cpu not in os.sched_getaffinity(0):
        p.error("cpu must be in this process's allowed affinity")
    if max(a.microbatch, a.candidate_microbatch) > a.batch:
        p.error("microbatch cannot exceed the effective batch")
    if a.module_dir:
        from benchmark_workload import consume

        with dataset_storage.prepare(a.dataset):
            print(json.dumps(consume(a), allow_nan=False))
        return
    if a.out is None or a.out.exists():
        p.error("--out must name a new result file")
    try:
        benchmark(a)
    except (ValueError, OSError, subprocess.SubprocessError) as error:
        location = f"Saved report: {a.out}" if a.out.exists() else "Failed before report creation."
        p.exit(1, f"Benchmark failed: {error}. {location}\n")


if __name__ == "__main__":
    main()
