"""Refresh safety and incremental-posterior regressions."""

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

import generation_strength_refresh as refresh


def game(i=1, **changes):
    return dict(
        id=i,
        model_a="a",
        model_b="b",
        score_a=1.0,
        a_is_black=True,
        start_seed=4,
        start_mode="opening",
        status="done",
        **changes,
    )


def payload(games):
    return {
        "state": {"slots": games, "retired": []},
        "fit": {"retirement": {"generations": {"g": {"members": ["a", "b"]}}}},
    }


class RefreshTests(unittest.TestCase):
    def test_key_ignores_capture_time_and_unfinished_games(self):
        a = payload([game()])
        b = payload([game(), {**game(2), "status": "running"}])
        b["state"]["_checked_utc"] = "later"
        self.assertEqual(refresh.evidence_key(a), refresh.evidence_key(b))
        b["state"]["slots"][0]["score_a"] = 0.0
        self.assertNotEqual(refresh.evidence_key(a), refresh.evidence_key(b))

    def test_atomic_rejects_nonfinite_output_without_replacing_result(self):
        with tempfile.TemporaryDirectory() as folder:
            p = Path(folder) / "result.json"
            refresh.atomic(p, {"value": 1})
            with self.assertRaises(ValueError):
                refresh.atomic(p, {"value": float("nan")})
            self.assertEqual(json.loads(p.read_text()), {"value": 1})

    def test_convergence_gate(self):
        fit = {
            "divergences": 0,
            "max_tree_depth_hits": 0,
            "diagnostics": {"x": {"max_rhat": 1.01, "min_ess": 400}},
            "derived_diagnostics": {},
        }
        self.assertTrue(refresh.valid_fit(fit))
        fit["diagnostics"]["x"]["max_rhat"] = 1.1
        self.assertFalse(refresh.valid_fit(fit))

    def test_requests_coalesce_and_identical_results_skip_work(self):
        from argparse import Namespace
        from types import SimpleNamespace
        from unittest.mock import patch

        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            snapshot = root / "input.json"
            snapshot.write_text(json.dumps(payload([game()])))
            args = Namespace(
                work_dir=str(root / "work"),
                web_dir=str(root),
                snapshot=str(snapshot),
                baseline=None,
                baseline_snapshot=None,
            )
            with patch.object(refresh.subprocess, "Popen", return_value=SimpleNamespace(pid=123)) as start:
                with patch.object(refresh, "alive", return_value=False):
                    refresh.request(args)
                snapshot.write_text(json.dumps(payload([game(), game(2)])))
                with patch.object(refresh, "alive", return_value=True):
                    refresh.request(args)
                self.assertEqual(start.call_count, 1)
                queued = json.loads((root / "work/requested.json").read_text())
                self.assertEqual(len(queued["state"]["slots"]), 2)
                refresh.atomic(root / "generation-strength-live.json", {"evidence_key": refresh.evidence_key(queued)})
                with patch.object(refresh, "alive", return_value=False):
                    refresh.request(args)
                self.assertEqual(start.call_count, 1)

    def test_worker_failure_preserves_last_accepted_report(self):
        from argparse import Namespace
        from unittest.mock import patch

        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            refresh.atomic(root / "config.json", {"web": str(root)})
            refresh.atomic(root / "requested.json", payload([game()]))
            refresh.atomic(root / "generation-strength-live.json", {"old": True})
            with (
                patch.object(refresh, "compute", side_effect=RuntimeError("fit did not converge")),
                patch.object(refresh.os, "nice"),
                patch.object(refresh.os, "sched_setaffinity"),
            ):
                with self.assertRaisesRegex(RuntimeError, "did not converge"):
                    refresh.worker(Namespace(work_dir=str(root)))
            self.assertEqual(json.loads((root / "generation-strength-live.json").read_text()), {"old": True})
            self.assertEqual(json.loads((root / "generation-strength-status.json").read_text())["phase"], "failed")
            self.assertIsNone(json.loads((root / "worker.json").read_text())["pid"])

    def test_install_hook_is_idempotent_and_preserves_venv_path(self):
        from dashboard.install_refresh_hook import install

        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            path = root / "finish_generation_dashboard.py"
            original = '    print("Published", payload["deployment"]["stage"], state["_checked_utc"], flush=True)'
            path.write_text(original)
            python = root / "venv/bin/python"
            install(root, root / "work", python)
            once = path.read_text()
            install(root, root / "work", python)
            self.assertEqual(path.read_text(), once)
            self.assertIn(str(python), once)
            self.assertIn(original, once)


@unittest.skipUnless(
    importlib.util.find_spec("numpyro") and importlib.util.find_spec("arviz_stats"),
    "requires optional analysis dependencies",
)
class ImportanceTests(unittest.TestCase):
    def setUp(self):
        import numpy as np

        self.np = np
        self.samples = {
            "theta": np.zeros((2, 500, 2)),
            "black": np.full((2, 500), np.log(3)),
            "log_nu": np.full((2, 500), -40.0),
            "sigma_open": np.zeros((2, 500)),
        }
        self.design = {"names": ["a", "b"]}

    def test_color_pair_likelihood_and_mask(self):
        from generation_strength import observations

        pair = [game(), {**game(2), "a_is_black": False, "score_a": 0.0}]
        obs, _ = observations(pair, self.design["names"])
        actual = refresh.block_log_likelihood(self.samples, obs)
        self.np.testing.assert_allclose(actual, self.np.log(0.75 * 0.75))
        obs["mask"][0, 1] = 0
        self.np.testing.assert_allclose(refresh.block_log_likelihood(self.samples, obs), self.np.log(0.75))

    def test_update_accounts_for_partial_pair_deletions_and_corrections(self):
        from unittest.mock import patch

        from scipy.special import logsumexp

        self.samples["theta"][:, :, 0] = self.np.linspace(-0.5, 0.5, 1000).reshape(2, 500)
        self.samples["sigma_open"][:] = 0.6
        old = [game()]
        new = [game(), {**game(2), "a_is_black": False, "score_a": 0.0}]
        from generation_strength import observations

        for before, after in [(old, new), (new, old), (old, [{**game(), "score_a": 0.0}])]:
            a, _ = observations(before, self.design["names"])
            b, _ = observations(after, self.design["names"])
            exact = refresh.block_log_likelihood(self.samples, b) - refresh.block_log_likelihood(self.samples, a)
            with patch("arviz_stats.base.array_stats.psislw", side_effect=lambda x, **kw: (x, 0.1)):
                weights, _, accepted = refresh.importance(self.samples, before, after, self.design)
            self.assertTrue(accepted)
            self.np.testing.assert_allclose(weights, self.np.exp(exact - logsumexp(exact)))

    def test_unchanged_evidence_uses_uniform_weights(self):
        weights, diagnostics, accepted = refresh.importance(self.samples, [game()], [game()], self.design)
        self.assertTrue(accepted)
        self.assertEqual(diagnostics["changed_blocks"], 0)
        self.np.testing.assert_allclose(weights, 0.001)

    def test_bad_importance_diagnostics_reject_update(self):
        from unittest.mock import patch

        self.samples["theta"][:, :, 0] = self.np.linspace(-1, 1, 1000).reshape(2, 500)
        with patch("arviz_stats.base.array_stats.psislw", side_effect=lambda x, **kw: (x, 0.8)):
            _, _, accepted = refresh.importance(self.samples, [], [game()], self.design)
        self.assertFalse(accepted)
