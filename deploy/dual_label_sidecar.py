#!/usr/bin/env python3
"""Add a second frozen teacher to a live run without restarting its games.

The original analyzer must be stopped first. This process owns analysis.request,
uses the tournament's shared CPU lease, and updates the existing label catalogue.
"""
import argparse
import fcntl
import json
import os
from pathlib import Path
import signal
import sys
import time

import tourney_analysis as a
import winner_teacher

BUDGET_MS = 30_000


def setup(run, model_path, check_only=False):
    control = run / 'analysis'
    live = a.read(control / 'supervisor.json')
    if not a.alive(live) or live.get('state') != 'running':
        raise ValueError('tournament supervisor is not running')
    prior = a.read(control / 'config.json')
    if prior.get('label_teacher_id') != 'NNUE_W512_v2':
        raise ValueError('expected frozen NNUE_W512_v2 teacher')
    a.validate_source(prior)
    model_path = model_path.resolve()
    checkpoint = a.read(model_path)
    if checkpoint.get('name') != 'NNUE_W2048_v3' or checkpoint['weights']['nnue']['width'] != 2048:
        raise ValueError('expected NNUE_W2048_v3 model')
    a.subprocess.run([prior['analyzer_bin'], '--validate-model', str(model_path)], check=True)
    if check_only:
        return dict(tournament_pid=live['tournament_pid'], original_analyzer_pid=live['sidecar_pid'],
                    old_teacher=prior['label_teacher_id'], new_teacher=checkpoint['name'],
                    new_sha256=a.file_digest(model_path), shared_cpu=prior['cpus'][3],
                    budget_ms=BUDGET_MS, early_stop='engine iterative-deepening soft stop')
    config_path = control / 'dual-label-config.json'
    if config_path.exists():
        saved = a.read(config_path)
        if (saved['new_model'] != str(model_path) or saved['new_sha256'] != a.file_digest(model_path)
                or saved.get('budget_ms') != BUDGET_MS):
            raise ValueError('existing dual-teacher binding differs')
        return saved
    binding = a.snapshot_model(model_path, model_path.read_bytes(), control)
    config = dict(run=str(run), old=prior, budget_ms=BUDGET_MS, new_model=str(model_path),
                  new_sha256=binding['sha256'], new_binding=binding,
                  analyzer_bin=prior['analyzer_bin'], analyzer_sha256=prior['analyzer_sha256'],
                  cpus=prior['cpus'])
    a.atomic(config_path, config)
    return config


def teacher_config(config, second):
    old = dict(config['old'], label_budget_ms=config['budget_ms'])
    if not second:
        return old
    model = config['new_model']
    return dict(old, label_teacher={'name': 'ab', 'model': model},
                label_teacher_id='NNUE_W2048_v3',
                models={**old['models'], model: config['new_binding']})


def paired_searches(config, moment, db):
    first_config = teacher_config(config, False)
    if moment.get('winner_teacher'):
        agent = moment['winner_teacher']
        first_config = dict(first_config, label_teacher=agent,
                            label_teacher_id='winning-handcrafted',
                            rolling_engines=False if agent.get('engine') else first_config.get('rolling_engines', False))
    model_key = first_config['label_teacher']['model']
    if moment.get('winner_teacher'):
        model_key = str(Path(model_key).resolve())
    old_sha = first_config['models'][model_key]['sha256']
    new_sha = config['new_sha256']
    existing = {s.get('model_sha256'): s for s in moment.get('searches', [])
                if s.get('budget_ms') == config['budget_ms']}
    ply = moment['center_ply']
    first = existing.get(old_sha) or a.search_position(first_config, moment, ply, db)
    if first.get('model_sha256') != old_sha:
        raise ValueError('first teacher search has the wrong model identity')
    # Save the first teacher before starting the second search. A restart reuses it.
    if moment.get('winner_teacher'):
        archived = {s.get('key', s.get('model_sha256')): s for s in moment.get('previous_teacher_searches', [])}
        for result in moment.get('searches', []):
            if result.get('model_sha256') not in (old_sha, new_sha):
                archived[result.get('key', result.get('model_sha256'))] = result
        moment['previous_teacher_searches'] = list(archived.values())
    moment['searches'] = [first]
    db.execute('UPDATE moments SET payload=? WHERE id=?', (json.dumps(moment), moment['id']))
    db.commit()
    second = existing.get(new_sha) or a.search_position(teacher_config(config, True), moment, ply, db)
    if second.get('model_sha256') != new_sha:
        raise ValueError('2048v3 search has the wrong model identity')
    moment['searches'] = [first, second]
    return moment


def run(config):
    run = Path(config['run'])
    control = run / 'analysis'
    request = control / 'analysis.request'
    a.validate_source(teacher_config(config, True))
    live = a.read(control / 'supervisor.json')
    if a.process_identity(live.get('sidecar_pid')) is not None and live.get('sidecar_state') == 'running':
        raise ValueError('original analyzer is still running; stop only its PID first')
    # A hard-killed previous dual collector may leave its request marker behind.
    # The original analyzer is confirmed stopped, so the new process adopts it.
    os.sched_setaffinity(0, {config['cpus'][3]})
    with (control / 'dual-label.lock').open('a+') as own_lock:
        fcntl.flock(own_lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        db = a.connect(run, 'training-labels.sqlite')
        db.executescript('CREATE TABLE IF NOT EXISTS dual_paired(id TEXT PRIMARY KEY);'
                         'CREATE TABLE IF NOT EXISTS dual_failures(id TEXT PRIMARY KEY, error TEXT, updated REAL);')
        try:
            while not a.STOPPING:
                live = a.read(control / 'supervisor.json')
                if not a.alive(live) or live.get('state') != 'running':
                    print('tournament stopped; dual labeling stopped', flush=True)
                    break
                winner_teacher.scan(a, db, run, config)
                a.scan(db, run, labels=True, label_candidates=lambda game: [] if
                       winner_teacher.winner_agent(config, game) else a.training_labels.candidates(game))
                batch = db.execute('SELECT * FROM moments WHERE id NOT IN (SELECT id FROM dual_paired) '
                                   "AND id NOT IN (SELECT id FROM dual_failures) ORDER BY CASE WHEN json_extract(payload,'$.winner_teacher') IS NOT NULL THEN 0 ELSE 1 END,id LIMIT 20").fetchall()
                remaining = db.execute('SELECT count(*) FROM moments WHERE id NOT IN '
                                       '(SELECT id FROM dual_paired)').fetchone()[0]
                a.atomic(control / 'dual-label-status.json', dict(state='waiting_for_cpu' if batch else 'idle',
                          remaining=remaining, failed=db.execute('SELECT count(*) FROM dual_failures').fetchone()[0],
                          completed=db.execute('SELECT count(*) FROM dual_paired').fetchone()[0],
                          winner_selected=db.execute("SELECT count(*) FROM moments WHERE json_extract(payload,'$.winner_teacher') IS NOT NULL").fetchone()[0],
                          winner_completed=db.execute("SELECT count(*) FROM moments JOIN dual_paired USING(id) WHERE json_extract(payload,'$.winner_teacher') IS NOT NULL").fetchone()[0],
                          updated=time.time()))
                if not batch:
                    request.unlink(missing_ok=True)
                    time.sleep(10)
                    continue
                request.touch()
                with (control / 'shared.lock').open('a+') as lease:
                    while not a.STOPPING:
                        try:
                            fcntl.flock(lease, fcntl.LOCK_EX | fcntl.LOCK_NB)
                            break
                        except BlockingIOError:
                            time.sleep(.2)
                    if a.STOPPING:
                        break
                    for row in batch:
                        if a.STOPPING:
                            break
                        moment = json.loads(row['payload'])
                        try:
                            moment = paired_searches(config, moment, db)
                            a.atomic(control / 'moments' / (row['id'] + '.json'), moment)
                            db.execute('UPDATE moments SET status=?,payload=?,error=NULL WHERE id=?',
                                       ('completed', json.dumps(moment), row['id']))
                            db.execute('INSERT INTO dual_paired VALUES(?)', (row['id'],))
                            db.commit()
                        except InterruptedError:
                            break
                        except Exception as exc:
                            db.execute('INSERT OR REPLACE INTO dual_failures VALUES(?,?,?)',
                                       (row['id'], str(exc), time.time()))
                            db.commit()
                            print(f'dual label failed {row["id"]}: {exc}', file=sys.stderr, flush=True)
        finally:
            request.unlink(missing_ok=True)
            db.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['check', 'prepare', 'run', 'status'])
    parser.add_argument('--run-dir', type=Path, required=True)
    parser.add_argument('--model', type=Path)
    args = parser.parse_args()
    run_dir = args.run_dir.resolve()
    if args.action in ('check', 'prepare'):
        if not args.model:
            parser.error(f'{args.action} requires --model')
        print(json.dumps(setup(run_dir, args.model, check_only=args.action == 'check'), indent=2))
    elif args.action == 'status':
        print(json.dumps(a.read(run_dir / 'analysis/dual-label-status.json'), indent=2))
    else:
        signal.signal(signal.SIGTERM, a.stop_handler)
        signal.signal(signal.SIGINT, a.stop_handler)
        run(a.read(run_dir / 'analysis/dual-label-config.json'))


if __name__ == '__main__':
    try:
        main()
    except Exception as exc:
        print(f'dual_label_sidecar: {exc}', file=sys.stderr, flush=True)
        sys.exit(1)
