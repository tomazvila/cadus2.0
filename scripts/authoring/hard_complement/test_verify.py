"""Adversarial controls for independent reconstruction and materiality evidence."""
import copy
import json
from pathlib import Path
import unittest

from verify import ROOT, evidence


class ReconstructionTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.report = json.loads((ROOT / 'target/hard-complement/regression/gate.json').read_text())

    def test_actual_rendered_instances(self):
        self.assertEqual(len(evidence(self.report)), 2)

    def test_wrong_stored_answer(self):
        mutated = copy.deepcopy(self.report)
        mutated['rows'][0]['evidence']['instances'][0]['answer'] = '999'
        with self.assertRaises(AssertionError):
            evidence(mutated)

    def test_changed_printed_base(self):
        mutated = copy.deepcopy(self.report)
        item = mutated['rows'][0]['evidence']['instances'][0]
        item['problem'] = item['problem'].replace('10^', 'e^')
        with self.assertRaises(AssertionError):
            evidence(mutated)

    def test_duplicate_instance(self):
        mutated = copy.deepcopy(self.report)
        items = mutated['rows'][0]['evidence']['instances']
        items[1] = copy.deepcopy(items[0])
        with self.assertRaises(AssertionError):
            evidence(mutated)

    def test_inert_axis(self):
        mutated = copy.deepcopy(self.report)
        for item in mutated['rows'][0]['evidence']['instances']:
            item['params']['decoration'] = 'same'
        with self.assertRaises(AssertionError):
            evidence(mutated)


if __name__ == '__main__':
    unittest.main()
