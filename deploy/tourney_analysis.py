#!/usr/bin/env python3
"""Run a tournament and a resumable evaluation-swing analyzer on four Linux CPUs."""
import argparse
import ctypes
import fcntl
import hashlib
import json
import os
from pathlib import Path
import selectors
import signal
import sqlite3
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parent.parent
STOPPING = False
SEARCH_HARD_SECONDS = 300
POLICY = "same-player-1000-window7-v1"


def stop_handler(*_):
    global STOPPING
    STOPPING = True


def read(path):
    return json.loads(Path(path).read_text())


def atomic(path, value):
    path = Path(path)
    tmp = path.with_name(path.name + ".tmp")
    tmp.write_text(json.dumps(value, indent=2) + "\n")
    os.replace(tmp, path)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def absolute(path):
    p = Path(path)
    return p if p.is_absolute() else ROOT / p


def connect(run):
    db = sqlite3.connect(run / "analysis/catalogue.sqlite")
    db.row_factory = sqlite3.Row
    db.executescript("""
        PRAGMA journal_mode=WAL;
        CREATE TABLE IF NOT EXISTS files(path TEXT PRIMARY KEY, hash TEXT);
        CREATE TABLE IF NOT EXISTS moments(
            id TEXT PRIMARY KEY, magnitude INTEGER, status TEXT, payload TEXT, error TEXT);
        CREATE TABLE IF NOT EXISTS searches(key TEXT PRIMARY KEY, payload TEXT);
    """)
    db.execute("UPDATE moments SET status='pending' WHERE status='running'")
    db.commit()
    return db


def candidates(game):
    moves = game["moves"]
    for i in range(2, len(moves)):
        a, b = moves[i - 2], moves[i]
        if a["color"] != b["color"]:
            continue
        if not isinstance(a.get("eval"), int) or not isinstance(b.get("eval"), int):
            continue
        change = b["eval"] - a["eval"]
        if abs(change) >= 1000:
            # i is zero-based N+1, so the center's one-based number N is i.
            yield dict(center_ply=i, earlier_ply=i - 1, later_ply=i + 1,
                       color=b["color"], earlier_eval=a["eval"], later_eval=b["eval"],
                       change=change, magnitude=abs(change),
                       plies=list(range(max(1, i - 3), min(len(moves), i + 3) + 1)))


def scan(db, run):
    state = read(run / "state.json")  # caller retries transient/incomplete state reads
    for slot in state["slots"]:
        if slot["status"] != "done" or not slot.get("game_path"):
            continue
        path = absolute(slot["game_path"])
        if db.execute("SELECT 1 FROM files WHERE path=?", (str(path),)).fetchone():
            continue
        try:
            data = path.read_bytes()
            game = json.loads(data)
            if game.get("abort_reason") or game.get("result") is None:
                continue
            game_hash = digest(data)
            for moment in candidates(game):
                key = digest(f"{POLICY}:{game_hash}:{moment['center_ply']}".encode())
                moment.update(id=key, game=str(path), game_hash=game_hash,
                              game_id=game["game_id"], slot_id=slot["id"])
                db.execute("INSERT OR IGNORE INTO moments VALUES(?,?,'pending',?,NULL)",
                           (key, moment["magnitude"], json.dumps(moment)))
            db.execute("INSERT INTO files VALUES(?,?)", (str(path), game_hash))
            db.commit()
            missing = sum(m.get("eval") is None for m in game["moves"])
            if missing:
                print(f"scan: {path.name}: {missing} moves lack search evaluations", flush=True)
        except (OSError, ValueError, KeyError, TypeError) as e:
            db.rollback()
            print(f"scan: retry {path}: {e}", file=sys.stderr, flush=True)


def parent_death_guard(parent):
    # Prevent an orphan search consuming the shared CPU after analyzer failure.
    def setup():
        if ctypes.CDLL(None, use_errno=True).prctl(1, signal.SIGKILL, 0, 0, 0) != 0:
            raise OSError(ctypes.get_errno(), "PR_SET_PDEATHSIG failed")
        if os.getppid() != parent:
            os.kill(os.getpid(), signal.SIGKILL)
    return setup


class BatchWorker:
    """One supervised child, bounded input cache, and one outstanding request.

    Reading bytes instead of readline keeps the watchdog effective on partial
    JSON. Only correlated iteration events may be persisted as search results.
    """
    def __init__(self, config):
        self.config = config
        self.proc = None
        self.err = None
        self.sequence = 0
        self.inputs = {}

    def verified(self, path, expected, parse=False):
        path = Path(path)
        st = path.stat()
        stamp = (st.st_dev, st.st_ino, st.st_size, st.st_mtime_ns, st.st_ctime_ns)
        key = (str(path), expected, parse)
        cached = self.inputs.get(key)
        if cached and cached[0] == stamp:
            return cached[1]
        data = path.read_bytes()
        if digest(data) != expected:
            raise ValueError(f"immutable input changed: {path}")
        value = json.loads(data) if parse else None
        # One game and the small model manifest fit; never grow with the tournament.
        if parse:
            self.inputs = {k: v for k, v in self.inputs.items() if not k[2]}
        if len(self.inputs) >= 64:
            self.inputs.clear()
        self.inputs[key] = (stamp, value)
        return value

    def close(self):
        if self.proc is not None:
            if self.proc.poll() is None:
                self.proc.kill()
            self.proc.wait()
            self.proc.stdin.close()
            self.proc.stdout.close()
            self.proc = None
        if self.err is not None:
            self.err.close()
            self.err = None

    def search(self, game, game_hash, ply, model, depth, budget, progress):
        if self.proc is None:
            env = {k: v for k, v in os.environ.items() if not k.startswith("TAIKYOKU_AB_")}
            self.err = (Path(self.config["run"]) / "analysis/search.stderr.log").open("ab")
            try:
                self.proc = subprocess.Popen([self.config["analyzer_bin"], "--batch-v1"],
                    stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.err, env=env,
                    preexec_fn=parent_death_guard(os.getpid()), bufsize=0)
                os.set_blocking(self.proc.stdout.fileno(), False)
                os.set_blocking(self.proc.stdin.fileno(), False)
            except Exception:
                self.close()
                raise
        self.sequence += 1
        request_id = str(self.sequence)
        outgoing = memoryview((json.dumps(dict(version=1, id=request_id, game=game,
            game_sha256=game_hash, ply=ply, model=model, depth=depth, time_ms=budget)) + "\n").encode())
        started = time.monotonic()
        best = None
        incoming = b""
        selector = selectors.DefaultSelector()
        selector.register(self.proc.stdout, selectors.EVENT_READ)
        selector.register(self.proc.stdin, selectors.EVENT_WRITE)
        try:
            while True:
                if STOPPING or time.monotonic() - started >= SEARCH_HARD_SECONDS:
                    self.close()
                    if STOPPING:
                        raise InterruptedError("analysis stopped")
                    return best, True
                for stream, _ in selector.select(min(.2, max(0, SEARCH_HARD_SECONDS - (time.monotonic() - started)))):
                    if stream.fileobj is self.proc.stdin:
                        try:
                            outgoing = outgoing[os.write(self.proc.stdin.fileno(), outgoing):]
                        except BlockingIOError:
                            continue
                        if not outgoing:
                            selector.unregister(self.proc.stdin)
                        continue
                    data = os.read(self.proc.stdout.fileno(), 65536)
                    if not data:
                        raise ValueError("batch worker exited before completion")
                    incoming += data
                    if len(incoming) > 8 * 1024 * 1024:
                        raise ValueError("oversized batch event")
                    while b"\n" in incoming:
                        line, incoming = incoming.split(b"\n", 1)
                        event = json.loads(line)
                        if event.get("version") != 1 or event.get("id") != request_id:
                            raise ValueError("uncorrelated batch event")
                        kind = event.pop("event", None)
                        event.pop("version")
                        event.pop("id")
                        if kind == "iteration":
                            if event["completed_depth"] > 0 or event.get("terminal"):
                                if best is None or event["completed_depth"] >= best["completed_depth"]:
                                    best = event
                                    progress(best)
                        elif kind == "complete":
                            if incoming:
                                raise ValueError("unexpected data after completion")
                            return best, False
                        elif kind == "error":
                            raise ValueError(f"batch search failed: {event.get('error')}")
                        else:
                            raise ValueError("unknown batch event")
        except Exception:
            self.close()
            raise
        finally:
            selector.close()


def search_position(config, moment, ply, db, worker=None):
    if worker:
        game = worker.verified(moment["game"], moment["game_hash"], parse=True)
    else:
        data = Path(moment["game"]).read_bytes()
        if digest(data) != moment["game_hash"]:
            raise ValueError("game changed since scan")
        game = json.loads(data)
    move = game["moves"][ply - 1]
    agent = game["black" if move["color"].lower() == "black" else "white"]
    if agent["name"] != "ab" or agent.get("engine"):
        raise ValueError("only current in-process ab agents are supported")
    model_key = str(absolute(agent["model"]).resolve())
    model = config["models"][model_key]
    if worker:
        worker.verified(model_key, model["sha256"])
        worker.verified(model["snapshot"], model["sha256"])
    elif digest(Path(model_key).read_bytes()) != model["sha256"]:
        raise ValueError(f"original model changed: {model_key}")
    recorded_depth = move.get("completed_depth")
    depth = recorded_depth + 1 if isinstance(recorded_depth, int) and recorded_depth > 0 else 64
    budget = 300_000 if isinstance(recorded_depth, int) and recorded_depth > 0 else 30_000
    identity = dict(policy=POLICY, game=moment["game_hash"], ply=ply, agent=agent,
                    model=model["sha256"], binary=config["analyzer_sha256"],
                    depth=depth, budget=budget)
    key = digest(json.dumps(identity, sort_keys=True).encode())
    cached = db.execute("SELECT payload FROM searches WHERE key=?", (key,)).fetchone()
    if cached:
        return json.loads(cached["payload"])
    run = Path(config["run"])
    log_path = run / "analysis/search.stderr.log"
    start = time.monotonic()
    best = None
    hard_timeout = False
    if worker:
        best, hard_timeout = worker.search(moment["game"], moment["game_hash"], ply,
            model["snapshot"], depth, budget,
            lambda result: atomic(run / "analysis" / (key + ".progress.json"), result))
        if not best:
            raise ValueError(f"no completed iteration; see {log_path}")
    else:
        env = {k: v for k, v in os.environ.items() if not k.startswith("TAIKYOKU_AB_")}
        with log_path.open("ab") as err:
            proc = subprocess.Popen([config["analyzer_bin"], moment["game"], str(ply),
                                     model["snapshot"], str(depth), str(budget)],
                                    stdout=subprocess.PIPE, stderr=err, env=env,
                                    preexec_fn=parent_death_guard(os.getpid()))
            selector = selectors.DefaultSelector()
            selector.register(proc.stdout, selectors.EVENT_READ)
            try:
                while True:
                    if STOPPING or time.monotonic() - start >= SEARCH_HARD_SECONDS:
                        hard_timeout = not STOPPING
                        proc.kill()
                        break
                    events = selector.select(0.2)
                    if events:
                        line = proc.stdout.readline()
                        if not line:
                            break
                        result = json.loads(line)
                        if result["completed_depth"] > 0 or result.get("terminal"):
                            best = result
                            # Durable iteration output even if this controller is interrupted.
                            atomic(run / "analysis" / (key + ".progress.json"), result)
                proc.wait(timeout=5)
            finally:
                if proc.poll() is None:
                    proc.kill()
                proc.wait()
                proc.stdout.close()
                selector.close()
        if STOPPING:
            raise InterruptedError("analysis stopped")
        if not best:
            raise ValueError(f"no completed iteration (exit {proc.returncode}); see {log_path}")
        if proc.returncode != 0 and not hard_timeout:
            raise ValueError(f"search failed (exit {proc.returncode}); see {log_path}")
    best.update(key=key, ply=ply, agent=agent, model_sha256=model["sha256"],
                original_eval=move.get("eval"), original_static_eval=move.get("static_eval"),
                original_depth=recorded_depth, original_move=move, target_depth=depth,
                budget_ms=budget, hard_timeout=hard_timeout, wall_ms=int((time.monotonic()-start)*1000))
    db.execute("INSERT OR REPLACE INTO searches VALUES(?,?)", (key, json.dumps(best)))
    db.commit()
    return best


def analyzer(config):
    run = Path(config["run"])
    control = run / "analysis"
    os.sched_setaffinity(0, {config["cpus"][3]})
    db = connect(run)
    request = control / "analysis.request"
    worker = BatchWorker(config) if config.get("analyzer_protocol") == 1 else None
    try:
        while not STOPPING:
            try:
                scan(db, run)
            except (OSError, ValueError, KeyError) as e:
                print(f"scan state retry: {e}", file=sys.stderr, flush=True)
            batch = db.execute(
                "SELECT * FROM moments WHERE status='pending' ORDER BY magnitude DESC,id LIMIT 20"
            ).fetchall()
            backlog = db.execute("SELECT count(*) FROM moments WHERE status='pending'").fetchone()[0]
            atomic(control / "analyzer-status.json", {"state": "waiting_for_cpu" if batch else "idle",
                                                     "backlog": backlog, "updated": time.time()})
            if not batch:
                request.unlink(missing_ok=True)
                for _ in range(100):
                    if STOPPING:
                        break
                    time.sleep(.1)
                continue
            request.touch()
            with (control / "shared.lock").open("a+") as lease:
                while not STOPPING:
                    try:
                        fcntl.flock(lease, fcntl.LOCK_EX | fcntl.LOCK_NB)
                        break
                    except BlockingIOError:
                        time.sleep(.2)
                if STOPPING:
                    break
                for row in batch:
                    if STOPPING:
                        break
                    moment = json.loads(row["payload"])
                    atomic(control / "analyzer-status.json", {
                        "state": "analyzing", "moment": row["id"], "backlog": backlog, "updated": time.time()})
                    db.execute("UPDATE moments SET status='running' WHERE id=?", (row["id"],))
                    db.commit()
                    try:
                        moment["searches"] = [search_position(config, moment, p, db, worker) for p in moment["plies"]]
                        moment["status"] = "completed"
                        atomic(control / "moments" / (row["id"] + ".json"), moment)
                        db.execute("UPDATE moments SET status='completed',payload=?,error=NULL WHERE id=?",
                                   (json.dumps(moment), row["id"]))
                        print(f"completed {row['id']} swing={row['magnitude']}", flush=True)
                    except InterruptedError:
                        db.execute("UPDATE moments SET status='pending' WHERE id=?", (row["id"],))
                        break
                    except Exception as e:
                        db.execute("UPDATE moments SET status='failed',error=? WHERE id=?", (str(e), row["id"]))
                        print(f"analysis failed {row['id']}: {e}", file=sys.stderr, flush=True)
                    finally:
                        db.commit()
    finally:
        if worker:
            worker.close()
        request.unlink(missing_ok=True)
        db.close()


def process_identity(pid):
    try:
        fields = Path(f"/proc/{pid}/stat").read_text().split(") ", 1)[1].split()
        return fields[19] if fields[0] != "Z" else None
    except (OSError, IndexError):
        return None


def alive(info):
    return info and process_identity(info["pid"]) == info["identity"]


def supervise(config):
    run = Path(config["run"])
    control = run / "analysis"
    with (control / "supervisor.lock").open("a+") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        global_lock = (ROOT / "data/run/tourney-analysis.lock").open("a+")
        fcntl.flock(global_lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        (control / "analysis.request").unlink(missing_ok=True)
        (ROOT / "data/run/TOURNEY_STOP").unlink(missing_ok=True)
        env = dict(os.environ, TAIKYOKU_COMPUTE_DIR=str(control),
                   TAIKYOKU_COMPUTE_CPUS=",".join(map(str, config["cpus"])))
        # Reserve shared CPU until analyzer has scanned the initial backlog.
        (control / "analysis.request").touch()
        with (control / "tournament.log").open("ab") as tlog, (control / "analyzer.log").open("ab") as alog:
            tourney = subprocess.Popen(config["command"], cwd=ROOT, env=env, stdout=tlog, stderr=tlog)
            try:
                worker = subprocess.Popen([sys.executable, str(Path(__file__).resolve()), "_analyze",
                                           "--run-dir", str(run)], cwd=ROOT, stdout=alog, stderr=alog)
            except Exception:
                tourney.terminate()
                tourney.wait()
                raise
            meta = {"pid": os.getpid(), "identity": process_identity(os.getpid()),
                    "tournament_pid": tourney.pid, "analyzer_pid": worker.pid, "state": "starting"}
            try:
                time.sleep(2)
                if tourney.poll() is not None or worker.poll() is not None:
                    raise RuntimeError("child exited during startup; see tournament.log and analyzer.log")
                meta["state"] = "running"
                atomic(control / "supervisor.json", meta)
                while not STOPPING and tourney.poll() is None:
                    if worker.poll() is not None and meta["state"] != "analyzer_failed":
                        (control / "analysis.request").unlink(missing_ok=True)
                        meta.update(state="analyzer_failed", analyzer_exit=worker.returncode)
                        atomic(control / "supervisor.json", meta)
                        print("Analyzer failed; tournament continues with four CPUs", file=sys.stderr, flush=True)
                    time.sleep(.5)
            finally:
                if worker.poll() is None:
                    worker.send_signal(signal.SIGTERM)
                    try:
                        worker.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        worker.kill()
                        worker.wait()
                (control / "analysis.request").unlink(missing_ok=True)
                if tourney.poll() is None:
                    tourney.send_signal(signal.SIGTERM)
                tourney.wait()  # existing engine stops between moves
                meta.update(state="stopped" if STOPPING else "failed", tournament_exit=tourney.returncode)
                atomic(control / "supervisor.json", meta)


def prepare(args, run):
    if args.action == "resume" and not (run / "state.json").is_file():
        raise ValueError("resume requires an existing state.json")
    if args.action == "start" and (run / "state.json").exists():
        raise ValueError("run already exists; use resume")
    state = read(run / "state.json") if args.action == "resume" else None
    entrants = state["entrants"] if state else read(absolute(args.manifest))["entrants"]
    allowed = sorted(os.sched_getaffinity(0))
    cpus = list(map(int, args.cpus.split(","))) if args.cpus else allowed[:4]
    if len(cpus) != 4 or len(set(cpus)) != 4 or not set(cpus) <= set(allowed):
        raise ValueError("exactly four distinct available CPUs are required")
    binaries = [ROOT / "target/release/taikyoku_shogi", ROOT / "target/release/analyze_position"]
    for path in binaries:
        if not os.access(path, os.X_OK):
            raise ValueError(f"missing executable {path}; build --release --bins first")
    # Avoid overlapping the legacy launcher or another run.
    for proc in Path("/proc").iterdir():
        if not proc.name.isdigit():
            continue
        try:
            argv = (proc / "cmdline").read_bytes().split(b"\0")
            if len(argv) > 1 and argv[1] == b"tournament" and b"taikyoku_shogi" in argv[0]:
                raise ValueError(f"tournament already running as PID {proc.name}; stop it first")
        except (FileNotFoundError, PermissionError, ProcessLookupError):
            pass
    validated = []
    for ent in entrants:
        if ent.get("engine"):
            raise ValueError("historical binary entrants are unsupported by this analyzer")
        source = absolute(ent["model"]).resolve()
        data = source.read_bytes()
        json.loads(data)
        subprocess.run([str(binaries[1]), "--validate-model", str(source)],
                       check=True, capture_output=True, text=True)
        validated.append((source, data))
    control = run / "analysis"
    control.mkdir(parents=True, exist_ok=True)
    for name in ["models", "moments", "bin"]:
        (control / name).mkdir(exist_ok=True)
    model_map = {}
    for source, data in validated:
        sha = digest(data)
        snapshot = control / "models" / (sha + ".json")
        snapshot.write_bytes(data)
        model_map[str(source)] = {"sha256": sha, "snapshot": str(snapshot)}
    # Pin the executables too; a later build cannot change a running analysis policy.
    frozen = []
    for binary in binaries:
        data = binary.read_bytes()
        target = control / "bin" / (binary.name + "-" + digest(data))
        if not target.exists():
            target.write_bytes(data)
            target.chmod(0o755)
        frozen.append(target)
    manifest = control / "manifest.json"
    atomic(manifest, {"entrants": entrants})
    depth = state["depth"] if state else args.depth
    budget = state.get("max_time_ms") if state else args.time_ms
    command = [str(frozen[0]), "tournament", "--manifest", str(manifest),
               "--run-id", run.name, "--outdir", str(run.parent),
               "--jobs", "4", "--depth", str(depth), "--format",
               state.get("format", "knockout") if state else "knockout"]
    if args.action == "resume":
        command.append("--resume")
    if budget is not None:
        command += ["--time-ms", str(budget)]
    return dict(run=str(run), cpus=cpus, models=model_map, command=command, analyzer_protocol=1,
                analyzer_bin=str(frozen[1]), analyzer_sha256=digest(frozen[1].read_bytes()))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["start", "resume", "stop", "status", "_supervise", "_analyze"])
    parser.add_argument("--run-dir", required=True)
    parser.add_argument("--manifest", default="models/royal-s2-twins-grid/manifest.json")
    parser.add_argument("--cpus", help="four CPU IDs; defaults to first four allowed")
    parser.add_argument("--depth", type=int, default=8)
    parser.add_argument("--time-ms", type=int, default=3000)
    args = parser.parse_args()
    run = absolute(args.run_dir).resolve()
    control = run / "analysis"
    signal.signal(signal.SIGTERM, stop_handler)
    signal.signal(signal.SIGINT, stop_handler)
    if args.action == "_analyze":
        return analyzer(read(control / "config.json"))
    if args.action == "_supervise":
        return supervise(read(control / "config.json"))
    meta = read(control / "supervisor.json") if (control / "supervisor.json").exists() else None
    if args.action == "status":
        counts = {}
        if (control / "catalogue.sqlite").exists():
            with sqlite3.connect(f"file:{control / 'catalogue.sqlite'}?mode=ro", uri=True) as db:
                counts = dict(db.execute("SELECT status,count(*) FROM moments GROUP BY status"))
                counts["cached_searches"] = db.execute("SELECT count(*) FROM searches").fetchone()[0]
        print(json.dumps({"supervisor": meta, "alive": bool(alive(meta)), "catalogue": counts,
                          "analyzer": read(control / "analyzer-status.json")
                          if (control / "analyzer-status.json").exists() else None}, indent=2))
        return
    if args.action == "stop":
        if not alive(meta):
            raise ValueError("no live supervisor for this run")
        os.kill(meta["pid"], signal.SIGTERM)
        print("Stop requested; waiting for the tournament to checkpoint.")
        while alive(meta):
            time.sleep(.5)
        return
    if alive(meta):
        raise ValueError("this run already has a live supervisor")
    # Serialize CLI startup before writing configuration or spawning children.
    (ROOT / "data/run").mkdir(parents=True, exist_ok=True)
    startup_lock = (ROOT / "data/run/tourney-analysis-start.lock").open("a+")
    fcntl.flock(startup_lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    config = prepare(args, run)
    atomic(control / "config.json", config)
    (control / "supervisor.json").unlink(missing_ok=True)
    with (control / "supervisor.log").open("ab") as log:
        child = subprocess.Popen([sys.executable, str(Path(__file__).resolve()), "_supervise",
                                  "--run-dir", str(run)], cwd=ROOT, stdout=log, stderr=log,
                                 start_new_session=True)
    for _ in range(100):
        if child.poll() is not None:
            raise RuntimeError("supervisor failed; tournament did not start successfully; see supervisor.log")
        if (control / "supervisor.json").exists():
            info = read(control / "supervisor.json")
            if info["state"] == "running":
                print(f"Running supervisor={child.pid}; logs/catalogue: {control}")
                return
        time.sleep(.1)
    child.terminate()
    raise RuntimeError("startup readiness timed out; stop requested; see supervisor.log")


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:
        suffix = "; tournament did not start successfully" if len(sys.argv) > 1 and sys.argv[1] in ("start", "resume") else ""
        print(f"tourney_analysis: {exc}{suffix}", file=sys.stderr)
        sys.exit(1)
