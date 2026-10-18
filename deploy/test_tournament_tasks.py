"""Durable request publication and per-model readiness; no production searches."""

import fcntl
import json
import tempfile
import threading
import unittest
from contextlib import ExitStack
from pathlib import Path
from unittest.mock import patch

import tournament_tasks as tasks
import tourney_analysis as a
import training_admission as admission
import upgrade_tournament_scheduler as upgrade


class TasksTests(unittest.TestCase):
    def test_clock_transition_preserves_schedule_and_requires_valid_pair(self):
        config = dict(
            command=["engine", "tournament", "--resume", "--initial-time-ms", "900000", "--increment-ms", "5000"],
            cpus=list(range(8)),
        )
        changed = upgrade.clock_transition(config, 60000, 1000)
        self.assertEqual(changed["time_control"], dict(initial_ms=60000, increment_ms=1000))
        self.assertEqual(
            changed["command"],
            ["engine", "tournament", "--resume", "--initial-time-ms", "60000", "--increment-ms", "1000"],
        )
        self.assertEqual(config["command"][4], "900000")
        self.assertEqual(changed["cpus"], config["cpus"])
        self.assertIs(upgrade.clock_transition(config, None, None), config)
        for initial, increment in [(None, 1000), (60000, None), (1000, 1000), (60000, 0)]:
            with self.assertRaises(ValueError):
                upgrade.clock_transition(config, initial, increment)
        with self.assertRaisesRegex(ValueError, "fixed move budget"):
            upgrade.clock_transition(dict(command=["engine", "--max-time-ms", "1000"]), 60000, 1000)

    def test_immutable_request_and_verified_receipt(self):
        with tempfile.TemporaryDirectory() as temp:
            run = Path(temp)
            control = run / "analysis/tasks"
            (control / "requests").mkdir(parents=True)
            (control / "receipts").mkdir()
            a.atomic(run / "state.json", dict(run_id="fixture"))
            with patch.object(tasks, "ready", return_value=control):
                ident, digest = tasks.submit(run, dict(kind="resume_assignments"), "stable-id")
                self.assertEqual(tasks.submit(run, dict(kind="resume_assignments"), "stable-id"), (ident, digest))
                with self.assertRaisesRegex(ValueError, "different immutable"):
                    tasks.submit(run, dict(kind="pause_assignments", reason="test"), "stable-id")
                receipt = control / "receipts/stable-id.json"
                a.atomic(receipt, dict(sha256=digest, status="rejected", error="changed model"))
                with self.assertRaisesRegex(RuntimeError, "changed model"):
                    tasks.wait(run, ident, digest)
                a.atomic(receipt, dict(sha256="wrong", status="applied"))
                with self.assertRaisesRegex(ValueError, "does not match"):
                    tasks.wait(run, ident, digest)
                a.atomic(receipt, dict(sha256=digest, status="applied"))
                self.assertEqual(tasks.wait(run, ident, digest)["status"], "applied")

    def test_model_a_admitted_while_b_still_trains_and_no_service_restart(self):
        with tempfile.TemporaryDirectory() as temp:
            out = Path(temp)
            for name in ["a", "b"]:
                (out / name).mkdir()
                a.atomic(out / name / "status.json", dict(state="completed" if name == "a" else "training"))
            config = out / "config.json"
            a.atomic(config, dict(out=str(out), admission_models=["a", "b"]))
            calls = []

            def admitted(config, model):
                calls.append(model.name)

            def next_round(_):
                self.assertEqual(calls, ["a"])
                a.atomic(out / "b/status.json", dict(state="completed"))

            with (
                patch("sys.argv", ["training_admission", str(config), "--watch"]),
                patch.object(admission, "admit", side_effect=admitted),
                patch.object(admission.time, "sleep", side_effect=next_round),
                patch.object(admission.subprocess, "run") as services,
            ):
                admission.main()
            self.assertEqual(calls, ["a", "b"])
            services.assert_not_called()
            self.assertEqual(a.read(out / "admission/status.json")["admitted"], ["a", "b"])

    def test_legacy_drain_keeps_gate_closed_until_all_leases_are_owned(self):
        with tempfile.TemporaryDirectory() as temp, ExitStack() as stack:
            run = Path(temp)
            control = run / "analysis"
            control.mkdir()
            a.atomic(run / "state.json", dict(slots=[]))
            gate = stack.enter_context((control / "allocation.lock").open("a+"))
            fcntl.flock(gate, fcntl.LOCK_EX)
            game = (control / "cpu-0.lock").open("a+")
            fcntl.flock(game, fcntl.LOCK_EX)
            attempts = threading.Event()
            claimed = threading.Event()

            def game_finished():
                game.close()
                attempts.set()
                with (control / "allocation.lock").open("a+") as reclaim:
                    fcntl.flock(reclaim, fcntl.LOCK_EX)
                    claimed.set()

            worker = threading.Thread(target=game_finished)
            worker.start()
            self.assertTrue(attempts.wait(5))
            with patch.object(upgrade, "external_leases", return_value=set()):
                upgrade.drain(control, stack, 8, [], lambda *args, **kwargs: None)
            self.assertFalse(claimed.is_set())
            stack.close()
            worker.join(5)
            self.assertTrue(claimed.is_set())

    def test_no_live_coordinator_fails_before_mutation(self):
        with tempfile.TemporaryDirectory() as temp:
            run = Path(temp)
            a.atomic(run / "state.json", dict(run_id="fixture"))
            with self.assertRaisesRegex(ValueError, "upgrade"):
                tasks.submit(run, dict(kind="resume_assignments"))
            self.assertEqual(json.loads((run / "state.json").read_text()), dict(run_id="fixture"))


if __name__ == "__main__":
    unittest.main()
