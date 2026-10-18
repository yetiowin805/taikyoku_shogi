"""End-to-end scheduler check with real coordinator and explicitly released fake games.

Run after building: python deploy/check_live_tasks.py target/debug/taikyoku_shogi
The fake worker does no search. Checks have test-only deadlines, not production limits.
"""

import os
import signal
import subprocess
import sys
import tempfile
import time
from pathlib import Path

import tournament_tasks as tasks
import tourney_analysis as a

WORKER = """#!/usr/bin/env python3
import json
import os
import sys
import time
from pathlib import Path
if sys.argv[1] != "tournament-game":
    print("1")
    raise SystemExit(0)
request, output = map(Path, sys.argv[2:4])
c = json.loads(request.read_text())["config"]
request.with_suffix(".pid").write_text(str(os.getpid()))
while not request.with_suffix(".release").exists():
    time.sleep(.02)
result = dict(format_version=2,game_id=request.stem,seed=c["seed"],black=c["black"],white=c["white"],start=c["start"],moves=[],result="Draw",stats=dict(move_count=0),timestamp=0)
tmp = output.with_suffix(".tmp")
tmp.write_text(json.dumps(result))
tmp.replace(output)
"""


def until(check, proc):
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        if proc.poll() is not None:
            raise AssertionError("Coordinator exited during fixture")
        result = check()
        if result:
            return result
        time.sleep(0.05)
    raise AssertionError("Integration check did not complete in 30 seconds")


def main():
    binary = Path(sys.argv[1]).resolve()
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        worker = root / "worker"
        worker.write_text(WORKER)
        worker.chmod(0o755)
        model = Path(sys.argv[2]).resolve() if len(sys.argv) > 2 else root / "model.json"
        if len(sys.argv) <= 2:
            model.write_text("{}")
        model_files = {str(model): a.file_digest(model)}
        nnue = a.read(model).get("weights", {}).get("nnue")
        if nnue:
            model_files[nnue["file"]] = nnue["sha256"]
        manifest = root / "manifest.json"
        a.atomic(manifest, dict(entrants=[dict(id=f"old{i}", model=str(model)) for i in range(20)]))
        run = root / "run"
        control = run / "analysis"
        control.mkdir(parents=True)
        bundle = dict(
            protocol=1,
            revision="fixture",
            engine_bin=str(worker),
            engine_sha256=a.file_digest(worker),
            analyzer_bin=str(worker),
            analyzer_sha256=a.file_digest(worker),
        )
        pointer = control / "active-engine.json"
        a.atomic(pointer, bundle)
        cpus = sorted(os.sched_getaffinity(0))[:8]
        if len(cpus) != 8:
            raise ValueError("Integration fixture needs eight available affinity IDs (workers do no compute)")
        env = dict(
            os.environ,
            TAIKYOKU_ENGINE_POINTER=str(pointer),
            TAIKYOKU_COMPUTE_DIR=str(control),
            TAIKYOKU_COMPUTE_CPUS=",".join(map(str, cpus)),
        )
        command = [
            str(binary),
            "tournament",
            "--manifest",
            str(manifest),
            "--outdir",
            str(root),
            "--run-id",
            "run",
            "--format",
            "knockout",
            "--starts",
            "opening",
            "--jobs",
            "8",
            "--top-two-worker",
            "--stop-file",
            str(root / "STOP"),
        ]
        with (root / "log").open("wb") as log:
            proc = subprocess.Popen(command, cwd=root, env=env, stdout=log, stderr=log)
            try:
                directory = control / "game-workers"
                until(lambda: len(list(directory.glob("*.pid"))) == 8, proc)
                initial = {p.name: p.read_text() for p in directory.glob("*.pid")}
                task_id, digest = tasks.submit(
                    run,
                    dict(
                        kind="admit",
                        entrants=[dict(entrant=dict(id="new", model=str(model)), files=model_files)],
                    ),
                    "001-admit",
                )
                tasks.wait(run, task_id, digest)
                assert len(a.read(run / "state.json")["entrants"]) == 21
                assert initial == {p.name: p.read_text() for p in directory.glob("*.pid")}
                for pid in initial.values():
                    os.kill(int(pid), 0)
                task_id, digest = tasks.submit(run, dict(kind="pause_assignments", reason="fixture"), "002-pause")
                tasks.wait(run, task_id, digest)
                for request in directory.glob("*.request.json"):
                    request.with_suffix(".release").touch()
                until(lambda: sum(s["status"] == "done" for s in a.read(run / "state.json")["slots"]) == 8, proc)
                time.sleep(0.5)
                paused = a.read(run / "state.json")
                assert not any(s["status"] == "running" for s in paused["slots"])
                assert initial == {p.name: p.read_text() for p in directory.glob("*.pid")}
                # A receipt loss and coordinator restart must not duplicate entrants/results.
                (control / "tasks/receipts/001-admit.json").unlink()
                until(lambda: (control / "tasks/receipts/001-admit.json").exists(), proc)
                proc.send_signal(signal.SIGTERM)
                proc.wait(timeout=10)
                a.atomic(control / "tasks/error.json", dict(error="old recovered failure"))
                proc = subprocess.Popen(command + ["--resume"], cwd=root, env=env, stdout=log, stderr=log)
                until(lambda: (control / "tasks/ready.json").exists(), proc)
                task_id, digest = tasks.submit(run, dict(kind="resume_assignments"), "003-resume")
                tasks.wait(run, task_id, digest)

                def finish_until_new_entrant():
                    state = a.read(run / "state.json")
                    if any("new" in [s["model_a"], s["model_b"]] and s["status"] != "pending" for s in state["slots"]):
                        return True
                    for request in directory.glob("*.request.json"):
                        request.with_suffix(".release").touch()
                    return False

                until(finish_until_new_entrant, proc)
                final = a.read(run / "state.json")
                for original in paused["slots"]:
                    if original["status"] == "done":
                        assert next(s for s in final["slots"] if s["id"] == original["id"]) == original
                assert len(final["entrants"]) == 21
                print(
                    "PASS: admitted with 8 live games; preserved worker PIDs; pause blocked reclaims; all results retained; restart recovered; new entrant assigned a game"
                )
            except BaseException:
                print((root / "log").read_text(), file=sys.stderr)
                raise
            finally:
                if proc.poll() is None:
                    proc.send_signal(signal.SIGTERM)
                    proc.wait(timeout=10)


if __name__ == "__main__":
    main()
