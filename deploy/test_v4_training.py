import copy,json,tempfile,unittest,fcntl
from pathlib import Path
from v4_training import retirement,Pool
import tourney_analysis as a

class V4SupervisorTests(unittest.TestCase):
    def test_retirement_keeps_games_brackets_and_idempotent_identity(self):
        state=dict(entrants=[dict(id='a'),dict(id='b')],slots=[dict(model_a='a',model_b='b',score_a=1,status='done')],knockouts=[dict(seeds=['a','b'])])
        original=copy.deepcopy(state);new=dict(id='v4',model='/frozen/model.json',engine=None)
        self.assertEqual(retirement(state,dict(a=1550,b=1450),new),'b')
        self.assertEqual(state['slots'],original['slots']);self.assertEqual(state['knockouts'],original['knockouts'])
        self.assertEqual(state['retired'],['b']);self.assertEqual(len(state['entrants']),3)
        self.assertIsNone(retirement(state,dict(a=1550,b=1450),new))
        with self.assertRaises(ValueError):retirement(state,{},dict(new,model='different'))
    def test_pool_never_borrows_an_inflight_game_and_returns_leases(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);control=root/'analysis';control.mkdir();out=root/'out';out.mkdir()
            games={}
            for i in (1,3,4,5,6,7):
                games[i]=(control/f'cpu-{i}.lock').open('a+');fcntl.flock(games[i],fcntl.LOCK_EX)
            a.atomic(control/'analysis-demand.json',dict(assigned=[1]))
            pool=Pool(control,out,list(range(8)));pool.tick();self.assertFalse(pool.leases)
            games[1].close();pool.tick();self.assertEqual(set(pool.leases),{1})
            self.assertEqual(a.read(out/'cpus.json')['cpus'],[1])
            pool.close()
            for f in games.values():f.close()
            self.assertEqual(a.read(control/'analysis-demand.json')['pending'],0)
            with (control/'cpu-1.lock').open('a+') as f:fcntl.flock(f,fcntl.LOCK_EX|fcntl.LOCK_NB)

if __name__=='__main__':unittest.main()
