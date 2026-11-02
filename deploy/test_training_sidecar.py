"""Process/lease tests use tiny stand-ins, never a real tournament or training job."""

import fcntl
import json
import os
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

import tourney_analysis as a
import training_sidecar as sidecar


def wait_for(predicate, seconds=8):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        if predicate():
            return
        time.sleep(0.02)
    raise AssertionError("Timed out waiting for fixture")


class SidecarTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.run = self.root / "run"
        self.control = self.run / "analysis"
        self.control.mkdir(parents=True)
        (self.run / "training").mkdir()
        (self.root / "data/run").mkdir(parents=True)
        self.procs = []
        allowed = sorted(os.sched_getaffinity(0))
        if len(allowed) < 4:
            self.skipTest("requires four available CPU IDs")
        self.cpus = allowed[:4]

    def tearDown(self):
        for p in self.procs:
            if p.poll() is None:
                p.terminate()
            try:
                p.wait(timeout=5)
            except subprocess.TimeoutExpired:
                p.kill()
                p.wait()
        self.tmp.cleanup()

    def config(self, code=0):
        program = self.root / "fake_training.py"
        program.write_text(
            "import json,os,time\nfrom pathlib import Path\n"
            f"root=Path({str(self.root)!r})\n"
            "def save(name, payload):\n"
            "    path = root / name\n"
            "    temporary = path.with_suffix('.tmp')\n"
            "    temporary.write_text(json.dumps(payload))\n"
            "    temporary.replace(path)\n"
            'save("started.json", dict(pid=os.getpid(),cpu=sorted(os.sched_getaffinity(0)),time=time.monotonic()))\n'
            'while not (root/"training.release").exists(): time.sleep(.01)\n'
            'save("finished.json", dict(time=time.monotonic()))\n'
            f"raise SystemExit({code})\n"
        )
        config_path = self.root / "training.json"
        config_path.write_text("{}")
        c = dict(
            run=str(self.run),
            cpus=self.cpus,
            sidecar="training",
            training=dict(
                config=str(config_path),
                config_sha256=a.file_digest(config_path),
                code_dir=str(self.root),
                code_hashes={program.name: a.file_digest(program)},
                out=str(self.root / "out"),
                command=[sys.executable, str(program)],
            ),
        )
        a.atomic(self.control / "config.json", c)
        return c

    def launch_worker(self):
        p = subprocess.Popen(
            [sys.executable, str(Path(a.__file__)), "_train", "--run-dir", str(self.run)],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        self.procs.append(p)
        return p

    def supervisor_command(self, *, finish_before_startup=False):
        # Control the supervisor's startup boundary and polling independently of
        # real child processes. No production sleep intervals are changed.
        program = self.root / "supervisor.py"
        program.write_text(f"""import signal
import subprocess
import sys
import time
from pathlib import Path
from types import SimpleNamespace

sys.path.insert(0, {str(Path(a.__file__).parent)!r})
import tourney_analysis as a

root = Path({str(self.root)!r})
children = []

def launch(*args, **kwargs):
    child = subprocess.Popen(*args, **kwargs)
    children.append(child)
    return child

def sleep(seconds):
    if seconds == 2:
        (root / "startup.waiting").touch()
        deadline = time.monotonic() + 8
        while not (root / "startup.release").exists():
            if time.monotonic() >= deadline:
                raise TimeoutError("fixture did not release supervisor startup")
            time.sleep(.01)
        if {finish_before_startup!r}:
            children[-1].wait(timeout=8)
    else:
        time.sleep(.01)

a.ROOT = root
a.time = SimpleNamespace(sleep=sleep, monotonic=time.monotonic)
a.subprocess = SimpleNamespace(Popen=launch, TimeoutExpired=subprocess.TimeoutExpired)
signal.signal(signal.SIGTERM, a.stop_handler)
a.supervise(a.read(root / "run/analysis/config.json"))
""")
        return [sys.executable, str(program)]

    def test_waits_for_game_boundary_and_releases_cpu_on_completion(self):
        self.config()
        with (self.control / "shared.lock").open("a+") as game:
            fcntl.flock(game, fcntl.LOCK_EX)
            p = self.launch_worker()
            wait_for(lambda: (self.control / "analysis.request").exists())
            self.assertFalse((self.root / "started.json").exists())
            fcntl.flock(game, fcntl.LOCK_UN)
            wait_for(lambda: (self.root / "started.json").exists())
            with self.assertRaises(BlockingIOError):
                fcntl.flock(game, fcntl.LOCK_EX | fcntl.LOCK_NB)
            self.assertEqual(json.loads((self.root / "started.json").read_text())["cpu"], [self.cpus[3]])
            (self.root / "training.release").touch()
            self.assertEqual(p.wait(timeout=5), 0)
            self.assertFalse((self.control / "analysis.request").exists())
            fcntl.flock(game, fcntl.LOCK_EX | fcntl.LOCK_NB)
            self.assertEqual(a.read(self.run / "training/status.json")["state"], "completed")

    def test_interruption_and_parent_death_stop_child_before_lease_is_free(self):
        for force in (False, True):
            with self.subTest(force=force):
                (self.root / "started.json").unlink(missing_ok=True)
                config = self.config()
                # Exercise the actual middle supervisor too: sidecar ->
                # continuation runner -> trainer, with the lease inherited twice.
                (self.root / "train.py").write_bytes((self.root / "fake_training.py").read_bytes())
                output = self.root / "out"
                output.mkdir(exist_ok=True)
                (output / "w32.training.pt").touch()
                job = dict(
                    out=str(output),
                    python=sys.executable,
                    widths=[32],
                    dataset="unused",
                    base="unused",
                    generation="test",
                    policy={},
                    recipes={"32": dict(seed=1, batch=4, embedding_lr=0.001, head_lr=0.001)},
                )
                wrapper = (
                    f"import sys;from pathlib import Path;sys.path.insert(0,{str(a.ROOT / 'training/nnue')!r});"
                    f"import continuation as c;c.HERE=Path({str(self.root)!r});c.run({job!r})"
                )
                config["training"]["command"] = [sys.executable, "-c", wrapper]
                a.atomic(self.control / "config.json", config)
                p = self.launch_worker()
                wait_for(lambda: (self.root / "started.json").exists())
                pid = a.read(self.root / "started.json")["pid"]
                p.send_signal(signal.SIGKILL if force else signal.SIGTERM)
                p.wait(timeout=6)
                wait_for(lambda pid=pid: a.process_identity(pid) is None)
                with (self.control / "shared.lock").open("a+") as game:
                    fcntl.flock(game, fcntl.LOCK_EX | fcntl.LOCK_NB)
                if not force:
                    self.assertFalse((self.control / "analysis.request").exists())
                    self.assertEqual(a.read(self.run / "training/status.json")["state"], "interrupted")
                (self.control / "analysis.request").unlink(missing_ok=True)

    def test_supervisor_finite_success_and_failure_return_fourth_worker(self):
        for code, after_startup in ((0, False), (7, False), (7, True)):
            with self.subTest(code=code, after_startup=after_startup):
                for name in (
                    "fourth.json",
                    "started.json",
                    "finished.json",
                    "training.release",
                    "startup.waiting",
                    "startup.release",
                ):
                    (self.root / name).unlink(missing_ok=True)
                (self.control / "supervisor.json").unlink(missing_ok=True)
                config = self.config(code=code)
                game = self.root / "fake_tournament.py"
                game.write_text(
                    "import fcntl,json,os,time\nfrom pathlib import Path\n"
                    f'root=Path({str(self.root)!r})\ncontrol=Path(os.environ["TAIKYOKU_COMPUTE_DIR"])\n'
                    "while True:\n"
                    ' if not (control/"analysis.request").exists():\n'
                    '  with (control/"shared.lock").open("a+") as f:\n'
                    "   try:\n"
                    "    fcntl.flock(f,fcntl.LOCK_EX|fcntl.LOCK_NB)\n"
                    '    if not (root/"fourth.json").exists():\n'
                    '     temporary=root/"fourth.tmp"\n'
                    "     temporary.write_text(json.dumps(dict(time=time.monotonic())))\n"
                    '     temporary.replace(root/"fourth.json")\n'
                    "   except BlockingIOError:pass\n"
                    " time.sleep(.02)\n"
                )
                config["command"] = [sys.executable, str(game)]
                a.atomic(self.control / "config.json", config)
                p = subprocess.Popen(
                    self.supervisor_command(finish_before_startup=not after_startup),
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                )
                self.procs.append(p)
                wait_for(lambda: (self.root / "startup.waiting").exists() and (self.root / "started.json").exists())
                self.assertFalse((self.root / "fourth.json").exists())
                if not after_startup:
                    (self.root / "training.release").touch()
                (self.root / "startup.release").touch()
                if after_startup:
                    wait_for(lambda: (self.control / "supervisor.json").exists())
                    self.assertEqual(a.read(self.control / "supervisor.json")["sidecar_state"], "running")
                    self.assertFalse((self.root / "fourth.json").exists())
                    (self.root / "training.release").touch()
                wait_for(lambda: (self.root / "fourth.json").exists())
                wait_for(
                    lambda: (
                        (self.control / "supervisor.json").exists()
                        and a.read(self.control / "supervisor.json")["sidecar_state"] != "running"
                    )
                )
                meta = a.read(self.control / "supervisor.json")
                self.assertEqual(meta["state"], "running")
                self.assertEqual(meta["sidecar_state"], "completed" if code == 0 else "failed")
                self.assertIsNone(meta["analyzer_pid"])
                self.assertGreaterEqual(
                    a.read(self.root / "fourth.json")["time"], a.read(self.root / "finished.json")["time"]
                )
                p.terminate()
                p.wait(timeout=5)

    def test_preflight_rejects_live_output_and_dry_run_does_not_write(self):
        model = self.root / "live/model.json"
        model.parent.mkdir()
        model.write_text("{}")
        a.atomic(self.run / "state.json", {"entrants": [{"model": str(model)}]})
        cfg = self.root / "request.json"
        cfg.write_text(json.dumps({"python": sys.executable}))
        fake = dict(python=sys.executable, out=str(model.parent), code_hashes={})
        with patch.object(
            sidecar.subprocess, "run", return_value=SimpleNamespace(returncode=0, stdout=json.dumps(fake))
        ):
            with self.assertRaisesRegex(ValueError, "live run/model"):
                sidecar.prepare(a, str(cfg), self.run, dry_run=True)
        before = set((self.run / "training").iterdir())
        fake["out"] = str(self.root / "output")
        with patch.object(
            sidecar.subprocess, "run", return_value=SimpleNamespace(returncode=0, stdout=json.dumps(fake))
        ):
            sidecar.prepare(a, str(cfg), self.run, dry_run=True)
        self.assertEqual(set((self.run / "training").iterdir()), before)

    def test_storage_preflight_does_not_launch_children(self):
        with (
            patch.object(a.storage_guard, "require_healthy", side_effect=RuntimeError("disk reserve")),
            patch.object(a.subprocess, "Popen") as launch,
        ):
            with self.assertRaisesRegex(RuntimeError, "disk reserve"):
                a.supervise({})
            launch.assert_not_called()

    def test_unexpected_zero_exit_is_supervisor_failure(self):
        config = dict(
            run=str(self.run),
            cpus=self.cpus,
            sidecar="none",
            command=[
                sys.executable,
                "-c",
                "import time; from pathlib import Path; "
                f"control=Path({str(self.control)!r}); "
                "\nwhile not (control/'supervisor.json').exists(): time.sleep(.01)\n",
            ],
        )
        a.atomic(self.control / "config.json", config)
        (self.root / "startup.release").touch()
        env = dict(os.environ, TAIKYOKU_STORAGE_CONFIG=str(self.root / "no-production-policy"))
        result = subprocess.run(self.supervisor_command(), env=env, capture_output=True, text=True, timeout=8)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("tournament exited unexpectedly (0)", result.stderr)
        self.assertEqual(a.read(self.control / "supervisor.json")["state"], "failed")


if __name__ == "__main__":
    unittest.main()
