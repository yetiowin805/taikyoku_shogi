import importlib.util
import math
import signal
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("benchmark_search", Path(__file__).with_name("benchmark_search.py"))
benchmark = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(benchmark)


def row(position, ratio, model=0):
    baseline = {"cpu_ms": 10.0, "ms": 10.0, "peak_rss_kib": 1024, "nodes": 7, "q_nodes": 3}
    candidate = {**baseline, "cpu_ms": 10.0 * ratio, "ms": 10.0 * ratio}
    return {
        "position": position,
        "model": model,
        "rep": 0,
        "measurements": {"baseline": baseline, "candidate": candidate},
    }


class BootstrapTests(unittest.TestCase):
    def test_unbalanced_grids_preserve_equal_case_weighting_in_resamples(self):
        rows = [row(0, 4.0), row(1, 1.0), row(1, 1.0, model=1)]
        positions = [{"game": "a"}, {"game": "b"}]
        # Drawing every cluster once must reproduce the original estimate,
        # including when one position has more model cases than another.
        rng = SimpleNamespace(choices=lambda population, k: list(population))
        with patch.object(benchmark.random, "Random", return_value=rng):
            result = benchmark.summarize(rows, "cpu_ms", positions, bootstrap=40)
        expected = math.pow(4.0, 1.0 / 3.0)
        self.assertAlmostEqual(result["candidate_over_baseline"], expected)
        for key in ("source_game_cluster_bootstrap_95", "position_cluster_bootstrap_95"):
            self.assertAlmostEqual(result[key][0], expected)
            self.assertAlmostEqual(result[key][1], expected)

    def test_shared_source_game_roots_stay_in_one_primary_cluster(self):
        rows = [row(0, 4.0), row(1, 4.0), row(2, 1.0)]
        positions = [
            {"game": "root-a", "source_game": "shared"},
            {"game": "root-b", "source_game": "shared"},
            {"game": "independent"},
        ]
        sample_sizes = []

        def draw(population, k):
            sample_sizes.append(k)
            return list(population)

        with patch.object(benchmark.random, "Random", return_value=SimpleNamespace(choices=draw)):
            result = benchmark.summarize(rows, "cpu_ms", positions, bootstrap=40)
        self.assertEqual(result["positions"], 3)
        self.assertEqual(result["source_games"], 2)
        self.assertEqual(sample_sizes, [2] * 40 + [3] * 40)
        self.assertAlmostEqual(result["candidate_over_baseline"], math.pow(4.0, 2.0 / 3.0))

    def test_single_cluster_intervals_are_omitted(self):
        positions = [{"game": "root-a", "source_game": "shared"}, {"game": "root-b", "source_game": "shared"}]
        result = benchmark.summarize([row(0, 2.0), row(1, 4.0)], "cpu_ms", positions, bootstrap=40)
        self.assertNotIn("source_game_cluster_bootstrap_95", result)
        self.assertIn("position_cluster_bootstrap_95", result)
        one = benchmark.summarize([row(0, 2.0)], "cpu_ms", positions, bootstrap=40)
        self.assertNotIn("source_game_cluster_bootstrap_95", one)
        self.assertNotIn("position_cluster_bootstrap_95", one)
        self.assertTrue(one["nodes_include_q_nodes"])


class InputTests(unittest.TestCase):
    def test_all_loaded_roots_are_verified_and_shared_games_hashed_once(self):
        with tempfile.TemporaryDirectory() as directory:
            selected = Path(directory) / "selected.json"
            unselected = Path(directory) / "unselected.json"
            selected.write_text("selected")
            unselected.write_text("unselected")
            positions = [
                {"game": str(path), "sha256": benchmark.sha(path)} for path in (selected, unselected, selected)
            ]
            with patch.object(benchmark, "sha", wraps=benchmark.sha) as hashed:
                benchmark.verify_games(positions)
                self.assertEqual(hashed.call_count, 2)
            unselected.write_text("changed")
            with self.assertRaisesRegex(ValueError, "game hash mismatch"):
                benchmark.verify_games(positions)


class TerminationTests(unittest.TestCase):
    def test_sigterm_unwinds_and_closes_both_servers(self):
        closed = []

        class Server:
            def __init__(self, arm, _label, _cpu):
                self.arm = arm

            def request(self, _case):
                benchmark.terminate(signal.SIGTERM, None)

            def close(self):
                closed.append(self.arm)

        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory)
            (out / "bin").mkdir()
            candidate = out / "candidate-binary"
            candidate.touch()
            args = SimpleNamespace(out=out, cpu=0, seed=1, sequential=False)
            corpus = {"models": [{"name": "fixture"}], "positions": [{"game": "fixture"}]}
            with patch.object(benchmark.HARNESS, "Server", Server):
                with self.assertRaises(SystemExit) as stopped:
                    benchmark.run_stage(args, [{"position": 0, "model": 0, "depth": 1}], corpus, "test", 2, candidate)
            self.assertEqual(stopped.exception.code, 128 + signal.SIGTERM)
        self.assertEqual(closed, ["baseline", "candidate"])


if __name__ == "__main__":
    unittest.main()
