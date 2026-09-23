"""Deterministic, bounded sampling for training-label collection (no training)."""
import hashlib
import json

POLICY = 'training-labels-v1'


def hashed(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True).encode()).hexdigest()


def candidates(game):
    # Historical helpers remain the legacy analyzer's responsibility. A modern
    # frozen teacher must not silently run through a historical CLI binding.
    if any(game[side].get('engine') or game[side].get('name') != 'ab'
           for side in ('black', 'white')):
        return []
    moves = game['moves']
    valid = [i for i, m in enumerate(moves) if type(m.get('eval')) is int
             and abs(m['eval']) < 900_000 and m.get('color') in ('Black', 'White')]
    # Exclude agent/evaluation metadata from grouping related opening prefixes.
    prefix = [{k: v for k, v in m.items() if k in ('from_file', 'from_rank', 'to_file', 'to_rank', 'promoted', 'data', 'color')}
              for m in moves[:64]]
    group = hashed([game.get('start'), prefix])
    split = 'validation' if int(group[:8], 16) % 10 == 0 else 'train'
    picked = {}
    def add(i, reason, pool_size, quota):
        if i not in picked:
            picked[i] = dict(center_ply=i+1, plies=[i+1], magnitude=0,
                             selection=reason, selection_pool_size=pool_size,
                             selection_quota=quota, split=split, split_group=group,
                             sampling_policy=POLICY, game_result=game['result'],
                             source_eval=moves[i]['eval'], score_perspective='black-absolute')
    # Twelve equal-progress strata; pick by hash rather than always the middle.
    for bucket in range(12):
        pool = valid[len(valid)*bucket//12:len(valid)*(bucket+1)//12]
        if pool:
            i = min(pool, key=lambda i: hashed([group, 'representative', i]))
            add(i, 'representative', len(pool), 1)
    swings = []
    for i in valid:
        if i >= 2 and moves[i-2].get('color') == moves[i]['color']:
            old = moves[i-2].get('eval')
            if type(old) is int and abs(old) < 900_000 and abs(moves[i]['eval']-old) >= 1000:
                swings.append((abs(moves[i]['eval']-old), i))
    # Space targeted examples too; avoid a single sequence filling the quota.
    selected = []
    for magnitude, i in sorted(swings, reverse=True):
        if i in picked or any(abs(i-j) < 4 for j in selected):
            continue
        add(i, 'evaluation_swing', len(swings), 5)
        picked[i]['magnitude'] = magnitude
        selected.append(i)
        if len(selected) == 5:
            break
    # This is explicitly an ending proxy, not a claim to detect royal threats.
    if game['result'] in ('BlackWins', 'WhiteWins'):
        for distance in (8, 24, 48):
            target = len(moves)-distance
            pool = [i for i in valid if i not in picked and abs(i-target) <= 4]
            if pool:
                add(min(pool, key=lambda i: abs(i-target)), 'decisive_ending', len(pool), 1)
    return list(picked.values())
