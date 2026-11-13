"""Admission can wait for games without trapping the scheduler behind its gate."""

import fcntl
import tempfile
import threading
import unittest
from pathlib import Path
from unittest.mock import patch

import training_admission as admission


class AdmissionCpuTests(unittest.TestCase):
    def test_scheduler_can_release_game_then_admission_restores_affinity(self):
        with tempfile.TemporaryDirectory() as folder:
            run = Path(folder)
            control = run / "analysis"
            control.mkdir()
            indices = [7, 1, 3, 4, 5, 6]
            leases = [(control / f"cpu-{i}.lock").open("a+") for i in indices]
            for lease in leases:
                fcntl.flock(lease, fcntl.LOCK_EX)
            queued = threading.Event()
            entered = threading.Event()
            finish = threading.Event()
            errors = []
            original = admission.BoundaryLeases

            def waiters(*args):
                result = original(*args)
                queued.set()
                return result

            def validate():
                try:
                    with admission.validation_cpu(dict(run=str(run), lease_indices=indices)):
                        entered.set()
                        if not finish.wait(5):
                            raise RuntimeError("Test did not release admission")
                        raise ValueError("Validation failed")
                except Exception as exc:
                    errors.append(exc)

            with (
                patch.object(admission, "BoundaryLeases", side_effect=waiters),
                patch.object(admission.os, "sched_getaffinity", return_value={1, 3}),
                patch.object(admission.os, "sched_setaffinity") as affinity,
            ):
                worker = threading.Thread(target=validate, daemon=True)
                worker.start()
                try:
                    self.assertTrue(queued.wait(5))
                    self.assertFalse(entered.is_set())
                    from supervise import allocation_busy

                    self.assertTrue(allocation_busy(control))
                    # The real coordinator enters this gate before it can reap
                    # finished games. Admission must leave it available.
                    with (control / "allocation.lock").open("a+") as gate:
                        fcntl.flock(gate, fcntl.LOCK_EX | fcntl.LOCK_NB)
                    leases[0].close()
                    self.assertTrue(entered.wait(5))
                    with (control / "cpu-7.lock").open("a+") as competing:
                        with self.assertRaises(BlockingIOError):
                            fcntl.flock(competing, fcntl.LOCK_EX | fcntl.LOCK_NB)
                finally:
                    finish.set()
                    for lease in leases:
                        lease.close()
                    worker.join(5)
                self.assertFalse(worker.is_alive())
                self.assertFalse(allocation_busy(control))
                self.assertEqual([str(e) for e in errors], ["Validation failed"])
                self.assertEqual(affinity.call_args_list[0].args, (0, {7}))
                self.assertEqual(affinity.call_args_list[-1].args, (0, {1, 3}))
                with (control / "cpu-7.lock").open("a+") as released:
                    fcntl.flock(released, fcntl.LOCK_EX | fcntl.LOCK_NB)
