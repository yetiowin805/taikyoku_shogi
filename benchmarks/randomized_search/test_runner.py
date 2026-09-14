import argparse,importlib.util,json,gzip,tempfile,unittest
from pathlib import Path
from unittest.mock import patch

HERE=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('runner',HERE/'run.py');runner=importlib.util.module_from_spec(spec);spec.loader.exec_module(runner)

class RandomizedTests(unittest.TestCase):
    def test_plan_balances_agents_and_cpus_and_never_compares_identical_configurations(self):
        cases=[dict(agent=f'agent{i}',ply=j) for i in range(8) for j in range(3)]
        jobs=runner.plan(cases,rounds=16)
        self.assertEqual(jobs,runner.plan(cases,rounds=16))
        for r in range(16):
            block=jobs[8*r:8*r+8]
            self.assertEqual(len({j['agent'] for j in block}),8)
            self.assertEqual(sum(j['control'] for j in block),4)
            self.assertEqual(sorted(j['cpu'] for j in block),[0,0,1,1,2,2,3,3])
            for job in block:
                self.assertEqual(cases[job['case']]['agent'],job['agent'])
                self.assertEqual(len(set(job['variants'])),2)
                if job['control']:self.assertIn(runner.BASELINE,job['variants'])

    def test_process_failures_are_recorded_and_stop_repeated_failures(self):
        with tempfile.TemporaryDirectory() as tmp:
            p=Path(tmp);binary=p/'fake';binary.write_text('fake')
            corpus=p/'corpus.json';corpus.write_text(json.dumps([dict(agent='a')]))
            jobs=[dict(pair=i,round=i,agent='a',case=0,cpu=0,variants=[runner.BASELINE,'S1-W0-T0'],control=True) for i in range(8)]
            args=argparse.Namespace(cpus='0',corpus=corpus,binary=binary,out=p/'out',seconds=100,budget_ms=3000,process_timeout=1,seed=1)
            def failed(*a,**kw):raise runner.subprocess.TimeoutExpired('fake',1)
            with patch.object(runner,'plan',return_value=jobs),patch.object(runner.os,'sched_getaffinity',return_value={0}),patch.object(runner.subprocess,'check_output',return_value='test'),patch.object(runner.subprocess,'run',side_effect=failed):
                with self.assertRaisesRegex(RuntimeError,'repeated search failures'):runner.run(args)
            rows=[json.loads(s) for s in gzip.open(p/'out/raw-cpu0.jsonl.gz','rt')]
            self.assertEqual(len(rows),6)
            self.assertTrue(all('error' in r for r in rows))
            self.assertEqual(json.loads((p/'out/finished.json').read_text())['state'],'failed')


class MaintenanceRecoveryTests(unittest.TestCase):
    def test_failed_benchmark_restores_original_supervisor_and_completed_slots(self):
        import hashlib,os,signal,subprocess,sys,time
        with tempfile.TemporaryDirectory() as tmp:
            repo=Path(tmp);(repo/'deploy').mkdir();(repo/'data/run').mkdir(parents=True)
            run=repo/'tourney';control=run/'analysis';control.mkdir(parents=True)
            for name in ['models','bin']:(control/name).mkdir()
            (run/'state.json').write_text(json.dumps(dict(slots=[dict(id=1,status='done')])))
            launcher=repo/'deploy/tourney_analysis.py'
            launcher.write_text('''import json,os,signal,sys,time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
def read(p):return json.loads(Path(p).read_text())
def alive(info):
 try:return Path('/proc/'+str(info['pid'])+'/stat').read_text().split(') ')[1][0]!='Z'
 except FileNotFoundError:return False
if __name__=='__main__':
 run=Path(sys.argv[sys.argv.index('--run-dir')+1]);p=run/'analysis/supervisor.json'
 if sys.argv[1]=='stop':
  info=read(p);os.kill(info['pid'],signal.SIGTERM)
  while alive(info):time.sleep(.01)
 else:
  p.write_text(json.dumps(dict(pid=os.getpid(),state='running')))
  while True:time.sleep(1)
''')
            binary=repo/'fake-engine';binary.write_text('#!/usr/bin/env python3\nimport sys\nsys.exit(0 if sys.argv[3]=="0" else 1)\n');binary.chmod(0o755)
            config=dict(models={},analyzer_bin=str(binary),analyzer_sha256=hashlib.sha256(binary.read_bytes()).hexdigest())
            (control/'config.json').write_text(json.dumps(config))
            corpus=repo/'corpus';corpus.mkdir();(corpus/'manifest.json').write_text(json.dumps(dict(inputs={})))
            (corpus/'corpus.json').write_text(json.dumps([dict(agent=f'a{i}') for i in range(8)]))
            original=subprocess.Popen([sys.executable,str(launcher),'_supervise','--run-dir',str(run)])
            restored=None
            try:
                for _ in range(100):
                    if (control/'supervisor.json').exists():break
                    time.sleep(.02)
                proc=subprocess.run([sys.executable,str(HERE/'maintenance.py'),'--repo',str(repo),'--run',str(run),
                    '--corpus',str(corpus/'corpus.json'),'--binary',str(binary),'--out',str(repo/'out'),
                    '--seconds','35','--cpus',','.join(map(str,sorted(os.sched_getaffinity(0))[:4]))],capture_output=True,text=True,timeout=30)
                restored=json.loads((control/'supervisor.json').read_text())['pid']
                status=json.loads((repo/'out/maintenance.json').read_text())
                self.assertNotEqual(proc.returncode,0)
                self.assertEqual(status['state'],'restored',proc.stderr)
                self.assertEqual(status['completed_slots_preserved'],1)
                self.assertNotEqual(restored,original.pid)
                os.kill(restored,0)
            finally:
                if restored and restored!=original.pid:
                    try:os.kill(restored,signal.SIGTERM)
                    except ProcessLookupError:pass
                if original.poll() is None:original.terminate()
                original.wait()

if __name__=='__main__':unittest.main()
