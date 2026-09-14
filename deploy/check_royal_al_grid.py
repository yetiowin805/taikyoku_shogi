#!/usr/bin/env python3
"""Validate all 32 agents and pinned engine identities before a future launch."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent
POLICY = {"policy": "royal-al-v1", "swap_removal": True, "aspiration": 500, "checkpoint_L_modes": True}


def check(manifest, binary, validator):
    policy = json.loads(subprocess.check_output([str(binary), "royal-al-policy"], cwd=ROOT, text=True))
    if policy != POLICY:
        raise ValueError("binary does not implement the required A/L search policy")
    manifest = manifest.resolve()
    entrants = json.loads(manifest.read_text())["entrants"]
    if len(entrants) != 32 or len({e['id'] for e in entrants}) != 32:
        raise ValueError("expected exactly 32 unique entrants")
    # Derive exact expected policies with a temporary generation; never rewrite this field.
    import tempfile
    with tempfile.TemporaryDirectory(prefix='royal-al-check-') as tmp:
        expected_dir = Path(tmp)/'field'
        subprocess.run([str(binary), 'royal-al-grid', '--out', str(expected_dir)], cwd=ROOT, check=True, stdout=subprocess.DEVNULL)
        expected = {e['id']: e for e in json.loads((expected_dir/'manifest.json').read_text())['entrants']}
        if set(expected) != {e['id'] for e in entrants}:
            raise ValueError('entrant IDs differ from the agreed field')
        models = []
        for ent in entrants:
            path = Path(ent['model'])
            path = path if path.is_absolute() else ROOT/path
            cp = json.loads(path.read_text())
            wanted = json.loads(Path(expected[ent['id']]['model']).read_text())
            if any(cp.get(k) != wanted[k] for k in ('name', 'weights', 'search_defaults')):
                raise ValueError(f"checkpoint policy/weights mismatch: {ent['id']}")
            if ent.get('engine') != expected[ent['id']].get('engine'):
                raise ValueError(f"incorrect engine pin: {ent['id']}")
            subprocess.run([str(validator), '--validate-model', str(path)], check=True, capture_output=True)
            models.append({'id': ent['id'], 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()})
    history = json.loads((ROOT/'models/history/manifest.json').read_text())
    revision = next(e['git'] for e in history['engines'] if e['id'] == 'LOGIC_PRE_ROYAL_AL')
    frozen = ROOT/'models/history/bin/LOGIC_PRE_ROYAL_AL'
    if not os.access(frozen, os.X_OK) or f'rev={revision}' not in frozen.with_suffix('.meta').read_text().splitlines():
        raise ValueError('missing/wrong frozen reference engine; run deploy/freeze_history.sh --id LOGIC_PRE_ROYAL_AL')
    helper = frozen.with_name(frozen.name + '.analyze_position')
    meta = dict(line.split('=', 1) for line in frozen.with_suffix('.meta').read_text().splitlines() if '=' in line)
    if not os.access(helper, os.X_OK) or hashlib.sha256(helper.read_bytes()).hexdigest() != meta.get('analyzer_sha256'):
        raise ValueError('missing/wrong historical analysis helper; rebuild the pinned engine')
    return {'historical_analyzer_sha256': meta['analyzer_sha256'], 'policy': policy, 'models': models, 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
            'frozen_revision': revision, 'frozen_binary_sha256': hashlib.sha256(frozen.read_bytes()).hexdigest()}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--manifest', type=Path, default=ROOT/'models/royal-al-grid/manifest.json')
    p.add_argument('--binary', type=Path, default=ROOT/'target/release/taikyoku_shogi')
    p.add_argument('--validator', type=Path, default=ROOT/'target/release/analyze_position')
    args = p.parse_args()
    try:
        result = check(args.manifest, args.binary.resolve(), args.validator.resolve())
        print(json.dumps(result, indent=2))
    except (OSError, ValueError, KeyError, StopIteration, subprocess.SubprocessError) as exc:
        print(f'royal-al preflight failed: {exc}; tournament did not start', file=sys.stderr)
        return 1
    return 0

if __name__ == '__main__':
    sys.exit(main())
