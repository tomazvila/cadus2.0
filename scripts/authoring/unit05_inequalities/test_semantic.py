"""Exhaustive semantics, collision, entropy, materiality and negative controls."""
from collections import Counter
from itertools import product
import json
from pathlib import Path
import re
import sys
import unittest

from build import ROOT, OUT, generate
from semantic import affine, math, reconstruct, verify


def family(problem):
    """Preserve the actual task's two sides, ignoring presentation and whitespace."""
    equation=next(s for s in math(problem) if re.search(r'<=|>=|<|>',s))
    left,op,right=re.split(r'(<=|>=|<|>)',equation)
    return (affine(left),op,affine(right))


def material(args):
    domains={k:v['values'] for k,v in args['params'].items()}
    expected=list(product(domains['a'],domains['b']))
    samples={tuple(s['params'][k] for k in ('a','b')):s for s in args['samples']}
    assert len(expected)>=12 and set(expected)==set(samples)
    answers={}
    signatures=set()
    for params,sample in samples.items():
        problem=args['statement'].format(**sample['params'])
        verify(problem,sample['expected'])
        sig=family(problem)
        assert sig not in signatures, 'Semantic collision'
        signatures.add(sig)
        answers[params]=sample['expected']
    assert len(set(answers.values()))>=12, 'Collapsed answer entropy'
    for a,b in expected:
        assert all(answers[a,b]!=answers[other,b] for other in domains['a'] if other!=a)
        assert all(answers[a,b]!=answers[a,other] for other in domains['b'] if other!=b)
    return signatures


class SemanticTests(unittest.TestCase):
    def test_generated_source_matches_checked_in_pending(self):
        _,recipes=generate()
        self.assertEqual(recipes,json.loads((OUT/'templates.json').read_text()))
        baseline=json.loads((OUT/'baseline-selected.json').read_text())
        self.assertEqual(baseline['confirmed_absent_pending'],27)
        self.assertTrue({r['kp_id'] for r in recipes} <= {r['kp_key'] for r in baseline['kps']})

    def test_four_material_exemplars_and_exhaustive_recipes(self):
        authored,recipes=generate()
        seen={}
        for key,examples in authored.items():
            self.assertEqual(len(examples),4)
            structures=set()
            for row in examples:
                verify(row['problem'],row['answer'],row['solution_sketch'])
                sig=family(row['problem'])
                self.assertNotIn(sig,seen,(key,seen.get(sig)))
                seen[sig]=key
                structures.add(re.sub(r'\d+','#',row['problem']))
            self.assertEqual(len(structures),4,key)
        for row in recipes:
            self.assertEqual(row['status'],'pending')
            self.assertNotIn('space_size',row['arguments'])
            for sig in material(row['arguments']):
                self.assertNotIn(sig,seen,(row['kp_id'],seen.get(sig)))
                seen[sig]=row['kp_id']

    def test_semantic_negative_controls(self):
        for problem,bad in [('Solve $-3x<=12$.','x <= -4'),
                            ('Solve $2x+1<7$.','x <= 3'),
                            ('Solve $x/3+2>=7$.','x >= 5')]:
            with self.assertRaises(AssertionError): verify(problem,bad)
        for bad in ['Solve $x-x<=2$.','Solve $2x+3<=2x+5$.']:
            with self.assertRaises(AssertionError): reconstruct(bad)
        args=generate()[1][0]['arguments']
        for mutation in ['sample','collision','cancelling']:
            changed=json.loads(json.dumps(args))
            if mutation=='sample': changed['samples'][0]['expected']='x <= 999'
            if mutation=='collision': changed['params']['a']['values'][1]=changed['params']['a']['values'][0]
            if mutation=='cancelling': changed['statement']='Solve $x+{a}-{a}<={b}$.'
            with self.assertRaises(AssertionError): material(changed)

    def test_current_rust_facts_and_scope_isolation(self):
        facts=json.loads(Path(FACTS).read_text())
        authored,_=generate()
        by_key={r['kp_key']:r for r in facts['kps']}
        for key,examples in authored.items():
            actual=by_key[key]['exemplars']
            self.assertEqual(len(actual),4)
            for row,wanted in zip(actual,examples):
                for field in wanted: self.assertEqual(row[field],wanted[field])
                self.assertTrue(row['authored_answer_decidable'],(key,row))
        before=json.loads(Path(BASELINE).read_text())
        for row in before['kps']:
            if row['kp_key'] not in authored:
                self.assertEqual(row,by_key[row['kp_key']])


if __name__=='__main__':
    FACTS=sys.argv.pop(1)
    BASELINE=sys.argv.pop(1)
    unittest.main()
