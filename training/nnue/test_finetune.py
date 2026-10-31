"""Best-state restarts and the early validation boundary survive process recovery."""

import hashlib
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import numpy as np
import torch

from pilot_fit import PilotNet, reduce_learning_rates, training_snapshot
from train import export

HERE = Path(__file__).parent


class FineTuneTest(unittest.TestCase):
    def test_reduction_restores_weights_and_moments_but_halves_current_rate(self):
        torch.manual_seed(4)
        net = PilotNet(16, 32)
        emb = torch.optim.SGD([net.embedding.weight], lr=0.0001)
        dense = torch.optim.Adam([net.output.weight, net.output.bias], lr=0.00015)

        def update():
            dense.zero_grad()
            net.output(torch.ones(1, 32)).sum().backward()
            dense.step()

        update()
        best = training_snapshot(net, emb, dense, {"name": "best"}, 8)
        for expected in (0.000075, 0.0000375):
            update()
            with torch.no_grad():
                net.embedding.weight.add_(0.1)
            reduce_learning_rates(net, emb, dense, 0.5, best)
            self.assertAlmostEqual(dense.param_groups[0]["lr"], expected)
            for name, value in net.state_dict().items():
                self.assertTrue(torch.equal(value, best["net"][name]), name)
            for key, value in dense.state_dict()["state"][0].items():
                self.assertTrue(torch.equal(value, best["dense"]["state"][0][key]), key)
        self.assertEqual(best["dense"]["param_groups"][0]["lr"], 0.00015)
        self.assertEqual(best["dense"]["state"][0]["step"].item(), 1)

    def test_early_and_post_reduction_resume_match_uninterrupted_run(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
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
                    split="train" if i < 40 else "validation" if i < 50 else "test",
                    source="recent",
                    label_source="saved",
                    selection="representative",
                    teacher="NNUE",
                )
                for i in range(60)
            ]
            (root / "samples.jsonl").write_text("".join(json.dumps(s) + "\n" for s in samples))
            identity = {
                name + "_sha256": hashlib.sha256((root / (name + "." + suffix)).read_bytes()).hexdigest()
                for name, suffix in [("features", "bin"), ("samples", "jsonl"), ("schema", "json")]
            }
            identity["scale"] = 2822.9057434005167
            (root / "dataset.json").write_text(json.dumps(identity))
            sha = export(PilotNet(16, 32), root / "parent.bin", schema)
            (root / "parent.json").write_text(
                json.dumps(
                    dict(
                        weights=dict(
                            nnue=dict(
                                file="parent.bin",
                                sha256=sha,
                                feature_hash=schema["names_hash"],
                                width=32,
                            )
                        )
                    )
                )
            )
            options = [
                str(root),
                "--parent",
                str(root / "parent.json"),
                "--init",
                "parent",
                "--loss",
                "wdl",
                "--width",
                "32",
                "--epochs",
                "0",
                "--epoch-samples",
                "16",
                "--batch",
                "4",
                "--microbatch",
                "1",
                "--early-validation-samples",
                "8",
                "--restore-best-on-reduce",
                "--patience",
                "3",
                "--lr-reductions",
                "2",
                "--embedding-lr",
                "0.0001",
                "--dense-lr",
                "0.00015",
                "--defer-test",
            ]
            # Keep real forward/backward/export operations but provide a controlled
            # validation curve: early best, then plateau. Test both interruption points.
            runner = """import json,os,sys,pilot_fit as fit
original = fit.evaluate
calls = 0
def evaluate(*args, **kwargs):
    global calls
    result = original(*args, **kwargs)
    calls += 1
    result['objective'] = 1.0 if '--resume' not in sys.argv and calls == 2 else 2.0
    return result
fit.evaluate = evaluate
def checkpoint_exit(message, **kwargs):
    epoch = json.loads(message).get('epoch')
    if os.environ.get('INTERRUPT') == str(epoch):
        raise SystemExit(99)
fit.print = checkpoint_exit
fit.main()
"""

            def run(name, resume=False, interrupt=""):
                command = [
                    sys.executable,
                    "-c",
                    runner,
                    *options,
                    "--out",
                    str(root / name),
                    "--checkpoint-dir",
                    str(root / (name + "-state")),
                ]
                if resume:
                    command.append("--resume")
                result = subprocess.run(
                    command, cwd=HERE, env=dict(os.environ, INTERRUPT=interrupt), capture_output=True, timeout=60
                )
                self.assertEqual(result.returncode, 99 if interrupt else 0, result.stderr.decode())

            def load(name):
                return torch.load(root / (name + "-state/training.pt"), weights_only=True)

            run("full")
            expected = load("full")
            self.assertEqual(expected["epoch"], 10)
            self.assertEqual(expected["plateau"]["stopped"], "plateau")
            self.assertEqual(expected["best_state"]["samples"], 8)
            self.assertEqual([m["epoch"] for m in expected["metrics"] if m.get("decision") == "reduce_lr"], [4, 7])
            self.assertAlmostEqual(expected["dense"]["param_groups"][0]["lr"], 0.0000375)
            for point in ("0.5", "4"):
                with self.subTest(interrupt=point):
                    name = "interrupted-" + point
                    run(name, interrupt=point)
                    interrupted = load(name)
                    self.assertEqual(interrupted["samples_in_epoch"], 8 if point == "0.5" else 0)
                    # Mimic a manifest updated before its training checkpoint commits.
                    manifest = root / name / "model.json"
                    manifest.write_text('{"stale":true}')
                    run(name, resume=True)
                    actual = load(name)
                    self.assertEqual(actual["plateau"], expected["plateau"])
                    for key in ("net", "emb", "dense", "best_state"):
                        self.assert_states_equal(actual[key], expected[key])
                    for actual_metric, expected_metric in zip(actual["metrics"], expected["metrics"], strict=True):
                        self.assertEqual(
                            {k: v for k, v in actual_metric.items() if k != "elapsed_s"},
                            {k: v for k, v in expected_metric.items() if k != "elapsed_s"},
                        )
                    self.assertEqual(
                        json.loads(manifest.read_text())["weights"], expected["best_state"]["model"]["weights"]
                    )

    def assert_states_equal(self, actual, expected):
        if isinstance(expected, torch.Tensor):
            self.assertTrue(torch.equal(actual, expected))
        elif isinstance(expected, dict):
            self.assertEqual(actual.keys(), expected.keys())
            for key in expected:
                # Model metadata names and publication times differ across output directories.
                if key not in ("name", "created_at"):
                    self.assert_states_equal(actual[key], expected[key])
        elif isinstance(expected, list):
            self.assertEqual(len(actual), len(expected))
            for a, e in zip(actual, expected, strict=True):
                self.assert_states_equal(a, e)
        else:
            self.assertEqual(actual, expected)


if __name__ == "__main__":
    unittest.main()
