import collections
import importlib.util
import json
import os
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("pgo_verify", Path(__file__).with_name("verify_pgo.py"))
verify = importlib.util.module_from_spec(spec)
spec.loader.exec_module(verify)


class VerifyPGO(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.model = self.root / "model.json"
        self.model.write_text("{}")
        self.game = self.root / "game.json"
        self.game.write_text("{}")
        self.corpus = dict(
            positions=[
                dict(name=f"root-{i}", game=str(self.game), ply=i, sha256=verify.workload.file_digest(self.game))
                for i in range(2)
            ],
            models=[dict(name="test-agent", path=str(self.model), sha256=verify.workload.file_digest(self.model))],
        )
        self.corpus_path = self.root / "corpus.json"
        self.corpus_path.write_text(json.dumps(self.corpus))
        # Exercise real pipes/worker cleanup with deterministic synthetic timings.
        self.helper = self.root / "helper"
        self.helper.write_text(
            "#!/usr/bin/env python3\n"
            "import json,sys\n"
            "c=json.load(open(sys.argv[1]))\n"
            "for line in sys.stdin:\n"
            " q=json.loads(line)\n"
            " r=dict(q,name=c['positions'][q['position']]['name'],agent=c['models'][q['model']]['name'],"
            "completed_depth=q['depth'],aborted=False,signature='a'*64,cpu_ms=10,ms=10,score=123,nodes=5,q_nodes=3,"
            "best=None,hash=123,pieces=400,legal=50,nnue=False)\n"
            " print(json.dumps(r),flush=True)\n"
        )
        self.helper.chmod(0o755)
        self.args = SimpleNamespace(
            corpus=self.corpus_path,
            baseline=self.helper,
            candidate=self.helper,
            out=self.root / "verification",
            cpu=min(os.sched_getaffinity(0)),
            repeats=4,
            depth=2,
            seed=123,
            request_timeout_seconds=5,
            require_speedup=False,
        )

    def tearDown(self):
        self.tmp.cleanup()

    def test_balanced_pairs_and_exact_identity(self):
        result = verify.verify(self.args)
        self.assertEqual(result["pairs"], 8)
        self.assertEqual(result["candidate_over_baseline"], 1)
        self.assertNotIn("source_game_cluster_bootstrap_95", result)
        counts = collections.Counter()
        for row in map(json.loads, (self.args.out / "pairs.jsonl").read_text().splitlines()):
            counts[row["position"], row["order"][0]] += 1
        self.assertEqual(set(counts.values()), {2})
        self.assertEqual(len((self.args.out / "warmups.jsonl").read_text().splitlines()), 2)

    def test_same_time_is_not_an_accepted_speedup(self):
        self.args.require_speedup = True
        with self.assertRaisesRegex(ValueError, "did not reduce"):
            verify.verify(self.args)
        self.assertTrue((self.args.out / "summary.json").exists())

    def test_search_signature_mismatch_rejects_before_summary(self):
        candidate = self.root / "candidate"
        candidate.write_text(self.helper.read_text().replace("signature='a'*64", "signature='b'*64"))
        candidate.chmod(0o755)
        self.args.candidate = candidate
        with self.assertRaisesRegex(ValueError, "mismatch in signature"):
            verify.verify(self.args)
        self.assertFalse((self.args.out / "summary.json").exists())

    def test_missing_parity_field_in_both_results_is_rejected(self):
        self.helper.write_text(self.helper.read_text().replace("legal=50,", ""))
        with self.assertRaisesRegex(ValueError, "missing baseline result field legal"):
            verify.verify(self.args)
        self.assertFalse((self.args.out / "summary.json").exists())

    def test_changed_input_after_search_rejects(self):
        original = verify.workload.audit_inputs
        calls = 0

        def audit(*args):
            nonlocal calls
            calls += 1
            if calls == 2:
                self.model.write_text('{"changed":true}')
            return original(*args)

        with patch.object(verify.workload, "audit_inputs", side_effect=audit):
            with self.assertRaisesRegex(ValueError, "input changed"):
                verify.verify(self.args)
        self.assertFalse((self.args.out / "summary.json").exists())

    def test_cluster_statistics_keep_all_related_roots(self):
        corpus = dict(positions=[dict(game="a"), dict(game="a"), dict(game="b")], models=[dict(name="m")])
        rows = [
            dict(position=i, model=0, baseline=dict(cpu_ms=10), candidate=dict(cpu_ms=5 if i < 2 else 10))
            for i in range(3)
        ]
        result = verify.summarize(rows, corpus)
        self.assertAlmostEqual(result["candidate_over_baseline"], 0.5 ** (2 / 3))
        self.assertEqual(result["source_game_cluster_bootstrap_95"], [0.5, 1.0])


if __name__ == "__main__":
    unittest.main()
