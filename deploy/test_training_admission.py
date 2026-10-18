"""Content-pinned admission and interrupted acknowledgement recovery."""

import os
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import training_admission as admission
import upgrade_tournament_scheduler as upgrade
from training_admission import a


class AdmissionTests(unittest.TestCase):
    def test_snapshot_and_retry_preserve_new_results(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            run, out = root / "run", root / "out"
            control = run / "analysis"
            for folder in [control / "tasks/requests", control / "tasks/receipts", out / "NNUE_test"]:
                folder.mkdir(parents=True)
            model = out / "NNUE_test"
            blob = model / "network.bin"
            blob.write_bytes(b"immutable test weights")
            a.atomic(model / "status.json", dict(state="completed"))
            a.atomic(
                model / "model.json",
                dict(name=model.name, weights=dict(nnue=dict(file=blob.name, sha256=a.file_digest(blob)))),
            )
            state = dict(run_id="run", entrants=[dict(id="old", model="/old")], slots=[dict(status="done", score_a=1)])
            a.atomic(run / "state.json", state)
            a.atomic(control / "config.json", dict(models={}, command=["unchanged-engine"]))
            config = dict(run=str(run), out=str(out))
            ids = []

            def acknowledge(run, ident, digest):
                ids.append(ident)
                request = a.read(control / "tasks/requests" / f"{ident}.json")
                entry = request["command"]["entrants"][0]["entrant"]
                current = a.read(run / "state.json")
                if entry not in current["entrants"]:
                    current["entrants"].append(entry)
                a.atomic(run / "state.json", current)
                if len(ids) == 1:
                    raise OSError("receipt lost after coordinator commit")
                return dict(status="applied", sha256=digest)

            with (
                patch.object(admission.tournament_tasks, "ready", return_value=control / "tasks"),
                patch.object(admission.tournament_tasks, "wait", side_effect=acknowledge),
                patch.object(admission.storage_guard, "require_healthy"),
                patch.object(
                    admission.engine_updates, "active", return_value=dict(engine_bin="engine", analyzer_bin="analyzer")
                ),
                patch.object(admission.subprocess, "run") as validate,
                patch("artifact_storage.settings", return_value={}),
            ):
                with self.assertRaisesRegex(OSError, "receipt lost"):
                    admission.admit(config, model)
                current = a.read(run / "state.json")
                current["slots"].append(dict(status="done", score_a=0))
                a.atomic(run / "state.json", current)
                admission.admit(config, model)
                self.assertEqual(ids[0], ids[1])
                self.assertEqual(a.read(run / "state.json"), current)
                self.assertEqual(len(current["entrants"]), 2)
                self.assertTrue(any(call.args[0][1] == "--validate-model" for call in validate.call_args_list))
                frozen = Path(current["entrants"][-1]["model"])
                self.assertNotEqual(frozen, model / "model.json")
                binding = a.read(control / "config.json")["models"][str(frozen)]
                a.validate_model_binding(str(frozen), binding)
                self.assertEqual(a.read(control / "config.json")["command"], ["unchanged-engine"])
                self.assertEqual(a.read(out / "admission/NNUE_test/status.json")["state"], "admitted")

    def test_external_lease_exemption_requires_actual_service_owner(self):
        with tempfile.TemporaryDirectory() as temp:
            control = Path(temp)
            lease = control / "cpu-1.lock"
            lease.touch()
            stat = lease.stat()
            key = f"{os.major(stat.st_dev):x}:{os.minor(stat.st_dev):x}:{stat.st_ino}"
            for pid, expected in [(123, {1}), (999, set())]:
                with (
                    patch.object(upgrade.subprocess, "check_output", return_value="123\n"),
                    patch.object(Path, "read_text", return_value=f"1: FLOCK ADVISORY WRITE {pid} {key} 0 EOF\n"),
                ):
                    self.assertEqual(upgrade.external_leases(control, ["trainer.service"], 8), expected)

    def test_unfinished_model_cannot_be_admitted(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "out/model").mkdir(parents=True)
            a.atomic(root / "out/model/status.json", dict(state="training"))
            with patch.object(admission.tournament_tasks, "ready"), patch.object(admission, "freeze_models") as freeze:
                with self.assertRaisesRegex(ValueError, "finish training"):
                    admission.admit(dict(run=str(root / "run"), out=str(root / "out")), root / "out/model")
                freeze.assert_not_called()


if __name__ == "__main__":
    unittest.main()
