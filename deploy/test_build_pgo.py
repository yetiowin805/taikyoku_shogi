"""The PGO build transaction is exercised with fake compiler/LLVM tools."""

import contextlib
import importlib.util
import io
import json
import os
import signal
import subprocess
import sys
import time
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from test_pgo_workload import WorkloadFixture

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("pgo_build", ROOT / "deploy/pgo_build.py")
b = importlib.util.module_from_spec(spec)
spec.loader.exec_module(b)

RUSTC = """#!/usr/bin/env python3
import os,sys
if sys.argv[1:]==["--print","sysroot"]: print(os.environ["TEST_SYSROOT"])
else: print("rustc 1.97.1 (fixture)\\nhost: x86_64-unknown-linux-gnu\\nLLVM version: 22.1.6")
"""
CARGO = """#!/usr/bin/env python3
import json,os,shutil,sys
from pathlib import Path
if sys.argv[1:]==["-V"]:
    print("cargo 1.97.1 (fixture)")
    raise SystemExit(0)
flags=os.environ["CARGO_ENCODED_RUSTFLAGS"]
stage="profile-use" if "profile-use=" in flags else "instrumented"
with Path(os.environ["FAKE_CARGO_CALLS"]).open("a") as output:
    output.write(json.dumps(dict(cwd=os.getcwd(),argv=sys.argv,flags=flags,
        marker=Path("src/lib.rs").read_text(),jobs=os.environ.get("CARGO_BUILD_JOBS")))+"\\n")
if os.environ.get("FAKE_FAIL")==stage: raise SystemExit(9)
host=sys.argv[sys.argv.index("--target")+1]
release=Path(os.environ["CARGO_TARGET_DIR"])/host/"release"
(release/"examples").mkdir(parents=True,exist_ok=True)
for target in (release/"examples/nnue_tournament_bench",release/"taikyoku_shogi",release/"analyze_position"):
    shutil.copy2(os.environ["FAKE_TEMPLATE"], target)
if os.environ.get("FAKE_MUTATE") and stage=="profile-use":
    Path("src/lib.rs").write_text("changed during compilation")
"""
PROFDATA = """#!/usr/bin/env python3
import os,sys
from pathlib import Path
if sys.argv[1:]==["--version"]:
    print("LLVM version VERSION")
else:
    if os.environ.get("FAKE_FAIL")=="merge": raise SystemExit(8)
    Path(sys.argv[sys.argv.index("-o")+1]).write_bytes(b"merged fixture profile")
"""


class BuildPGOTests(WorkloadFixture):
    def setUp(self):
        super().setUp()
        self.source = self.root / "source"
        (self.source / "src").mkdir(parents=True)
        (self.source / "examples").mkdir()
        (self.source / "Cargo.toml").write_text('[package]\nname="fixture"\nversion="0.1.0"\n')
        (self.source / "Cargo.lock").write_text("fixture lock\n")
        (self.source / "src/lib.rs").write_text("the requested preserved source\n")
        (self.source / "examples/nnue_tournament_bench.rs").write_text("fixture benchmark source\n")
        # This poison runner proves the installed runner is used instead of REF tooling.
        poison = self.source / "benchmarks/nnue_followup/profile_workload.py"
        poison.parent.mkdir(parents=True)
        poison.write_text("raise SystemExit('wrong source-tree runner')\n")
        self.tools = self.root / "tools"
        self.tools.mkdir()
        self.sysroot = self.root / "sysroot"
        self.llvm = self.sysroot / "lib/rustlib/x86_64-unknown-linux-gnu/bin/llvm-profdata"
        self.llvm.parent.mkdir(parents=True)
        self.write_tool(self.tools / "rustc", RUSTC)
        self.write_tool(self.tools / "cargo", CARGO)
        self.write_tool(self.llvm, PROFDATA.replace("VERSION", "22.1.6"))
        self.calls = self.root / "cargo-calls.jsonl"
        self.output = self.root / "pgo-output"
        self.env = dict(
            os.environ,
            PATH=str(self.tools) + os.pathsep + os.environ["PATH"],
            TEST_SYSROOT=str(self.sysroot),
            FAKE_STARTS=str(self.starts),
            FAKE_TEMPLATE=str(self.binary),
            FAKE_CARGO_CALLS=str(self.calls),
            RUSTFLAGS="-C opt-level=3",
        )
        for name in ("CARGO_ENCODED_RUSTFLAGS", "CARGO_BUILD_TARGET", "RUSTC", "FAKE_MODE", "FAKE_FAIL", "FAKE_MUTATE"):
            self.env.pop(name, None)

    def write_tool(self, path, text):
        path.write_text(text)
        path.chmod(0o755)

    def build(self, profdata="auto", *, env=None, extra=()):
        return subprocess.run(
            [
                "bash",
                str(ROOT / "deploy/build_pgo.sh"),
                str(self.corpus_path),
                str(self.output),
                str(profdata),
                "--source-dir",
                str(self.source),
                "--positions",
                "0",
                "--models",
                "0",
                "1",
                "--repeats",
                "1",
                *extra,
            ],
            env=self.env if env is None else env,
            text=True,
            capture_output=True,
            timeout=15,
        )

    def metadata(self):
        return json.loads((self.output / "build.json").read_text())

    def test_archive_build_pins_source_and_uses_installed_runner(self):
        result = self.build()
        self.assertEqual(result.returncode, 0, result.stderr)
        meta = self.metadata()
        self.assertEqual(meta["status"], "complete")
        self.assertEqual(meta["source_dir"], str(self.source))
        self.assertEqual(meta["source"]["kind"], "archive")
        self.assertIsNone(meta["source"]["revision"])
        self.assertIsNone(meta["source"]["diff"])
        self.assertTrue(meta["source"]["description"].startswith("archive-sha256:"))
        self.assertEqual(meta["llvm"]["profdata"], str(self.llvm))
        self.assertEqual(set(meta["binaries"]), {"instrumented", "benchmark", "taikyoku_shogi", "analyze_position"})
        self.assertIn(str(self.blob), meta["workload"]["inputs"])
        self.assertEqual(len(meta["profiles"]), 2)
        calls = [json.loads(line) for line in self.calls.read_text().splitlines()]
        self.assertEqual(len(calls), 2)
        self.assertEqual({call["cwd"] for call in calls}, {str(self.source)})
        self.assertTrue(all(call["marker"] == "the requested preserved source\n" for call in calls))
        self.assertTrue(all("target-cpu=native" in call["flags"] and "opt-level=3" in call["flags"] for call in calls))
        self.assertTrue(all(call["jobs"] == "1" for call in calls))
        command = next(command for command in meta["commands"] if command["stage"] == "profile workload")
        self.assertEqual(command["argv"][1], str(ROOT / "benchmarks/nnue_followup/profile_workload.py"))
        self.assert_reaped()

    def test_path_fallback_and_explicit_tool_still_require_exact_llvm(self):
        self.write_tool(self.llvm, PROFDATA.replace("VERSION", "22.1.5"))
        fallback = self.tools / "llvm-profdata-22"
        self.write_tool(fallback, PROFDATA.replace("VERSION", "22.1.6"))
        result = self.build()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.metadata()["llvm"]["profdata"], str(fallback))
        self.output = self.root / "mismatch-output"
        result = self.build(self.llvm)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("LLVM discovery", result.stderr)
        self.assertIn("no tournament was started or updated", result.stderr)
        self.assertEqual(self.metadata()["status"], "failed")

    def test_each_failed_stage_is_loud_and_preserves_failure_metadata(self):
        for stage in ("instrumented", "merge", "profile-use"):
            with self.subTest(stage=stage):
                self.output = self.root / stage
                result = self.build(env=dict(self.env, FAKE_FAIL=stage))
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("PGO build failed during", result.stderr)
                self.assertIn("no tournament was started or updated", result.stderr)
                self.assertEqual(self.metadata()["status"], "failed")
                self.assertFalse((self.output / "bin/benchmark").exists())

    def test_source_mutation_after_profile_rejects_success(self):
        result = self.build(env=dict(self.env, FAKE_MUTATE="1"))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("source tree changed", result.stderr)
        self.assertEqual(self.metadata()["status"], "failed")

    def test_workload_binary_override_cannot_profile_a_different_artifact(self):
        result = self.build(extra=("--binary", str(self.binary)))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("freshly built instrumented binary", result.stderr)
        self.assertEqual(self.metadata()["status"], "failed")
        self.assertEqual(len(self.calls.read_text().splitlines()), 1)

    def test_interruption_kills_descendant_after_direct_child_exits(self):
        descendant = self.root / "descendant.py"
        pidfile = self.root / "descendant.pid"
        parent_pidfile = self.root / "parent.pid"
        descendant.write_text(
            "import os,signal,time\n"
            "from pathlib import Path\n"
            "signal.signal(signal.SIGTERM,signal.SIG_IGN)\n"
            f"pidfile=Path({str(pidfile)!r})\n"
            "pidfile.with_suffix('.tmp').write_text(str(os.getpid()))\n"
            "pidfile.with_suffix('.tmp').replace(pidfile)\n"
            "time.sleep(30)\n"
        )
        parent = self.root / "parent.py"
        parent.write_text(
            "import os,subprocess,sys,time\n"
            "from pathlib import Path\n"
            f"subprocess.Popen([sys.executable,{str(descendant)!r}])\n"
            f"while not Path({str(pidfile)!r}).exists(): time.sleep(0.01)\n"
            f"pidfile=Path({str(parent_pidfile)!r})\n"
            "pidfile.with_suffix('.tmp').write_text(str(os.getpid()))\n"
            "pidfile.with_suffix('.tmp').replace(pidfile)\n"
        )
        wrapper = self.root / "wrapper.py"
        wrapper.write_text(
            "import signal,sys\n"
            "from pathlib import Path\n"
            "from types import SimpleNamespace\n"
            f"sys.path.insert(0,{str(ROOT / 'deploy')!r})\n"
            "import pgo_build as b\n"
            "signal.signal(signal.SIGTERM,b.interrupted)\n"
            f"args=SimpleNamespace(output=Path({str(self.output)!r}),source_dir=Path({str(self.source)!r}))\n"
            f"b.Build(args,[]).command([sys.executable,{str(parent)!r}],capture=True)\n"
        )
        proc = subprocess.Popen([sys.executable, str(wrapper)], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        pid = None
        try:
            deadline = time.monotonic() + 5
            while not pidfile.exists() and time.monotonic() < deadline:
                time.sleep(0.01)
            self.assertTrue(pidfile.exists())
            pid = int(pidfile.read_text())
            while time.monotonic() < deadline:
                if parent_pidfile.exists():
                    parent_stat = Path(f"/proc/{int(parent_pidfile.read_text())}/stat")
                    if not parent_stat.exists() or parent_stat.read_text().split(") ")[1].startswith("Z "):
                        break
                time.sleep(0.01)
            else:
                self.fail("direct builder child did not exit before interruption")
            proc.send_signal(signal.SIGTERM)
            _, stderr = proc.communicate(timeout=5)
            self.assertNotEqual(proc.returncode, 0, stderr)
            deadline = time.monotonic() + 2
            while time.monotonic() < deadline:
                stat = Path(f"/proc/{pid}/stat")
                if not stat.exists() or stat.read_text().split(") ")[1].startswith("Z "):
                    break
                time.sleep(0.01)
            else:
                self.fail("TERM-ignoring descendant survived builder interruption")
        finally:
            if proc.poll() is None:
                proc.kill()
                proc.communicate()
            if pid is not None:
                with contextlib.suppress(ProcessLookupError):
                    os.kill(pid, signal.SIGKILL)

    def test_encoded_empty_flags_override_ordinary_flags(self):
        result = self.build(env=dict(self.env, CARGO_ENCODED_RUSTFLAGS=""))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("opt-level=3", self.metadata()["flags"])

    def test_git_identity_includes_staged_untracked_and_ignores_build_output(self):
        for args in (
            ["init", "--quiet"],
            ["add", "."],
            ["-c", "user.name=fixture", "-c", "user.email=fixture@example.com", "commit", "--quiet", "-m", "fixture"],
        ):
            subprocess.run(["git", "-C", str(self.source), *args], check=True, capture_output=True)
        (self.source / "src/lib.rs").write_text("staged change\n")
        subprocess.run(["git", "-C", str(self.source), "add", "src/lib.rs"], check=True)
        untracked = self.source / "src/new.rs"
        untracked.write_text("untracked source\n")
        args = SimpleNamespace(output=self.output, source_dir=self.source)
        build = b.Build(args, [])
        identity = build.source_identity()
        self.assertEqual(identity["kind"], "git")
        self.assertIn("staged change", identity["diff"])
        self.assertIn("src/new.rs", identity["files"])
        self.output.mkdir()
        (self.output / "artifact").write_text("not source")
        target = self.source / "target"
        target.mkdir()
        (target / "artifact.rs").write_text("not source")
        self.assertEqual(build.source_identity(), identity)
        untracked.write_text("changed untracked source\n")
        self.assertNotEqual(build.source_identity(), identity)

    def test_source_symlink_escape_is_rejected_before_compilation(self):
        (self.source / "src/escape.rs").symlink_to(self.binary)
        result = self.build()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("symlink escapes", result.stderr)
        self.assertFalse(self.calls.exists())

    def test_existing_output_is_not_overwritten(self):
        self.output.mkdir()
        marker = self.output / "keep"
        marker.write_text("original")
        result = self.build()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("already exists", result.stderr)
        self.assertEqual(marker.read_text(), "original")

    def test_failure_message_survives_full_metadata_disk(self):
        build = SimpleNamespace(
            stage="instrumented build",
            metadata={},
            run=mock.Mock(side_effect=RuntimeError("compiler failed")),
            save=mock.Mock(side_effect=OSError("disk full")),
        )
        stderr = io.StringIO()
        with (
            mock.patch.object(b, "Build", return_value=build),
            mock.patch.object(b.sys, "argv", ["pgo_build.py", "corpus", "output", "auto"]),
            mock.patch.object(b.sys, "stderr", stderr),
            mock.patch.object(b.signal, "signal"),
            self.assertRaises(SystemExit) as error,
        ):
            b.main()
        self.assertEqual(error.exception.code, 1)
        self.assertIn("failed during instrumented build: compiler failed", stderr.getvalue())
        self.assertIn("no tournament was started or updated", stderr.getvalue())
        self.assertIn("Could not save failed PGO build metadata: disk full", stderr.getvalue())


if __name__ == "__main__":
    unittest.main()
