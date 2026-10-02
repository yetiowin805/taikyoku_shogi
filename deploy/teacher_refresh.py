"""Bounded, resumable replacement of the sidecar's default second teacher."""
import json
import sqlite3
import time
from collections import defaultdict
from pathlib import Path


def select_backfill(db, new_sha):
    groups = defaultdict(list)
    for row in db.execute('SELECT payload FROM moments JOIN dual_paired USING(id)'):
        moment = json.loads(row['payload'])
        searches = moment.get('searches', [])
        if any(s.get('model_sha256') == new_sha and s.get('budget_ms') == 30000 for s in searches):
            continue
        groups[moment['game_hash']].append(moment)
    selected = []
    for moments in groups.values():
        def priority(m):
            scores = [s['score'] for s in m.get('searches', []) if type(s.get('score')) is int]
            disagreement = max(scores)-min(scores) if scores else 0
            return (m.get('selection') == 'winner_pre_mate', disagreement,
                    m.get('magnitude', 0), m['id'])
        picked = []
        for m in sorted(moments, key=priority, reverse=True):
            if all(abs(m['center_ply']-p['center_ply']) >= 8 for p in picked):
                picked.append(m)
            if len(picked) == 4:
                break
        ordered = sorted(moments, key=lambda m: (m['center_ply'], m['id']))
        for bucket in range(4):
            pool = ordered[len(ordered)*bucket//4:len(ordered)*(bucket+1)//4]
            pool = [m for m in pool if all(abs(m['center_ply']-p['center_ply']) >= 4 for p in picked)]
            if pool:
                picked.append(pool[len(pool)//2])
        selected.extend(m['id'] for m in picked)
    return sorted(selected)


def apply_backfill(db, config):
    refresh = config.get('teacher_refresh')
    if not refresh:
        return
    # The manifest is written first; this transaction is safely replayed if the
    # process dies between the config update and database commit.
    with db:
        for key in refresh['ids']:
            row = db.execute('SELECT payload FROM moments WHERE id=?', (key,)).fetchone()
            if not row:
                raise ValueError(f'missing backfill position {key}')
            moment = json.loads(row['payload'])
            if moment.get('teacher_refresh') == refresh['id']:
                continue
            moment['teacher_refresh'] = refresh['id']
            db.execute("UPDATE moments SET status='pending',payload=?,error=NULL WHERE id=?", (json.dumps(moment), key))
            db.execute('DELETE FROM dual_paired WHERE id=?', (key,))
            db.execute('DELETE FROM dual_failures WHERE id=?', (key,))


def replace(a, run, model, check_only=False, engine=None):
    control = run / 'analysis'
    path = control / 'dual-label-config.json'
    config = a.read(path)
    model = model.resolve()
    data = model.read_bytes()
    checkpoint = json.loads(data)
    name = checkpoint.get('name')
    if not name:
        raise ValueError('teacher checkpoint must have a name')
    a.validate_source(config['old'])
    helper = a.analyzer_for_agent(config['old'], {'engine': engine}) if engine else config
    a.subprocess.run([helper['analyzer_bin'], '--validate-model', str(model)], check=True)
    sha = a.digest(data)
    db = a.connect(run, 'training-labels.sqlite')
    try:
        if config['new_sha256'] == sha and config.get('new_teacher_agent', {}).get('engine') == engine:
            if not check_only:
                apply_backfill(db, config)
            return dict(teacher=name, changed=False)
        ids = select_backfill(db, sha)
        report = dict(teacher=name, sha256=sha, backfill_positions=len(ids), changed=True)
        if check_only:
            return report
        # Caller owns dual-label.lock, so no collector can race this backup.
        backup = control / 'teacher-backups' / str(time.time_ns())
        backup.mkdir(parents=True)
        (backup / path.name).write_bytes(path.read_bytes())
        with sqlite3.connect(backup / 'training-labels.sqlite') as dest:
            db.backup(dest)
        binding = a.snapshot_model(model, data, control)
        config.update(new_model=str(model), new_sha256=sha, new_binding=binding,
                      new_teacher_id=name, new_teacher_agent=dict(name='ab', model=str(model), **({'engine': engine} if engine else {})), teacher_refresh=dict(id=f"{sha}:{time.time_ns()}", ids=ids))
        a.atomic(path, config)
        apply_backfill(db, config)
        return dict(report, backup=str(backup))
    finally:
        db.close()
