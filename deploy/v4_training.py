"""Supervised v4 pipeline; borrow CPUs at game boundaries, restart once for admission.

Works with the existing eight-CPU coordinator. Training does not interrupt games.
Admission uses the normal restart: unfinished games requeue, finished results persist.
Run under systemd with KillMode=control-group; output is outside the live run.
"""
import argparse,fcntl,json,os,signal,subprocess,sys,time
from pathlib import Path
import shutil

import tourney_analysis as a
from adaptive_analysis import allocation

FLEX=(1,3,4,5,6,7)


def retirement(state,ratings,candidate):
    """Pure state transition: keep all records, old brackets and retired references."""
    if candidate['id'] in {e['id'] for e in state['entrants']}:
        existing=next(e for e in state['entrants'] if e['id']==candidate['id'])
        if any(existing.get(k)!=candidate.get(k) for k in ('id','model','engine')):
            raise ValueError('Candidate identity is already bound to another model')
        return None
    active={e['id'] for e in state['entrants']} - set(state.get('retired',[]))
    if not active or not all(n in ratings for n in active):raise ValueError('Incomplete retirement ranking')
    lowest=min(active,key=lambda n:(ratings[n],n))
    state['retired']=sorted(set(state.get('retired',[]))|{lowest})
    state['entrants'].append(candidate);state['order_neutral_only']=True
    return lowest


class Pool:
    def __init__(self,control,out,cpus):
        self.control,self.out,self.cpus=control,out,cpus
        self.leases={};self.want=1
    def tick(self):
        with allocation(self.control) as d:
            assigned=d.get('assigned',[])
            d.update(pid=os.getpid(),updated=time.time(),pending=self.want)
            # Reserve an already-idle flexible CPU under the same admission lock.
            for i in dict.fromkeys(assigned+list(FLEX)):
                if i not in FLEX:raise ValueError("Invalid flexible CPU reservation")
                if len(self.leases) >= self.want:break
                if i not in self.leases and self.claim(i):assigned.append(i)
            held=[i for i in self.leases if i in FLEX]
            d['assigned']=held+[i for i in assigned if i in FLEX and i not in held][:max(0,self.want-len(held))]
            for i in d['assigned']:
                if i not in self.leases:self.claim(i)
        a.atomic(self.out/'cpus.json',dict(cpus=[self.cpus[i] for i in sorted(self.leases) if i in FLEX],updated=time.time()))
    def claim(self,i):
        f=(self.control/f'cpu-{i}.lock').open('a+')
        try:fcntl.flock(f,fcntl.LOCK_EX|fcntl.LOCK_NB)
        except BlockingIOError:f.close();return False
        self.leases[i]=f;return True
    def close(self):
        with allocation(self.control) as d:d.update(pid=0,pending=0,assigned=[],updated=time.time())
        for f in self.leases.values():f.close()
        self.leases.clear()


def service(action):
    subprocess.run(['systemctl',action,'taikyoku-dual-labels.service'],check=True)


def recover(config):
    c=a.read(config);repo=Path(c['repo']);run=Path(c['run']);out=Path(c['out'])
    plan=a.read(out/'rollout.json') if (out/'rollout.json').exists() else {}
    if plan.get('state') in ('stopping','prepared'):
        meta=a.read(run/'analysis/supervisor.json')
        if not a.alive(meta):
            if plan['state']=='prepared':
                backup=Path(plan['backup'])
                for name in ['state.json','analysis/config.json','analysis/manifest.json']:
                    shutil.copy2(backup/name,run/name)
            subprocess.run([sys.executable,str(repo/'deploy/tourney_analysis.py'),'resume','--run-dir',str(run),
                            '--cpus',','.join(map(str,c['cpus'])),'--top-two-worker','--sidecar','none'],check=True,timeout=300)
            a.atomic(out/'rollout.json',dict(plan,state='rolled_back'))
    service('start')


def main():
    p=argparse.ArgumentParser();p.add_argument('config',type=Path);p.add_argument('--recover',action='store_true');args=p.parse_args()
    if args.recover:recover(args.config);return
    c=a.read(args.config);repo=Path(c['repo']);run=Path(c['run']);out=Path(c['out']);control=run/'analysis'
    out.mkdir(parents=True,exist_ok=True)
    a.ROOT=repo
    def stopped(*_):raise InterruptedError('v4 supervisor interrupted; progress is resumable')
    signal.signal(signal.SIGTERM,stopped);signal.signal(signal.SIGINT,stopped)
    lock=(out/'supervisor.lock').open('a+');fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
    for path,sha in c['code_hashes'].items():
        if a.file_digest(path)!=sha:raise ValueError('Pinned code changed: '+path)
    for path,sha in c['input_hashes'].items():
        if a.file_digest(path)!=sha:raise ValueError('Pinned input changed: '+path)
    if len(c['cpus'])!=8 or len(set(c['cpus']))!=8 or not set(c['cpus'])<=os.sched_getaffinity(0):raise ValueError('Eight distinct available CPUs required')
    for protected in [run,Path(c['dataset']),Path(c['parent']).parent]:
        if out.resolve().is_relative_to(protected.resolve()) or protected.resolve().is_relative_to(out.resolve()):
            raise ValueError('Output overlaps protected inputs')
    if shutil.disk_usage(out).free < 2200*1024**2:raise ValueError('Need at least 2.2 GiB free before training')
    proc=None;pool=Pool(control,out,c['cpus']);own=None;coordinator_stopped=False
    def status(phase,**fields):a.atomic(out/'status.json',dict(state=phase,updated=time.time(),cpus=list(pool.leases),**fields))
    def run_job(command,phase):
        nonlocal proc
        for path,sha in c['code_hashes'].items():
            if a.file_digest(path)!=sha:raise ValueError('Pinned code changed: '+path)
        while not pool.leases:pool.tick();status('waiting_for_cpu',next=phase);time.sleep(1)
        pool.tick();cpus={c['cpus'][i] for i in pool.leases if i in FLEX}
        status(phase,command=command)
        with (out/(phase+'.log')).open('ab') as log:
            def setup():os.sched_setaffinity(0,cpus);a.parent_death_guard(os.getppid())()
            proc=subprocess.Popen(command,cwd=repo,stdout=log,stderr=log,start_new_session=True,preexec_fn=setup,
                env=dict(os.environ,OMP_NUM_THREADS='1',MKL_NUM_THREADS='1',OPENBLAS_NUM_THREADS='1'),
                pass_fds=tuple(f.fileno() for f in pool.leases.values()))
            started=time.monotonic()
            while proc.poll() is None:
                pool.tick();status(phase,pid=proc.pid,elapsed_s=time.monotonic()-started)
                if time.monotonic()-started>c.get('stage_limit_hours',6)*3600:raise TimeoutError(phase+' exceeded stage budget; checkpoint retained')
                time.sleep(1)
            code=proc.returncode;proc=None
            if code:raise RuntimeError(f'{phase} failed ({code}); see {phase}.log')
    try:
        service('stop') # analysis results persist; running games are untouched
        own=(control/'dual-label.lock').open('a+');fcntl.flock(own,fcntl.LOCK_EX|fcntl.LOCK_NB)
        with allocation(control) as d:d.update(pid=os.getpid(),pending=1,assigned=[],updated=time.time())
        dataset=out/'dataset';candidate=out/'NNUE_W512_v4/model.json'
        train=repo/'training/nnue'
        run_job([c['python'],str(train/'v4_data.py'),'--out',str(dataset),'--old',c['dataset'],
                 '--games',str(repo/'data/raw/tourney'),'--repo',str(repo),'--binary',c['nnue_tool'],'--base',c['base'],
                 '--runs',*c['runs'],'--catalogue',*c['catalogues']],'prepare')
        pool.want=2
        cmd=[c['python'],str(train/'pilot_fit.py'),str(dataset),'--parent',c['parent'],'--out',str(candidate.parent),
             '--init','parent','--width','512','--loss','wdl','--mix','0','--microbatch','1','--batch','8',
             '--threads','2','--cpu-file',str(out/'cpus.json'),'--epochs','12','--patience','2','--lr-reductions','1',
             '--defer-test','--seed','20261003']
        if (candidate.parent/'training.pt').exists():cmd.append('--resume')
        if not (out/'validation.json').exists():
            run_job(cmd,'training')
            run_job([c['python'],str(train/'v4_validate.py'),'--dataset',str(dataset),'--parent',c['parent'],
                     '--candidate',str(candidate),'--binary',c['nnue_tool'],'--out',str(out/'validation.json')],'validation')
        ready=a.read(out/'validation.json')
        if not ready.get('ready') or ready['candidate_sha256']!=a.file_digest(candidate):raise ValueError('Candidate readiness/hash mismatch')
        subprocess.run([str(repo/'target/release/taikyoku_shogi'),'tournament-game-validate',str(candidate)],check=True)
        status('admitting')
        for path,sha in c['code_hashes'].items():
            if a.file_digest(path)!=sha:raise ValueError('Pinned code changed before admission: '+path)
        backup=out/('pre-admission-'+str(time.time_ns()))
        backup.mkdir()
        a.atomic(out/'rollout.json',dict(state='stopping',backup=str(backup)))
        coordinator_stopped=True
        subprocess.run([sys.executable,str(repo/'deploy/tourney_analysis.py'),'stop','--run-dir',str(run)],check=True,timeout=120)
        # Take the authoritative backup AFTER the stop checkpoint, so a game that
        # finishes during shutdown is preserved on either deployment or rollback.
        for name in ['state.json','ratings.json','elo.json','order-neutral-ratings.json','analysis/config.json','analysis/manifest.json']:
            src=run/name;dest=backup/name;dest.parent.mkdir(exist_ok=True,parents=True)
            shutil.copy2(src,dest)
        a.atomic(out/'rollout.json',dict(state='prepared',backup=str(backup)))
        # State may contain final checkpoint housekeeping; no result can arrive now.
        state=a.read(run/'state.json')
        fitted=json.loads(subprocess.check_output([c['ratings_bin'],str(run/'state.json')],text=True))
        if not fitted['ratings']:raise ValueError('Rating fit failed before admission')
        retired=retirement(state,fitted['ratings'],dict(id='NNUE_W512_v4',model=str(candidate),engine=None))
        a.atomic(run/'state.json',state)
        # Retain the exact original game executable selection. This rollout only
        # changes the coordinator, training, and entrant/rating state.
        subprocess.run([sys.executable,str(repo/'deploy/tourney_analysis.py'),'resume','--run-dir',str(run),
                        '--cpus',','.join(map(str,c['cpus'])),'--top-two-worker','--sidecar','none'],check=True,timeout=300)
        coordinator_stopped=False
        a.atomic(out/'rollout.json',dict(state='deployed',backup=str(backup),retired=retired))
        cfg=a.read(control/'dual-label-config.json')
        cfg['auto_champion']['ratings_bin']=c['ratings_bin'];cfg['auto_champion']['ratings_sha256']=a.file_digest(c['ratings_bin'])
        a.atomic(control/'dual-label-config.json',cfg)
        status('completed',retired=retired,admitted='NNUE_W512_v4',validation=str(out/'validation.json'))
    except BaseException as exc:
        if proc and proc.poll() is None:
            os.killpg(proc.pid,signal.SIGTERM)
            try:proc.wait(timeout=15)
            except subprocess.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait()
        if coordinator_stopped:
            plan=a.read(out/'rollout.json')
            backup=Path(plan['backup'])
            if plan['state']=='prepared':
                for name in ['state.json','analysis/config.json','analysis/manifest.json']:
                    shutil.copy2(backup/name,run/name)
            subprocess.run([sys.executable,str(repo/'deploy/tourney_analysis.py'),'resume','--run-dir',str(run),
                            '--cpus',','.join(map(str,c['cpus'])),'--top-two-worker','--sidecar','none'],check=True,timeout=300)
        status('failed',error=str(exc));raise
    finally:
        pool.close()
        if own:own.close()
        service('start')

if __name__=='__main__':main()
