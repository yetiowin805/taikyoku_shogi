"""Completed-run admission preserves results and recovers partial publication."""

import copy
import fcntl
import tempfile
import unittest
from contextlib import ExitStack
from pathlib import Path
from unittest.mock import patch

import training_admission as admission
from training_admission import a


class AdmissionTests(unittest.TestCase):
    def test_watcher_waits_for_training_service_cleanup(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            config = root / "config.json"
            a.atomic(config, dict(out=str(root), admission_models=["NNUE_a", "NNUE_b"]))
            with (
                patch("sys.argv", ["training_admission", str(config), "--watch"]),
                patch.object(admission.signal, "signal"),
                patch.object(admission, "completed", return_value=True),
                patch.object(admission.subprocess, "check_output", side_effect=["deactivating\n", "inactive\n"]),
                patch.object(admission.time, "sleep") as wait,
                patch.object(admission, "admit") as admit,
            ):
                admission.main()
                wait.assert_called_once_with(30)
                self.assertEqual(admit.call_args.args[1], [root / "NNUE_a", root / "NNUE_b"])

    def test_identity_is_idempotent_and_preserves_history(self):
        old = dict(
            entrants=[dict(id="old", model="/old")],
            slots=[dict(status="done", score_a=1)],
            ratings={"old": 1700},
            retired=["retired"],
            knockouts=[dict(seeds=["old"])],
        )
        candidate = dict(id="new", model="/new")
        result = admission.add_entrants(old, [candidate])
        self.assertEqual(
            {k: v for k, v in result.items() if k != "entrants"}, {k: v for k, v in old.items() if k != "entrants"}
        )
        self.assertEqual(admission.add_entrants(result, [candidate]), result)
        self.assertEqual(old["entrants"], [dict(id="old", model="/old")])
        with self.assertRaisesRegex(ValueError, "identity changed"):
            admission.add_entrants(result, [dict(candidate, model="/different")])
        with self.assertRaisesRegex(ValueError, "already retired"):
            admission.add_entrants(dict(result, retired=["new"]), [candidate])

    def test_drain_does_not_take_an_inflight_cpu(self):
        with tempfile.TemporaryDirectory() as temp, ExitStack() as stack:
            control = Path(temp)
            game = (control / "cpu-2.lock").open("a+")
            fcntl.flock(game, fcntl.LOCK_EX)
            leases = {}
            admission.claim_idle(control, leases, stack, 8)
            self.assertEqual(set(leases), set(range(8)) - {2})
            game.close()
            admission.claim_idle(control, leases, stack, 8)
            self.assertEqual(set(leases), set(range(8)))
            with (control / "cpu-0.lock").open("a+") as contender:
                with self.assertRaises(BlockingIOError):
                    fcntl.flock(contender, fcntl.LOCK_EX | fcntl.LOCK_NB)
                stack.close()
                fcntl.flock(contender, fcntl.LOCK_EX | fcntl.LOCK_NB)

    def test_admission_restart_and_partial_publication_recovery(self):
        for fail_after_state in (False, True):
            with self.subTest(interrupt=fail_after_state), tempfile.TemporaryDirectory() as temp:
                root = Path(temp)
                run, out = root / "run", root / "out"
                control = run / "analysis"
                control.mkdir(parents=True)
                out.mkdir()
                model = out / "NNUE_test"
                model.mkdir()
                blob = model / "network.bin"
                blob.write_bytes(b"immutable test weights")
                a.atomic(model / "status.json", dict(state="completed"))
                a.atomic(
                    model / "model.json",
                    dict(name=model.name, weights=dict(nnue=dict(file=blob.name, sha256=a.file_digest(blob)))),
                )
                state = dict(
                    entrants=[dict(id="old", model="/old")],
                    slots=[dict(status="done", score_a=1)],
                    ratings={"old": 1600},
                    knockouts=[dict(seeds=["old"])],
                )
                cfg = dict(
                    cpus=list(range(8)),
                    sidecar="none",
                    models={},
                    command=["unchanged-engine"],
                    time_control=dict(initial_ms=900000, increment_ms=5000),
                )
                a.atomic(run / "state.json", state)
                a.atomic(control / "config.json", cfg)
                a.atomic(control / "manifest.json", dict(entrants=state["entrants"]))
                a.atomic(control / "dual-label-config.json", dict(old=dict(models={}), teacher="unchanged"))
                a.atomic(control / "supervisor.json", dict(state="running"))
                config = dict(run=str(run), out=str(out))
                calls = []

                def service(action, name, calls=calls, run=run, state=state):
                    calls.append((action, name))
                    if action == "start" and name == "taikyoku-coordinator.service":
                        current = a.read(run / "state.json")
                        self.assertEqual(current["slots"], state["slots"])
                        self.assertEqual(current["ratings"], state["ratings"])
                        self.assertEqual(current["knockouts"], state["knockouts"])
                        self.assertEqual(len(current["entrants"]), 2)

                original_atomic = a.atomic
                fail = fail_after_state

                def atomic(path, value, original_atomic=original_atomic, run=run):
                    nonlocal fail
                    original_atomic(path, value)
                    if fail and Path(path) == run / "state.json":
                        fail = False
                        raise OSError("simulated interruption after state publication")

                with (
                    patch.object(admission, "service", side_effect=service),
                    patch.object(admission.storage_guard, "require_healthy"),
                    patch.object(
                        admission.engine_updates,
                        "active",
                        return_value=dict(engine_bin="engine", analyzer_bin="analyzer"),
                    ),
                    patch.object(admission.subprocess, "run") as validate,
                    patch.object(a, "alive", return_value=True),
                    patch.object(a, "atomic", side_effect=atomic),
                    patch("artifact_storage.settings", return_value={}),
                ):
                    if fail_after_state:
                        with self.assertRaisesRegex(OSError, "simulated interruption"):
                            admission.admit(config, [model])
                        self.assertEqual(a.read(out / "admission/status.json")["state"], "failed")
                        self.assertEqual(calls[-1], ("start", "taikyoku-dual-labels.service"))
                        # A new finished result must never be overwritten by a
                        # stale pre-admission backup during recovery.
                        state["slots"].append(dict(status="done", score_a=0))
                        current = a.read(run / "state.json")
                        current["slots"] = copy.deepcopy(state["slots"])
                        original_atomic(run / "state.json", current)
                    admission.admit(config, [model])
                    self.assertEqual(a.read(out / "admission/status.json")["state"], "admitted")
                    self.assertEqual(validate.call_args.args[0][1], "--validate-model")
                    before = len(calls)
                    admission.admit(config, [model])
                    self.assertEqual(calls[before:], [("start", "taikyoku-dual-labels.service")])
                current = a.read(control / "config.json")
                self.assertEqual(current["command"], cfg["command"])
                self.assertEqual(current["time_control"], cfg["time_control"])
                self.assertEqual(a.read(control / "dual-label-config.json")["old"]["models"], current["models"])
                frozen = Path(a.read(run / "state.json")["entrants"][-1]["model"])
                self.assertNotEqual(frozen, model / "model.json")
                self.assertTrue(frozen.exists())

    def test_unfinished_model_cannot_be_admitted(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "run/analysis").mkdir(parents=True)
            (root / "out/model").mkdir(parents=True)
            a.atomic(root / "out/model/status.json", dict(state="training"))
            with patch.object(admission, "service") as service:
                with self.assertRaisesRegex(ValueError, "finish training"):
                    admission.admit(dict(run=str(root / "run"), out=str(root / "out")), [root / "out/model"])
                service.assert_not_called()


if __name__ == "__main__":
    unittest.main()
