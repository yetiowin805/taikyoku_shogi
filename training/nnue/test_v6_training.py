"""v6 objective, export, sampling, gate, and eight-run recovery regressions."""

import json
import os
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path
from unittest.mock import patch

import numpy as np
import torch

import v6_data as data
import v6_dataset as dataset
import v6_fit as fit
import v6_pipeline as pipeline
from pilot_fit import PilotNet
from quantized import Quantized
from test_support import integration
from train import export


class TrainingTests(unittest.TestCase):
    def test_selected_campaign_uses_original_a_and_distinct_material_runs(self):
        config = dict(out="/training", parents={"384": "/p384", "512": "/p512"})
        self.assertEqual(len(pipeline.tasks(config)), 12)
        names = ["NNUE_W512_v6A", "NNUE_W384_v6Cm", "NNUE_W384_v6Dm", "NNUE_W512_v6Cm"]
        jobs = pipeline.tasks(dict(config, training_models=names))
        self.assertEqual([j["name"] for j in jobs], ["prelude-512-A", names[0], "prelude-384-A", *names[1:]])
        for job in jobs:
            if job["name"].endswith("m"):
                self.assertTrue(job["fit"]["retain_material"])
                self.assertTrue(job["fit"]["start"].endswith("-A/start.pt"))
        original = next(j for j in pipeline.tasks(config) if j["name"] == names[0])
        self.assertEqual(jobs[1], original)
        for invalid in ([], names + [names[0]], ["NNUE_W512_v6Bm"], ["prelude-512-A"]):
            with self.assertRaises(ValueError):
                pipeline.tasks(dict(config, training_models=invalid))

    def test_confident_error_has_correct_gradient_in_all_regimes(self):
        for regime in "ABCD":
            value = torch.tensor([12.0], requires_grad=True)
            loss = fit.objective(value, torch.tensor([0.01]), torch.tensor([float("nan")]), torch.tensor([1.0]), regime)
            loss.sum().backward()
            self.assertGreater(value.grad.item(), 0.05)
        value = torch.tensor([0.0])
        plain = fit.objective(value, torch.tensor([0.2]), torch.tensor([float("nan")]), torch.tensor([1.0]), "B")
        mixed = fit.objective(value, torch.tensor([0.2]), torch.tensor([1.0]), torch.tensor([1.0]), "B")
        self.assertTrue(torch.isfinite(plain + mixed).all())

    def test_completion_requires_every_stage_and_no_failed_jobs(self):
        with tempfile.TemporaryDirectory() as tmp:
            db = data.connect(tmp)
            for key in ("initialized", "audit_planned", "deep_planned"):
                data.set_meta(db, key, {})
            self.assertFalse(dataset.completed(db))
            db.execute(
                "INSERT INTO jobs(id,position_id,teacher,tier,budget_ms,priority,status) VALUES('j','p','primary','broad',1,1,'pending')"
            )
            self.assertFalse(dataset.completed(db))
            db.execute("UPDATE jobs SET status='failed'")
            self.assertFalse(dataset.completed(db))
            db.execute("UPDATE jobs SET status='completed'")
            self.assertTrue(dataset.completed(db))
            db.execute("DELETE FROM meta WHERE key='deep_planned'")
            self.assertFalse(dataset.completed(db))
            db.close()

    def test_hierarchy_does_not_overweight_long_games(self):
        rows = [
            dict(split="train", stratum="representative", group="g", game=game, phase=0, confidence=1.0)
            for game in ["a"] * 10 + ["b"]
        ]
        rows.append(dict(split="train", stratum="challenge", group="h", game="c", phase=0, confidence=0.5))
        dataset.sampling_weights(rows)
        self.assertAlmostEqual(sum(r["sampling_weight"] for r in rows), 1)
        self.assertAlmostEqual(sum(r["sampling_weight"] for r in rows if r["game"] == "a"), rows[10]["sampling_weight"])
        self.assertAlmostEqual(rows[-1]["sampling_weight"], 0.2 / 0.95)

    def test_rank_orientation_and_stability(self):
        def row(turn, q, scores):
            return dict(
                parent="root",
                split="train",
                turn=turn,
                q=q,
                observations=[dict(family=f, budget=b, score=s) for f, b, s in scores],
            )

        a = row(-1, 0.2, [("p", 10, 3), ("p", 30, 4), ("c", 30, 3)])
        b = row(-1, 0.8, [("p", 10, -3), ("p", 30, -4), ("c", 30, -3)])
        pairs = dataset.ranking_pairs([a, b], 1)
        self.assertEqual(len(pairs), 1)
        self.assertGreater(pairs[0]["target"], 0.9)
        b["observations"][0]["score"] = 9
        self.assertEqual(dataset.ranking_pairs([a, b], 1), [])

    def test_total_export_flag_and_exact_quantized_parity(self):
        with tempfile.TemporaryDirectory() as tmp:
            net = PilotNet(16, 32)
            path = Path(tmp) / "net.bin"
            schema = dict(channels=92, hash="00" * 32)
            export(net, path, schema, output_mode="total")
            q = Quantized(path, 16)
            self.assertEqual(q.output_mode, "total")
            x = torch.tensor([1, 2, 3, 4])
            off = torch.tensor([0, 2, 4])
            self.assertLessEqual(abs(q.residual([1, 2], [3, 4]) - net(x, off).item() * 1000), 1)

    def test_integer_activation_ties_match_exports_at_both_production_widths(self):
        with tempfile.TemporaryDirectory() as tmp:
            for width in (32, 384, 512):
                torch.manual_seed(23)
                net = PilotNet(64, width)
                with torch.no_grad():
                    net.embedding.weight.uniform_(-0.03, 0.03)
                    for layer in (net.h1, net.h2):
                        layer.weight.copy_(torch.randint(-4, 5, layer.weight.shape) / 64)
                        layer.bias.copy_(torch.randint(0, 4000, layer.bias.shape) / 8128)
                    net.output.weight.uniform_(-2, 2)
                path = Path(tmp) / "network.bin"
                export(net, path, dict(channels=92, hash="00" * 32))
                q = Quantized(path, 64)
                for _ in range(64):
                    x = torch.randint(64, (64,))
                    off = torch.tensor([0, 32, 64])
                    expected = q.residual(x[:32].numpy(), x[32:].numpy())
                    with torch.no_grad():
                        actual = net(x, off).item() * 1000
                    self.assertLessEqual(abs(expected - actual), 0.51, (width, expected, actual))

    def test_batch_and_export_failure_resume_exact_optimizer_and_sampler(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "data"
            source.mkdir()
            schema = dict(channels=92, features=16, hash="00" * 32, names_hash="11" * 32)
            torch.manual_seed(23)
            net = PilotNet(16, 32)
            blob = root / "parent.bin"
            sha = export(net, blob, schema)
            parent = root / "parent.json"
            parent.write_text(
                json.dumps(
                    dict(
                        name="parent",
                        weights=dict(
                            piece={}, nnue=dict(file=blob.name, sha256=sha, width=32, feature_hash=schema["names_hash"])
                        ),
                    )
                )
            )
            np.array([1, 2, 3, 4], dtype="<u4").tofile(source / "features.bin")
            rows = [
                dict(
                    offset=0,
                    us=2,
                    them=2,
                    score=0,
                    material=0,
                    q=0.2 if i % 2 else 0.7,
                    outcome=None,
                    confidence=1,
                    turn=1,
                    split=split,
                    stratum="representative",
                    group=split,
                    game=split,
                    phase=0,
                    sampling_weight=1 / 8,
                )
                for split in ("train", "validation")
                for i in range(8)
            ]
            (source / "samples.jsonl").write_text("\n".join(json.dumps(r) for r in rows) + "\n")
            (source / "schema.json").write_text(json.dumps(schema))
            (source / "pairs.json").write_text("[]")
            (source / "dataset.json").write_text(
                json.dumps(
                    dict(
                        scale=dataset.SCALE,
                        files={
                            n: data.file_hash(source / n)
                            for n in ("features.bin", "samples.jsonl", "schema.json", "pairs.json")
                        },
                    )
                )
            )
            config = dict(
                dataset=str(source),
                parent=str(parent),
                out=str(root / "resumed"),
                width=32,
                regime="A",
                prelude=True,
                seed=9,
                threads=1,
                window_draws=16,
            )
            original = fit.objective
            calls = 0

            def interrupt(*args):
                nonlocal calls
                calls += 1
                if calls == 8:
                    fit.STOP = True
                return original(*args)

            with patch.object(torch, "set_num_interop_threads"), patch.object(fit.signal, "signal"):
                with patch.object(fit, "objective", side_effect=interrupt), self.assertRaises(InterruptedError):
                    fit.fit(config)
                state = torch.load(root / "resumed/training.pt", weights_only=True)
                self.assertEqual(state["offset"], 8)
                fit.fit(config)
                fit.fit(dict(config, out=str(root / "uninterrupted")))
            a = torch.load(root / "resumed/start.pt", weights_only=True)
            b = torch.load(root / "uninterrupted/start.pt", weights_only=True)
            for key in a:
                self.assertTrue(torch.equal(a[key], b[key]), key)
            main_config = dict(config, prelude=False, start=str(root / "uninterrupted/start.pt"))
            main_config["out"] = str(root / "export-resumed")
            publish = fit.publish
            retry_pending = True

            def count_draws(*args):
                self.assertFalse(retry_pending, "replayed optimization before retrying failed export")
                return original(*args)

            def retry_export(*args):
                nonlocal retry_pending
                retry_pending = False
                return publish(*args)

            with patch.object(torch, "set_num_interop_threads"), patch.object(fit.signal, "signal"):
                with patch.object(fit, "publish", side_effect=ValueError("export failed")):
                    with self.assertRaisesRegex(ValueError, "export failed"):
                        fit.fit(main_config)
                saved = torch.load(root / "export-resumed/training.pt", weights_only=True)
                self.assertEqual((saved["window"], saved["offset"]), (0, 16))
                with (
                    patch.object(fit, "objective", side_effect=count_draws),
                    patch.object(fit, "publish", side_effect=retry_export),
                ):
                    fit.fit(main_config)
                fit.fit(dict(main_config, out=str(root / "export-uninterrupted")))
            a = torch.load(root / "export-resumed/training.pt", weights_only=True)["net"]
            b = torch.load(root / "export-uninterrupted/training.pt", weights_only=True)["net"]
            for key in a:
                self.assertTrue(torch.equal(a[key], b[key]), key)

    def test_label_votes_orientation_and_duplicate_split_quarantine(self):
        with tempfile.TemporaryDirectory() as tmp:
            db = data.connect(tmp)
            db.execute(
                "INSERT INTO games VALUES(?,?)",
                ("g", data.encoded(dict(path="game", result="BlackWins", outcome_eligible=True))),
            )
            for key, split, feature in [
                ("a", "train", "shared"),
                ("b", "validation", "shared"),
                ("c", "train", "unique"),
            ]:
                payload = dict(
                    id=key,
                    game_id="g",
                    group=split,
                    split=split,
                    phase=0,
                    stratum="representative",
                    ply=1,
                    path=[],
                    parent=None,
                )
                probe = dict(
                    turn=-1,
                    terminal=None,
                    canonical_position_hash=feature,
                    state_history_hash=key,
                    material_black=2,
                    range_two_movers=1,
                    feature_schema_hash="schema",
                )
                db.execute(
                    "INSERT INTO positions VALUES(?,?,?,?,?,?)",
                    (key, "g", 1, data.encoded(payload), data.encoded(probe), 1),
                )
                for family, budget, score in [("p", 10, 1000), ("p", 30, 3000), ("c", 30, 5000)]:
                    result = dict(
                        label=dict(
                            bound="completed_root_search", score_perspective="black-absolute", score_black=score
                        ),
                        teacher=dict(family=family),
                        budget_ms=budget,
                    )
                    db.execute(
                        "INSERT INTO jobs(id,position_id,teacher,tier,budget_ms,priority,status,result) VALUES(?,?,?,?,?,?,?,?)",
                        (
                            key + family + str(budget),
                            key,
                            "primary" if family == "p" else "challenger",
                            "broad",
                            budget,
                            0,
                            "completed",
                            data.encoded(result),
                        ),
                    )
            rows, report = dataset.select_rows(db)
            db.close()
            self.assertEqual(len(rows), 1)
            row = rows[0]
            self.assertEqual(row["outcome"], 0)
            self.assertEqual(row["material"], -2)
            self.assertAlmostEqual(row["q"], (data.probability(-3000) + data.probability(-5000)) / 2)
            self.assertEqual(report["excluded"]["feature_split_or_history_conflict"], 2)

    @integration
    def test_legacy_and_material_runs_handoff_and_idempotent_restart(self):
        allowed = set(os.sched_getaffinity(0))
        if not {1, 3, 4, 5, 6, 7} <= allowed:
            self.skipTest("integration requires configured Linux CPUs 1,3-7")
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            out = root / "training"
            source = out / "dataset"
            source.mkdir(parents=True)
            schema = dict(channels=92, features=16, hash="00" * 32, names_hash="11" * 32)
            parents = {}
            for width in (384, 512):
                net = PilotNet(16, width)
                for parameter in net.parameters():
                    parameter.data.zero_()
                blob = root / f"w{width}.bin"
                sha = export(net, blob, schema)
                model = root / f"w{width}.json"
                model.write_text(
                    json.dumps(
                        dict(
                            name="parent",
                            weights=dict(
                                piece={},
                                nnue=dict(file=blob.name, sha256=sha, width=width, feature_hash=schema["names_hash"]),
                            ),
                        )
                    )
                )
                parents[str(width)] = str(model)
            np.array([1, 2, 3, 4], dtype="<u4").tofile(source / "features.bin")
            rows = []
            for split in ("train", "validation", "test"):
                for _i in range(8):
                    rows.append(
                        dict(
                            offset=0,
                            us=2,
                            them=2,
                            score=0,
                            material=0,
                            q=0.5,
                            outcome=None,
                            confidence=1,
                            turn=1,
                            split=split,
                            stratum="representative",
                            group=split,
                            game=split,
                            phase=0,
                            sampling_weight=1 / 8,
                        )
                    )
            (source / "samples.jsonl").write_text("\n".join(json.dumps(r) for r in rows) + "\n")
            (source / "schema.json").write_text(json.dumps(schema))
            (source / "pairs.json").write_text(json.dumps([dict(a=0, b=1, target=0.5, weight=1, root="r")]))
            (source / "dataset.json").write_text(
                json.dumps(
                    dict(
                        scale=dataset.SCALE,
                        files={
                            n: data.file_hash(source / n)
                            for n in ("features.bin", "samples.jsonl", "schema.json", "pairs.json")
                        },
                    )
                )
            )
            collection = root / "collection"
            collection.mkdir()
            db = data.connect(collection)
            for key in ("initialized", "audit_planned", "deep_planned"):
                data.set_meta(db, key, {})
            db.execute(
                "INSERT INTO jobs(id,position_id,teacher,tier,budget_ms,priority,status) VALUES('j','p','primary','broad',1,1,'pending')"
            )
            db.commit()
            db.close()
            (collection / "status.json").write_text(json.dumps(dict(state="collecting")))
            control = root / "control"
            control.mkdir()
            config = dict(
                out=str(out),
                catalogue=str(collection / "catalogue.sqlite"),
                parents=parents,
                control=str(control),
                lease_indices=[7, 1, 3, 4, 5, 6],
                identities={},
                python=sys.executable,
                window_draws=8,
                training_models=[
                    f"NNUE_W{width}_v6{regime}" for width in (384, 512) for regime in ("A", "B", "C", "D", "Cm", "Dm")
                ],
            )
            config_path = root / "config.json"
            config_path.write_text(json.dumps(config))
            command = [sys.executable, str(Path(pipeline.__file__)), str(config_path)]
            process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            try:
                until = time.monotonic() + 10
                while not (out / "status.json").exists() and time.monotonic() < until:
                    time.sleep(0.05)
                self.assertEqual(json.loads((out / "status.json").read_text())["state"], "waiting_for_collection")
                self.assertFalse(list(out.glob("prelude-*")))
                db = data.connect(collection)
                db.execute("UPDATE jobs SET status='completed'")
                db.commit()
                db.close()
                (collection / "status.json").write_text(json.dumps(dict(state="completed")))
                _, errors = process.communicate(timeout=180)
                self.assertEqual(process.returncode, 0, errors.decode())
            finally:
                if process.poll() is None:
                    process.terminate()
                    process.communicate(timeout=30)
            ready = json.loads((out / "ready.json").read_text())
            self.assertEqual(len(ready["models"]), 12)
            before = {p: data.file_hash(p) for p in out.glob("*/complete.json")}
            subprocess.run(command, check=True, timeout=30, capture_output=True)
            self.assertEqual(before, {p: data.file_hash(p) for p in before})
            for model in ready["models"]:
                desc = json.loads(Path(model).read_text())["weights"]["nnue"]
                retained = Path(model).parent.name.endswith("m")
                self.assertEqual(desc["output_mode"], "residual" if "v6A" in model or retained else "total")
                if retained:
                    metrics = json.loads((Path(model).parent / "metrics.json").read_text())
                    self.assertTrue(all(row["alpha"] == 1 for row in metrics))


if __name__ == "__main__":
    unittest.main()
