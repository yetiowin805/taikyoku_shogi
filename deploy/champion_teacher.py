"""Order-neutral teacher promotion policy; called with the collector lock held."""
import json
import math
import os
from pathlib import Path
import tempfile
import time

import teacher_refresh


def check(a, run, config):
    policy = config.get('auto_champion')
    if not policy:
        return config
    status_path = run / 'analysis/champion-status.json'
    stamp = None
    try:
        state = a.read(run / 'state.json')
        results = sorted((s['id'], s['model_a'], s['model_b'], s.get('score_a'))
                         for s in state['slots'] if s['status'] == 'done')
        stamp = a.digest(json.dumps([results, state['entrants'], config['new_sha256'], policy], sort_keys=True).encode())
        previous = a.read(status_path) if status_path.exists() else {}
        if previous.get('fingerprint') == stamp:
            if previous.get('state') != 'error' or time.time()-previous.get('updated', 0) < 60:
                return config
        binary = Path(policy['ratings_bin'])
        if a.file_digest(binary) != policy['ratings_sha256']:
            raise ValueError('order-neutral rating helper changed')
        margin = policy.get('margin', 50)
        if not math.isfinite(margin) or margin < 50:
            raise ValueError('champion margin must be finite and at least 50')
        # Fit one immutable state snapshot, never a file being rewritten by the
        # coordinator. This helper is read-only and does not update live ratings.
        with tempfile.NamedTemporaryFile(mode='w', suffix='.json', dir=run / 'analysis') as snapshot:
            json.dump(state, snapshot); snapshot.flush()
            result = a.subprocess.run([str(binary), snapshot.name], check=True,
                                      capture_output=True, text=True, timeout=30)
        fit = json.loads(result.stdout)
        status = dict(fingerprint=stamp, updated=time.time(), current=config.get('new_teacher_id', 'NNUE_W2048_v3'),
                      games=len(results), margin=margin, fit_status=fit['status'])
        if fit['status'] != 'converged':
            a.atomic(status_path, dict(status, state='no_finite_fit', components=fit.get('components', [])))
            return config
        ratings = fit['ratings']
        if fit['games'] != len(results) or not ratings or not all(type(v) in (int, float) and math.isfinite(v) for v in ratings.values()):
            raise ValueError('invalid or mismatched order-neutral fit')
        current = status['current']
        if current not in ratings:
            raise ValueError(f'current teacher {current} is absent from the rating fit')
        leader = min(ratings, key=lambda name: (-ratings[name], name))
        lead = ratings[leader]-ratings[current]
        status.update(leader=leader, leader_rating=ratings[leader], current_rating=ratings[current], lead=lead)
        if leader == current or lead < margin:
            a.atomic(status_path, dict(status, state='holding'))
            return config
        entrant = next((e for e in state['entrants'] if e['id'] == leader), None)
        if not entrant:
            raise ValueError(f'leader {leader} is not a current entrant')
        model = Path(entrant['model']).resolve()
        binding = config['old']['models'].get(str(model))
        if not binding or binding['sha256'] != a.file_digest(model):
            raise ValueError(f'leader {leader} lacks its unchanged frozen checkpoint')
        if a.read(model).get('name') != leader:
            raise ValueError('leader checkpoint name does not match entrant identity')
        report = teacher_refresh.replace(a, run, model, engine=entrant.get('engine'))
        updated = a.read(run / 'analysis/dual-label-config.json')
        event = dict(status, state='switched', **report)
        a.atomic(status_path, event)
        with (run / 'analysis/champion-switches.jsonl').open('a') as log:
            log.write(json.dumps(event)+'\n'); log.flush(); os.fsync(log.fileno())
        print(f"teacher champion: {current} -> {leader}, lead {lead:.1f}, backfill {report.get('backfill_positions', 0)}", flush=True)
        return updated
    except Exception as exc:
        a.atomic(status_path, dict(state='error', fingerprint=stamp, updated=time.time(), error=str(exc)))
        print(f'champion selection failed; retaining current teacher: {exc}', flush=True)
        # A failure after the durable switch must not make this worker continue
        # with the obsolete in-memory config.
        return a.read(run / 'analysis/dual-label-config.json')
