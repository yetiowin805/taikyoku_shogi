"""Bounded, versioned backfill of winning handcrafted teachers."""
import json
from pathlib import Path

import training_labels as labels

POLICY = 'winner-teacher-v1'


def winner_agent(config, game):
    side = {'BlackWins': 'black', 'WhiteWins': 'white'}.get(game.get('result'))
    if not side:
        return None
    agent = game[side]
    other = game['white' if side == 'black' else 'black']
    # Use the run's frozen content bindings, not a name-based NNUE heuristic.
    def checkpoint(spec):
        key = str(Path(spec['model']).resolve())
        binding = config['old']['models'][key]
        return json.loads(Path(binding['snapshot']).read_text())
    if agent.get('name') != 'ab' or other.get('name') != 'ab':
        return None
    if checkpoint(agent).get('weights', {}).get('nnue'):
        return None
    if not checkpoint(other).get('weights', {}).get('nnue'):
        return None
    return {k: agent[k] for k in ('name', 'model', 'engine') if k in agent}


def candidates(game):
    moves = game['moves']
    loser = 'White' if game['result'] == 'BlackWins' else 'Black'
    sign = 1 if loser == 'Black' else -1
    valid = {i for i, m in enumerate(moves) if type(m.get('eval')) is int
             and abs(m['eval']) < 900000 and m.get('color') in ('Black', 'White')}
    prefix = [{k: v for k, v in m.items() if k in
               ('from_file', 'from_rank', 'to_file', 'to_rank', 'promoted', 'data', 'color')}
              for m in moves[:64]]
    group = labels.hashed([game.get('start'), prefix])
    picked = {}
    events = []
    for i in range(2, len(moves)):
        a, b = moves[i-2], moves[i]
        if b.get('color') != loser or a.get('color') != loser:
            continue
        if type(a.get('eval')) is not int or type(b.get('eval')) is not int:
            continue
        old, new = sign*a['eval'], sign*b['eval']
        if abs(old) >= 900000:
            continue
        if new <= -900000 or (abs(new) < 900000 and old-new >= 1000):
            events.append((new <= -900000, old-new, i))
    chosen = []
    for mate, magnitude, i in sorted(events, reverse=True):
        if any(abs(i-j) < 16 for j in chosen):
            continue
        points = [j for j in (i-8, i-4, i-2) if j in valid and moves[j]['color'] == loser]
        if not points:
            continue
        chosen.append(i)
        for j in points:
            picked[j] = dict(selection='winner_pre_mate' if mate else 'winner_pre_swing',
                             event_ply=i+1, offset_plies=j-i, magnitude=magnitude)
        if len(chosen) == 2:
            break
    valid = sorted(valid)
    for bucket in range(4):
        pool = valid[len(valid)*bucket//4:len(valid)*(bucket+1)//4]
        pool = [i for i in pool if all(abs(i-j) >= 4 for j in picked)]
        if pool:
            j = min(pool, key=lambda i: labels.hashed([group, POLICY, i]))
            picked[j] = dict(selection='winner_representative', magnitude=0)
    return [dict(center_ply=i+1, plies=[i+1], split_group=group,
                 split='validation' if int(group[:8], 16) % 10 == 0 else 'train',
                 sampling_policy=POLICY, selection_quota=10, selection_pool_size=len(valid),
                 game_result=game['result'], source_eval=moves[i]['eval'],
                 score_perspective='black-absolute', **metadata)
            for i, metadata in sorted(picked.items())]


def scan(a, db, run, config):
    db.execute('CREATE TABLE IF NOT EXISTS winner_scanned(policy TEXT, path TEXT, PRIMARY KEY(policy,path))')
    for slot in a.read(run / 'state.json')['slots']:
        if slot['status'] != 'done' or not slot.get('game_path'):
            continue
        path = a.absolute(slot['game_path'])
        if db.execute('SELECT 1 FROM winner_scanned WHERE policy=? AND path=?', (POLICY, str(path))).fetchone():
            continue
        try:
            data = path.read_bytes()
            game = json.loads(data)
            if game.get('abort_reason') or not game.get('result'):
                continue
            agent = winner_agent(config, game)
            if agent:
                game_hash = a.digest(data)
                # Reuse existing positions rather than creating duplicate training rows.
                existing = {json.loads(r['payload'])['center_ply']: r for r in
                            db.execute('SELECT * FROM moments WHERE json_extract(payload,\'$.game_hash\')=?', (game_hash,))}
                for sample in candidates(game):
                    old = existing.get(sample['center_ply'])
                    moment = json.loads(old['payload']) if old else {}
                    if old:
                        moment.setdefault('previous_sampling', {k: moment.get(k) for k in
                                          ('sampling_policy', 'selection', 'selection_quota', 'selection_pool_size')})
                    moment.update(sample)
                    key = old['id'] if old else a.digest(f"{POLICY}:{game_hash}:{sample['center_ply']}".encode())
                    moment.update(id=key, game=str(path), game_hash=game_hash, game_id=game['game_id'],
                                  slot_id=slot['id'], winner_teacher=agent, teacher_policy=POLICY)
                    db.execute("INSERT OR REPLACE INTO moments VALUES(?,?,'pending',?,NULL)",
                               (key, moment['magnitude'], json.dumps(moment)))
                    db.execute('DELETE FROM dual_paired WHERE id=?', (key,))
                    db.execute('DELETE FROM dual_failures WHERE id=?', (key,))
            db.execute('INSERT INTO winner_scanned VALUES(?,?)', (POLICY, str(path)))
            db.commit()
        except (OSError, ValueError, KeyError, TypeError) as exc:
            db.rollback()
            print(f'winner scan: retry {path}: {exc}', flush=True)
