import math,unittest
from analyze import summarize
class AnalysisTests(unittest.TestCase):
 def rows(self,logs,repeats):
  return [dict(position=i,model=0,measurements={'stock':{'ms':100,'cpu_ms':100},'candidate':{'ms':100/math.exp(x),'cpu_ms':100/math.exp(x)}}) for i,x in enumerate(logs) for _ in range(repeats)]
 def test_repetition_does_not_inflate_independent_position_count(self):
  a=summarize(self.rows([.1,.2,.1,.2],1),'candidate')
  b=summarize(self.rows([.1,.2,.1,.2],20),'candidate')
  self.assertEqual(a['positions'],4);self.assertEqual(b['positions'],4)
  self.assertEqual(a['p_two_sided'],b['p_two_sided']);self.assertAlmostEqual(a['speedup'],b['speedup'])
  self.assertEqual(a['p_two_sided'],.125)
 def test_null_and_balanced_effect(self):
  a=summarize(self.rows([0]*8,2),'candidate');self.assertEqual(a['speedup'],1);self.assertEqual(a['p_two_sided'],1)
  b=summarize(self.rows([.1,-.1]*4,2),'candidate');self.assertAlmostEqual(b['speedup'],1);self.assertEqual(b['p_two_sided'],1)
 def test_eight_independent_improvements(self):
  a=summarize(self.rows([math.log(1.2)]*8,3),'candidate')
  self.assertAlmostEqual(a['speedup'],1.2);self.assertEqual(a['p_two_sided'],2/256)
if __name__=='__main__':unittest.main()
