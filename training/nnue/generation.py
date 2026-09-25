"""Train a complete generation and atomically admit it to a new tournament run.

Run under systemd on CPU 3 with a hard runtime limit and ExecStopPost restore.
Live model files and the old tournament's entrant identities are never rewritten.
"""
import argparse
import fcntl
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

import numpy as np
from pilot_data import digest
import pilot_vps

WIDTHS=[256,384,512,768,1024,1536,2048,3072]


def read(p):return json.loads(Path(p).read_text())
def write(p,value):pilot_vps.write(Path(p),value)


def preflight(c):
    if c.get('widths')!=WIDTHS:raise ValueError('Configuration widths differ from this generation')
    out=Path(c['out']).resolve()
    for protected in [c['old_run'],c['dataset'],str(Path(c['repo'])/'models')]:
        p=Path(protected).resolve()
        if out.is_relative_to(p) or p.is_relative_to(out):raise ValueError('Output overlaps protected inputs')
    if Path(c['old_run']).resolve()==Path(c['new_run']).resolve():raise ValueError('A new run directory is required')
    for path,sha in c['code_hashes'].items():
        if digest(path)!=sha:raise ValueError('Pinned training code changed: '+path)
    for path,sha in c['parent_hashes'].items():
        if digest(path)!=sha:raise ValueError('Frozen parent model changed: '+path)
    data=read(Path(c['dataset'])/'dataset.json')
    if digest(Path(c['dataset'])/'dataset.json')!=c['dataset_sha256']:raise ValueError('Dataset identity changed')
    for name,ext in [('samples','jsonl'),('features','bin'),('schema','json')]:
        if digest(Path(c['dataset'])/(name+'.'+ext))!=data[name+'_sha256']:raise ValueError('Dataset contents changed')


def standings(state):
    """Equal-weight Bradley-Terry fit; weak zero-centred prior handles separation."""
    names=sorted(e['id'] for e in state['entrants']); index={n:i for i,n in enumerate(names)}
    games=[s for s in state['slots'] if s['status']=='done' and s.get('score_a') is not None]
    if not games:raise ValueError('No completed games for retirement ranking')
    a=np.array([index[s['model_a']] for s in games]);b=np.array([index[s['model_b']] for s in games])
    score=np.array([s['score_a'] for s in games]);assert np.isin(score,[0,.5,1]).all()
    design=np.eye(len(names))[a]-np.eye(len(names))[b]
    x=np.zeros(len(names))
    for _ in range(100):
        p=1/(1+np.exp(-np.clip(design@x,-40,40)))
        grad=design.T@(p-score)+.1*x
        h=(design.T*(p*(1-p)))@design+.1*np.eye(len(names))
        step=np.linalg.solve(h,grad); x-=step
        if np.max(np.abs(step))<1e-9:break
    return sorted([dict(id=n,rating=1500+float(x[i])*400/np.log(10),
                        games=int(np.sum(a==i)+np.sum(b==i))) for i,n in enumerate(names)],
                  key=lambda r:(r['rating'],r['id']))


def restore(c):
    out=Path(c['out']); flag=out/'lease-state.json'
    if flag.exists() and read(flag).get('needs_restore'):
        pilot_vps.ROOT=out
        pilot_vps.restore(c)
        write(flag,dict(needs_restore=False,restored=time.time()))


def live(run):
    path=Path(run)/'analysis/supervisor.json'
    if not path.exists():return False
    m=read(path)
    try:return m.get('state')=='running' and pilot_vps.identity(m['pid'])==m['identity']
    except FileNotFoundError:return False


def recover(c):
    """ExecStopPost also recovers interruption during the tournament switchover."""
    restore(c)
    path=Path(c['out'])/'rollout.json'
    if not path.exists():return
    plan=read(path)
    if plan['state'] not in ('prepared','rolling_back'):return
    if live(c['new_run']):
        Path(c['repo'],'data/run/royal-nnue-current-run.txt').write_text(str(Path(c['new_run']).relative_to(c['repo']))+'\n')
        plan.update(state='deployed',recovered=True);write(path,plan);return
    subprocess.run(['systemctl','stop',c['unit']+'-launch'],check=False)
    # A stop already delivered to the old supervisor may still be checkpointing.
    time.sleep(30)
    if not live(c['old_run']):external_service(c,'resume',c['old_run'],suffix='rollback')
    plan.update(state='rolled_back',recovered=True);write(path,plan)


def historical_bundle(e, frozen, out):
    """Recreate the exact helper/metadata pair for an already-frozen engine."""
    source=Path(e['engine']); binding=frozen['historical_engines'][str(source)]
    helper=Path(binding['analyzer_bin'])
    assert digest(source)==binding['engine_sha256'] and digest(helper)==binding['analyzer_sha256']
    root=out/'historical';root.mkdir(exist_ok=True)
    dest=root/source.name
    for a,b in [(source,dest),(helper,dest.with_name(dest.name+'.analyze_position'))]:
        if b.exists():assert digest(a)==digest(b)
        else:shutil.copy2(a,b)
    dest.with_name(dest.name+'.meta').write_text('\n'.join([
        'rev='+binding['revision'],'engine_sha256='+binding['engine_sha256'],
        'analyzer_sha256='+binding['analyzer_sha256']])+'\n')
    e['engine']=str(dest)


def external_service(c, action, run, manifest=None, suffix='launch'):
    repo=Path(c['repo']); unit=c['unit']+'-'+suffix
    args=['systemd-run','--quiet','--unit='+unit,'--service-type=oneshot',
          '--property=RemainAfterExit=yes','--property=WorkingDirectory='+str(repo),
          '--property=KillMode=control-group','--property=CPUAffinity=0-3',
          '/usr/bin/python3',str(repo/'deploy/tourney_analysis.py'),action,'--run-dir',str(run),'--cpus','0,1,2,3']
    if manifest:args += ['--manifest',str(manifest),'--sidecar','analysis','--label-teacher','NNUE_W512_v2',
                         '--depth','8','--time-ms','3000']
    subprocess.run(args,check=True)
    meta=Path(run)/'analysis/supervisor.json'
    until=time.monotonic()+180
    while time.monotonic()<until:
        if meta.exists():
            status=read(meta)
            if status.get('state')=='running':
                try:
                    if pilot_vps.identity(status['pid'])==status['identity']:return
                except FileNotFoundError:pass
        unit_state=subprocess.run(['systemctl','show',unit,'-p','ActiveState','--value'],capture_output=True,text=True,check=True).stdout.strip()
        if unit_state=='failed':break
        time.sleep(1)
    subprocess.run(['systemctl','stop',unit],check=False)
    raise RuntimeError(f'{unit} did not launch; inspect journalctl -u {unit}')


def rollout(c, models):
    repo=Path(c['repo']); old=Path(c['old_run']);out=Path(c['out']);new=Path(c['new_run'])
    # Fully validate every candidate before touching the current tournament.
    validator=repo/'target/release/analyze_position'
    if len(models)!=len(WIDTHS):raise ValueError('Incomplete generation; will not replace tournament field')
    for model in models:
        subprocess.run([str(validator),'--validate-model',str(model)],check=True,capture_output=True)
    current=(repo/'data/run/royal-nnue-current-run.txt').read_text().strip()
    if (repo/current).resolve()!=old.resolve():raise ValueError('Live run changed; refusing stale automatic rollout')
    frozen=read(old/'analysis/config.json')
    if digest(frozen['command'][0])!=digest(repo/'target/release/taikyoku_shogi'):
        raise ValueError('Live and launch engines differ; automatic rollout needs review')
    state=read(old/'state.json'); ranking=standings(state); removed={r['id'] for r in ranking[:len(WIDTHS)]}
    if 'NNUE_W512_v2' in removed:raise ValueError('Retirement would remove the frozen analysis teacher')
    kept=[dict(e) for e in state['entrants'] if e['id'] not in removed]
    for e in kept:
        for field in ('model','engine'):
            if e.get(field):e[field]=str((repo/e[field]).resolve())
        if e.get('engine'):historical_bundle(e,frozen,out)
    entrants=kept+[dict(id=read(m)['name'],model=str(m)) for m in models]
    assert len(entrants)==32 and len({e['id'] for e in entrants})==32
    for e in entrants:
        assert Path(e['model']).is_file()
        if e.get('engine'):assert Path(e['engine']).is_file()
        helper=str(e['engine'])+'.analyze_position' if e.get('engine') else str(validator)
        subprocess.run([helper,'--validate-model',e['model']],check=True,capture_output=True)
    if shutil.disk_usage(out).free < sum((m.parent/read(m)['weights']['nnue']['file']).stat().st_size for m in models)+3*1024**3:
        raise RuntimeError('Insufficient disk for immutable tournament snapshots')
    manifest=out/'manifest.json';write(manifest,dict(entrants=entrants))
    plan=dict(state='prepared',removed=sorted(removed),ranking=ranking,new_run=str(new),
              old_run=str(old),state_sha256=digest(old/'state.json'),method='all completed games, equal weight, Bradley-Terry, ridge=0.1')
    write(out/'rollout.json',plan)
    subprocess.run(['/usr/bin/python3',str(repo/'deploy/tourney_analysis.py'),'stop','--run-dir',str(old)],cwd=repo,check=True,timeout=120)
    try:
        backup=out/'old-run-backup';backup.mkdir(exist_ok=True)
        for p in ('state.json','analysis/config.json','analysis/manifest.json','analysis/supervisor.json'):
            target=backup/p;target.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(old/p,target)
        # Freeze final retirement ranking after all terminal game results are saved.
        final=read(old/'state.json');rank=standings(final); removed={r['id'] for r in rank[:len(WIDTHS)]}
        if removed!=set(plan['removed']):
            raise RuntimeError('Retirement selection changed during stop; rollback rather than deploy stale selection')
        external_service(c,'start',new,manifest)
        (repo/'data/run/royal-nnue-current-run.txt').write_text(str(new.relative_to(repo))+'\n')
        plan.update(state='deployed',finished=time.time());write(out/'rollout.json',plan)
    except Exception as exc:
        plan.update(state='rolling_back',error=str(exc));write(out/'rollout.json',plan)
        subprocess.run(['systemctl','stop',c['unit']+'-launch'],check=False)
        external_service(c,'resume',old,suffix='rollback')
        plan.update(state='rolled_back');write(out/'rollout.json',plan)
        raise


def run(c):
    out=Path(c['out']);out.mkdir(parents=True,exist_ok=True);pilot_vps.ROOT=out
    preflight(c)
    status=dict(state='waiting_for_cpu',started=time.time(),models={});write(out/'status.json',status)
    write(out/'lease-state.json',dict(needs_restore=True))
    try:
        pilot_vps.pause_at_boundary(c)
        control=Path(c['control']);(control/'analysis.request').touch()
        with (control/'shared.lock').open('a+') as lease:
            until=time.monotonic()+7200
            while True:
                try:fcntl.flock(lease,fcntl.LOCK_EX|fcntl.LOCK_NB);break
                except BlockingIOError:
                    if time.monotonic()>until:raise TimeoutError('Game did not release shared CPU within two hours')
                    time.sleep(.2)
            models=[]
            for width in WIDTHS:
                name=f'NNUE_W{width}_v3';dest=out/name
                parent=Path(c['repo'])/f'models/nnue-v2/NNUE_W{width}_v2.json'
                init='parent' if parent.exists() else 'fresh'
                if not parent.exists():parent=Path(c['repo'])/'models/nnue-v2/NNUE_W512_v2.json'
                status.update(state='training',width=width);write(out/'status.json',status)
                saved_status=read(dest/'status.json').get('state') if (dest/'status.json').exists() else None
                if saved_status not in ('trained','completed'):
                    if shutil.disk_usage(out).free < 3*238464*width*4+1024**3:
                        raise RuntimeError(f'Insufficient disk headroom for width {width}')
                    args=[sys.executable,'training/nnue/pilot_fit.py',c['dataset'],'--parent',str(parent),
                          '--out',str(dest),'--init',init,'--loss','wdl','--width',str(width),'--epochs','12',
                          '--microbatch','1','--patience','2','--lr-reductions','1','--defer-test']
                    if (dest/'recipe.json').exists():args.append('--resume')
                    pilot_vps.command(args,out/(name+'.log'),12*3600)
                model=dest/'model.json'; metrics=read(dest/'metrics.json')
                best=min(m['validation']['objective'] for m in metrics if m['epoch']>0)
                if init=='parent' and best>=metrics[0]['validation']['objective']:
                    raise ValueError(f'{name} did not improve validation; field will not be replaced')
                if read(dest/'status.json')['state']!='completed':
                    pilot_vps.command([sys.executable,'training/nnue/generation_score.py',c['dataset'],str(model)],
                                      out/(name+'-quality.log'),2*3600)
                # Exact exported evaluator parity, legal moves, completed search on held-out games.
                status['state']='validating';write(out/'status.json',status)
                pilot_vps.command([sys.executable,'training/nnue/pilot_compare.py','verify',c['pilot_root'],
                                   '--model',str(model),'--cpu','3'],out/(name+'-checks.log'),1800)
                cp=read(model);d=cp['weights']['nnue']
                assert d['width']==width and digest(model.parent/d['file'])==d['sha256']
                models.append(model.resolve())
                status['models'][name]=dict(state='validated',model=str(model),sha256=digest(model),init=init,
                                           test=read(dest/'test.json'),best_validation=best)
                write(out/'status.json',status)
                # Optimizer state is needed only for an unfinished width. Keep all inference models.
                (dest/'training.pt').unlink(missing_ok=True)
            write(out/'ready.json',dict(models=status['models']))
        restore(c)
        status['state']='admitting';write(out/'status.json',status)
        rollout(c,models)
        status.update(state='completed',finished=time.time());write(out/'status.json',status)
    except Exception as exc:
        status.update(state='failed',error=str(exc),finished=time.time());write(out/'status.json',status)
        raise
    finally:restore(c)


def main():
    p=argparse.ArgumentParser();p.add_argument('action',choices=['run','restore']);p.add_argument('config',type=Path)
    a=p.parse_args();c=read(a.config)
    if a.action=='restore':recover(c)
    else:
        Path(c['out']).mkdir(parents=True,exist_ok=True)
        with (Path(c['out'])/'generation.lock').open('a+') as lock:
            fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
            run(c)

if __name__=='__main__':main()
