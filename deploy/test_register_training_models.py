import unittest
from register_training_models import merge_bindings

class BindingTests(unittest.TestCase):
    def test_new_agent_is_available_without_rebinding_historical_teachers(self):
        old={'old':{'sha256':'unchanged'}}
        current=dict(old,new={'sha256':'v4'})
        self.assertEqual(merge_bindings(old,current),current)
        self.assertNotIn('new',old)
        self.assertEqual(merge_bindings(current,current),current)
        with self.assertRaises(ValueError):merge_bindings(old,{'old':{'sha256':'different'}})
