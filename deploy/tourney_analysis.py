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


def analyzer_for_agent(config, agent):
    if not agent.get("engine"):
        return config
    path = absolute(agent["engine"]).resolve()
    entry = config.get("historical_engines", {}).get(str(path))
    if not entry:
        raise ValueError(f"no matching historical analyzer for {path}")
    if digest(path.read_bytes()) != entry["engine_sha256"]:
        raise ValueError(f"historical engine changed: {path}")
    if digest(Path(entry["analyzer_bin"]).read_bytes()) != entry["analyzer_sha256"]:
        raise ValueError(f"historical analyzer changed: {entry['analyzer_bin']}")
    return entry


def validate_source(source):
    binary = Path(source["analyzer_bin"])
    if not os.access(binary, os.X_OK) or digest(binary.read_bytes()) != source["analyzer_sha256"]:
        raise ValueError(f"source analyzer missing or changed: {binary}")
    for original, model in source["models"].items():
        if any(digest(Path(path).read_bytes()) != model["sha256"] for path in (original, model["snapshot"])):
            raise ValueError(f"source model changed: {original}")
    for engine in source.get("historical_engines", {}):
        analyzer_for_agent(source, {"engine": engine})


def historical_pair(source, previous):
    """Validate a freeze-produced pair, or the pinned pair from a resumed run."""
    if str(source) in previous.get("historical_engines", {}):
        return analyzer_for_agent(previous, {"engine": str(source)})
    helper = source.with_name(source.name + ".analyze_position")
    meta = source.with_name(source.name + ".meta")
    if not os.access(source, os.X_OK) or not os.access(helper, os.X_OK):
        raise ValueError(f"missing historical engine/helper pair for {source}; run freeze_history.sh")
    fields = dict(line.split("=", 1) for line in meta.read_text().splitlines() if "=" in line)
    entry = dict(engine_sha256=digest(source.read_bytes()), analyzer_bin=str(helper),
                 analyzer_sha256=digest(helper.read_bytes()), revision=fields.get("rev"))
    if not entry["revision"] or any(entry[k] != fields.get(k) for k in ("engine_sha256", "analyzer_sha256")):
        raise ValueError(f"historical engine/helper metadata mismatch: {source}")
    return entry


def search_position(config, moment, ply, db):
    # Carried games keep their original analysis engine/checkpoint bindings while
    # results are written into the new run's catalogue and CPU lease.
    destination = config["run"]
    for source in config.get("analysis_sources", []):
        if Path(moment["game"]).resolve().parent == Path(source["run"]).resolve():
            config = dict(source, run=destination)
            break
    data = Path(moment["game"]).read_bytes()
    if digest(data) != moment["game_hash"]:
        raise ValueError("game changed since scan")
    game = json.loads(data)
    move = game["moves"][ply - 1]
    agent = game["black" if move["color"].lower() == "black" else "white"]
    if agent["name"] != "ab":
        raise ValueError("only ab agents are supported")
    helper = analyzer_for_agent(config, agent)
    model_key = str(absolute(agent["model"]).resolve())
    model = config["models"][model_key]
    if digest(Path(model_key).read_bytes()) != model["sha256"]:
        raise ValueError(f"original model changed: {model_key}")
    recorded_depth = move.get("completed_depth")
    depth = recorded_depth + 1 if isinstance(recorded_depth, int) and recorded_depth > 0 else 64
    budget = 300_000 if isinstance(recorded_depth, int) and recorded_depth > 0 else 30_000
    identity = dict(policy=POLICY, game=moment["game_hash"], ply=ply, agent=agent,
                    model=model["sha256"], binary=helper["analyzer_sha256"],
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
    env = {k: v for k, v in os.environ.items() if not k.startswith("TAIKYOKU_AB_")}
    command = [helper["analyzer_bin"], moment["game"], str(ply), model["snapshot"], str(depth), str(budget)]
    if agent.get("engine"):
        command.append("--allow-historical")
    with log_path.open("ab") as err:
        proc = subprocess.Popen(command,
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
                analyzer_sha256=helper["analyzer_sha256"], engine_sha256=helper.get("engine_sha256"),
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
    try:
        while not STOPPING:
            try:
                scan(db, run)
                for source in config.get("analysis_sources", []):
                    scan(db, Path(source["run"]))
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
                        moment["searches"] = [search_position(config, moment, p, db) for p in moment["plies"]]
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
    previous = read(run / "analysis/config.json") if state and (run / "analysis/config.json").exists() else {}
    sources = previous.get("analysis_sources", [])
    carry = getattr(args, "carry_analysis_from", None)
    if carry:
        if state:
            raise ValueError("--carry-analysis-from is for a new run only; resume keeps saved sources")
        old_run = absolute(carry).resolve()
        if old_run == run:
            raise ValueError("cannot carry analysis from the new run itself")
        old = read(old_run / "analysis/config.json")
        if not (old_run / "state.json").is_file() or not (old_run / "analysis/catalogue.sqlite").is_file():
            raise ValueError("source run has no saved state/catalogue")
        # Make a flattened snapshot of bindings. Never substitute the new engine
        # for an old in-process game merely because it has the same agent name.
        sources = [dict(old, analysis_sources=[])] + old.get("analysis_sources", [])
    for source in sources:
        validate_source(source)
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
    historical = {}
    for ent in entrants:
        validator = binaries[1]
        if ent.get("engine"):
            engine = absolute(ent["engine"]).resolve()
            entry = historical_pair(engine, previous)
            historical[str(engine)] = entry
            validator = Path(entry["analyzer_bin"])
        source = absolute(ent["model"]).resolve()
        data = source.read_bytes()
        json.loads(data)
        subprocess.run([str(validator), "--validate-model", str(source)],
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
    if carry:
        destination = control / "catalogue.sqlite"
        if destination.exists():
            raise ValueError("new run already has a catalogue; refusing to overwrite it")
        with sqlite3.connect(f"file:{old_run / 'analysis/catalogue.sqlite'}?mode=ro", uri=True) as source_db:
            with sqlite3.connect(destination) as destination_db:
                source_db.backup(destination_db)
                for moment_id, payload in destination_db.execute("SELECT id,payload FROM moments WHERE status='completed'"):
                    atomic(control / "moments" / (moment_id + ".json"), json.loads(payload))
    def freeze(binary):
        if binary.resolve().parent == (control / "bin").resolve():
            return binary
        data = binary.read_bytes()
        target = control / "bin" / (binary.name + "-" + digest(data))
        if not target.exists():
            target.write_bytes(data)
            target.chmod(0o755)
        return target
    frozen = [freeze(binary) for binary in binaries]
    historical_map = {}
    for ent in entrants:
        if not ent.get("engine"):
            continue
        original = str(absolute(ent["engine"]).resolve())
        entry = historical[original].copy()
        engine_target = freeze(Path(original))
        entry["analyzer_bin"] = str(freeze(Path(entry["analyzer_bin"])))
        historical_map[original] = entry
        historical_map[str(engine_target)] = entry
        ent["engine"] = str(engine_target)
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
    return dict(run=str(run), cpus=cpus, models=model_map, command=command, historical_engines=historical_map, analysis_sources=sources,
                analyzer_bin=str(frozen[1]), analyzer_sha256=digest(frozen[1].read_bytes()))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["start", "resume", "stop", "status", "_supervise", "_analyze"])
    parser.add_argument("--run-dir", required=True)
    parser.add_argument("--manifest", default="models/royal-s2-twins-grid/manifest.json")
    parser.add_argument("--carry-analysis-from", help="continue a previous run catalogue with its original engine bindings")
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
