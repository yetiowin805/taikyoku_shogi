"""Exercise numerical failure reports and real isolated trainer/profile workers."""

import json
import os
import subprocess
import tempfile
import unittest
from argparse import Namespace
from pathlib import Path
from unittest.mock import patch

import numpy as np

import benchmark_training as bench


def fixture(root):
    schema = dict(features=16, channels=92, hash="00" * 32, names_hash="11" * 32)
    (root / "schema.json").write_text(json.dumps(schema))
    np.tile(np.array([0, 1, 2, 3], dtype="<u4"), 60).tofile(root / "features.bin")
    samples = [
        dict(
            offset=i * 4,
            us=2,
            them=2,
            score=1000,
            material=100,
            outcome=float(i % 4 != 0),
            group=str(i),
            game=str(i),
            split="train" if i < 40 else "validation",
            source="recent",
            label_source="saved",
            selection="representative",
            teacher="NNUE",
        )
        for i in range(60)
    ]
    (root / "samples.jsonl").write_text("".join(json.dumps(sample) + "\n" for sample in samples))
    identity = {
        name + "_sha256": bench.digest(root / (name + "." + suffix))
        for name, suffix in [("features", "bin"), ("samples", "jsonl"), ("schema", "json")]
    }
    (root / "dataset.json").write_text(json.dumps(identity))
    return identity


def options(root, **changes):
    values = dict(
        dataset=root,
        out=root / "result.json",
        baseline_rev="HEAD",
        widths=[32],
        pairs=1,
        steps=2,
        seed=20261025,
        batch=8,
        microbatch=1,
        candidate_microbatch=1,
        validation_samples=8,
        cpu=min(os.sched_getaffinity(0)),
        timeout=60,
        artifacts=False,
        profile=False,
        allow_numerical_differences=False,
    )
    values.update(changes)
    return Namespace(**values)


def worker_result():
    return dict(
        train_s=1.0,
        validation_s=2.0,
        workload_s=3.0,
        process_s=4.0,
        peak_rss_kib=100,
        weights_sha256="same",
        losses=[1.0, 2.0],
        metrics=dict(objective=3.0),
        validation_scores=[10.0, -20.0],
        sample_ids_sha256="same",
        dataset_manifest_sha256="same",
    )


class BenchmarkTests(unittest.TestCase):
    def test_diagnostics_include_structure_and_nonfinite_rejection(self):
        result = bench.numerical_difference(dict(x=[1.0, 3.0], old=True), dict(x=[1.5], new=True))
        self.assertFalse(result["exact"])
        self.assertEqual(result["max_abs_error"], 0.5)
        self.assertEqual(set(result["changed_structure_or_values"]), {"result.x.length", "result.old", "result.new"})
        with self.assertRaisesRegex(ValueError, "Nonfinite"):
            bench.numerical_difference([float("nan")], [0.0])

    def test_mismatch_is_saved_before_failure_and_can_be_explicitly_screened(self):
        for allow in (False, True):
            with self.subTest(allow=allow), tempfile.TemporaryDirectory() as temp:
                root = Path(temp)
                fixture(root)
                a = options(root, allow_numerical_differences=allow)
                baseline, candidate = worker_result(), worker_result()
                baseline["dataset_manifest_sha256"] = candidate["dataset_manifest_sha256"] = bench.digest(
                    root / "dataset.json"
                )
                candidate.update(weights_sha256="different", losses=[1.0, 2.5])
                with patch.object(bench, "run_worker", side_effect=[baseline, candidate]):
                    if allow:
                        bench.benchmark(a)
                    else:
                        with self.assertRaisesRegex(ValueError, "Numerical mismatch"):
                            bench.benchmark(a)
                report = json.loads(a.out.read_text())
                self.assertEqual(report["status"], "numerical_differences_allowed" if allow else "failed")
                self.assertEqual(report["raw"][0]["comparison"]["losses"]["max_abs_error"], 0.5)
                self.assertEqual(report["summary"]["32"]["exact_pairs"], 0)

    def test_different_sample_order_cannot_be_allowed(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            fixture(root)
            a = options(root, allow_numerical_differences=True)
            baseline, candidate = worker_result(), worker_result()
            baseline["dataset_manifest_sha256"] = candidate["dataset_manifest_sha256"] = bench.digest(
                root / "dataset.json"
            )
            candidate["sample_ids_sha256"] = "different"
            with patch.object(bench, "run_worker", side_effect=[baseline, candidate]):
                with self.assertRaisesRegex(ValueError, "Sampled IDs or dataset identity changed"):
                    bench.benchmark(a)
            self.assertEqual(json.loads(a.out.read_text())["status"], "failed")

    def test_manifest_changed_for_both_workers_cannot_be_allowed(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            fixture(root)
            a = options(root, allow_numerical_differences=True)
            with patch.object(bench, "run_worker", side_effect=[worker_result(), worker_result()]):
                with self.assertRaisesRegex(ValueError, "Dataset identity changed during benchmark"):
                    bench.benchmark(a)
            self.assertEqual(json.loads(a.out.read_text())["status"], "failed")

    def test_summary_uses_paired_ratios_and_supported_variability(self):
        rows = []
        for base, candidate in ((1.0, 4.0), (10.0, 5.0), (2.0, 2.0)):
            left, right = worker_result(), worker_result()
            left["train_s"], right["train_s"] = base, candidate
            rows.append(dict(width=32, results=dict(baseline=left, candidate=right), comparison=dict(exact=True)))
        stats = bench.summarize(rows)["32"]["metrics"]["train_s"]
        self.assertEqual(stats["paired_ratio"]["median"], 1.0)
        self.assertNotEqual(stats["paired_ratio"]["median"], stats["candidate"]["median"] / stats["baseline"]["median"])
        self.assertEqual(stats["paired_ratio"]["n"], 3)
        self.assertIsNone(bench.distribution([1.0])["sample_sd"])

    def test_profile_and_artifacts_use_real_reference_with_unchanged_inputs(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            identity = fixture(root)
            before = {p.name: bench.digest(p) for p in root.iterdir()}
            a = options(root, profile=True, artifacts=True)
            bench.benchmark(a)
            report = json.loads(a.out.read_text())
            self.assertEqual(report["status"], "exact")
            self.assertEqual(report["summary"]["32"]["pairs"], 1)
            self.assertEqual(len(report["profiles"]), 2)
            for profiled in report["profiles"]:
                self.assertTrue(profiled["parity"]["exact"])
                training = profiled["result"]["profile"]["training"]
                self.assertFalse(training["unavailable"])
                self.assertEqual(training["callables"]["forward"]["calls"], 16)
                self.assertEqual(training["callables"]["backward"]["calls"], 16)
                self.assertGreaterEqual(training["callables"]["coalesce"]["calls"], 16)
                self.assertGreater(profiled["result"]["checkpoint_bytes"], 0)
            self.assertEqual(report["dataset"], identity)
            for name, sha in before.items():
                self.assertEqual(bench.digest(root / name), sha)
            self.assertEqual(set(p.name for p in root.iterdir()), set(before) | {"result.json"})

    def test_sharded_input_and_corruption_rejection(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            identity = fixture(root)
            shard = root / "features.bin"
            identity["feature_shards"] = [dict(path=str(shard), sha256=identity.pop("features_sha256"))]
            (root / "dataset.json").write_text(json.dumps(identity))
            a = options(root)
            result = bench.run_worker(a, Path(bench.__file__).parent, 32, a.seed, 1)
            self.assertEqual(result["train_samples"], 16)
            shard.write_bytes(shard.read_bytes() + b"\x00\x00\x00\x00")
            with self.assertRaises(subprocess.CalledProcessError) as caught:
                bench.run_worker(a, Path(bench.__file__).parent, 32, a.seed, 1)
            self.assertIn("Feature shard changed", caught.exception.stderr)

    def test_worker_failure_is_saved_and_existing_output_is_preserved(self):
        for error, status in ((subprocess.TimeoutExpired("worker", 1), "failed"), (KeyboardInterrupt(), "interrupted")):
            with self.subTest(status=status), tempfile.TemporaryDirectory() as temp:
                root = Path(temp)
                fixture(root)
                a = options(root)
                with patch.object(bench, "run_worker", side_effect=error):
                    with self.assertRaises(type(error)):
                        bench.benchmark(a)
                report = json.loads(a.out.read_text())
                self.assertEqual(report["status"], status)
                before = a.out.read_bytes()
                with self.assertRaises(FileExistsError):
                    bench.benchmark(a)
                self.assertEqual(a.out.read_bytes(), before)


if __name__ == "__main__":
    unittest.main()
