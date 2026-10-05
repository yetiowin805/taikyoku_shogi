"""Run two isolated v4.5 continuations; lease CPUs without stopping games.

Pauses the analysis service, restores it on exit, never admits or replaces models.
The systemd unit must also restart analysis in ExecStopPost for hard-kill recovery.
"""
import argparse
import fcntl
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

import tourney_analysis as a
from v4_training import Pool


def terminate(proc):
    if proc and proc.poll() is None:
        os.killpg(proc.pid, signal.SIGTERM)
        proc.wait()


def main():
    p=argparse.ArgumentParser(); p.add_argument('config',type=Path); args=p.parse_args()
    c=a.read(args.config); out=Path(c['out']); run=Path(c['run']); code=Path(c['code'])
    out.mkdir(parents=True,exist_ok=True)
    lock=(out/'supervisor.lock').open('a+'); fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
    def interrupted(*_): raise InterruptedError('Training stopped; epoch checkpoints are resumable')
    signal.signal(signal.SIGTERM,interrupted); signal.signal(signal.SIGINT,interrupted)
    def verify():
        for path,sha in {**c['code_hashes'],**c['input_hashes']}.items():
            if a.file_digest(path)!=sha: raise ValueError('Frozen input/code changed: '+path)
    verify()
    if not set(c['cpus']) <= os.sched_getaffinity(0): raise ValueError('Unavailable CPUs')
    pool=Pool(run/'analysis',out,c['cpus']); pool.want=2
    proc=None; own=None; paused=False; claimed=False
    def status(state,**kwargs): a.atomic(out/'status.json',dict(state=state,updated=time.time(),**kwargs))
    def job(command,name):
        nonlocal proc
        verify()
        while not pool.leases:
            pool.tick(); status('waiting_for_cpu',stage=name,leased=list(pool.leases));time.sleep(1)
        cpus={c['cpus'][i] for i in pool.leases}
        def setup():
            os.sched_setaffinity(0,cpus); a.parent_death_guard(os.getppid())()
        with (out/(name+'.log')).open('ab') as log:
            proc=subprocess.Popen(command,stdout=log,stderr=log,start_new_session=True,preexec_fn=setup,
                pass_fds=tuple(f.fileno() for f in pool.leases.values()),
                env=dict(os.environ,OMP_NUM_THREADS='2',MKL_NUM_THREADS='2',OPENBLAS_NUM_THREADS='1'))
            started=time.monotonic()
            while proc.poll() is None:
                pool.tick();status('running',stage=name,pid=proc.pid,cpus=[c['cpus'][i] for i in sorted(pool.leases)],elapsed_s=time.monotonic()-started)
                time.sleep(2)
            result=proc.returncode;proc=None
            if result:raise RuntimeError(name+' failed: '+str(result))
    try:
        status('pausing_analysis')
        paused=True
        subprocess.run(['systemctl','stop','taikyoku-dual-labels.service'],check=True)
        own=(run/'analysis/dual-label.lock').open('a+');fcntl.flock(own,fcntl.LOCK_EX|fcntl.LOCK_NB)
        claimed=True
        dataset=out/'dataset'
        job([c['python'],str(code/'training/nnue/v45_data.py'),'--source',c['dataset'],
             '--parent',c['parents']['512'],'--out',str(dataset),'--cpu-file',str(out/'cpus.json')],'prepare')
        for width in (512,384):
            model=out/f'NNUE_W{width}_v4.5'
            if (model/'status.json').exists() and a.read(model/'status.json').get('state')=='completed':continue
            command=[c['python'],str(code/'training/nnue/pilot_fit.py'),str(dataset),
                '--parent',c['parents'][str(width)],'--out',str(model),'--init','parent','--width',str(width),
                '--loss','wdl','--mix','0','--epochs','0','--epoch-samples','65536','--patience','2',
                '--lr-reductions','1','--batch','8','--microbatch','1','--threads','2',
                '--cpu-file',str(out/'cpus.json'),'--seed','20261004']
            if (model/'recipe.json').exists():command.append('--resume')
            job(command,'train-'+str(width))
        status('completed',models=[str(out/f'NNUE_W{w}_v4.5/model.json') for w in (512,384)])
    except BaseException as e:
        status('failed',error=str(e));raise
    finally:
        terminate(proc)
        if claimed:pool.close()
        if own:own.close()
        if paused:subprocess.run(['systemctl','start','taikyoku-dual-labels.service'],check=True)

if __name__=='__main__':main()
