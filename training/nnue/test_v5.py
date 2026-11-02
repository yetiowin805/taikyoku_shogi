"""Leakage, labels, and the single controlled variable in the v5 experiment."""

import copy
import hashlib
import json
import os
import sqlite3
import subprocess
import sys
import tempfile
import unittest
from collections import Counter
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

import numpy as np
import torch

import dataset_storage
import v5_weights
from difficulty_weights import multiplier
from pilot_fit import PilotNet, from_quantized, weights_for
from plateau import Plateau, Policy
from recent_data import Weights, assign_groups, classify_positions, label_position
from test_support import integration
from train import export
from v5_snapshot import snapshot
from v5_weights import STRATA, mixture_weights


class V5Tests(unittest.TestCase):
    def test_mixes_preserve_within_stratum_weights_and_holdout(self):
        samples = [dict(stratum=s, sampling_weight=w) for s in STRATA for w in (1, 3)]
        original = copy.deepcopy(samples)
        weights = weights_for(samples, list(range(6)))
        for proportions in ((0.90, 0.08, 0.02), (0.85, 0.12, 0.03), (0.95, 0.04, 0.01)):
            result = mixture_weights(samples, weights, proportions)
            totals = Counter()
            for i, w in result.items():
                totals[samples[i]["stratum"]] += w
            for s, share in zip(STRATA, proportions, strict=True):
                self.assertAlmostEqual(totals[s] / sum(result.values()), share)
            for i in (0, 2, 4):
                self.assertAlmostEqual(result[i + 1] / result[i], 3)
        self.assertEqual(samples, original)
        self.assertEqual(weights_for(samples, list(range(6))), weights)
        with self.assertRaises(ValueError):
            mixture_weights(samples, {0: 1}, (0.90, 0.08, 0.02))

    def test_dense_weights_balance_related_games_and_episodes(self):
        samples = []
        for st in STRATA:
            for game, count in [("a", 10), ("b", 2)]:
                samples.extend(
                    dict(split="train", stratum=st, group="g", game_sha256=game, episode="ep") for _ in range(count)
                )
        weights = Weights(dict(zip(STRATA, (0.90, 0.08, 0.02), strict=True)))
        for s in samples:
            weights.add(s)
        totals = Counter()
        for s in samples:
            totals[s["stratum"], s["game_sha256"]] += weights.weight(s)
        for st, share in zip(STRATA, (0.90, 0.08, 0.02), strict=True):
            self.assertAlmostEqual(totals[st, "a"], share / 2)
            self.assertAlmostEqual(totals[st, "b"], share / 2)

    def test_parent_exposure_pairs_and_conflicting_prefixes(self):
        with tempfile.TemporaryDirectory() as temp:
            p = Path(temp) / "parent.jsonl"
            p.write_text(
                "".join(
                    json.dumps(dict(game_sha256=h, group=h, split=sp, canonical_position_hash=h)) + "\n"
                    for h, sp in [("a", "train"), ("c", "test")]
                )
            )
            records = {
                h: dict(prefix=prefix, pair_seed=pair, run="run")
                for h, prefix, pair in [("a", "p", 1), ("b", "q", 1), ("c", "r", 2), ("d", "r", 3)]
            }
            self.assertEqual(assign_groups(records, p), {"a"})
            self.assertEqual(records["a"]["group"], records["b"]["group"])
            self.assertEqual(records["c"]["group"], records["d"]["group"])
            self.assertEqual(records["d"]["split"], "test")
            records["c"]["prefix"] = "p"
            assign_groups(records, p)
            self.assertTrue(all(records[h]["quarantine"] for h in "abc"))

    def test_mates_are_claims_precursors_keep_finite_targets(self):
        game = dict(
            moves=[dict(color="Black" if i % 2 == 0 else "White", eval=200 if i < 140 else 1000000) for i in range(142)]
        )
        selected, _ = classify_positions(game, "g", {}, Counter())
        self.assertEqual(selected[0]["stratum"], "representative")
        self.assertEqual(selected[139]["stratum"], "precursor")
        self.assertEqual(selected[117]["stratum"], "precursor")  # 23 plies before recognition
        self.assertEqual(selected[116]["stratum"], "representative")  # 24 plies
        self.assertEqual(selected[76]["stratum"], "representative")  # 64 plies
        self.assertNotIn("plies_before_mate", selected[116])
        self.assertEqual(len(selected), len(game["moves"]))
        self.assertLess(selected[139]["target_probability"], 0.5)
        self.assertEqual(selected[140]["stratum"], "mate")
        self.assertEqual(selected[140]["mate_status"], "search_claim")
        black = label_position(dict(color="Black", eval=200), [])
        white = label_position(dict(color="White", eval=200), [])
        self.assertAlmostEqual(black["target_probability"] + white["target_probability"], 1)

    def test_snapshot_filters_teacher_identity_and_freezes_completed_games(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            run = root / "run"
            (run / "analysis").mkdir(parents=True)
            parent = root / "parent.json"
            parent.write_text("{}")
            sha = hashlib.sha256(parent.read_bytes()).hexdigest()
            game = dict(
                result="BlackWins",
                black=dict(name="ab", model="winner.json"),
                white=dict(name="ab", model="loser.json"),
                moves=[dict(color=c, eval=e) for c, e in [("White", -100), ("Black", 200), ("White", -100)]],
            )
            path = root / "game.json"
            path.write_text(json.dumps(game))
            h = hashlib.sha256(path.read_bytes()).hexdigest()
            (run / "state.json").write_text(
                json.dumps(
                    dict(slots=[dict(status="done", game_path="game.json", start_seed=42), dict(status="running")])
                )
            )
            (run / "analysis/dual-label-config.json").write_text(
                json.dumps(dict(old=dict(models={str(root / "winner.json"): dict(sha256="winner")})))
            )
            db = sqlite3.connect(run / "analysis/training-labels.sqlite")
            db.execute("create table moments(id integer, status text, payload text)")
            searches = [
                dict(model_sha256=t, score=200, completed_depth=2, ply=2, game_hash=h) for t in [sha, "winner", "wrong"]
            ]
            db.execute(
                "insert into moments values(1,?,?)",
                ("completed", json.dumps(dict(game_hash=h, center_ply=2, searches=searches))),
            )
            db.commit()
            db.close()
            config = dict(out=str(root / "out"), run=str(run), repo=str(root), parent=str(parent))
            snapshot(config)
            result = json.loads((root / "out/snapshot/labels.jsonl").read_text())
            self.assertEqual({s["model_sha256"] for s in result["searches"]}, {sha, "winner"})
            (run / "state.json").write_text("{}")
            snapshot(config)  # immutable completed snapshot is reused
            self.assertEqual(len(json.loads((root / "out/snapshot/snapshot.json").read_text())["games"]), 1)

    def fixture(self, root):
        schema = dict(features=16, channels=92, hash="00" * 32, names_hash="11" * 32)
        (root / "schema.json").write_text(json.dumps(schema))
        np.tile(np.array([0, 1, 2, 3], dtype="<u4"), 60).tofile(root / "features.bin")
        samples = [
            dict(
                offset=i * 4,
                us=2,
                them=2,
                score=(i % 7 - 3) * 500,
                material=100,
                outcome=float(i % 2),
                group=str(i // 20),
                game="game-" + str(i // 20),
                split="train" if i < 20 else "validation" if i < 40 else "test",
                source="recent",
                label_source="saved",
                selection="representative",
                teacher="NNUE",
                stratum=STRATA[i % 3],
                episode="episode",
                sampling_weight=1 / 20,
                target_probability=0.5,
                inherited_holdout=False,
            )
            for i in range(60)
        ]
        (root / "samples.jsonl").write_text("".join(json.dumps(s) + "\n" for s in samples))

        def sha(p):
            return hashlib.sha256(p.read_bytes()).hexdigest()

        identity = dict(
            scale=2822.9057434005167,
            samples_sha256=sha(root / "samples.jsonl"),
            schema_sha256=sha(root / "schema.json"),
            feature_shards=[dict(path=str(root / "features.bin"), sha256=sha(root / "features.bin"))],
        )
        (root / "dataset.json").write_text(json.dumps(identity))
        torch.set_num_threads(1)
        model_sha = export(PilotNet(16, 32), root / "parent.bin", schema)
        (root / "parent.json").write_text(
            json.dumps(
                dict(
                    weights=dict(
                        nnue=dict(file="parent.bin", sha256=model_sha, feature_hash=schema["names_hash"], width=32)
                    )
                )
            )
        )
        (root / "cpus.json").write_text(json.dumps(dict(cpus=[min(os.sched_getaffinity(0))])))

    def test_preparation_preserves_holdouts_and_reuses_scoring(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            self.fixture(root)
            options = SimpleNamespace(
                source=root, parent=root / "parent.json", out=root / "prepared", cpu_file=root / "cpus.json"
            )
            original = (root / "samples.jsonl").read_bytes()
            with (
                patch.object(torch, "set_num_interop_threads"),
                patch.object(os, "sched_setaffinity"),
                patch("pilot_fit.from_quantized", wraps=from_quantized) as load,
            ):
                v5_weights.prepare(options)
                view = options.out / "sample-view.json"
                before = view.read_bytes()
                scored = (options.out / "difficulty.jsonl").read_bytes()
                v5_weights.prepare(options)
                self.assertEqual(load.call_count, 1)
                self.assertEqual(view.read_bytes(), before)
                self.assertEqual((options.out / "difficulty.jsonl").read_bytes(), scored)
                actual = list(dataset_storage.rows(options.out / "samples.jsonl"))
                samples = [json.loads(line) for line in original.splitlines()]
                self.assertEqual(actual[20:], samples[20:])
                self.assertTrue(all("parent_probability_error" in row for row in actual[:20]))
                self.assertFalse((options.out / "samples.jsonl").exists())
                self.assertEqual((root / "samples.jsonl").read_bytes(), original)
                view.write_bytes(b"changed")
                with self.assertRaisesRegex(ValueError, "Prepared samples changed"):
                    v5_weights.prepare(options)
                view.write_bytes(before)
                parent = json.loads(options.parent.read_text())
                parent["name"] = "changed"
                options.parent.write_text(json.dumps(parent))
                with self.assertRaisesRegex(ValueError, "Preparation inputs changed"):
                    v5_weights.prepare(options)

    @integration
    def test_shared_preparation_and_warm_started_cli_run(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            self.fixture(root)
            directory = Path(__file__).parent
            command = [
                sys.executable,
                str(directory / "v5_weights.py"),
                "--source",
                str(root),
                "--parent",
                str(root / "parent.json"),
                "--out",
                str(root / "prepared"),
                "--cpu-file",
                str(root / "cpus.json"),
            ]
            policy = root / "archive-policy.json"
            policy.write_text(
                json.dumps(
                    dict(root=str(root / "archive"), remote=str(root / "remote"), local_remote=True, min_free_bytes=0)
                )
            )
            environment = patch.dict(os.environ, TAIKYOKU_ARCHIVE_CONFIG=str(policy))
            environment.start()
            self.addCleanup(environment.stop)
            archive = dataset_storage.archive_storage.configured()
            archive.archive([root / "features.bin", root / "samples.jsonl"], "features")
            archive.evict([root / "features.bin", root / "samples.jsonl"])
            self.assertFalse((root / "features.bin").exists())
            subprocess.run(command, check=True, capture_output=True, timeout=60)
            self.assertTrue((root / "features.bin").exists())
            self.assertFalse((root / "prepared/samples.jsonl").exists())
            out = root / "trained"
            subprocess.run(
                [
                    sys.executable,
                    str(directory / "pilot_fit.py"),
                    str(root / "prepared"),
                    "--parent",
                    str(root / "parent.json"),
                    "--out",
                    str(out),
                    "--init",
                    "parent",
                    "--width",
                    "32",
                    "--loss",
                    "wdl",
                    "--epochs",
                    "4",
                    "--epoch-samples",
                    "8",
                    "--batch",
                    "8",
                    "--microbatch",
                    "1",
                    "--stratum-proportions",
                    ".90,.08,.02",
                ],
                check=True,
                capture_output=True,
                timeout=60,
            )
            initial = json.loads((out / "metrics.json").read_text())[0]["validation"]
            self.assertEqual(json.loads((out / "status.json").read_text())["state"], "completed")
            self.assertEqual(set(initial["by_stratum"]), set(STRATA))

    def test_smooth_difficulty_and_unlimited_plateau(self):
        self.assertEqual(multiplier(0), 1)
        self.assertEqual(multiplier(0.35), 2)
        self.assertEqual(multiplier(0.9), 2)
        policy = Policy(min_epochs=4, max_epochs=None, patience=2, lr_reductions=1)
        tracker = Plateau(1)
        for epoch in range(1, 101):
            self.assertEqual(tracker.observe(epoch, 0.9**epoch, policy), "continue")
        decisions = [tracker.observe(epoch, 0.9**100, policy) for epoch in range(101, 105)]
        self.assertEqual(decisions, ["continue", "reduce_lr", "continue", "plateau"])


if __name__ == "__main__":
    unittest.main()
