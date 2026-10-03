"""Eight CPUs: retain tournament worker 0 and top-two worker 2; lend six at boundaries."""
import contextlib
import fcntl
import json
import multiprocessing
import os
from pathlib import Path
import signal
import sqlite3
import time
import threading

import tourney_analysis as a
import champion_teacher
import teacher_refresh
import winner_teacher
import dual_label_sidecar as dual

ELIGIBLE = ('id NOT IN (SELECT id FROM dual_paired) '
            'AND id NOT IN (SELECT id FROM dual_failures)')
ORDER = ("CASE WHEN json_extract(payload,'$.teacher_refresh') IS NOT NULL THEN 0 ELSE 1 END, "
         "CASE WHEN json_extract(payload,'$.winner_teacher') IS NOT NULL THEN 0 ELSE 1 END,id")

@contextlib.contextmanager
def allocation(control):
    with (control / 'allocation.lock').open('a+') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        path = control / 'analysis-demand.json'
        demand = a.read(path) if path.exists() else dict(assigned=[])
        yield demand
        a.atomic(path, demand)


def pending_count(db):
    return db.execute('SELECT count(*) FROM moments WHERE ' + ELIGIBLE).fetchone()[0]


def position_job(config, cpu, row_id, parent_pid):
    # Exit if the manager dies, including its compute children via their own guard.
    signal.signal(signal.SIGTERM, a.stop_handler)
    signal.signal(signal.SIGINT, a.stop_handler)
    a.parent_death_guard(parent_pid)()
    os.sched_setaffinity(0, {cpu})
    db = sqlite3.connect(Path(config['run']) / 'analysis/training-labels.sqlite', timeout=30)
    db.row_factory = sqlite3.Row
    try:
        row = db.execute('SELECT payload FROM moments WHERE id=?', (row_id,)).fetchone()
        moment = dual.paired_searches(config, json.loads(row['payload']), db)
        a.atomic(Path(config['run']) / 'analysis/moments' / (row_id + '.json'), moment)
        db.execute('UPDATE moments SET status=?,payload=?,error=NULL WHERE id=?',
                   ('completed', json.dumps(moment), row_id))
        db.execute('INSERT OR REPLACE INTO dual_paired VALUES(?)', (row_id,))
        db.commit()
    except InterruptedError:
        pass  # partial teacher/search results already committed; retry on resume
    except Exception as exc:
        db.execute('INSERT OR REPLACE INTO dual_failures VALUES(?,?,?)', (row_id, str(exc), time.time()))
        db.commit()
        raise
    finally:
        db.close()


def run(config):
    run_dir = Path(config['run'])
    control = run_dir / 'analysis'
    cpus = config['cpus']
    if len(cpus) != 8 or len(set(cpus)) != 8 or not set(cpus) <= os.sched_getaffinity(0):
        raise ValueError('adaptive analysis requires eight distinct available CPUs')
    a.validate_source(dual.teacher_config(config, True))
    # Spawn avoids inheriting SQLite connections and unrelated CPU lease fds.
    context = multiprocessing.get_context('spawn')
    active = {}  # worker index -> (process, row ID, exclusive CPU lease)
    with (control / 'dual-label.lock').open('a+') as own:
        fcntl.flock(own, fcntl.LOCK_EX | fcntl.LOCK_NB)
        db = a.connect(run_dir, 'training-labels.sqlite')
        db.execute('PRAGMA busy_timeout=30000')
        db.executescript('CREATE TABLE IF NOT EXISTS dual_paired(id TEXT PRIMARY KEY);'
                        'CREATE TABLE IF NOT EXISTS dual_failures(id TEXT PRIMARY KEY,error TEXT,updated REAL);')
        teacher_refresh.apply_backfill(db, config)
        with allocation(control) as demand:
            demand.update(assigned=[], pending=pending_count(db), pid=os.getpid(), updated=time.time())
        heartbeat_stop = threading.Event()
        def heartbeat():
            while not heartbeat_stop.wait(5):
                with allocation(control) as demand:
                    demand.update(pid=os.getpid(), updated=time.time())
        heartbeat_thread = threading.Thread(target=heartbeat, daemon=True)
        heartbeat_thread.start()
        seen_live = False
        fingerprint = None
        last_scan = 0
        draining = False
        try:
            while not a.STOPPING:
                live = a.read(control / 'supervisor.json')
                running = a.alive(live) and live.get('state') == 'running'
                if seen_live and not running:
                    break
                seen_live |= running
                for index, (proc, row_id, lease) in list(active.items()):
                    if proc.is_alive():
                        continue
                    proc.join()
                    if proc.exitcode and not db.execute('SELECT 1 FROM dual_failures WHERE id=?', (row_id,)).fetchone():
                        db.execute('INSERT OR REPLACE INTO dual_failures VALUES(?,?,?)',
                                   (row_id, f'worker exited {proc.exitcode}', time.time()))
                        db.commit()
                    lease.close()
                    del active[index]
                # Scan and teacher migration run only with no outstanding jobs:
                # otherwise a backfill could race a worker committing an old label.
                if running and time.time() - last_scan >= 10:
                    try:
                        state = a.read(run_dir / 'state.json')
                        current = [(s['id'], s.get('score_a')) for s in state['slots'] if s['status'] == 'done']
                    except (OSError, ValueError):
                        current = fingerprint  # coordinator may be replacing its state
                    draining = current != fingerprint
                    if draining and not active:
                        config = champion_teacher.check(a, run_dir, a.read(control / 'dual-label-config.json'))
                        teacher_refresh.apply_backfill(db, config)
                        winner_teacher.scan(a, db, run_dir, config)
                        a.scan(db, run_dir, labels=True, label_candidates=lambda game: [] if
                               winner_teacher.winner_agent(config, game) else a.training_labels.candidates(game))
                        fingerprint = current
                        draining = False
                    if not draining:
                        last_scan = time.time()
                pending = pending_count(db)  # includes positions currently being processed
                with allocation(control) as demand:
                    assigned = demand['assigned']
                    # A finished analysis CPU returns to games if no unclaimed work remains.
                    idle = [i for i in assigned if i not in active]
                    available = max(0, pending - len(active))
                    keep = idle[:available] if not draining else idle
                    demand.update(assigned=[i for i in assigned if i in active or i in keep],
                                  pending=pending, pid=os.getpid(), updated=time.time())
                    assigned = list(demand['assigned'])
                if running and not draining:
                    busy_ids = {job[1] for job in active.values()}
                    rows = db.execute('SELECT id FROM moments WHERE ' + ELIGIBLE +
                                      ' ORDER BY ' + ORDER + ' LIMIT 12').fetchall()
                    queue = [row['id'] for row in rows if row['id'] not in busy_ids]
                    for index in assigned:
                        if index in active or not queue:
                            continue
                        if index in (0, 2) or index not in range(8):
                            raise ValueError('invalid reserved analysis CPU')
                        lease = (control / f'cpu-{index}.lock').open('a+')
                        try:
                            fcntl.flock(lease, fcntl.LOCK_EX | fcntl.LOCK_NB)
                        except BlockingIOError:
                            lease.close()
                            continue
                        row_id = queue.pop(0)
                        proc = context.Process(target=position_job, args=(config, cpus[index], row_id, os.getpid()))
                        proc.start()
                        active[index] = (proc, row_id, lease)
                a.atomic(control / 'dual-label-status.json', dict(
                    state='running' if active else 'waiting_for_cpu' if pending else 'idle',
                    default_teacher=config.get('new_teacher_id'), remaining=pending,
                    completed=db.execute('SELECT count(*) FROM dual_paired').fetchone()[0],
                    failed=db.execute('SELECT count(*) FROM dual_failures').fetchone()[0],
                    active_cpus=[cpus[i] for i in active], reserved_workers=assigned,
                    worker_positions={str(i): job[1] for i, job in active.items()},
                    updated=time.time()))
                time.sleep(.2)
        finally:
            heartbeat_stop.set()
            heartbeat_thread.join()
            for proc, _, _ in active.values():
                if proc.is_alive(): proc.terminate()
            for proc, _, lease in active.values():
                proc.join(timeout=10)
                if proc.is_alive(): proc.kill(); proc.join()
                lease.close()
            with allocation(control) as demand:
                demand.update(assigned=[], pending=0, pid=0, updated=time.time())
            db.close()
