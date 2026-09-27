"""Immutable engine bundles and atomic selection for future game processes."""
import fcntl
import json
import os
from pathlib import Path
import subprocess
import time

PROTOCOL = 1


def require_clock_support(binary):
    result = subprocess.run([str(binary), 'tournament-game-clock-protocol'],
                            check=True, capture_output=True, text=True, timeout=30)
    if result.stdout.strip() != '1':
        raise ValueError('engine does not support Fischer clocks; publish a clock-capable bundle first')


def verify(api, bundle):
    if bundle.get('protocol') != PROTOCOL:
        raise ValueError('unsupported game worker protocol')
    for field in ('engine', 'analyzer'):
        path = Path(bundle[field + '_bin'])
        if not path.is_absolute() or not os.access(path, os.X_OK) or api.file_digest(path) != bundle[field + '_sha256']:
            raise ValueError(f'engine bundle missing or changed: {path}')
    return bundle


def active(api, run):
    return verify(api, api.read(Path(run) / 'analysis/active-engine.json'))


def snapshot(api, run, engine, analyzer, revision):
    if not revision or not revision.strip():
        raise ValueError('a revision/build description is required')
    control = Path(run) / 'analysis'
    directory = control / 'engine-builds'
    directory.mkdir(parents=True, exist_ok=True)
    bundle = dict(protocol=PROTOCOL, revision=revision)
    for field, source in [('engine', engine), ('analyzer', analyzer)]:
        source = Path(source).resolve()
        if not os.access(source, os.X_OK):
            raise ValueError(f'missing executable: {source}')
        data = source.read_bytes()
        sha = api.digest(data)
        target = directory / (field + '-' + sha)
        if not target.exists():
            tmp = target.with_suffix('.tmp')
            tmp.write_bytes(data)
            tmp.chmod(0o555)
            os.replace(tmp, target)
        bundle[field + '_bin'] = str(target)
        bundle[field + '_sha256'] = sha
    verify(api, bundle)
    result = subprocess.run([bundle['engine_bin'], 'tournament-game-protocol'],
                            check=True, capture_output=True, text=True, timeout=30)
    if result.stdout.strip() != str(PROTOCOL):
        raise ValueError('engine does not support rolling game processes; enable with a new coordinator first')
    return bundle


def publish(api, run, bundle):
    """Caller holds update.lock and has checked models before this single commit point."""
    control = Path(run) / 'analysis'
    ident = api.digest(json.dumps(bundle, sort_keys=True).encode())
    api.atomic(control / 'engine-builds' / (ident + '.json'), bundle)
    api.atomic(control / 'active-engine.json', bundle)
    return ident


def update(api, args, run):
    control = run / 'analysis'
    config = api.read(control / 'config.json')
    if not config.get('rolling_engines'):
        raise ValueError('run has no rolling coordinator; stop/resume once with --rolling-engines')
    if not args.rollback_build and not args.compatible_speedup:
        raise ValueError('update requires --compatible-speedup; behavior changes need new entrant identities/run')
    with (control / 'update.lock').open('a+') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        before = active(api, run)
        if args.rollback_build:
            ident = args.rollback_build
            if len(ident) != 64 or any(c not in '0123456789abcdef' for c in ident):
                raise ValueError('rollback build must be a saved 64-character build ID')
            bundle = verify(api, api.read(control / 'engine-builds' / (ident + '.json')))
        else:
            bundle = snapshot(api, run, api.absolute(args.engine), api.absolute(args.analyzer), args.revision)
        if config.get('time_control') or ((run / 'state.json').exists() and api.read(run / 'state.json').get('time_control')):
            require_clock_support(bundle['engine_bin'])
        # Verify all default-engine models, including the label teacher. Frozen
        # historical agents keep their original executable/helper and schema.
        manifest = api.read(control / 'manifest.json')
        for agent in manifest['entrants']:
            if agent.get('engine'):
                continue
            model = str(api.absolute(agent['model']).resolve())
            api.validate_model_binding(model, config['models'][model])
            subprocess.run([bundle['engine_bin'], 'tournament-game-validate', model],
                           check=True, capture_output=True, text=True, timeout=120)
            subprocess.run([bundle['analyzer_bin'], '--validate-model', model],
                           check=True, capture_output=True, text=True, timeout=120)
        # Nothing above changes a running process or the active selection.
        ident = api.digest(json.dumps(bundle, sort_keys=True).encode())
        api.atomic(control / 'last-engine-update.json', dict(build=ident, previous=before,
                   selected=bundle, time=time.time(), effect='future games and teacher analysis requests'))
        publish(api, run, bundle)
        print(json.dumps(dict(build=ident, selected=bundle, running_games='unchanged'), indent=2))


def status(api, run):
    control = run / 'analysis'
    bundle = active(api, run)
    ident = api.digest(json.dumps(bundle, sort_keys=True).encode())
    slots = {s['id'] for s in api.read(run / 'state.json')['slots'] if s['status'] == 'running'}
    games = []
    for path in (control / 'game-workers').glob('slot-*.request.json'):
        try:
            slot = int(path.name.split('-')[1].split('.')[0])
            if slot in slots:
                request = api.read(path)
                games.append(dict(slot=slot, bundle=request['bundle']))
        except (OSError, ValueError, KeyError):
            continue  # A worker may be creating/removing its request concurrently.
    return dict(build=ident, selected=bundle, running_games=games)
