"""Pilot treatment, exposure, sampling and cutoff regression tests (run remotely)."""

import json
import tempfile
import unittest
from pathlib import Path

import torch

import v6_data as data
import v6_dataset as dataset
import v6_fit as fit
import v6_pipeline as pipeline
import v7_pilot
import v7_pilot_data as pilot_data
import v7_pilot_sampling as sampling


class PilotTests(unittest.TestCase):
    def test_clean_child_exit_is_not_enough_to_complete_a_stage(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "training").mkdir()
            status = root / "training/status.json"
            status.write_text(json.dumps(dict(state="stopped")))
            with self.assertRaisesRegex(ValueError, "without completing"):
                v7_pilot.stage_state(root, "training")
            status.write_text(json.dumps(dict(state="pilot_budget")))
            self.assertEqual(v7_pilot.stage_state(root, "training"), "pilot_budget")
            with self.assertRaises(FileNotFoundError):
                v7_pilot.stage_state(root, "diagnostics")
            (root / "report.json").write_text(json.dumps(dict(incomplete_runs=["E"])))
            self.assertEqual(v7_pilot.stage_state(root, "diagnostics"), "incomplete")

    def test_small_training_challenge_quota_retains_a_targeted_example(self):
        game = dict(
            result="BlackWins", moves=[dict(color="Black" if i % 2 == 0 else "White", eval=i * 10) for i in range(100)]
        )
        rows = data.select_roots(game, "game", "train", 1, representative=4, challenge=2, random_challenge_min=1)
        challenges = [r for r in rows if r["stratum"] == "challenge"]
        self.assertEqual(len(challenges), 2)
        self.assertEqual(sum(r["selection"] == "random_challenge" for r in challenges), 1)
        validation = data.select_roots(
            game, "game", "validation", 1, representative=4, challenge=2, random_challenge_min=1
        )
        self.assertTrue(all(r["selection"] == "random_challenge" for r in validation if r["stratum"] == "challenge"))

    def test_targets_and_fixed_material_task_matrix(self):
        logits = torch.tensor([0.3, -0.2])
        q = torch.tensor([0.6, 0.4])
        y = torch.tensor([1.0, float("nan")])
        c = torch.tensor([0.3, 2.0])
        expected = torch.nn.functional.binary_cross_entropy_with_logits(
            logits, torch.tensor([0.72, 0.4]), reduction="none"
        )
        torch.testing.assert_close(fit.objective(logits, q, y, c, "Ao"), expected)
        torch.testing.assert_close(
            fit.objective(logits, q, y, c, "Cu"), fit.objective(logits, q, y, torch.ones(2), "C")
        )
        torch.testing.assert_close(fit.objective(logits, q, y, c, "E"), fit.objective(logits, q, y, c, "A"))
        jobs = pipeline.tasks(dict(sensitivity=True, out="/pilot", parents={"256": "/parent"}, training_deadline=1000))
        self.assertEqual(len(jobs), 15)
        self.assertEqual(len({j["name"] for j in jobs}), 15)
        for job in jobs[1:]:
            self.assertEqual(job["dependencies"], ["prelude-256-A"])
            self.assertEqual(job["fit"]["pilot_windows"], 12)
            self.assertEqual(job["fit"]["pilot_probe_dataset"], "/pilot/dataset")
            self.assertTrue(job["fit"]["regime"] in ("A", "Ao", "E") or job["fit"]["retain_material"])

    def test_saved_multileg_move(self):
        move = pilot_data.saved_move(
            dict(
                from_file=1,
                from_rank=2,
                to_file=3,
                to_rank=4,
                promoted=True,
                data=dict(TwoStep=dict(intermediate_file=2, intermediate_rank=3)),
            )
        )
        self.assertEqual(move["data"], {"TwoStep": {"intermediate": {"file": 2, "rank": 3}}})
        self.assertTrue(move["promoted"])

    def test_group_selection_never_uses_test_or_quarantine(self):
        rows = {
            str(i): dict(id=str(i), group=str(i // 2), split="train" if i < 12 else "validation", quarantine=i == 12)
            for i in range(18)
        }
        rows["test"] = dict(id="test", group="test", split="test", quarantine=False)
        selected = pilot_data.select_games(rows, dict(seed=1, sensitivity=dict(train_groups=3, validation_groups=2)))
        self.assertEqual(len(selected), 5)
        self.assertEqual(len({r["group"] for r in selected.values()}), 5)
        self.assertNotIn("test", selected)
        self.assertNotIn("12", selected)

    def test_v6_parent_game_field_preserves_inherited_exposure(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "samples.jsonl"
            path.write_text(
                json.dumps(dict(game="sha", group="old", split="train", canonical_position_hash="feature")) + "\n"
            )
            db = data.connect(temp)
            self.addCleanup(db.close)
            prior = data.parent_index(db, [path])
            self.assertEqual(prior["sha"], {("old", "train")})
            self.assertEqual(db.execute("SELECT hash FROM parent_features").fetchone()[0], "feature")

    def test_cutoff_excludes_partial_families_and_does_not_hide_failure(self):
        with tempfile.TemporaryDirectory() as temp:
            db = data.connect(temp)
            self.addCleanup(db.close)
            root = dict(game_id="game", ply=1, path=[], group="group", split="train")
            key = data.add_position(db, root)
            data.add_job(db, key, "primary", "broad", 10, 1)
            data.set_meta(db, "initialized", True)
            pilot_data.cutoff(db, {})
            self.assertIn(key, data.get_meta(db, "pilot_cutoff")["excluded_roots"])
            self.assertEqual(db.execute("SELECT status FROM jobs").fetchone()[0], "cancelled")
            db.execute("UPDATE jobs SET status='failed'")
            self.assertFalse(dataset.completed(db))

    def test_adaptive_mass_bounds_and_resume(self):
        rows = [
            dict(
                group=str(i % 24),
                phase=i // 48,
                material=0,
                material_total=1000,
                legal_promotion=False,
                raw_confidence=1,
                sampling_weight=1 / 96,
            )
            for i in range(96)
        ]
        ids = list(range(len(rows)))
        initial = sampling.initialize(rows, ids, lambda _: 0.1)
        self.assertEqual(initial["updates"], 0)
        changed = sampling.update(initial, rows, ids, lambda indices: 0.5 if indices[0] < 48 else 0.001)
        self.assertGreater(changed["total_variation"], 0)
        weights = sampling.sampling_weights(initial, rows, ids)
        self.assertAlmostEqual(sum(weights), 1)
        self.assertTrue(all(0.5 / 96 <= w <= 2 / 96 for w in weights))
        resumed = json.loads(json.dumps(initial))
        self.assertEqual(sampling.sampling_weights(resumed, rows, ids), weights)
        sampling.update(initial, rows, ids, lambda _: 0.2)
        sampling.update(resumed, rows, ids, lambda _: 0.2)
        self.assertEqual(initial, resumed)

    def test_unsupported_cells_keep_base_mass(self):
        rows = [dict(group="one", phase=0, material=0, raw_confidence=1, sampling_weight=0.5) for _ in range(2)]
        state = sampling.initialize(rows, [0, 1], lambda _: 0.1)
        report = sampling.update(state, rows, [0, 1], lambda _: 0.9)
        self.assertEqual(report["supported_mass"], 0)
        self.assertEqual(sampling.sampling_weights(state, rows, [0, 1]), [0.5, 0.5])


if __name__ == "__main__":
    unittest.main()
