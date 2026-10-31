import contextlib
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

import update_tourney_engine as update


class EngineUpdate(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.run = self.root / "run"
        self.control = self.run / "analysis"
        self.control.mkdir(parents=True)
        update.analysis.atomic(self.control / "config.json", {"rolling_engines": True})
        engine = self.root / "old-engine"
        engine.write_text("#!/bin/sh\nexit 0\n")
        engine.chmod(0o755)
        digest = update.analysis.file_digest(engine)
        self.bundle = dict(
            protocol=1,
            revision="active-custom-source",
            engine_bin=str(engine),
            engine_sha256=digest,
            analyzer_bin=str(engine),
            analyzer_sha256=digest,
        )
        update.analysis.atomic(self.control / "active-engine.json", self.bundle)
        self.pointer = (self.control / "active-engine.json").read_bytes()
        self.source = self.root / "non-git-archive"
        self.source.mkdir()
        (self.source / "Cargo.toml").write_text('[package]\nname="archive"\nversion="0.1.0"\n')
        self.output = self.root / "prepared"
        self.source_identity = dict(description="archive-sha256:" + "a" * 64, manifest_sha256="a" * 64)
        self.compiler = "rustc fixture\nhost: fixture-host\nLLVM version: 22.1.6\n"
        self.calls = []
        self.fail = None
        self.drift = False

    def args(self, *extra):
        args = update.parser().parse_args(
            [
                str(self.run),
                "--compatible-speedup",
                "--pgo",
                "--source-dir",
                str(self.source),
                "--out",
                str(self.output),
                *extra,
            ]
        )
        update.validate_args(args)
        return args

    def command(self, instance, stage, argv, **kwargs):
        argv = list(map(str, argv))
        self.calls.append((stage, argv, kwargs))
        instance.stage = stage
        instance.save()
        if stage == self.fail:
            raise RuntimeError("injected " + stage + " failure")
        if stage == "prepare-corpus":
            inputs = Path(argv[argv.index("--out") + 1])
            inputs.mkdir()
            corpus = dict(positions=[{}, {}], models=[{}, {}])
            update.analysis.atomic(inputs / "corpus.json", corpus)
            update.analysis.atomic(inputs / "verify-corpus.json", corpus)
        elif stage == "compiler":
            return self.compiler
        elif stage == "pgo-build":
            directory = self.output / "pgo"
            (directory / "bin").mkdir(parents=True)
            identity = dict(self.source_identity)
            if self.drift:
                identity["manifest_sha256"] = "b" * 64
            hashes = {}
            for name in ("taikyoku_shogi", "analyze_position", "benchmark"):
                path = directory / "bin" / name
                path.write_text("fixture " + name)
                hashes[name] = update.analysis.file_digest(path)
            update.analysis.atomic(
                directory / "build.json",
                {
                    "status": "complete",
                    "source": identity,
                    "rustc": self.compiler,
                    "binaries": hashes,
                },
            )

    def execute(self, args):
        with (
            patch.object(update.Update, "command", autospec=True, side_effect=self.command),
            patch.object(update, "source_identity", return_value=self.source_identity),
        ):
            update.pgo(args)

    def test_non_git_prepare_only_forwards_models_and_keeps_active_pointer(self):
        args = self.args(
            "--prepare-only", "--models", "agent one", "agent-two", "--max-models", "8", "--verify-games", "3"
        )
        self.execute(args)
        self.assertEqual(
            [stage for stage, _, _ in self.calls],
            [
                "prepare-corpus",
                "compiler",
                "native-build",
                "pgo-build",
                "verification",
            ],
        )
        prepare = self.calls[0][1]
        self.assertEqual(prepare[prepare.index("--models") + 1 :], ["agent one", "agent-two"])
        self.assertEqual(prepare[prepare.index("--verify-games") + 1], "3")
        native = self.calls[2]
        self.assertEqual(native[1][-2:], ["--target", "fixture-host"])
        self.assertEqual(native[2]["cwd"], self.source)
        self.assertEqual(native[2]["env"]["CARGO_BUILD_JOBS"], "1")
        pgo = self.calls[3]
        self.assertEqual(pgo[1][pgo[1].index("--source-dir") + 1], str(self.source))
        self.assertEqual(pgo[1][3], "auto")
        self.assertEqual(pgo[2]["env"], native[2]["env"])
        self.assertIn("--require-speedup", self.calls[-1][1])
        self.assertEqual((self.control / "active-engine.json").read_bytes(), self.pointer)
        record = update.analysis.read(self.output / "update.json")
        self.assertEqual(record["status"], "prepared")
        self.assertEqual(record["revision"], self.source_identity["description"])

    def test_publication_is_last_and_uses_captured_build_cas(self):
        self.execute(self.args())
        stage, argv, _ = self.calls[-1]
        self.assertEqual(stage, "publication")
        self.assertEqual(argv[argv.index("--revision") + 1], self.source_identity["description"])
        expected = update.analysis.digest(json.dumps(self.bundle, sort_keys=True).encode())
        self.assertEqual(argv[argv.index("--expected-build") + 1], expected)
        self.assertEqual(update.analysis.read(self.output / "update.json")["status"], "published")

    def test_each_failed_preparation_stage_leaves_selection_and_evidence(self):
        for stage in ("prepare-corpus", "native-build", "pgo-build", "verification"):
            with self.subTest(stage=stage):
                self.output = self.root / stage
                self.calls.clear()
                self.fail = stage
                with self.assertRaisesRegex(RuntimeError, "No engine publication was attempted"):
                    self.execute(self.args())
                self.assertFalse(any(name == "publication" for name, _, _ in self.calls))
                self.assertEqual((self.control / "active-engine.json").read_bytes(), self.pointer)
                record = update.analysis.read(self.output / "update.json")
                self.assertEqual((record["status"], record["stage"]), ("failed", stage))

    def test_source_drift_rejects_before_verification_and_publication(self):
        self.drift = True
        with self.assertRaisesRegex(RuntimeError, "different source identities"):
            self.execute(self.args())
        self.assertEqual(self.calls[-1][0], "pgo-build")
        self.assertEqual((self.control / "active-engine.json").read_bytes(), self.pointer)

    def test_preexisting_output_and_unenabled_run_do_not_start_helpers(self):
        self.output.mkdir()
        marker = self.output / "operator-file"
        marker.write_text("preserve")
        with self.assertRaises(FileExistsError):
            self.execute(self.args())
        self.assertEqual(marker.read_text(), "preserve")
        self.assertEqual(self.calls, [])
        update.analysis.atomic(self.control / "config.json", {"rolling_engines": False})
        with self.assertRaisesRegex(ValueError, "rolling-engines"):
            self.execute(self.args())
        self.assertEqual(self.calls, [])

    def test_old_native_positional_ref_and_source_exclusion(self):
        args = update.parser().parse_args([str(self.run), "--compatible-speedup", "origin/custom"])
        self.assertEqual(args.ref, "origin/custom")
        self.assertFalse(args.pgo)

        @contextlib.contextmanager
        def source(_args, _instance):
            yield self.source, "requested-revision"

        with (
            patch.object(update, "source_tree", source),
            patch.object(update.Update, "command", autospec=True, side_effect=self.command),
        ):
            update.native(args)
        self.assertEqual([name for name, _, _ in self.calls], ["native-build", "publication"])
        self.assertEqual(self.calls[-1][1][self.calls[-1][1].index("--revision") + 1], "requested-revision")
        self.assertNotIn("--expected-build", self.calls[-1][1])
        with contextlib.redirect_stderr(None), self.assertRaises(SystemExit):
            update.parser().parse_args(
                [
                    str(self.run),
                    "--compatible-speedup",
                    "origin/main",
                    "--source-dir",
                    str(self.source),
                ]
            )

    def test_odd_verification_repetitions_are_rejected(self):
        with self.assertRaisesRegex(ValueError, "even"):
            self.args("--verify-repeats", "3")

    def test_termination_reaps_stage_descendants(self):
        pid_file = self.root / "grandchild.pid"
        child = self.root / "child.py"
        child.write_text(
            "import subprocess,sys,time\n"
            "from pathlib import Path\n"
            "code='import signal,time;signal.signal(signal.SIGTERM,signal.SIG_IGN);time.sleep(60)'\n"
            "p=subprocess.Popen([sys.executable,'-c',code])\n"
            "Path(sys.argv[1]).write_text(str(p.pid))\n"
            "time.sleep(60)\n"
        )
        script = (
            "import signal,sys\n"
            f"sys.path.insert(0,{str(update.ROOT / 'deploy')!r})\n"
            "import update_tourney_engine as u\n"
            "from types import SimpleNamespace\n"
            "signal.signal(signal.SIGTERM,u.interrupted)\n"
            f"u.Update(SimpleNamespace(run='fixture',nice=19)).command('fixture',[sys.executable,{str(child)!r},{str(pid_file)!r}],capture=True)\n"
        )
        proc = subprocess.Popen([sys.executable, "-c", script], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        descendant = None
        try:
            deadline = time.monotonic() + 5
            while not pid_file.exists() and time.monotonic() < deadline:
                time.sleep(0.02)
            self.assertTrue(pid_file.exists())
            descendant = int(pid_file.read_text())
            proc.send_signal(signal.SIGTERM)
            proc.communicate(timeout=15)
            self.assertNotEqual(proc.returncode, 0)
            deadline = time.monotonic() + 2
            while update.analysis.process_identity(descendant) is not None and time.monotonic() < deadline:
                time.sleep(0.02)
            self.assertIsNone(update.analysis.process_identity(descendant))
        finally:
            if proc.poll() is None:
                proc.kill()
            proc.communicate()
            if descendant:
                with contextlib.suppress(ProcessLookupError):
                    os.kill(descendant, signal.SIGKILL)


if __name__ == "__main__":
    unittest.main()
