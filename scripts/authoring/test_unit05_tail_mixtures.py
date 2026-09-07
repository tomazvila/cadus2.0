"""Prompt-derived balance equations, exhaustive materiality, and negative controls."""
from collections import Counter
from fractions import Fraction as Q
from itertools import product
import json
import math
from pathlib import Path
import re
import sys
import unittest

from test_unit05_residual import scalar, signature
from test_unit05_residual_second import canonical_rows, model as prior_model, solved
from unit05_tail_common import DEST, ROOT
sys.path.insert(0, str(ROOT/'scripts/review'))
from foundations_content_audit import build_report, pending_templates

KEYS = {f'systems-mixture-problems/kp{i}' for i in range(1,4)}


def reconstruct(key, problem):
    premise = problem.split(' Let x')[0].split(' Give the')[0]
    nums = [Q(n) for n in re.findall(r'\d+(?:/\d+)?', premise)]
    assert len(nums) == 4, premise
    p, q, total, target = nums
    if key.endswith('kp3'):
        assert 'priced at' in premise
        if 'cents/kg' in premise:
            p, q = p/100, q/100
        if 'grams' in premise and 'kilograms' not in premise:
            total /= 1000
        value = target if 'total value' in premise else target*total
    else:
        if 'pure fractions' not in premise:
            p, q = p/100, q/100
        value = target if 'containing' in premise else total*target
        if 'containing' not in premise and 'pure fraction' not in premise:
            value /= 100
        assert 0 <= p <= 1 and 0 <= q <= 1
    assert total > 0 and min(p,q) < value/total < max(p,q)
    return [(Q(1), Q(1), total), (p,q,value)]


def verify(key, problem, answer, sketch=None):
    rows = reconstruct(key, problem)
    supplied = tuple(scalar(v,{}) for v in answer.strip('() ').split(','))
    _, _, total = rows[0]
    p,q,value = rows[1]
    x,y = solved(rows)
    expected = (total,p,q,value) if key.endswith('kp1') else (x,y)
    assert supplied == expected, (problem, supplied, expected)
    assert x > 0 and y > 0
    if sketch is not None:
        assert len(sketch) >= 90
        for chunk in re.findall(r'\$([^$]+)\$', sketch):
            if '=' in chunk:
                values = [scalar(v,dict(x=x,y=y)) for v in chunk.split('=')]
                assert len(set(values)) == 1, chunk
    return canonical_rows(rows), supplied


class Mixtures(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.facts = json.loads(Path(FACTS).read_text())
        cls.kps = {r['kp_key']: r for r in cls.facts['kps']}
        cls.recipes = json.loads((DEST/'mixtures.json').read_text())

    def test_authored_and_audit(self):
        report = build_report(self.facts, pending_templates(ROOT/'docs/content-foundations'))
        for row in report['kps']:
            if row['kp_key'] in KEYS:
                # Static source facts leave production gating explicitly unverified.
                self.assertEqual([issue['code'] for issue in row['issues']],
                                 ['pending_template_production_gate_declined'])
        for key in KEYS:
            exemplars = self.kps[key]['exemplars']
            self.assertEqual(len(exemplars),4)
            for row in exemplars:
                self.assertTrue(row['authored_answer_decidable'])
                verify(key,row['problem'],row['answer'],row['solution_sketch'])

    def test_exhaustive_materiality_and_collisions(self):
        seen = self.collision_boundary()
        self.assertEqual({r['kp_id'] for r in self.recipes}, KEYS)
        for recipe in self.recipes:
            key, args = recipe['kp_id'], recipe['arguments']
            self.assertEqual(recipe['status'],'pending')
            self.assertEqual(args['constraints'],[])
            self.assertNotIn('space_size',args)
            domains = {k:v['values'] for k,v in args['params'].items()}
            tuples = set(product(*domains.values()))
            outputs = {}
            for sample in args['samples']:
                params = tuple(sample['params'][k] for k in domains)
                self.assertNotIn(params,outputs)
                problem = args['statement'].format(**sample['params'])
                sig,answer = verify(key,problem,sample['expected'])
                self.assertNotIn(sig,seen,(key,seen.get(sig),problem))
                seen[sig] = key
                outputs[params] = answer
            self.assertEqual(set(outputs),tuples)
            self.assertEqual(len(outputs),12)
            counts = Counter(outputs.values())
            entropy = -sum(n/12*math.log2(n/12) for n in counts.values())
            self.assertGreaterEqual(entropy,math.log2(12)-1e-12)
            for a,b in tuples:
                for other in domains['a']:
                    if a != other:
                        self.assertNotEqual(outputs[a,b],outputs[other,b])
                for other in domains['b']:
                    if b != other:
                        self.assertNotEqual(outputs[a,b],outputs[a,other])

    def collision_boundary(self):
        seen = {}
        for key,kp in self.kps.items():
            for row in kp['exemplars']:
                sig = self.read_signature(key,row['problem'],row['answer'])
                if sig is None:
                    continue
                if key in KEYS or seen.get(sig) in KEYS:
                    self.assertNotIn(sig,seen,(key,seen.get(sig)))
                seen[sig] = key
        count = 0
        for key,rows in pending_templates(ROOT/'docs/content-foundations').items():
            if key in KEYS:
                continue
            for row in rows:
                args = row['document'].get('arguments',{})
                for sample in args.get('samples',[]):
                    problem = args['statement'].format(**sample['params'])
                    sig = self.read_signature(key,problem,sample['expected'])
                    if sig is not None:
                        self.assertNotIn(seen.get(sig),KEYS)
                        seen[sig] = key
                        count += 1
        self.assertGreaterEqual(count,96)
        return seen

    def read_signature(self,key,problem,answer):
        if key in KEYS:
            return verify(key,problem,answer)[0]
        try:
            return canonical_rows(prior_model(key,problem)[0])
        except (AssertionError,ValueError,KeyError,SyntaxError,ZeroDivisionError):
            try:
                return signature(problem)
            except (AssertionError,ValueError,KeyError,SyntaxError,ZeroDivisionError,StopIteration):
                return None

    def test_every_sample_rejects_perturbed_balance_and_answer(self):
        for recipe in self.recipes:
            key,args = recipe['kp_id'],recipe['arguments']
            for sample in args['samples']:
                problem = args['statement'].format(**sample['params'])
                answer = sample['expected']
                values = [Q(s) for s in answer.strip('()').split(',')]
                for index in range(len(values)):
                    bad = values.copy()
                    bad[index] += 1
                    with self.assertRaises(AssertionError):
                        verify(key,problem,'('+','.join(map(str,bad))+')')
                mutated = re.sub(r'\d+',lambda m:str(int(m[0])+1),problem,count=1)
                with self.assertRaises(AssertionError):
                    verify(key,mutated,answer)
        with self.assertRaises(AssertionError):
            verify('systems-mixture-problems/kp2',
                   'Blend 20% and 40% acid into 10 liters at 60%.', '(-10,20)')


if __name__ == '__main__':
    FACTS = sys.argv.pop(1)
    unittest.main()
