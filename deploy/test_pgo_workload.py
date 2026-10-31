"""PGO protocol and process tests use tiny stand-ins, never real search."""

import hashlib
import importlib.util
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

WORKLOAD = Path(__file__).resolve().parents[1] / "benchmarks/nnue_followup/profile_workload.py"
spec = importlib.util.spec_from_file_location("pgo_workload", WORKLOAD)
w = importlib.util.module_from_spec(spec)
spec.loader.exec_module(w)

FAKE_BENCHMARK = """#!/usr/bin/env python3
import hashlib,json,os,sys,time
from pathlib import Path
corpus=json.loads(Path(sys.argv[1]).read_text())
mode=os.environ.get("FAKE_MODE", "")
with Path(os.environ["FAKE_STARTS"]).open("a") as output:
    output.write(json.dumps(dict(pid=os.getpid(),corpus=sys.argv[1]))+"\\n")
count=0
for line in sys.stdin:
    request=json.loads(line)
    count+=1
    if mode=="partial":
        sys.stdout.write("{")
        sys.stdout.flush()
        time.sleep(30)
    if mode=="eof":
        break
    result=dict(request, completed_depth=request["depth"],aborted=False,
        name=corpus["positions"][request["position"]]["name"],
        agent=corpus["models"][request["model"]]["name"],
        signature=hashlib.sha256(line.encode()).hexdigest())
    if mode=="wrong-model": result["model"]+=1
    if mode=="incomplete": result["completed_depth"]-=1
    if mode=="aborted": result["aborted"]=True
    if mode=="bad-signature": result["signature"]="unverified"
    if mode=="changing": result["signature"]=hashlib.sha256(str(count).encode()).hexdigest()
    if mode=="oversize":
        sys.stdout.write("x"*4097)
    else:
        encoded=(json.dumps(result,ensure_ascii=False)+"\\n").encode()
        sys.stdout.buffer.write(encoded[:3])
        sys.stdout.buffer.flush()
        sys.stdout.buffer.write(encoded[3:])
    sys.stdout.flush()
if mode!="no-profiles":
    profile=os.environ.get("LLVM_PROFILE_FILE")
    if profile:
        Path(profile.replace("%p",str(os.getpid())).replace("%m","fixture")).write_bytes(b"raw profile")
if mode=="nonzero": raise SystemExit(7)
"""


class WorkloadFixture(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.binary = self.root / "benchmark"
        self.binary.write_text(FAKE_BENCHMARK)
        self.binary.chmod(0o755)
        self.starts = self.root / "starts.jsonl"
        self.blob = self.root / "network.bin"
        self.blob.write_bytes(b"small fixture network")
        self.corpus = dict(positions=[], models=[])
        for index in range(2):
            game = self.root / f"game-{index}.json"
            game.write_text(json.dumps(dict(moves=[dict(color="Black")], result="Draw")))
            self.corpus["positions"].append(
                dict(name=f"root-{index}", game=str(game), ply=0, sha256=w.file_digest(game))
            )
            model = self.root / f"model-{index}.json"
            model.write_text(
                json.dumps(dict(weights=dict(nnue=dict(file=self.blob.name, sha256=w.file_digest(self.blob)))))
            )
            self.corpus["models"].append(dict(name=f"model-{index}", path=str(model), sha256=w.file_digest(model)))
        self.corpus_path = self.root / "corpus.json"
        self.save_corpus()

    def save_corpus(self):
        self.corpus_path.write_text(json.dumps(self.corpus))

    def args(self, **changes):
        args = dict(
            binary=self.binary,
            corpus=self.corpus_path,
            out=self.root / "output",
            positions=[0],
            models=[0, 1],
            repeats=2,
            depth=2,
            cpu=min(os.sched_getaffinity(0)),
            request_timeout_seconds=1,
        )
        args.update(changes)
        return SimpleNamespace(**args)

    def environment(self, mode=""):
        return patch.dict(os.environ, dict(FAKE_STARTS=str(self.starts), FAKE_MODE=mode))

    def assert_reaped(self):
        for line in self.starts.read_text().splitlines():
            pid = json.loads(line)["pid"]
            with self.assertRaises(ProcessLookupError):
                os.kill(pid, 0)


class ProfileWorkloadTests(WorkloadFixture):
    def test_model_processes_are_separate_and_plan_audits_all_eager_inputs(self):
        with self.environment():
            w.run(self.args())
        starts = [json.loads(line) for line in self.starts.read_text().splitlines()]
        self.assertEqual(len({row["pid"] for row in starts}), 2)
        plan = json.loads((self.root / "output/plan.json").read_text())
        self.assertEqual(plan["status"], "complete")
        self.assertEqual(len(plan["profiles"]), 2)
        self.assertIn(self.corpus["positions"][1]["game"], plan["inputs"])
        self.assertIn(str(self.blob), plan["inputs"])
        rows = [json.loads(line) for line in (self.root / "output/searches.jsonl").read_text().splitlines()]
        self.assertEqual([row["model"] for row in rows], [0, 0, 1, 1])
        self.assert_reaped()

    def test_invalid_indices_and_duplicate_indices_fail_before_spawn(self):
        for changes in (dict(models=[-1]), dict(positions=[2]), dict(models=[0, 0]), dict(positions=[0, 0])):
            with self.subTest(changes=changes), self.environment(), self.assertRaises(ValueError):
                w.run(self.args(**changes))
        self.assertFalse(self.starts.exists())

    def test_unrequested_root_and_selected_blob_are_hash_checked(self):
        game = Path(self.corpus["positions"][1]["game"])
        game.write_text("changed game")
        with self.environment(), self.assertRaisesRegex(ValueError, "profile input changed"):
            w.run(self.args())
        self.corpus["positions"][1]["sha256"] = w.file_digest(game)
        self.save_corpus()
        self.blob.write_bytes(b"changed blob")
        with self.environment(), self.assertRaisesRegex(ValueError, "profile input changed"):
            w.run(self.args())
        self.assertFalse(self.starts.exists())

    def test_invalid_results_and_failed_exit_are_rejected_and_reaped(self):
        for mode in (
            "wrong-model",
            "incomplete",
            "aborted",
            "bad-signature",
            "changing",
            "nonzero",
            "no-profiles",
            "eof",
        ):
            with self.subTest(mode=mode), self.environment(mode):
                with self.assertRaises((RuntimeError, ValueError)):
                    w.run(self.args(out=self.root / mode, models=[0]))
                plan = json.loads((self.root / mode / "plan.json").read_text())
                self.assertEqual(plan["status"], "failed")
                self.assert_reaped()

    def test_partial_line_has_deadline_and_oversized_record_is_bounded(self):
        with self.environment("partial"):
            started = time.monotonic()
            with self.assertRaises(TimeoutError):
                w.run(self.args(models=[0], request_timeout_seconds=0.2))
            self.assertLess(time.monotonic() - started, 3)
            self.assert_reaped()
        with self.environment("oversize"), patch.object(w, "MAX_LINE_BYTES", 4096):
            with self.assertRaisesRegex(ValueError, "line size"):
                w.run(self.args(out=self.root / "oversize", models=[0]))
            self.assert_reaped()

    def test_sigterm_reaps_child_and_marks_failed_plan(self):
        env = dict(os.environ, FAKE_STARTS=str(self.starts), FAKE_MODE="partial")
        command = [
            sys.executable,
            str(WORKLOAD),
            "--binary",
            str(self.binary),
            "--corpus",
            str(self.corpus_path),
            "--out",
            str(self.root / "output"),
            "--models",
            "0",
            "--positions",
            "0",
            "--repeats",
            "1",
        ]
        proc = subprocess.Popen(command, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        try:
            deadline = time.monotonic() + 5
            while not self.starts.exists() and time.monotonic() < deadline:
                time.sleep(0.02)
            self.assertTrue(self.starts.exists())
            proc.send_signal(signal.SIGTERM)
            proc.communicate(timeout=5)
            self.assertNotEqual(proc.returncode, 0)
            self.assert_reaped()
            self.assertEqual(json.loads((self.root / "output/plan.json").read_text())["status"], "failed")
        finally:
            if proc.poll() is None:
                proc.kill()
                proc.communicate()

    def test_python310_digest_matches_reference(self):
        self.assertEqual(w.file_digest(self.blob), hashlib.sha256(self.blob.read_bytes()).hexdigest())


if __name__ == "__main__":
    unittest.main()
