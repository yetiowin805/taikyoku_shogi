"""Final-export admission gates, per-model readiness, and restart recovery."""

import copy
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import test_training_admission
import training_admission as admission
from training_admission import a


class V6AdmissionTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.out = Path(self.temp.name)
        self.model = self.out / "NNUE_W384_v6B"
        self.model.mkdir()
        parent = self.out / "parent.json"
        parent.write_text("parent descriptor")
        start = self.out / "prelude-384-B/start.pt"
        start.parent.mkdir()
        start.write_bytes(b"shared prelude weights")
        dataset = self.out / "dataset/dataset.json"
        dataset.parent.mkdir()
        dataset.write_text("dataset manifest")
        dispatched = dict(
            width=384,
            regime="B",
            out=str(self.model),
            parent=str(parent),
            start=str(start),
            dataset=str(dataset.parent),
        )
        a.atomic(self.out / (self.model.name + ".json"), dispatched)
        self.recipe = dict(
            dispatched,
            parent_sha256=a.file_digest(parent),
            start_sha256=a.file_digest(start),
            dataset_sha256=a.file_digest(dataset),
        )
        a.atomic(self.model / "recipe.json", self.recipe)
        a.atomic(
            self.out / "pipeline-config.json",
            dict(parents={"384": str(parent)}, identities={str(parent): a.file_digest(parent)}),
        )
        self.complete = dict(
            recipe=a.digest(json.dumps(self.recipe, sort_keys=True, separators=(",", ":")).encode()),
            windows=1,
            best=0.2,
            reason="plateau",
        )
        a.atomic(self.model / "complete.json", self.complete)
        blob = self.model / "network.bin"
        blob.write_bytes(b"quantized inference weights")
        self.checkpoint = dict(
            name=self.model.name,
            weights=dict(nnue=dict(file=blob.name, sha256=a.file_digest(blob), width=384, output_mode="total")),
        )
        a.atomic(self.model / "model.json", self.checkpoint)
        self.metrics = [
            dict(validation=dict(selection=0.2), export=dict(max_score_error=1, positions=64, output_mode="total"))
        ]
        a.atomic(self.model / "metrics.json", self.metrics)
        a.atomic(self.model / "status.json", dict(state="completed"))

    def test_complete_export_passes_but_intermediate_or_changed_inputs_fail(self):
        admission.validate_v6(self.model)
        for filename, content in [
            ("complete.json", dict(self.complete, reason="max_epochs")),
            ("complete.json", dict(self.complete, recipe="wrong")),
            ("recipe.json", dict(self.recipe, regime="A")),
            ("model.json", dict(self.checkpoint, name="wrong")),
            ("metrics.json", [dict(validation=dict(selection=0.2))]),
        ]:
            with self.subTest(filename=filename, content=content):
                path = self.model / filename
                original = path.read_bytes()
                a.atomic(path, content)
                with self.assertRaises(ValueError):
                    admission.validate_v6(self.model)
                path.write_bytes(original)
        for filename in (self.recipe["parent"], self.recipe["start"], self.recipe["dataset"] + "/dataset.json"):
            path = Path(filename)
            original = path.read_bytes()
            path.write_bytes(b"changed input")
            with self.assertRaisesRegex(ValueError, "input changed"):
                admission.validate_v6(self.model)
            path.write_bytes(original)
        (self.model / "complete.json").unlink()
        with self.assertRaises(FileNotFoundError):
            admission.validate_v6(self.model)

    def test_wrong_mode_bad_parity_and_corrupt_export_are_rejected(self):
        wrong = copy.deepcopy(self.checkpoint)
        wrong["weights"]["nnue"]["output_mode"] = "residual"
        a.atomic(self.model / "model.json", wrong)
        with self.assertRaisesRegex(ValueError, "output mode"):
            admission.validate_v6(self.model)

        a.atomic(self.model / "model.json", self.checkpoint)
        for error in (3, float("nan")):
            changed = copy.deepcopy(self.metrics)
            changed[0]["export"]["max_score_error"] = error
            a.atomic(self.model / "metrics.json", changed)
            with self.assertRaisesRegex(ValueError, "parity evidence"):
                admission.validate_v6(self.model)
        (self.model / "network.bin").write_bytes(b"corruption")
        with self.assertRaisesRegex(ValueError, "network changed"):
            admission.validate_v6(self.model)

    def test_material_variant_requires_residual_export_and_a_start(self):
        old = self.model
        self.model = self.out / "NNUE_W384_v6Cm"
        old.rename(self.model)
        start = self.out / "prelude-384-A/start.pt"
        start.parent.mkdir()
        start.write_bytes(b"matched A prelude")
        recipe = dict(
            self.recipe,
            regime="C",
            retain_material=True,
            out=str(self.model),
            start=str(start),
            start_sha256=a.file_digest(start),
        )
        a.atomic(self.model / "recipe.json", recipe)
        dispatched = {k: v for k, v in recipe.items() if k not in ("dataset_sha256", "parent_sha256", "start_sha256")}
        a.atomic(self.out / (self.model.name + ".json"), dispatched)
        complete = dict(
            self.complete, recipe=a.digest(json.dumps(recipe, sort_keys=True, separators=(",", ":")).encode())
        )
        a.atomic(self.model / "complete.json", complete)
        checkpoint = copy.deepcopy(self.checkpoint)
        checkpoint["name"] = self.model.name
        checkpoint["weights"]["nnue"]["output_mode"] = "residual"
        a.atomic(self.model / "model.json", checkpoint)
        metrics = copy.deepcopy(self.metrics)
        metrics[0].update(alpha=1)
        metrics[0]["export"]["output_mode"] = "residual"
        a.atomic(self.model / "metrics.json", metrics)
        admission.validate_v6(self.model)
        metrics[0]["alpha"] = 0
        a.atomic(self.model / "metrics.json", metrics)
        with self.assertRaisesRegex(ValueError, "removed its material"):
            admission.validate_v6(self.model)

    def test_ready_model_is_admitted_without_waiting_for_other_widths_or_regimes(self):
        pending = "NNUE_W512_v6D"
        config = dict(out=str(self.out), run=str(self.out / "run"), admission_models=[self.model.name, pending])
        path = self.out / "config.json"
        a.atomic(path, config)
        with patch("sys.argv", ["training_admission.py", str(path)]), patch.object(admission, "admit") as admit:
            admission.main()
        admit.assert_called_once_with(config, self.model)
        status = a.read(self.out / "admission/status.json")
        self.assertEqual(status["admitted"], [self.model.name])
        self.assertEqual(status["waiting"], [pending])

    def test_search_smoke_requires_a_completed_nonterminal_iteration(self):
        game = self.out / "smoke.json"
        game.write_text("pinned training position")
        smoke = dict(game=str(game), sha256=a.file_digest(game), ply=1)
        bundle = dict(engine_bin="engine", analyzer_bin="analyzer")
        path = str(self.model / "model.json")
        with (
            patch.object(a, "validate_model_binding"),
            patch.object(admission.engine_updates, "require_search_support"),
            patch.object(admission.subprocess, "run") as run,
        ):
            run.return_value.stdout = '{"completed_depth":0,"terminal":true}'
            with self.assertRaisesRegex(ValueError, "nonterminal iteration"):
                admission.validate_entries(bundle, [dict(model=path)], {path: {}}, smoke)
            run.return_value.stdout = '{"completed_depth":1,"best_move":"P1a1b"}'
            admission.validate_entries(bundle, [dict(model=path)], {path: {}}, smoke)
            game.write_text("changed")
            with self.assertRaisesRegex(ValueError, "smoke game changed"):
                admission.validate_entries(bundle, [dict(model=path)], {path: {}}, smoke)

    def test_restart_keeps_acknowledged_retired_model_without_readmission(self):
        fixture = test_training_admission.AdmissionTests()
        with fixture.admission_fixture() as (run, model, control, _):

            def acknowledge(run, ident, digest):
                request = a.read(control / "tasks/requests" / f"{ident}.json")
                entry = request["command"]["entrants"][0]["entrant"]
                self.assertEqual(entry["generation"], "v6")
                a.atomic(run / "state.json", dict(entrants=[entry], retired=[entry["id"]]))
                receipt = dict(status="applied", sha256=digest)
                a.atomic(control / "tasks/receipts" / f"{ident}.json", receipt)
                return receipt

            config = dict(run=str(run), out=str(model.parent))
            with patch.object(admission.tournament_tasks, "wait", side_effect=acknowledge):
                admission.admit_checkpoints(run, [model], model.parent / "admission" / model.name, generation="v6")
            self.assertTrue(admission.confirmed_admission(config, model.name))
            (model / "network.bin").unlink()
            self.assertTrue(admission.confirmed_admission(config, model.name))
            a.atomic(run / "state.json", dict(entrants=[]))
            with self.assertRaisesRegex(ValueError, "missing from tournament"):
                admission.confirmed_admission(config, model.name)


if __name__ == "__main__":
    unittest.main()
