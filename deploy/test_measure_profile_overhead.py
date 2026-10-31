import collections
import contextlib
import io
import json
import math
import os
import signal
import tempfile
import threading
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

import measure_profile_overhead as observer


class ObserverCostTests(unittest.TestCase):
    def test_ack_accepts_only_line_with_optional_nul_terminator(self):
        for ack in (b"ack\n", b"ack\n\0"):
            observer.validate_ack(ack)
        for ack in (b"ack", b" ack\n", b"ack\n\0\0", b"ack\nextra", b"ack\r\n", b"ack\n\0extra"):
            with self.subTest(ack=ack), self.assertRaisesRegex(RuntimeError, "unexpected perf acknowledgement"):
                observer.validate_ack(ack)

    def test_equal_pair_estimate_and_paired_interval(self):
        rows = [
            dict(
                detached=dict(cpu_ms=10, ms=20),
                attached=dict(cpu_ms=11, ms=22, recorder=dict(cpu_seconds=0.1, sampling_wall_seconds=1)),
            ),
            dict(
                detached=dict(cpu_ms=100, ms=200),
                attached=dict(cpu_ms=90, ms=180, recorder=dict(cpu_seconds=0.2, sampling_wall_seconds=2)),
            ),
        ]
        result = observer.summarize(rows, seed=10)
        self.assertAlmostEqual(result["metrics"]["cpu_ms"]["attached_over_detached"], math.sqrt(1.1 * 0.9))
        self.assertEqual(result["metrics"]["cpu_ms"]["paired_bootstrap_95"], [0.9, 1.1])
        self.assertAlmostEqual(result["profiler_cpu_seconds"], 0.3)
        self.assertEqual(result["sampling_wall_seconds"], 3)
        self.assertIn("does not establish", result["interpretation"])

    def test_balanced_alternating_orders_per_root(self):
        entries = list(observer.schedule([0, 2], 8, 17))
        counts = collections.Counter((position, arms[0]) for _, position, arms in entries)
        self.assertEqual(set(counts.values()), {4})
        for position in (0, 2):
            starts = [arms[0] for _, root, arms in entries if root == position]
            self.assertTrue(all(a != b for a, b in zip(starts, starts[1:], strict=False)))

    def test_control_pipe_ack_and_stop_only_recorder(self):
        with tempfile.TemporaryDirectory() as temp:
            processes = []

            class FakePerf:
                def __init__(self, command, **options):
                    self.returncode = None
                    self.signals = []
                    self.command = command
                    Path(command[command.index("-o") + 1]).write_bytes(b"perf fixture")
                    control, ack = (os.dup(fd) for fd in options["pass_fds"])

                    def acknowledge():
                        try:
                            self.enable = os.read(control, 64)
                            os.write(ack, b"ack\n")
                        finally:
                            os.close(control)
                            os.close(ack)

                    self.thread = threading.Thread(target=acknowledge)
                    self.thread.start()
                    processes.append(self)

                def poll(self):
                    return self.returncode

                def send_signal(self, sig):
                    self.signals.append(sig)
                    self.returncode = 0

                def wait(self, timeout=None):
                    self.thread.join(timeout)
                    return self.returncode

            args = SimpleNamespace(frequency=19, call_graph="dwarf")
            with patch.object(observer.subprocess, "Popen", FakePerf):
                with observer.Recorder(1234, Path(temp) / "sample.perf.data", args, {}) as recorder:
                    self.assertEqual(processes[0].enable, b"enable\n")
                    self.assertIsNotNone(recorder.enabled)
            self.assertEqual(processes[0].signals, [signal.SIGINT])
            self.assertIn("--no-inherit", processes[0].command)
            self.assertIn("dwarf,8192", processes[0].command)
            self.assertGreater(recorder.result["perf_data_bytes"], 0)

    def test_sample_audit_hides_callchains_and_rejects_other_pids(self):
        with tempfile.TemporaryDirectory() as temp:
            recorder = dict(data_path=str(Path(temp) / "sample.perf.data"), target_pid=1234)
            rows = [dict(attached=dict(recorder=recorder))]
            with patch.object(observer.subprocess, "run", return_value=SimpleNamespace(stdout="1234\n")) as decode:
                observer.audit_samples(rows, {}, "/fixture/perf")
                self.assertIn("--hide-call-graph", decode.call_args.args[0])
                self.assertEqual(recorder["target_samples"], 1)
                decode.return_value.stdout = "5678\n"
                with self.assertRaisesRegex(ValueError, "outside its benchmark target"):
                    observer.audit_samples(rows, {}, "/fixture/perf")

    def experiment(self, mismatch=False, zero_samples=False, missing_perf=False):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        root = Path(self.tmp.name)
        game, model, binary = (root / name for name in ("game.json", "model.json", "benchmark"))
        for path in (game, model, binary):
            path.write_text("{}")
        corpus = dict(
            positions=[
                dict(name=f"root-{i}", game=str(game), ply=i, sha256=observer.workload.file_digest(game))
                for i in range(2)
            ],
            models=[dict(name="model", path=str(model), sha256=observer.workload.file_digest(model))],
        )
        corpus_path = root / "corpus.json"
        corpus_path.write_text(json.dumps(corpus))
        args = SimpleNamespace(
            binary=binary,
            corpus=corpus_path,
            out=root / "out",
            cpu=min(os.sched_getaffinity(0)),
            models=[0],
            positions=[0, 1],
            depth=2,
            repeats=4,
            batch_repeats=2,
            frequency=19,
            call_graph="none",
            seed=17,
            request_timeout_seconds=2,
        )
        state = dict(attached=False, exited=False)

        class FakeWorker:
            proc = SimpleNamespace(pid=1234)

            def __init__(self, *args, **kwargs):
                pass

            def __enter__(self):
                return self

            def __exit__(self, *args):
                state["exited"] = True

            def request(self, query):
                return dict(
                    query,
                    completed_depth=query["depth"],
                    name=corpus["positions"][query["position"]]["name"],
                    agent="model",
                    aborted=False,
                    signature=("b" if mismatch and state["attached"] else "a") * 64,
                    best=None,
                    score=10,
                    nodes=5,
                    q_nodes=2,
                    hash=123,
                    pieces=5,
                    legal=3,
                    nnue=False,
                    cpu_ms=11 if state["attached"] else 10,
                    ms=22 if state["attached"] else 20,
                )

        class FakeRecorder:
            def __init__(self, pid, path, *_args):
                self.result = None
                self.path = str(path)
                self.pid = pid

            def __enter__(self):
                state["attached"] = True
                return self

            def __exit__(self, *args):
                state["attached"] = False
                self.result = dict(cpu_seconds=0.001, sampling_wall_seconds=1, data_path=self.path, target_pid=self.pid)

        def perf_tool(command, **_options):
            return SimpleNamespace(
                stdout=("" if zero_samples else "1234\n1234\n") if command[1] == "script" else "perf fixture"
            )

        with contextlib.ExitStack() as stack:
            stack.enter_context(patch.object(observer.workload, "ProfileWorker", FakeWorker))
            stack.enter_context(patch.object(observer, "Recorder", FakeRecorder))
            stack.enter_context(patch.object(observer, "target_cpu", return_value=dict(start_ticks=1, cpu_ticks=100)))
            stack.enter_context(
                patch.object(observer.shutil, "which", return_value=None if missing_perf else "/fixture/perf")
            )
            stack.enter_context(patch.object(observer.subprocess, "run", side_effect=perf_tool))
            stack.enter_context(contextlib.redirect_stdout(io.StringIO()))
            if missing_perf:
                with self.assertRaisesRegex(RuntimeError, "perf executable not found"):
                    observer.run(args)
            elif mismatch or zero_samples:
                with self.assertRaisesRegex(
                    ValueError, "signature/nodes changed" if mismatch else "no target CPU samples"
                ):
                    observer.run(args)
            else:
                result = observer.run(args)
                self.assertEqual(result["pairs"], 8)
                self.assertEqual(result["measured_searches"], 32)
                self.assertAlmostEqual(result["metrics"]["cpu_ms"]["attached_over_detached"], 1.1)
        self.assertEqual(state["exited"], not missing_perf)
        return args.out

    def test_same_warm_worker_and_saved_equal_work_pairs(self):
        out = self.experiment()
        rows = [json.loads(line) for line in (out / "pairs.jsonl").read_text().splitlines()]
        self.assertTrue(all(row["attached"]["searches"] == row["detached"]["searches"] == 2 for row in rows))
        self.assertEqual(len((out / "responses.jsonl").read_text().splitlines()), 32)
        self.assertEqual(json.loads((out / "plan.json").read_text())["status"], "complete")
        self.assertEqual(out.stat().st_mode & 0o777, 0o700)
        self.assertEqual((out / "responses.jsonl").stat().st_mode & 0o777, 0o600)

    def test_mismatched_signature_fails_closed_and_cleans_up(self):
        out = self.experiment(mismatch=True)
        self.assertFalse((out / "summary.json").exists())
        self.assertEqual(json.loads((out / "plan.json").read_text())["status"], "failed")

    def test_header_only_recording_is_inconclusive(self):
        out = self.experiment(zero_samples=True)
        self.assertFalse((out / "summary.json").exists())
        self.assertIn("no target CPU samples", json.loads((out / "plan.json").read_text())["error"])

    def test_missing_perf_retains_failed_plan(self):
        out = self.experiment(missing_perf=True)
        self.assertFalse((out / "summary.json").exists())
        self.assertEqual(json.loads((out / "plan.json").read_text())["status"], "failed")


if __name__ == "__main__":
    unittest.main()
