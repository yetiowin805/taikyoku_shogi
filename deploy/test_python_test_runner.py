"""Scheduling and real worker failure/coverage checks without importing PyTorch."""

import importlib.util
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("run_python_tests.py")
spec = importlib.util.spec_from_file_location("python_test_runner", SCRIPT)
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class RunnerTests(unittest.TestCase):
    def test_new_modules_and_sparse_cpu_ids_are_partitioned_once(self):
        modules = {"test_continuation", "test_benchmark_training", "test_finetune", "test_v5", "test_new"}
        groups = runner.plan(modules, [3, 7, 11, 19], 4, isolate_cpus=True)
        self.assertEqual(sorted(m for names, _ in groups for m in names), sorted(modules))
        self.assertEqual(sorted(cpu for _, cpus in groups for cpu in cpus), [3, 7, 11, 19])
        self.assertTrue(all(names and cpus for names, cpus in groups))
        self.assertEqual(len(runner.plan(modules, [7], 4)), 1)
        self.assertTrue(all(cpus == [3, 7, 11, 19] for _, cpus in runner.plan(modules, [3, 7, 11, 19], 4)))
        with self.assertRaises(ValueError):
            runner.plan(modules, [7], 0)

    def test_missing_duplicate_unexpected_and_unexecuted_tests_are_rejected(self):
        for ids, count in [(["a"], 1), (["a", "a"], 2), (["a", "c"], 2), (["a", "b"], 1)]:
            with self.subTest(ids=ids, count=count), self.assertRaises(ValueError):
                runner.verify(["a", "b"], [dict(ids=ids, count=count)])
        runner.verify(["a", "b"], [dict(ids=["b"], count=1), dict(ids=["a"], count=1)])

    def fixture(self, root, name, body):
        (root / (name + ".py")).write_text("import os\nimport unittest\nclass Case(unittest.TestCase):\n" + body)

    def run_fixture(self, root, *args):
        return subprocess.run(
            [sys.executable, str(SCRIPT), str(root), *args],
            cwd=root,
            capture_output=True,
            text=True,
            timeout=15,
        )

    def test_workers_cover_new_modules_and_propagate_any_failure(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.fixture(root, "test_alpha", "    def test_pass(self):\n        self.assertTrue(True)\n")
            self.fixture(root, "test_new", "    def test_failure(self):\n        self.fail('sentinel failure')\n")
            result = self.run_fixture(root)
            self.assertNotEqual(result.returncode, 0, result.stdout)
            self.assertIn("sentinel failure", result.stdout)
            self.assertIn("Ran 2 tests", result.stdout)
            self.assertIn("failures=1, errors=0, skipped=0; coverage verified", result.stdout)
            self.assertIn("test_pass (test_alpha.Case.test_pass)", result.stdout)

    @unittest.skipUnless(hasattr(os, "sched_getaffinity"), "requires Linux CPU affinity")
    def test_workers_inherit_disjoint_affinity_and_single_worker_keeps_all_cpus(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for name in ("test_alpha", "test_beta"):
                output = root / (name + ".json")
                self.fixture(
                    root,
                    name,
                    "    def test_affinity(self):\n"
                    "        import json\n"
                    "        from pathlib import Path\n"
                    f"        Path({str(output)!r}).write_text(json.dumps(sorted(os.sched_getaffinity(0))))\n",
                )
            result = self.run_fixture(root, "--jobs", "2", "--isolate-cpus")
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            cpus = [json.loads((root / (name + ".json")).read_text()) for name in ("test_alpha", "test_beta")]
            allowed = os.sched_getaffinity(0)
            self.assertEqual(set(cpus[0]) | set(cpus[1]), allowed)
            if len(allowed) > 1:
                self.assertFalse(set(cpus[0]) & set(cpus[1]))
            result = self.run_fixture(root, "--jobs", "1")
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            for name in ("test_alpha", "test_beta"):
                self.assertEqual(set(json.loads((root / (name + ".json")).read_text())), allowed)

    def test_discovery_errors_and_empty_suites_are_fatal(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            result = self.run_fixture(root)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("No tests discovered", result.stderr)
            (root / "test_broken.py").write_text("raise RuntimeError('broken discovery')\n")
            result = self.run_fixture(root)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("Test discovery failed", result.stderr)
            self.assertIn("broken discovery", result.stderr)

    def test_worker_crash_without_a_result_is_fatal(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.fixture(root, "test_crash", "    def test_crash(self):\n        os._exit(23)\n")
            result = self.run_fixture(root)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("worker exited 23 without a result", result.stderr)

    def test_suite_selection_runs_marked_tests_without_skipping_or_hiding_failures(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.fixture(
                root,
                "test_groups",
                "    def test_unit(self):\n        pass\n"
                "    def test_process(self):\n        self.fail('integration sentinel')\n"
                "    test_process.test_suite = 'integration'\n",
            )
            all_ids = {test.id() for test in runner.discover(root)}
            unit_ids = {test.id() for test in runner.discover(root, "unit")}
            integration_ids = {test.id() for test in runner.discover(root, "integration")}
            self.assertEqual(all_ids, unit_ids | integration_ids)
            self.assertFalse(unit_ids & integration_ids)
            for suite, count, status in (("unit", 1, 0), ("integration", 1, 1), ("all", 2, 1)):
                with self.subTest(suite=suite):
                    # Exercise both direct execution and worker-side selection.
                    jobs = "1" if suite == "unit" else "2"
                    result = self.run_fixture(root, "--suite", suite, "--jobs", jobs)
                    self.assertEqual(result.returncode, status, result.stdout + result.stderr)
                    self.assertIn(f"Ran {count} tests", result.stdout)
                    self.assertIn("skipped=0; coverage verified", result.stdout)
            with self.assertRaisesRegex(ValueError, "No integration tests"):
                self.fixture(root, "test_only_unit", "    def test_unit(self):\n        pass\n")
                # A missing/typoed marker must not turn an intended integration run green.
                (root / "test_groups.py").unlink()
                runner.discover(root, "integration")

    def test_unknown_suite_marker_is_fatal(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.fixture(
                root,
                "test_bad_group",
                "    def test_bad(self):\n        pass\n    test_bad.test_suite = 'intergation'\n",
            )
            result = self.run_fixture(root, "--suite", "unit")
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("Unknown test suite", result.stderr)


if __name__ == "__main__":
    unittest.main()
