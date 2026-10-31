"""Content-pinned admission and interrupted acknowledgement recovery."""

import os
import tempfile
import unittest
from contextlib import contextmanager
from pathlib import Path
from unittest.mock import patch

import training_admission as admission
import upgrade_tournament_scheduler as upgrade
from training_admission import a


class AdmissionTests(unittest.TestCase):
    @contextmanager
    def admission_fixture(self, trial=None):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            run, model = root / "run", root / "out" / "NNUE_test"
            control = run / "analysis"
            for folder in [control / "tasks/requests", control / "tasks/receipts", model]:
                folder.mkdir(parents=True)
            blob = model / "network.bin"
            blob.write_bytes(b"immutable test weights")
            checkpoint = dict(name=model.name, weights=dict(nnue=dict(file=blob.name, sha256=a.file_digest(blob))))
            if trial is not None:
                checkpoint["search_defaults"] = dict(experiment="history_tactical_lmr_q_fix", trial=trial)
            else:
                a.atomic(model / "status.json", dict(state="completed"))
            a.atomic(model / "model.json", checkpoint)
            a.atomic(run / "state.json", dict(run_id="run", entrants=[], slots=[]))
            a.atomic(control / "config.json", dict(models={}, command=["unchanged-engine"]))
            with (
                patch.object(admission.tournament_tasks, "ready", return_value=control / "tasks"),
                patch.object(admission, "check_storage"),
                patch.object(
                    admission.engine_updates, "active", return_value=dict(engine_bin="engine", analyzer_bin="analyzer")
                ),
                patch.object(admission.engine_updates, "require_search_support") as support,
                patch.object(admission.subprocess, "run"),
                patch("artifact_storage.settings", return_value={"model_store": str(root / "store")}),
            ):
                yield run, model, control, support

    def test_trial_cohort_is_pinned_and_idempotent_without_changing_old_admissions(self):
        for trial, expected in [(None, None), ("none", None), ("q_refill", "search")]:
            with self.subTest(trial=trial), self.admission_fixture(trial) as (run, model, control, _):
                requests = []

                def acknowledge(run, ident, digest, *, requests=requests, expected=expected):
                    request = a.read(control / "tasks/requests" / f"{ident}.json")
                    entry = request["command"]["entrants"][0]["entrant"]
                    requests.append((ident, entry))
                    self.assertEqual(entry.get("generation"), expected)
                    if expected is None:
                        self.assertNotIn("generation", entry)
                    state = a.read(run / "state.json")
                    if not state["entrants"]:
                        state["entrants"].append(entry)
                    self.assertEqual(state["entrants"], [entry])
                    a.atomic(run / "state.json", state)
                    return dict(status="applied", sha256=digest)

                with patch.object(admission.tournament_tasks, "wait", side_effect=acknowledge):
                    for _ in range(2):
                        if trial is None:
                            admission.admit(dict(run=str(run), out=str(model.parent)), model)
                        else:
                            admission.admit_checkpoints(run, [model], model.parent / "receipt", search_variants=True)
                self.assertEqual(requests[0], requests[1])
                self.assertEqual(a.read(run / "state.json")["entrants"][0].get("generation"), expected)
                self.assertEqual(a.read(control / "config.json")["command"], ["unchanged-engine"])

    def test_trial_receipt_cannot_silently_drop_its_cohort(self):
        with self.admission_fixture("q_refill") as (run, model, control, _):

            def acknowledge(run, ident, digest):
                request = a.read(control / "tasks/requests" / f"{ident}.json")
                entry = request["command"]["entrants"][0]["entrant"]
                self.assertEqual(entry.pop("generation"), "search")
                state = a.read(run / "state.json")
                state["entrants"] = [entry]
                a.atomic(run / "state.json", state)
                return dict(status="applied", sha256=digest)

            with patch.object(admission.tournament_tasks, "wait", side_effect=acknowledge):
                with self.assertRaisesRegex(RuntimeError, "not reflected"):
                    admission.admit_checkpoints(run, [model], model.parent / "receipt", search_variants=True)

    def test_admission_rechecks_trial_support_after_preparation(self):
        with self.admission_fixture() as (run, model, control, support):
            a.atomic(run / "state.json", dict(entrants=[dict(id="base", model=str(model / "model.json"))]))
            config = dict(
                run=str(run),
                out=str(model.parent / "search"),
                search_variants={
                    "HLF_RF": dict(base_entrant="base", experiment="history_tactical_lmr_q_fix", trial="q_refill"),
                },
            )
            models = admission.prepare_search_variants(config)
            support.side_effect = ValueError("active engine does not support q_refill")
            with patch.object(admission.tournament_tasks, "submit") as submit:
                with self.assertRaisesRegex(ValueError, "does not support"):
                    admission.admit_checkpoints(run, models, model.parent / "receipt", search_variants=True)
                submit.assert_not_called()
            self.assertEqual(a.read(control / "config.json")["models"], {})
            self.assertEqual(support.call_args.args[2], ["q_refill"])

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
                pinned = request["command"]["entrants"][0]
                entry = pinned["entrant"]
                descriptor = a.read(entry["model"])["weights"]["nnue"]
                self.assertTrue(Path(descriptor["file"]).is_symlink())
                self.assertEqual(pinned["files"][descriptor["file"]], descriptor["sha256"])
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
                patch("artifact_storage.settings", return_value={"model_store": str(root / "store")}),
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

    def test_search_variants_preserve_network_and_weights_without_fake_training(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            run, out = root / "run", root / "out"
            run.mkdir()
            source = root / "model.json"
            blob = root / "network.bin"
            blob.write_bytes(b"one shared network")
            model = dict(
                name="base",
                search_defaults=dict(depth=2, quiescence_depth=2),
                weights=dict(piece={"Pawn": 1}, nnue=dict(file=blob.name, sha256=a.file_digest(blob))),
            )
            a.atomic(source, model)
            a.atomic(run / "state.json", dict(entrants=[dict(id="base", model=str(source))]))
            config = dict(
                run=str(run),
                out=str(out),
                base_entrant="base",
                search_variants={"base_H": "history", "base_Q": "scout_q"},
            )
            with (
                patch.object(admission.engine_updates, "active", return_value=dict(engine_bin="engine")),
                patch.object(admission.engine_updates, "require_search_support"),
            ):
                models = admission.prepare_search_variants(config)
                self.assertEqual(len(models), 2)
                self.assertEqual(models, admission.prepare_search_variants(config))
                candidate = dict(engine_bin="candidate")
                with patch.object(admission.engine_updates, "verify", return_value=candidate) as verify:
                    self.assertEqual(models, admission.prepare_search_variants(config, bundle=candidate))
                    verify.assert_called_once_with(a, candidate)
                for directory in models:
                    actual = a.read(directory / "model.json")
                    self.assertEqual(actual["weights"]["piece"], model["weights"]["piece"])
                    self.assertEqual(Path(actual["weights"]["nnue"]["file"]), blob)
                    self.assertEqual(actual["weights"]["nnue"]["sha256"], model["weights"]["nnue"]["sha256"])
                    self.assertFalse((directory / "status.json").exists())
                config["search_variants"]["base_H"] = "guarded_null"
                with self.assertRaisesRegex(ValueError, "definition changed"):
                    admission.prepare_search_variants(config)

    def test_search_package_can_transfer_across_distinct_registered_networks(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            run = root / "run"
            run.mkdir()
            entrants = []
            originals = {}
            for name in ["base512", "base384"]:
                directory = root / name
                directory.mkdir()
                blob = directory / "network.bin"
                blob.write_bytes(name.encode())
                model = dict(
                    name=name,
                    search_defaults=dict(depth=2),
                    weights=dict(piece={"Pawn": len(name)}, nnue=dict(file=blob.name, sha256=a.file_digest(blob))),
                )
                source = directory / "model.json"
                a.atomic(source, model)
                originals[name] = source.read_bytes()
                entrants.append(dict(id=name, model=str(source)))
            a.atomic(run / "state.json", dict(entrants=entrants))
            config = dict(
                run=str(run),
                out=str(root / "out"),
                base_entrant="base512",
                search_variants={
                    "new512": "history_tactical_lmr_q_fix",
                    "new384": dict(base_entrant="base384", experiment="history_tactical_lmr_q_fix"),
                },
            )
            with (
                patch.object(admission.engine_updates, "active", return_value=dict(engine_bin="engine")),
                patch.object(admission.engine_updates, "require_search_support"),
            ):
                models = admission.prepare_search_variants(config)
                self.assertEqual(len(models), 2)
                self.assertEqual(models, admission.prepare_search_variants(config))
                for directory, original in zip(models, entrants, strict=True):
                    new = a.read(directory / "model.json")
                    old = a.read(original["model"])
                    self.assertEqual(new["weights"]["piece"], old["weights"]["piece"])
                    self.assertEqual(new["weights"]["nnue"]["sha256"], old["weights"]["nnue"]["sha256"])
                    self.assertEqual(Path(original["model"]).read_bytes(), originals[original["id"]])
                config["search_variants"]["duplicate512"] = dict(
                    base_entrant="base512", experiment="history_tactical_lmr_q_fix"
                )
                with self.assertRaisesRegex(ValueError, "distinct search policies per baseline"):
                    admission.prepare_search_variants(config)
                config["search_variants"]["duplicate512"] = dict(
                    base_entrant="base384", experiment="history_tactical_lmr_q_fix", typo=True
                )
                with self.assertRaisesRegex(ValueError, "Invalid search variant specification"):
                    admission.prepare_search_variants(config)

    def test_search_support_cannot_silently_fall_back_to_old_engine(self):
        with patch.object(admission.engine_updates.subprocess, "run") as probe:
            probe.return_value.stdout = '["baseline","history"]'
            admission.engine_updates.require_search_support("engine", ["baseline", "history"])
            with self.assertRaisesRegex(ValueError, "does not support"):
                admission.engine_updates.require_search_support("engine", ["scout_q"])
            probe.return_value.stdout = "usage: old engine"
            with self.assertRaises(ValueError):
                admission.engine_updates.require_search_support("old", ["history"])

    def test_new_search_trial_requires_an_explicit_capability(self):
        with patch.object(admission.engine_updates.subprocess, "run") as probe:
            probe.return_value.stdout = '["history"]'
            admission.engine_updates.require_search_support("engine", ["history"], ["none"])
            self.assertEqual(probe.call_count, 1)
            with self.assertRaisesRegex(ValueError, "does not support"):
                admission.engine_updates.require_search_support("engine", ["history"], ["q_refill"])
            self.assertEqual(probe.call_args.args[0][1], "tournament-search-trials")
            probe.return_value.stdout = '["none","q_refill"]'
            admission.engine_updates.require_search_support("engine", ["baseline"], ["q_refill"])

    def test_distinct_trials_share_foundation_without_mutating_it(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            run = root / "run"
            run.mkdir()
            blob = root / "network.bin"
            blob.write_bytes(b"fixed network")
            source = root / "model.json"
            model = dict(
                name="base",
                search_defaults=dict(depth=2),
                weights=dict(piece={"Pawn": 1}, nnue=dict(file=blob.name, sha256=a.file_digest(blob))),
            )
            a.atomic(source, model)
            original = source.read_bytes()
            a.atomic(run / "state.json", dict(entrants=[dict(id="base", model=str(source))]))
            config = dict(
                run=str(run),
                out=str(root / "out"),
                search_variants={
                    "HLF_RF": dict(base_entrant="base", experiment="history_tactical_lmr_q_fix", trial="q_refill"),
                    "HLF_QH": dict(base_entrant="base", experiment="history_tactical_lmr_q_fix", trial="q_history"),
                },
            )
            with (
                patch.object(admission.engine_updates, "active", return_value=dict(engine_bin="engine")),
                patch.object(admission.engine_updates, "require_search_support") as support,
            ):
                models = admission.prepare_search_variants(config)
                self.assertEqual(len(models), 2)
                self.assertEqual(source.read_bytes(), original)
                self.assertEqual(
                    [a.read(path / "model.json")["search_defaults"]["trial"] for path in models],
                    ["q_refill", "q_history"],
                )
                self.assertEqual(support.call_args.args[2], ["q_refill", "q_history"])
                config["search_variants"]["duplicate"] = dict(config["search_variants"]["HLF_RF"])
                with self.assertRaisesRegex(ValueError, "distinct search policies"):
                    admission.prepare_search_variants(config)

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
