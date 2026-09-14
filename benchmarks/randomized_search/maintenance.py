#!/usr/bin/env python3
"""Bounded tournament pause, benchmark, and restoration of the frozen configuration."""
import argparse, fcntl, hashlib, importlib.util, json, os, shutil, signal, subprocess, sys, time
from pathlib import Path

HERE=Path(__file__).resolve().parent

def load_launcher(repo):
    spec=importlib.util.spec_from_file_location('original_launcher',repo/'deploy/tourney_analysis.py')
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module

def restore(repo,run,config,old_pid=None):
    launcher=load_launcher(repo);control=run/'analysis';info=launcher.read(control/'supervisor.json')
    for _ in range(120):
        if not launcher.alive(info) or info['pid']!=old_pid:break
        time.sleep(1);info=launcher.read(control/'supervisor.json')
    if launcher.alive(info) and info['pid']==old_pid:raise RuntimeError('original supervisor has not stopped; restoration uncertain')
    if launcher.alive(info):
        if info['state']!='running':raise RuntimeError('supervisor alive but not healthy; inspect status')
        return info
    if launcher.read(control/'config.json')!=config:raise RuntimeError('configuration changed; refusing a different launch')
    for model in config['models'].values():
        if hashlib.sha256(Path(model['snapshot']).read_bytes()).hexdigest()!=model['sha256']:
            raise RuntimeError('original model snapshot changed; refusing restoration')
    if hashlib.sha256(Path(config['analyzer_bin']).read_bytes()).hexdigest()!=config['analyzer_sha256']:
        raise RuntimeError('original analyzer binary changed; refusing restoration')
    with (control/'supervisor.log').open('ab') as log:
        proc=subprocess.Popen([sys.executable,str(repo/'deploy/tourney_analysis.py'),'_supervise','--run-dir',str(run)],
            cwd=repo,stdout=log,stderr=log,start_new_session=True)
    for _ in range(100):
        time.sleep(.2);info=launcher.read(control/'supervisor.json')
        if info['pid']==proc.pid and info['state']=='running' and launcher.alive(info):return info
        if proc.poll() is not None:raise RuntimeError('restoration failed; see supervisor.log')
    raise RuntimeError('restoration readiness timed out')

def stop_group(proc):
    if proc is not None and proc.poll() is None:
        os.killpg(proc.pid,signal.SIGTERM)
        try:proc.wait(timeout=20)
        except subprocess.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait()

def main():
    p=argparse.ArgumentParser(description=__doc__)
    for name in ['repo','run','corpus','binary','out']:p.add_argument('--'+name,type=Path,required=True)
    p.add_argument('--seconds',type=int,default=10800);p.add_argument('--cpus',default='0,1,2,3');a=p.parse_args()
    for name in ['repo','run','corpus','binary','out']:setattr(a,name,getattr(a,name).resolve())
    if a.out.exists():raise ValueError('maintenance output already exists')
    if not os.access(a.binary,os.X_OK):raise ValueError('benchmark binary is not executable')
    manifest=json.loads((a.corpus.parent/'manifest.json').read_text())
    for path,expected in manifest['inputs'].items():
        if hashlib.sha256((a.corpus.parent/path).read_bytes()).hexdigest()!=expected:raise ValueError('corpus identity changed')
    cpus=list(map(int,a.cpus.split(',')))
    if len(cpus)!=4 or len(set(cpus))!=4 or not set(cpus)<=os.sched_getaffinity(0):raise ValueError('need four allowed distinct CPUs')
    # Exercise one original checkpoint from every agent before stopping services.
    cases=json.loads(a.corpus.read_text());first={}
    for i,c in enumerate(cases):first.setdefault(c['agent'],i)
    for i in first.values():
        subprocess.run([str(a.binary),str(a.corpus),str(i),'0','3000','S0-W0-T0','1'],
            stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,check=True,timeout=30)
    launcher=load_launcher(a.repo);control=a.run/'analysis';config=launcher.read(control/'config.json')
    original=launcher.read(control/'supervisor.json')
    if not launcher.alive(original):raise ValueError('expected a live tournament supervisor')
    lock=(a.repo/'data/run/random-search-maintenance.lock').open('a+')
    fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
    a.out.mkdir(parents=True);backup=a.out/'backup';backup.mkdir()
    (backup/'config.json').write_text(json.dumps(config,indent=2)+'\n')
    runner=None;requested_stop=False;status=dict(state='preparing',started_utc=time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),seconds=a.seconds)
    def record():
        tmp=a.out/'maintenance.tmp';tmp.write_text(json.dumps(status,indent=2)+'\n');tmp.replace(a.out/'maintenance.json')
    def interrupted(*_):raise InterruptedError('maintenance interrupted')
    signal.signal(signal.SIGTERM,interrupted);signal.signal(signal.SIGINT,interrupted)
    try:
        record();requested_stop=True
        subprocess.run([sys.executable,str(a.repo/'deploy/tourney_analysis.py'),'stop','--run-dir',str(a.run)],cwd=a.repo,check=True,timeout=120)
        for filename in ['state.json','ratings.json']:
            if (a.run/filename).exists():shutil.copy2(a.run/filename,backup/filename)
        for filename in ['catalogue.sqlite','catalogue.sqlite-wal','catalogue.sqlite-shm','manifest.json']:
            if (control/filename).exists():shutil.copy2(control/filename,backup/filename)
        for dirname in ['models','bin']:shutil.copytree(control/dirname,backup/dirname)
        status.update(state='benchmarking',benchmark_started_utc=time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()));record()
        with (a.out/'progress.log').open('ab') as log:
            runner=subprocess.Popen([sys.executable,str(HERE/'run.py'),'--corpus',str(a.corpus),'--binary',str(a.binary),
                '--out',str(a.out/'results'),'--seconds',str(a.seconds),'--cpus',a.cpus],stdout=log,stderr=log,start_new_session=True)
            status['benchmark_pid']=runner.pid;record()
            code=runner.wait(timeout=a.seconds+60)
            status['benchmark_exit']=code
            if code:raise RuntimeError(f'benchmark exited {code}; see progress.log')
    except BaseException as exc:
        status['error']=str(exc);raise
    finally:
        # Ignore a second TERM while restoring; outer watchdog still supplies a hard bound.
        signal.signal(signal.SIGTERM,signal.SIG_IGN);signal.signal(signal.SIGINT,signal.SIG_IGN)
        stop_group(runner)
        if requested_stop:
            status['state']='restoring';record()
            try:
                status['supervisor']=restore(a.repo,a.run,config,original['pid'])
                before=json.loads((backup/'state.json').read_text()) if (backup/'state.json').exists() else None
                after=launcher.read(a.run/'state.json')
                if before:
                    done=lambda s:{x['id'] for x in s['slots'] if x['status']=='done'}
                    if not done(before)<=done(after):raise RuntimeError('completed slots missing after restore')
                    status['completed_slots_preserved']=len(done(before))
                status['state']='restored';status['restored_utc']=time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime())
            except BaseException as exc:
                status.update(state='restore_failed',restore_error=str(exc));record();raise
        record()
    if (a.out/'results/finished.json').exists():
        subprocess.run([sys.executable,str(HERE/'summarize.py'),str(a.out/'results')],check=True)
    print(json.dumps(status,indent=2))

if __name__=='__main__':
    try:main()
    except BaseException as exc:
        print(f'benchmark maintenance failed: {exc}; inspect maintenance.json and supervisor.log',file=sys.stderr);sys.exit(1)
