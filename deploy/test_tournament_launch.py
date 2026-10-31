import fcntl
import os
import shutil
import signal
import subprocess
import tempfile
import time
import unittest
from pathlib import Path


class TournamentLaunchTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        deploy = self.root / "deploy"
        deploy.mkdir()
        source = Path(__file__).parent
        self.profiles = sorted(source.glob("run_*swiss.sh"))
        for script in [
            *self.profiles,
            *source.glob("pull_build_*swiss.sh"),
            source / "tournament_launch.sh",
            source / "run_scale_sample.sh",
        ]:
            shutil.copy2(script, deploy / script.name)
        (self.root / "data/run").mkdir(parents=True)
        (self.root / "models").mkdir()
        (self.root / "models/ab-seed.json").write_text("{}")
        self.tools = self.root / "tools"
        self.tools.mkdir()
        for name in ["pgrep", "systemctl"]:
            stub = self.tools / name
            stub.write_text("#!/bin/sh\nexit 1\n")
            stub.chmod(0o755)
        self.engine = self.root / "engine"
        self.engine.write_text("""#!/usr/bin/env python3
import os
import sys
import time
from pathlib import Path
with Path(os.environ['CALLS']).open('a') as output:
    output.write(sys.argv[1] + '\\n')
if sys.argv[1] != 'tournament':
    time.sleep(float(os.environ.get('GRID_DELAY', '0')))
    code = int(os.environ.get('GRID_RC', '0'))
    if code:
        raise SystemExit(code)
    destination = Path(sys.argv[sys.argv.index('--out') + 1])
    destination.mkdir(parents=True, exist_ok=True)
    (destination / 'manifest.json').write_text('{}')
    raise SystemExit(0)
Path(os.environ['ENGINE_PID']).write_text(str(os.getpid()))
if os.environ.get('START_FAIL'):
    raise SystemExit(42)
if os.environ.get('START_EXIT_ZERO'):
    raise SystemExit(0)
time.sleep(float(os.environ.get('START_DELAY', '0')))
if not os.environ.get('NEVER_READY'):
    Path(os.environ['TAIKYOKU_TOURNAMENT_READY_FILE']).write_text(f'ready {os.getpid()}\\n')
while True:
    time.sleep(.05)
""")
        self.engine.chmod(0o755)
        self.notify = self.root / "notify"
        self.notify.write_text("#!/bin/sh\nexit 0\n")
        self.notify.chmod(0o755)
        self.manifest = self.root / "manifest.json"
        self.manifest.write_text("{}")
        self.pid = self.root / "data/run/test.pid"
        self.lock = self.root / "data/run/test.lock"
        self.calls = self.root / "calls"
        self.engine_pid = self.root / "engine.pid"
        self.groups = []
        self.addCleanup(self.kill_groups)

    def kill_groups(self):
        for group in self.groups:
            try:
                os.killpg(group, signal.SIGKILL)
            except ProcessLookupError:
                pass
        self.groups.clear()

    def start(self, profile="run_pst_swiss.sh", *, skip=True, detach=True, extra=(), **overrides):
        env = dict(
            os.environ,
            PATH=f"{self.tools}:{os.environ['PATH']}",
            BIN=str(self.engine),
            NOTIFY=str(self.notify),
            PID_FILE=str(self.pid),
            LOCK_FILE=str(self.lock),
            CALLS=str(self.calls),
            ENGINE_PID=str(self.engine_pid),
            TOURNEY_START_TIMEOUT_SECONDS="2",
        )
        env.pop("TAIKYOKU_LAUNCH_CHILD", None)
        env.pop("TAIKYOKU_TOURNAMENT_READY_FILE", None)
        env.update(overrides)
        command = ["bash", str(self.root / "deploy" / profile), "--run-id", "test", "--manifest", str(self.manifest)]
        if detach:
            command.append("--detach")
        if skip:
            command.append("--skip-gen")
        command.extend(extra)
        process = subprocess.Popen(
            command, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, start_new_session=True
        )
        self.groups.append(process.pid)
        return process

    def result(self, process):
        stdout, stderr = process.communicate(timeout=10)
        return subprocess.CompletedProcess(process.args, process.returncode, stdout, stderr)

    def test_all_profiles_finish_preflight_before_detaching(self):
        for profile in self.profiles:
            with self.subTest(profile=profile.name):
                result = self.result(self.start(profile.name, skip=False, GRID_DELAY="0.05", GRID_RC="42"))
                self.assertNotEqual(result.returncode, 0, result.stdout)
                self.assertIn("grid generation failed", result.stderr)
                self.assertIn("tournament did not start", result.stderr)
                self.assertNotIn("Running pid=", result.stdout)
                self.assertFalse(self.pid.exists())
                self.assertFalse(self.engine_pid.exists())
                self.assertFalse(list((self.root / "data/run").glob("*.log")))

    def test_slow_grid_failure_cannot_outlive_a_successful_detach_check(self):
        process = self.start(skip=False, GRID_DELAY="1.2", GRID_RC="42")
        result = self.result(process)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("grid generation failed", result.stderr)
        self.assertNotIn("Running pid=", result.stdout)
        self.assertFalse(self.pid.exists())
        self.assertFalse(list((self.root / "data/run").glob("*.log")))

    def test_missing_binary_manifest_resume_and_busy_lock_are_loud(self):
        cases = [
            dict(BIN=str(self.root / "missing")),
            dict(extra=("--manifest", str(self.root / "missing.json"))),
            dict(extra=("--resume",)),
        ]
        for options in cases:
            with self.subTest(options=options):
                result = self.result(self.start(**options))
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("tournament did not start", result.stderr)
                self.assertFalse(self.pid.exists())
        with self.lock.open("w") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            result = self.result(self.start())
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("lock failed", result.stderr)
            self.assertFalse(self.pid.exists())

    def test_detach_waits_for_engine_readiness_and_inherits_the_lock(self):
        process = self.start(START_DELAY="0.3")
        try:
            process.wait(timeout=0.1)
        except subprocess.TimeoutExpired:
            pass
        else:
            self.fail("launcher returned before the delayed engine was ready")
        self.assertFalse(self.pid.exists())
        result = self.result(process)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("Running pid=", result.stdout)
        self.assertTrue(self.pid.exists())
        self.assertEqual(self.calls.read_text().splitlines(), ["tournament"])
        with self.lock.open("w") as lock:
            with self.assertRaises(BlockingIOError):
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)

    def test_generated_grid_runs_once_and_the_child_reuses_preflight(self):
        result = self.result(self.start(skip=False))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.calls.read_text().splitlines(), ["pst-grid", "tournament"])

    def test_every_profile_can_acknowledge_readiness(self):
        for profile in self.profiles:
            with self.subTest(profile=profile.name):
                result = self.result(self.start(profile.name))
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertTrue(self.pid.exists())
                self.kill_groups()
                deadline = time.monotonic() + 2
                with self.lock.open("w") as lock:
                    while True:
                        try:
                            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                            break
                        except BlockingIOError:
                            self.assertLess(time.monotonic(), deadline)
                            time.sleep(0.01)

    def test_engine_failure_and_readiness_timeout_do_not_publish_a_pid(self):
        for options in [dict(START_FAIL="1"), dict(NEVER_READY="1", TOURNEY_START_TIMEOUT_SECONDS="1")]:
            with self.subTest(options=options):
                result = self.result(self.start(**options))
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("tournament did not start", result.stderr)
                self.assertNotIn("Running pid=", result.stdout)
                self.assertFalse(self.pid.exists())

    def test_foreground_startup_failure_does_not_publish_a_pid(self):
        result = self.result(self.start(detach=False, START_FAIL="1"))
        self.assertEqual(result.returncode, 42)
        self.assertIn("tournament did not start", result.stderr)
        self.assertFalse(self.pid.exists())

    def test_foreground_zero_exit_without_readiness_is_not_success(self):
        result = self.result(self.start(detach=False, START_EXIT_ZERO="1"))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("exited without readiness", result.stderr)
        self.assertFalse(self.pid.exists())

    def test_pull_wrappers_report_failed_build_output(self):
        for script in sorted((self.root / "deploy").glob("pull_build_*swiss.sh")):
            with self.subTest(script=script.name):
                result = subprocess.run(
                    ["bash", str(script), "--skip-pull", "--skip-build", "--no-kill"],
                    env=dict(os.environ, BIN=str(self.root / "missing")),
                    text=True,
                    capture_output=True,
                    timeout=5,
                )
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("tournament did not start", result.stderr)


if __name__ == "__main__":
    unittest.main()
