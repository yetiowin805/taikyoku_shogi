"""V7 admission must reject incomplete treatments and changed training inputs."""

import copy
import json
import tempfile
import time
import unittest
from pathlib import Path
from unittest.mock import patch

import training_admission as admission
from training_admission import a


class V7AdmissionTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.out = Path(self.temp.name)
        self.model = self.out / "NNUE_W256_v7_S2_Cu"
        self.model.mkdir()
        parent = self.out / "parent.json"
        start = self.out / "prelude-256-A/start.pt"
        dataset = self.out / "views/S2/dataset.json"
        for path in (parent, start, dataset):
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(path.name)
        dispatched = dict(
            width=256,
            regime="Cu",
            full_v7=True,
            retain_material=True,
            out=str(self.model),
            parent=str(parent),
            start=str(start),
            dataset=str(dataset.parent),
        )
        a.atomic(self.out / (self.model.name + ".json"), dispatched)
        recipe = dict(
            dispatched,
            parent_sha256=a.file_digest(parent),
            start_sha256=a.file_digest(start),
            dataset_sha256=a.file_digest(dataset),
        )
        a.atomic(self.model / "recipe.json", recipe)
        a.atomic(
            self.out / "pipeline-config.json",
            dict(parents={"256": str(parent)}, identities={str(parent): a.file_digest(parent)}),
        )
        complete = dict(
            recipe=a.digest(json.dumps(recipe, sort_keys=True, separators=(",", ":")).encode()),
            windows=8,
            best=0.2,
            reason="plateau",
        )
        a.atomic(self.model / "complete.json", complete)
        a.atomic(
            self.model / "storage-complete.json", dict(complete_sha256=a.file_digest(self.model / "complete.json"))
        )
        blob = self.model / "network.bin"
        blob.write_bytes(b"weights")
        a.atomic(
            self.model / "model.json",
            dict(
                name=self.model.name,
                weights=dict(nnue=dict(file=blob.name, sha256=a.file_digest(blob), width=256, output_mode="residual")),
            ),
        )
        metrics = [
            dict(
                alpha=1,
                validation=dict(selection=0.2),
                export=dict(max_score_error=1, positions=8, output_mode="residual"),
            )
            for _ in range(8)
        ]
        a.atomic(self.model / "metrics.json", metrics)

    def test_completed_treatment_passes(self):
        admission.validate_v7(self.model)

    def test_changed_or_incomplete_artifacts_are_rejected(self):
        for name, field, value in (
            ("complete.json", "reason", "budget"),
            ("complete.json", "windows", 7),
            ("storage-complete.json", "complete_sha256", "stale"),
            ("recipe.json", "retain_material", False),
            ("model.json", "name", "wrong"),
        ):
            with self.subTest(name=name, field=field):
                path = self.model / name
                saved = a.read(path)
                changed = copy.deepcopy(saved)
                changed[field] = value
                a.atomic(path, changed)
                with self.assertRaises(ValueError):
                    admission.validate_v7(self.model)
                a.atomic(path, saved)

    def test_dataset_and_network_mutation_are_rejected(self):
        for path in (self.out / "views/S2/dataset.json", self.model / "network.bin"):
            with self.subTest(path=path):
                saved = path.read_bytes()
                path.write_bytes(b"changed")
                with self.assertRaises(ValueError):
                    admission.validate_v7(self.model)
                path.write_bytes(saved)

    def test_watcher_admits_each_ready_model_after_storage_finalization(self):
        a.atomic(self.model / "status.json", dict(state="completed"))
        config = dict(
            out=str(self.out),
            run=str(self.out / "run"),
            completion_policy="nnue-v7",
            admission_models=[self.model.name, "NNUE_W384_v7_S3_E"],
        )
        path = self.out / "admission-config.json"
        a.atomic(path, config)
        marker = self.model / "storage-complete.json"
        saved = marker.read_bytes()
        marker.unlink()
        with patch("sys.argv", ["training_admission.py", str(path)]), patch.object(admission, "admit") as admit:
            admission.main()
            admit.assert_not_called()
            marker.write_bytes(saved)
            admission.main()
            admit.assert_called_once_with(config, self.model)

    def test_validation_reservation_preserves_other_owner_and_releases_its_demand(self):
        control = self.out / "control"
        control.mkdir()
        path = control / "analysis-demand.json"
        foreign = dict(pid=999999, pending=2, assigned=[1], updated=time.time())
        a.atomic(path, foreign)
        with patch.object(admission.os, "kill"):
            admission.validation_demand(control)
            self.assertEqual(a.read(path), foreign)
            admission.validation_demand(control, release=True)
            self.assertEqual(a.read(path), foreign)
        a.atomic(path, dict(pid=0))
        admission.validation_demand(control)
        self.assertEqual(a.read(path)["pending"], 6)
        self.assertEqual(a.read(path)["assigned"], [])
        admission.validation_demand(control, release=True)
        self.assertEqual(a.read(path)["pending"], 0)
