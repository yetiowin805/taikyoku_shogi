"""Build a dense recent-only corpus from an immutable pilot_snapshot archive.

No searches, live database writes, training, or model changes. Export one game at
 a time with the existing Rust replayer; completed shards survive interruptions.
"""
import argparse
from collections import Counter, defaultdict
import json
import math
from pathlib import Path
import shutil
import subprocess

import numpy as np
from pilot_data import MOVE_KEYS, digest, hashed, position_key
from v4_data import SCALE, probability

POLICY = 'recent-dense-v1'
SPLITS = ('test', 'validation', 'train')
PROPORTIONS = {'representative': .75, 'precursor': .20, 'mate': .05}


def rows(path):
    with Path(path).open() as f:
        for line in f:
            if line.strip():
                yield json.loads(line)


def write_json(path, value):
    path = Path(path)
    tmp = path.with_suffix(path.suffix + '.tmp')
    tmp.write_text(json.dumps(value, indent=2) + '\n')
    tmp.replace(path)


def finite(value):
    return type(value) in (int, float) and math.isfinite(value)


def score_from_probability(q):
    q = min(1-1e-6, max(1e-6, q))
    return SCALE * math.log(q / (1-q))


def teacher_labels(snapshot, manifest, diagnostics):
    labels = defaultdict(dict)
    for name in manifest['labels']:
        for moment in rows(snapshot / name):
            if moment.get('score_perspective', 'black-absolute') != 'black-absolute':
                raise ValueError('Unknown moment score perspective')
            for s in moment.get('searches', []):
                if (not finite(s.get('score')) or not finite(s.get('completed_depth'))
                        or s['completed_depth'] < 1 or type(s.get('ply')) is not int or s['ply'] < 1):
                    diagnostics['invalid_analysis_record'] += 1
                    continue
                if s.get('score_perspective', 'black-absolute') != 'black-absolute':
                    raise ValueError('Unknown teacher score perspective')
                teacher = s.get('model_sha256')
                if not teacher:
                    raise ValueError('Completed analysis lacks teacher content identity')
                if s.get('game_hash', moment['game_hash']) != moment['game_hash']:
                    raise ValueError('Analysis game hash mismatch')
                key = (moment['game_hash'], s['ply']-1)
                label = dict(model_sha256=teacher, score=s['score'], completed_depth=s['completed_depth'],
                             model=s.get('agent', {}).get('model'), engine_sha256=s.get('engine_sha256'),
                             analyzer_sha256=s.get('analyzer_sha256'), key=s.get('key'),
                             elapsed_ms=s.get('elapsed_ms'), hard_timeout=s.get('hard_timeout', False),
                             selection=moment.get('selection'), best_move=s.get('best_move'))
                iterations = [i for i in s.get('iterations', []) if finite(i.get('score'))]
                label['iteration_probability_change'] = (abs(probability(iterations[-1]['score'])-
                    probability(iterations[-2]['score'])) if len(iterations) >= 2 else None)
                old = labels[key].get(teacher)
                # Same teacher only: depth is not used to compare different engines.
                if old is None or (label['completed_depth'], label['elapsed_ms'] or 0, label['key'] or '') > (
                        old['completed_depth'], old['elapsed_ms'] or 0, old['key'] or ''):
                    labels[key][teacher] = label
    return {k: [v[t] for t in sorted(v)] for k, v in labels.items()}


def assign_groups(records, parent_samples):
    """Inherit previous exposure, keep pairs/prefixes together, quarantine conflicts."""
    prior = defaultdict(set)
    parent_train = set()
    for s in rows(parent_samples):
        prior[s['game_sha256']].add((s['group'], s['split']))
        if s['split'] == 'train':
            parent_train.add(s['canonical_position_hash'])
    roots = {h: h for h in records}
    def find(h):
        while roots[h] != h:
            roots[h] = roots[roots[h]]
            h = roots[h]
        return h
    seen = {}
    for h, r in sorted(records.items()):
        keys = [('prefix', r['prefix']), ('pair', hashed([r['run'], r['pair_seed']]))]
        keys += [('parent', g) for g, _ in prior[h]]
        for key in keys:
            if key in seen:
                roots[find(h)] = find(seen[key])
            seen[key] = h
    groups = defaultdict(list)
    for h in records:
        groups[find(h)].append(h)
    for members in groups.values():
        inherited = {pair for h in members for pair in prior[h]}
        splits = {sp for _, sp in inherited}
        group = min(g for g, _ in inherited) if inherited else min(members)
        b = int(hashed(group)[:8], 16) % 10
        split = next(iter(splits)) if len(splits) == 1 else ('test' if b == 0 else 'validation' if b == 1 else 'train')
        for h in members:
            records[h].update(group=group, split=split, quarantine=len(splits) > 1,
                              inherited_holdout=bool(inherited) and split != 'train')
    return parent_train


def label_position(move, searches):
    """Targets and outcomes are STM-relative; raw saved/teacher scores stay Black-relative."""
    sign = 1 if move['color'] == 'Black' else -1
    saved = move.get('eval') if finite(move.get('eval')) else None
    probs = [probability(s['score']) for s in searches]
    q = sum(probs)/len(probs) if probs else probability(saved)
    mate = (saved is not None and abs(saved) >= 900000) or any(abs(s['score']) >= 900000 for s in searches)
    return dict(score=sign*score_from_probability(q), target_probability=q if sign == 1 else 1-q,
                saved_score=saved, label_source='analysis' if searches else 'saved',
                teacher_scores=searches, mate=mate, mate_status='search_claim' if mate else None,
                teacher_probability_spread=max(probs)-min(probs) if probs else 0,
                saved_target_disagreement=abs(probability(saved)-q) if saved is not None else None,
                completed_depth=min(s['completed_depth'] for s in searches) if searches else move.get('completed_depth'))


def classify_positions(game, game_hash, labels, diagnostics):
    moves = game['moves']
    selected = {}
    for i, m in enumerate(moves):
        ss = labels.get((game_hash, i), [])
        if m.get('color') not in ('Black', 'White') or (not ss and not finite(m.get('eval'))):
            diagnostics['invalid_saved_record'] += 1
            continue
        selected[i] = label_position(m, ss)
    # Cluster neighboring mate claims into episodes; include both players and
    # preserve finite precursor labels instead of propagating a loss backwards.
    episodes = []
    for i, s in selected.items():
        if s['mate']:
            if not episodes or i-episodes[-1][-1] > 16:
                episodes.append([i])
            else:
                episodes[-1].append(i)
    mate_episode = {i: ep[0] for ep in episodes for i in ep}
    for i, s in selected.items():
        s.update(stratum='representative', selection='representative',
                 episode='phase-'+str(min(15, i*16//max(1, len(moves)))))
        if i in mate_episode:
            s.update(stratum='mate', selection='mate', episode='mate-'+str(mate_episode[i]))
        else:
            following = next((ep[0] for ep in episodes if 0 < ep[0]-i <= 128), None)
            if following is not None:
                s.update(stratum='precursor', selection='mate_precursor', episode='mate-'+str(following),
                         plies_before_mate=following-i)
    return selected, episodes


def analysis_candidates(game, game_hash, selected, episodes):
    """Offline queue only: broad coverage plus capped episodes and disagreement.

    These are position quotas, not a promise about runtime CPU allocation.
    A later scheduler can reserve compute shares without altering this corpus.
    """
    reasons = defaultdict(set)
    valid = sorted(selected)
    for b in range(8):
        pool = valid[b*len(valid)//8:(b+1)*len(valid)//8]
        if pool:
            reasons[min(pool, key=lambda i: hashed([game_hash, 'random', i]))].add('representative')
    for ep in episodes[:4]:
        for distance in (0, 2, 4, 8, 16, 32, 64, 128):
            for i in (ep[0]-distance, ep[0]-distance-1):
                if i in selected:
                    reasons[i].add('mate_window')
    suspect = []
    winner = {'BlackWins': 'Black', 'WhiteWins': 'White'}.get(game.get('result'))
    for i, s in selected.items():
        m = game['moves'][i]
        spread = s['teacher_probability_spread']
        if spread >= .2:
            suspect.append((spread, i, 'teacher_disagreement'))
        if finite(m.get('static_eval')) and s['saved_score'] is not None:
            delta = abs(probability(m['static_eval'])-probability(s['saved_score']))
            if delta >= .2:
                suspect.append((delta, i, 'static_search_disagreement'))
        if winner and m['color'] != winner and s['saved_score'] is not None:
            q = probability(s['saved_score'])
            own = q if m['color'] == 'Black' else 1-q
            if own >= .65 and not s['mate']:
                suspect.append((own-.5, i, 'optimism_in_loss'))
        changes = [t['iteration_probability_change'] for t in s['teacher_scores']
                   if t['iteration_probability_change'] is not None]
        if changes and max(changes) >= .15:
            suspect.append((max(changes), i, 'iteration_instability'))
    picked = []
    for magnitude, i, reason in sorted(suspect, reverse=True):
        if any(abs(i-j) < 32 for j in picked):
            continue
        reasons[i].add(reason)
        picked.append(i)
        if len(picked) == 8:
            break
    return [dict(game_hash=game_hash, ply=i+1, reasons=sorted(rs),
                 operation='refine' if selected[i]['teacher_scores'] else 'initial',
                 initial_budget_ms=10000, suggested_extension_ms=60000,
                 score_perspective='black-absolute') for i, rs in sorted(reasons.items())]


class Weights:
    """Stratum -> related-game group -> game -> phase/episode -> position."""
    def __init__(self, proportions):
        self.proportions = proportions
        self.counts = Counter()
        self.groups = defaultdict(set)
        self.games = defaultdict(set)
        self.episodes = defaultdict(set)
    def add(self, s):
        sp, st, gr, ga, ep = (s[k] for k in ('split', 'stratum', 'group', 'game_sha256', 'episode'))
        self.groups[sp, st].add(gr)
        self.games[sp, st, gr].add(ga)
        self.episodes[sp, st, gr, ga].add(ep)
        self.counts[sp, st, gr, ga, ep] += 1
    def weight(self, s):
        sp, st, gr, ga, ep = (s[k] for k in ('split', 'stratum', 'group', 'game_sha256', 'episode'))
        norm = sum(v for k, v in self.proportions.items() if self.groups[sp, k])
        return (self.proportions[st]/norm/len(self.groups[sp, st])/len(self.games[sp, st, gr])/
                len(self.episodes[sp, st, gr, ga])/self.counts[sp, st, gr, ga, ep])


def build(a):
    snapshot, out = a.snapshot.resolve(), a.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    manifest = json.loads((snapshot/'snapshot.json').read_text())
    inputs = {str(p.resolve()): digest(p) for p in [snapshot/'snapshot.json', a.parent_samples, a.base, a.binary]}
    for name in manifest['labels']:
        inputs[str(snapshot/name)] = digest(snapshot/name)
    recipe = dict(policy=POLICY, inputs=inputs, proportions=a.proportions, scale=SCALE,
                  builder_sha256=digest(__file__), game_paths_root=str(snapshot))
    recipe_path = out/'recipe.json'
    if recipe_path.exists() and json.loads(recipe_path.read_text()) != recipe:
        raise ValueError('Inputs or recipe changed: use a new output directory')
    write_json(recipe_path, recipe)
    if (out/'dataset.json').exists():
        verify(out)
        return
    diagnostics = Counter()
    records = {}
    for entry in manifest['games']:
        h, path = entry['sha256'], snapshot/entry['path']
        if digest(path) != h:
            raise ValueError('Game content changed: '+str(path))
        g = json.loads(path.read_text())
        if g.get('abort_reason') or g.get('result') not in ('BlackWins', 'WhiteWins', 'Draw'):
            diagnostics['incomplete_game'] += 1
            continue
        prefix = [{k: m[k] for k in MOVE_KEYS if k in m} for m in g['moves'][:64]]
        records[h] = dict(entry, path=str(path), prefix=hashed([g['start'], prefix]))
    parent_train = assign_groups(records, a.parent_samples)
    labels = teacher_labels(snapshot, manifest, diagnostics)
    write_json(out/'games.json', records)
    seen = set()
    stats = Counter()
    weights = Weights(a.proportions)
    shards = []
    offset = 0
    schema = None
    raw = out/'samples.unweighted.jsonl'
    with raw.open('w') as index, (out/'analysis-candidates.jsonl').open('w') as queue:
        for n, (h, r) in enumerate(sorted(records.items(), key=lambda kv: (SPLITS.index(kv[1]['split']), kv[0]))):
            if r['quarantine']:
                diagnostics['conflicting_parent_split_game'] += 1
                continue
            game = json.loads(Path(r['path']).read_text())
            selected, episodes = classify_positions(game, h, labels, diagnostics)
            if not selected:
                continue
            shard = out/'shards'/h
            job = out/'jobs'/f'{h}.jsonl'
            job.parent.mkdir(exist_ok=True)
            scores = {str(i): s['score']*(1 if game['moves'][i]['color'] == 'Black' else -1)
                      for i, s in selected.items()}
            job.write_text(json.dumps(dict(game=r['path'], plies=sorted(selected), scores=scores,
                                           split='train' if r['split'] == 'train' else 'validation'))+'\n')
            if not (shard/'dataset.json').exists():
                if shard.exists():
                    shutil.rmtree(shard)  # Only this builder's incomplete derived shard.
                subprocess.run([str(a.binary.resolve()), 'export', str(a.base.resolve()), str(job), str(shard)],
                               check=True, stdout=subprocess.DEVNULL)
            identity = json.loads((shard/'dataset.json').read_text())
            for name, ext in [('features', 'bin'), ('samples', 'jsonl'), ('schema', 'json')]:
                if digest(shard/f'{name}.{ext}') != identity[name+'_sha256']:
                    raise ValueError('Completed shard changed: '+str(shard))
            if identity['baseline_sha256'] != inputs[str(a.base.resolve())]:
                raise ValueError('Baseline changed')
            current_schema = json.loads((shard/'schema.json').read_text())
            if schema is not None and schema != current_schema:
                raise ValueError('Feature schema differs between shards')
            schema = current_schema
            feature_path = shard/'features.bin'
            count = feature_path.stat().st_size//4
            data = np.memmap(feature_path, mode='r', dtype='<u4') if count else np.array([], dtype='<u4')
            retained = set()
            exported = 0
            for s in rows(shard/'samples.jsonl'):
                exported += 1
                key = position_key(data, s)
                if r['split'] != 'train' and key in parent_train:
                    diagnostics['heldout_seen_by_parent_training'] += 1
                    continue
                if key in seen:
                    diagnostics['duplicate_feature_position'] += 1
                    continue
                seen.add(key)
                i = s['ply']
                s.update(selected[i])
                s.update(offset=s['offset']+offset, canonical_position_hash=key, source='recent',
                         split=r['split'], group=r['group'], game_sha256=h,
                         inherited_holdout=r['inherited_holdout'], side=game['moves'][i]['color'],
                         game_length=len(game['moves']), outcome=(None if game['result'] == 'Draw' else
                         float((game['result'] == 'BlackWins') == (game['moves'][i]['color'] == 'Black'))),
                         game_result=game['result'], draw_outcome_unclassified=game['result'] == 'Draw',
                         original_agent=game[game['moves'][i]['color'].lower()])
                # Preserve all label evidence; outcome mixture and material baseline
                # are later training choices, not irrevocably baked into the dataset.
                index.write(json.dumps(s, separators=(',', ':'))+'\n')
                weights.add(s)
                retained.add(i)
                stats['samples'] += 1
                stats['split:'+s['split']] += 1
                stats['stratum:'+s['stratum']] += 1
                stats['label:'+s['label_source']] += 1
                if not s['inherited_holdout'] and s['split'] != 'train':
                    stats['fresh_holdout:'+s['split']] += 1
            diagnostics['exporter_omitted_terminal_or_duplicate'] += len(selected)-exported
            for c in analysis_candidates(game, h, selected, episodes):
                if c['ply']-1 in retained:
                    queue.write(json.dumps(dict(c, game=r['path'], group=r['group'], split=r['split']))+'\n')
                    stats['analysis_candidates'] += 1
            if count:
                shards.append(dict(path=str(feature_path), sha256=identity['features_sha256']))
            offset += count
            print(json.dumps(dict(games=n+1, total_games=len(records), **stats)), flush=True)
    if any(not stats['split:'+sp] for sp in SPLITS):
        raise ValueError('Corpus lacks a train/validation/test split')
    weight_totals = Counter()
    games_kept = defaultdict(set)
    with (out/'samples.jsonl.tmp').open('w') as f:
        for s in rows(raw):
            s['sampling_weight'] = weights.weight(s)
            weight_totals[s['split']+':'+s['stratum']] += s['sampling_weight']
            games_kept[s['split']].add(s['game_sha256'])
            f.write(json.dumps(s, separators=(',', ':'))+'\n')
    (out/'samples.jsonl.tmp').replace(out/'samples.jsonl')
    raw.unlink()
    write_json(out/'schema.json', schema)
    report = dict(policy=POLICY, counts=stats, diagnostics=diagnostics, weight_totals=weight_totals,
                  games_by_split={k: len(v) for k, v in games_kept.items()},
                  feature_bytes=offset*4, new_searches=0, historical_training_samples=0,
                  caveats=['Mate fields are search claims, not formal proofs.',
                           'Inherited holdouts are not fresh tests of a previously selected parent.',
                           'Unclassified draws retain teacher labels but have no outcome target.',
                           'Feature-identical positions are deduplicated; history-dependent values cannot be represented by these inputs.',
                           'Analysis queue is offline; no live analyzer policy has been changed.'])
    write_json(out/'report.json', report)
    identity = dict(version=2, policy=POLICY, scale=SCALE, samples=stats['samples'], feature_shards=shards,
                    baseline_sha256=inputs[str(a.base.resolve())],
                    samples_sha256=digest(out/'samples.jsonl'), schema_sha256=digest(out/'schema.json'),
                    recipe_sha256=digest(recipe_path), report_sha256=digest(out/'report.json'),
                    games_sha256=digest(out/'games.json'), queue_sha256=digest(out/'analysis-candidates.jsonl'))
    write_json(out/'dataset.json', identity)  # Completion marker, written last.
    verify(out)


def verify(out):
    identity = json.loads((out/'dataset.json').read_text())
    for key, name in [('samples', 'samples.jsonl'), ('schema', 'schema.json'), ('recipe', 'recipe.json'),
                      ('report', 'report.json'), ('games', 'games.json'), ('queue', 'analysis-candidates.jsonl')]:
        if digest(out/name) != identity[key+'_sha256']:
            raise ValueError('Dataset file changed: '+name)
    bounds = []
    offset = 0
    for shard in identity['feature_shards']:
        p = Path(shard['path'])
        if digest(p) != shard['sha256'] or p.stat().st_size % 4:
            raise ValueError('Feature shard changed: '+str(p))
        end = offset+p.stat().st_size//4
        bounds.append((offset, end))
        offset = end
    groups = {}
    positions = set()
    weights = Counter()
    count = 0
    for s in rows(out/'samples.jsonl'):
        if s['group'] in groups and groups[s['group']] != s['split']:
            raise ValueError('Group leakage')
        groups[s['group']] = s['split']
        if s['canonical_position_hash'] in positions:
            raise ValueError('Feature-position leakage')
        positions.add(s['canonical_position_hash'])
        if not any(a <= s['offset'] and s['offset']+s['us']+s['them'] <= b for a, b in bounds):
            raise ValueError('Feature offset outside shard')
        if not finite(s['score']) or not 0 <= s['target_probability'] <= 1 or not 0 < s['sampling_weight'] <= 1:
            raise ValueError('Invalid target or weight')
        weights[s['split']] += s['sampling_weight']
        count += 1
    if count != identity['samples'] or set(weights) != set(SPLITS) or any(abs(v-1) > 1e-8 for v in weights.values()):
        raise ValueError('Dataset count/weight totals invalid')
    print(json.dumps(dict(verified=True, samples=count, split_weights=weights)), flush=True)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    sub = p.add_subparsers(dest='command', required=True)
    b = sub.add_parser('build')
    for name in ('snapshot', 'out', 'parent-samples', 'base', 'binary'):
        b.add_argument('--'+name, type=Path, required=True)
    b.add_argument('--precursor-weight', type=float, default=.20)
    b.add_argument('--mate-weight', type=float, default=.05)
    v = sub.add_parser('verify')
    v.add_argument('out', type=Path)
    a = p.parse_args()
    if a.command == 'verify':
        verify(a.out)
    else:
        if not 0 < a.precursor_weight < 1 or not 0 < a.mate_weight < 1 or a.precursor_weight+a.mate_weight >= 1:
            p.error('Weights must be positive and sum to less than one')
        a.proportions = dict(representative=1-a.precursor_weight-a.mate_weight,
                             precursor=a.precursor_weight, mate=a.mate_weight)
        build(a)

if __name__ == '__main__':
    main()
