import argparse
from collections import Counter, defaultdict
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import numpy as np
from pilot_data import digest
from recent_data import (assign_groups, analysis_candidates, build, classify_positions,
                         label_position, PROPORTIONS, rows, teacher_labels, verify, Weights)
from v4_data import probability


class RecentDataTests(unittest.TestCase):
    def test_mate_sign_and_disagreeing_teachers_are_preserved(self):
        s = label_position(dict(color='White', eval=-1000000, completed_depth=3), [])
        self.assertTrue(s['mate'])
        self.assertEqual(s['target_probability'], 1)
        self.assertGreater(s['score'], 0)
        self.assertEqual(s['saved_score'], -1000000)
        teachers = [dict(score=1000000, completed_depth=2), dict(score=0, completed_depth=4)]
        s = label_position(dict(color='White', eval=2000), teachers)
        self.assertEqual(s['target_probability'], .25)
        self.assertAlmostEqual(probability(s['score']), .25)
        self.assertEqual(s['teacher_probability_spread'], .5)
        self.assertEqual(s['teacher_scores'], teachers)

    def test_full_coverage_precursors_keep_their_own_scores_and_queue_both_sides(self):
        moves = [dict(color='Black' if i % 2 == 0 else 'White', eval=100) for i in range(300)]
        for m in moves[280:]:
            m['eval'] = 1000000
        game = dict(moves=moves, result='BlackWins')
        selected, episodes = classify_positions(game, 'g', {}, Counter())
        self.assertEqual(len(selected), 300)
        self.assertEqual(len(episodes), 1)
        self.assertEqual(selected[151]['stratum'], 'representative')
        self.assertEqual(selected[152]['stratum'], 'precursor')
        self.assertAlmostEqual(selected[152]['score'], 100)
        self.assertEqual(selected[280]['stratum'], 'mate')
        queue = analysis_candidates(game, 'g', selected, episodes)
        mate_plies = {c['ply'] for c in queue if 'mate_window' in c['reasons']}
        self.assertTrue({153, 152, 281, 280} <= mate_plies)
        self.assertTrue(all(1 <= c['ply'] <= len(moves) for c in queue))
        self.assertEqual(queue, analysis_candidates(game, 'g', selected, episodes))

    def test_equal_group_game_and_episode_influence(self):
        samples = []
        for st in PROPORTIONS:
            for gr, games in [('a', ['one', 'two']), ('b', ['three'])]:
                for ga in games:
                    for ep, n in [('long', 40), ('short', 1)]:
                        samples.extend(dict(split='train', stratum=st, group=gr, game_sha256=ga,
                                            episode=ep) for _ in range(n))
        w = Weights(PROPORTIONS)
        for s in samples:
            w.add(s)
        totals = Counter()
        for s in samples:
            weight = w.weight(s)
            totals[s['stratum']] += weight
            totals[s['stratum'], s['group']] += weight
            totals[s['stratum'], s['game_sha256'], s['episode']] += weight
        for st, fraction in PROPORTIONS.items():
            self.assertAlmostEqual(totals[st], fraction)
            self.assertAlmostEqual(totals[st, 'a'], totals[st, 'b'])
            self.assertAlmostEqual(totals[st, 'one', 'long'], totals[st, 'one', 'short'])
        lone = dict(split='test', stratum='representative', group='z', game_sha256='z', episode='z')
        w.add(lone)
        self.assertAlmostEqual(w.weight(lone), 1)

    def test_pair_groups_inherit_splits_and_conflicts_are_quarantined(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d)/'parent.jsonl'
            parent = [dict(game_sha256='a', group='old-a', split='train', canonical_position_hash='seen'),
                      dict(game_sha256='b', group='old-b', split='test', canonical_position_hash='test')]
            p.write_text(''.join(json.dumps(s)+'\n' for s in parent))
            records = {h: dict(prefix=h, run='r', pair_seed=seed) for h, seed in [('a', 1), ('b', 2), ('c', 1)]}
            self.assertEqual(assign_groups(records, p), {'seen'})
            self.assertEqual(records['c']['split'], 'train')
            self.assertEqual(records['a']['group'], records['c']['group'])
            records['b']['pair_seed'] = 1
            assign_groups(records, p)
            self.assertTrue(all(r['quarantine'] for r in records.values()))

    def test_labels_use_completed_iterations_and_validate_perspective(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            path = root/'labels.jsonl'
            searches = [dict(model_sha256='x', ply=1, score=10, completed_depth=2),
                        dict(model_sha256='x', ply=1, score=20, completed_depth=3),
                        dict(model_sha256='x', ply=1, score=1000000, completed_depth=0)]
            path.write_text(json.dumps(dict(game_hash='g', searches=searches))+'\n')
            diag = Counter()
            labels = teacher_labels(root, dict(labels=['labels.jsonl']), diag)
            self.assertEqual(labels['g', 0][0]['score'], 20)
            self.assertEqual(diag['invalid_analysis_record'], 1)
            searches[0]['score_perspective'] = 'side-to-move'
            path.write_text(json.dumps(dict(game_hash='g', searches=searches))+'\n')
            with self.assertRaisesRegex(ValueError, 'perspective'):
                teacher_labels(root, dict(labels=['labels.jsonl']), Counter())

    def test_resumable_builder_integrity_outcomes_and_shard_offsets(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            snapshot = root/'snapshot'
            snapshot.mkdir()
            entries, parent = [], []
            for n, split in enumerate(('train', 'validation', 'test')):
                game = dict(start={'seed': n}, moves=[dict(color='Black', eval=100), dict(color='White', eval=1000000)],
                            result='Draw' if n == 1 else 'BlackWins', black={'name': 'ab'}, white={'name': 'ab'})
                p = snapshot/f'g{n}.json'
                p.write_text(json.dumps(game))
                h = digest(p)
                entries.append(dict(path=p.name, sha256=h, run='recent', pair_seed=n))
                parent.append(dict(game_sha256=h, split=split, group=str(n), canonical_position_hash='unused'+str(n)))
            (snapshot/'snapshot.json').write_text(json.dumps(dict(games=entries, labels=[])))
            prior = root/'parent.jsonl'
            prior.write_text(''.join(json.dumps(s)+'\n' for s in parent))
            base, binary = root/'base', root/'binary'
            base.write_text('{}');binary.write_text('fixture')
            a = argparse.Namespace(snapshot=snapshot, out=root/'out', parent_samples=prior,
                                   base=base, binary=binary, proportions=PROPORTIONS)
            calls = []
            def export(cmd, **kwargs):
                calls.append(cmd)
                job = next(rows(cmd[3]))
                dst = Path(cmd[4]);dst.mkdir(parents=True)
                n = int(Path(job['game']).stem[1:])
                np.array([n*10+1,n*10+2,n*10+3,n*10+4], dtype='<u4').tofile(dst/'features.bin')
                samples = [dict(game=job['game'], ply=i, offset=2*i, us=1, them=1, material=0, split=job['split']) for i in range(2)]
                (dst/'samples.jsonl').write_text(''.join(json.dumps(s)+'\n' for s in samples))
                (dst/'schema.json').write_text('{}')
                identity = {name+'_sha256':digest(dst/(name+'.'+ext)) for name, ext in [('samples','jsonl'),('features','bin'),('schema','json')]}
                identity['baseline_sha256'] = digest(base)
                (dst/'dataset.json').write_text(json.dumps(identity))
            with patch('recent_data.subprocess.run', side_effect=export):
                build(a)
                self.assertEqual(len(calls), 3)
                # Simulate interruption after all feature exports but before publication.
                (a.out/'dataset.json').unlink()
                build(a)
                self.assertEqual(len(calls), 3)
            samples = list(rows(a.out/'samples.jsonl'))
            self.assertEqual(len(samples), 6)
            self.assertTrue(all(s['outcome'] is None for s in samples if s['game_result'] == 'Draw'))
            self.assertTrue(all(s['outcome'] == (1 if s['side'] == 'Black' else 0) for s in samples if s['game_result'] == 'BlackWins'))
            self.assertEqual(sorted(s['offset'] for s in samples), list(range(0, 12, 2)))
            verify(a.out)
            (a.out/'samples.jsonl').write_text('corrupt')
            with self.assertRaisesRegex(ValueError, 'changed'):
                verify(a.out)

if __name__ == '__main__':
    unittest.main()
