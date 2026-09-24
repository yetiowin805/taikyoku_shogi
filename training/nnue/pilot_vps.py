"""Bounded single-CPU pilot, with a recoverable pause of the existing analyzer.

Run under systemd with KillMode=control-group, RuntimeMaxSec and ExecStopPost
invoking this file's restore command. The tournament process is never signalled.
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

ROOT = Path('data/nnue-pilot-20260924')
PARENT = Path('data/nnue-admission-v2/NNUE_W512_v2.json')


def write(path, value):
    tmp = path.with_suffix('.tmp')
    tmp.write_text(json.dumps(value, indent=2)+'\n')
    tmp.replace(path)


def identity(pid):
    return Path(f'/proc/{pid}/stat').read_text().split(') ', 1)[1].split()[19]


def restore(config):
    try:
        if identity(config['analyzer_pid']) == config['analyzer_identity']:
            os.kill(config['analyzer_pid'], signal.SIGCONT)
            write(ROOT/'restoration.json', dict(state='analyzer_resumed', time=time.time()))
        else:
            raise RuntimeError('Analyzer PID changed; refusing to signal unrelated process')
    except Exception as exc:
        write(ROOT/'restoration.json', dict(state='failed', error=str(exc)))
        raise


def pause_at_boundary(config):
    pid = config['analyzer_pid']
    until = time.monotonic()+1200
    while time.monotonic() < until:
        if identity(pid) != config['analyzer_identity']:
            raise RuntimeError('Analyzer process changed')
        os.kill(pid, signal.SIGSTOP)
        # Wait until the stop has taken effect before inspecting children/locks.
        for _ in range(100):
            if Path(f'/proc/{pid}/stat').read_text().split(') ',1)[1].startswith('T'):
                break
            time.sleep(.01)
        else:
            raise RuntimeError('Analyzer did not stop')
        children = Path(f'/proc/{pid}/task/{pid}/children').read_text().strip()
        shared = (Path(config['control'])/'shared.lock').resolve()
        locked = any('lock:' in p.read_text()
                     for p in Path(f'/proc/{pid}/fdinfo').iterdir()
                     if Path(f'/proc/{pid}/fd/{p.name}').resolve() == shared)
        if not children and not locked:
            return
        os.kill(pid, signal.SIGCONT)
        time.sleep(.25)
    raise TimeoutError('No safe analyzer boundary within 20 minutes')


def command(args, log, timeout):
    with log.open('ab') as output:
        child = subprocess.Popen(args, stdout=output, stderr=subprocess.STDOUT, start_new_session=True)
        try:
            code = child.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            os.killpg(child.pid, signal.SIGTERM)
            try:
                child.wait(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(child.pid, signal.SIGKILL)
                child.wait()
            raise TimeoutError(f'Command budget expired: {args}')
        if code:
            raise RuntimeError(f'Command exited {code}: {args}; see {log}')


def run(config):
    control = Path(config['control'])
    report = dict(state='waiting_for_cpu', started=time.time(), arms={}, cpu=3,
                  epochs=6, training_limit_minutes=65, total_limit_hours=10.5)
    write(ROOT/'vps-status.json', report)
    try:
        pause_at_boundary(config)
        (control/'analysis.request').touch()
        with (control/'shared.lock').open('a+') as lease:
            until=time.monotonic()+1200
            while True:
                try:
                    fcntl.flock(lease, fcntl.LOCK_EX | fcntl.LOCK_NB)
                    break
                except BlockingIOError:
                    if time.monotonic()>until:
                        raise TimeoutError('Fourth game did not finish within 20 minutes')
                    time.sleep(.2)
            report.update(state='training', acquired=time.time())
            write(ROOT/'vps-status.json', report)
            arms=[('warm-huber','parent','huber',0),('fresh-huber','fresh','huber',0),
                  ('warm-wdl','parent','wdl',0),('warm-wdl10','parent','wdl',.1)]
            for name, init, loss, mix in arms:
                report['active']=name
                write(ROOT/'vps-status.json',report)
                try:
                    command([sys.executable,'training/nnue/pilot_fit.py',str(ROOT/'dataset'),
                             '--parent',str(PARENT),'--out',str(ROOT/name),'--init',init,
                             '--loss',loss,'--mix',str(mix),'--epochs','6'],ROOT/(name+'.log'),65*60)
                    report['arms'][name]={'training':'completed'}
                except Exception as exc:
                    report['arms'][name]={'training':'incomplete','error':str(exc)}
                write(ROOT/'vps-status.json',report)
            report['state']='comparing'
            write(ROOT/'vps-status.json',report)
            # The existing finisher can score best completed checkpoints from budget-limited arms.
            command([sys.executable,'training/nnue/pilot_finish.py',str(ROOT),
                     '--cpus','3','--available-models'],ROOT/'finish.log',5.5*3600)
            report.update(state='completed',finished=time.time())
            write(ROOT/'vps-status.json',report)
    except Exception as exc:
        report.update(state='failed',error=str(exc),finished=time.time())
        write(ROOT/'vps-status.json',report)
        raise
    finally:
        restore(config)


def main():
    p=argparse.ArgumentParser()
    p.add_argument('action',choices=['run','restore'])
    p.add_argument('config',type=Path)
    a=p.parse_args()
    config=json.loads(a.config.read_text())
    if a.action=='restore':restore(config)
    else:run(config)


if __name__=='__main__':main()
