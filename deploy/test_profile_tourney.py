"""Identity, isolation and failure paths use fake /proc and tiny fake perf tools."""

import contextlib
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
from unittest.mock import patch

import profile_tourney as profile

PERF = """#!/usr/bin/env python3
import json,os,sys,time
from pathlib import Path
mode=os.environ.get('FAKE_PERF_MODE','')
with Path(os.environ['FAKE_PERF_LOG']).open('a') as out:
    out.write(json.dumps(dict(argv=sys.argv[1:],debug=os.environ.get('DEBUGINFOD_URLS')))+'\\n')
if sys.argv[1]=='--version': print('perf version fixture')
elif sys.argv[1]=='record':
    Path(sys.argv[sys.argv.index('-o')+1]).write_bytes(b'private fixture samples')
    if mode=='fail': raise SystemExit(7)
    if mode=='hang': time.sleep(30)
    path=Path(os.environ['FAKE_PROC_STAT'])
    text=path.read_text();prefix,rest=text.rsplit(') ',1);fields=rest.split()
    fields[11]=str(int(fields[11])+25)
    if mode=='reuse': fields[19]=str(int(fields[19])+1)
    path.write_text(prefix+') '+' '.join(fields))
elif sys.argv[1]=='report':
    if mode=='report-fail': raise SystemExit(8)
    print('  75.00% fixture::search')
elif sys.argv[1]=='script':
    if mode!='zero':
        print('778' if mode=='wrong-pid' else '777')
        print('777')
"""


class ProfileTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.run = self.root / "run"
        self.workers = self.run / "analysis/game-workers"
        self.workers.mkdir(parents=True)
        self.proc = self.root / "proc"
        self.pid = self.proc / "777"
        self.pid.mkdir(parents=True)
        (self.pid / "status").write_text("Cpus_allowed_list:\t2-3\n")
        boot_id = self.proc / "sys/kernel/random/boot_id"
        boot_id.parent.mkdir(parents=True)
        boot_id.write_text("fixture-boot-id\n")
        self.engine = self.root / "engine"
        self.engine.write_text("fixture immutable engine\n")
        self.engine.chmod(0o755)
        (self.pid / "exe").symlink_to(self.engine)
        (self.pid / "cwd").symlink_to(self.root)
        fields = ["0"] * 30
        fields[0], fields[11], fields[12], fields[19] = "R", "100", "20", "990"
        (self.pid / "stat").write_text("777 (engine) " + " ".join(fields))
        self.models = []
        for name in ("A", "B"):
            model = self.root / (name + ".json")
            model.write_text("{}")
            self.models.append(dict(id=name, model=str(model)))
        self.state = dict(entrants=self.models, slots=[dict(id=1, status="running", model_a="A", model_b="B")])
        self.save_state()
        bundle = dict(
            protocol=1,
            revision="archive-sha256:fixture",
            engine_bin=str(self.engine),
            engine_sha256=profile.digest(self.engine),
            analyzer_bin=str(self.engine),
            analyzer_sha256=profile.digest(self.engine),
        )
        self.request = self.workers / "slot-1.request.json"
        self.request.write_text(
            json.dumps(
                dict(
                    protocol=1,
                    bundle=bundle,
                    config=dict(
                        black=dict(name="ab", model=self.models[0]["model"]),
                        white=dict(name="ab", model=self.models[1]["model"]),
                    ),
                )
            )
        )
        self.set_argv()
        self.tools = self.root / "tools"
        self.tools.mkdir()
        (self.tools / "perf").write_text(PERF)
        (self.tools / "perf").chmod(0o755)
        self.log = self.root / "perf-calls.jsonl"
        self.env = dict(
            PATH=str(self.tools) + os.pathsep + os.environ["PATH"],
            FAKE_PERF_LOG=str(self.log),
            FAKE_PROC_STAT=str(self.pid / "stat"),
            DEBUGINFOD_URLS="https://must-not-contact.invalid",
        )
        self.proc_patch = patch.object(profile, "PROC", self.proc)
        self.proc_patch.start()
        self.addCleanup(self.proc_patch.stop)

    def set_argv(self, request=None, command="tournament-game"):
        argv = [str(self.engine), command, str(request or self.request), str(self.workers / "slot-1.result.json")]
        (self.pid / "cmdline").write_bytes(b"\0".join(arg.encode() for arg in argv) + b"\0")

    def save_state(self):
        (self.run / "state.json").write_text(json.dumps(self.state))

    def args(self, name="capture", *extra):
        return profile.parser().parse_args(
            ["--run-dir", str(self.run), "--out", str(self.root / name), "--seconds", "1", *extra]
        )

    def collect(self, args=None, mode=""):
        with patch.dict(os.environ, dict(self.env, FAKE_PERF_MODE=mode)):
            return profile.collect(args or self.args())

    def test_capture_records_exact_pair_identity_private_files_and_self_only_report(self):
        original = self.request.read_bytes()
        result = self.collect()
        self.assertEqual(result["status"], "complete")
        self.assertEqual(result["samples"], 2)
        self.assertEqual(result["worker"]["model_pair"], ["A", "B"])
        self.assertEqual(result["boot_id"], "fixture-boot-id")
        self.assertEqual(result["pid_before"]["cpus_allowed_list"], "2-3")
        self.assertEqual(result["worker"]["models"][0]["checkpoint_sha256"], profile.digest(self.models[0]["model"]))
        self.assertEqual(result["pid_before"]["start_ticks"], result["pid_after"]["start_ticks"])
        self.assertGreater(result["process_cpu_seconds"], 0)
        self.assertEqual(self.request.read_bytes(), original)
        self.assertFalse((self.workers / "slot-1.result.json").exists())
        calls = [json.loads(line) for line in self.log.read_text().splitlines()]
        self.assertTrue(all(call["debug"] == "" for call in calls))
        record = calls[1]["argv"]
        self.assertIn("--no-inherit", record)
        self.assertIn("cpu-clock:u", record)
        self.assertNotIn("--call-graph", record)
        self.assertEqual(record[-3:], ["--", "sleep", "1"])
        self.assertIn("--no-children", calls[2]["argv"])
        self.assertIn("--no-call-graph", calls[2]["argv"])
        self.assertEqual(calls[2]["argv"][-3:], ["--sort", "symbol", "--no-call-graph"])
        self.assertTrue(all("cpu_seconds" in row for row in result["commands"]))
        output = self.root / "capture"
        self.assertEqual(output.stat().st_mode & 0o777, 0o700)
        for path in output.iterdir():
            self.assertEqual(path.stat().st_mode & 0o777, 0o600)
        for name, entry in result["artifacts"].items():
            self.assertEqual(profile.digest(output / name), entry["sha256"])

    def test_dwarf_mode_is_bounded_and_explicit(self):
        self.collect(self.args("dwarf", "--call-graph", "dwarf", "--pid", "777"))
        record = json.loads(self.log.read_text().splitlines()[1])["argv"]
        self.assertEqual(record[record.index("--call-graph") + 1], "dwarf,8192")

    def test_invalid_worker_request_executable_slot_and_model_pair_are_rejected(self):
        self.set_argv(command="analyze-position")
        with self.assertRaisesRegex(ValueError, "exact tournament-game"):
            profile.worker(self.run, 777)
        outside = self.root / "slot-1.request.json"
        outside.write_bytes(self.request.read_bytes())
        self.set_argv(request=outside)
        with self.assertRaisesRegex(ValueError, "outside this run"):
            profile.worker(self.run, 777)
        self.set_argv()
        self.engine.write_text("changed binary")
        with self.assertRaisesRegex(ValueError, "executable hash"):
            profile.worker(self.run, 777)
        request = profile.read(self.request)
        request["bundle"]["engine_sha256"] = profile.digest(self.engine)
        self.request.write_text(json.dumps(request))
        self.state["slots"][0]["status"] = "done"
        self.save_state()
        with self.assertRaisesRegex(ValueError, "not running"):
            profile.worker(self.run, 777)
        self.state["slots"][0]["status"] = "running"
        self.save_state()
        request["config"]["white"]["model"] = self.models[0]["model"]
        self.request.write_text(json.dumps(request))
        with self.assertRaisesRegex(ValueError, "model pair"):
            profile.worker(self.run, 777)

    def test_zero_wrong_pid_perf_failure_and_reused_pid_retain_failure_evidence(self):
        for mode in ("zero", "wrong-pid", "fail", "report-fail", "reuse"):
            with self.subTest(mode=mode):
                args = self.args(mode)
                with self.assertRaises((ValueError, RuntimeError)):
                    self.collect(args, mode)
                metadata = profile.read(args.out / "metadata.json")
                self.assertEqual(metadata["status"], "failed")
                self.assertIn("perf.data", metadata["artifacts"])
                if mode == "reuse":
                    self.assertIn("PID was reused", metadata["error"])

    def test_missing_perf_existing_output_and_single_collector_lock(self):
        args = self.args("missing")
        with (
            patch.object(profile.shutil, "which", return_value=None),
            self.assertRaisesRegex(RuntimeError, "perf is not installed"),
        ):
            profile.collect(args)
        self.assertEqual(profile.read(args.out / "metadata.json")["status"], "failed")
        marker = args.out / "keep"
        marker.write_text("original")
        with self.assertRaises(FileExistsError):
            self.collect(args)
        self.assertEqual(marker.read_text(), "original")
        with (self.run / "analysis/profile-tourney.lock").open("a+") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            with self.assertRaises(BlockingIOError):
                self.collect(self.args("locked"))
        self.assertFalse(self.log.exists())

    def test_command_timeout_kills_only_its_group_including_descendants(self):
        output = self.root / "timeout"
        output.mkdir()
        descendant_pid = output / "descendant.pid"
        helper = self.root / "hanging.py"
        child_code = (
            "import os,signal,time;from pathlib import Path;signal.signal(signal.SIGTERM,signal.SIG_IGN);"
            f"Path({str(descendant_pid)!r}).write_text(str(os.getpid()));time.sleep(30)"
        )
        helper.write_text(
            f"import subprocess,sys,time\nsubprocess.Popen([sys.executable,'-c',{child_code!r}])\ntime.sleep(30)\n"
        )
        game = subprocess.Popen([sys.executable, "-c", "import time;time.sleep(30)"])
        descendant = None
        try:
            with self.assertRaises(subprocess.TimeoutExpired):
                profile.command([sys.executable, str(helper)], output, "hang", 0.3, dict(commands=[]))
            self.assertIsNone(game.poll())
            descendant = int(descendant_pid.read_text())
            deadline = time.monotonic() + 2
            while time.monotonic() < deadline:
                stat = Path(f"/proc/{descendant}/stat")
                if not stat.exists() or stat.read_text().rsplit(") ", 1)[1].startswith("Z "):
                    break
                time.sleep(0.01)
            else:
                self.fail("collector descendant survived timeout")
        finally:
            game.kill()
            game.wait()
            if descendant is not None:
                with contextlib.suppress(ProcessLookupError):
                    os.kill(descendant, signal.SIGKILL)

    def test_cli_rejects_excessive_duration_frequency_and_invalid_pid(self):
        for extra in (("--seconds", "61"), ("--seconds", "0"), ("--frequency", "100"), ("--pid", "-1")):
            with (
                self.subTest(extra=extra),
                patch.object(
                    sys,
                    "argv",
                    ["profile_tourney.py", "--run-dir", str(self.run), "--out", str(self.root / "cli"), *extra],
                ),
            ):
                with self.assertRaises(SystemExit) as failure:
                    profile.main()
                self.assertEqual(failure.exception.code, 2)


if __name__ == "__main__":
    unittest.main()
