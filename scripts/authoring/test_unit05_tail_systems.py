"""Semantic reconstruction and exhaustive source-only acceptance of the ten-KP tail."""
from collections import Counter
from itertools import product
import json
import math
from pathlib import Path
import sys
import unittest

from unit05_tail_common import DEST, ROOT
from unit05_tail_oracle import (external_signature, parse, perturbed_problem,
                               verify, wrong_answers)
sys.path.insert(0,str(ROOT/'scripts/review'))
from foundations_content_audit import build_report, family, pending_templates

BATCHES = ('checking','special','regions')
KEYS = {f'{t}/kp{i}' for t in ('checking-systems-solutions','systems-special-cases',
        'systems-of-linear-inequalities') for i in range(1,4)} | {'graphing-systems/kp3'}


class SystemsTail(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.facts = json.loads(Path(FACTS).read_text())
        cls.kps = {r['kp_key']:r for r in cls.facts['kps']}
        cls.recipes = [r for batch in BATCHES for r in json.loads((DEST/f'{batch}.json').read_text())]

    def test_scope_audit_and_distinct_authored_semantics(self):
        self.assertEqual({r['kp_id'] for r in self.recipes},KEYS)
        report = build_report(self.facts,pending_templates(ROOT/'docs/content-foundations'))
        for row in report['kps']:
            if row['kp_key'] in KEYS:
                self.assertEqual(row['issues'],[],row['kp_key'])
        for key in KEYS:
            rows = self.kps[key]['exemplars']
            self.assertEqual(len(rows),4)
            structures = set()
            for row in rows:
                verify(key,row['problem'],row['answer'],row['solution_sketch'])
                self.assertTrue(row['authored_answer_decidable'])
                self.assertNotIn(family(row['problem']),structures)
                structures.add(family(row['problem']))

    def test_all_154_pending_instances_materiality_entropy_and_collisions(self):
        seen = self.collision_boundary()
        total = 0
        for recipe in self.recipes:
            key,args = recipe['kp_id'],recipe['arguments']
            self.assertEqual(recipe['status'],'pending')
            self.assertNotIn('space_size',args)
            domains = {k:v['values'] for k,v in args['params'].items()}
            outputs = {}
            for sample in args['samples']:
                params = tuple(sample['params'][k] for k in domains)
                self.assertNotIn(params,outputs)
                problem = args['statement'].format(**sample['params'])
                sig,output = verify(key,problem,sample['expected'])
                self.assertTrue(sig not in seen,(key,seen.get(sig),problem))
                seen[sig] = key
                outputs[params] = output
            self.assertEqual(set(outputs),self.expected_tuples(key,domains))
            self.assertGreaterEqual(len(outputs),12)
            total += len(outputs)
            self.check_entropy_and_active_axes(key,domains,outputs)
        self.assertEqual(total,154)

    def expected_tuples(self,key,domains):
        tuples = set(product(*domains.values()))
        if key == 'checking-systems-solutions/kp2':
            return {(a,b) for a,b in tuples if (2*a+3*b == 11) != (a+b == 13)}
        if key == 'checking-systems-solutions/kp3':
            return {(a,b) for a,b in tuples if b-a in (1,3)}
        return tuples

    def check_entropy_and_active_axes(self,key,domains,outputs):
        counts = Counter(outputs.values())
        entropy = -sum(n/len(outputs)*math.log2(n/len(outputs)) for n in counts.values())
        if key.startswith('systems-special-cases/'):
            expected = {'none':9,'infinite':3} if key.endswith('kp1') else {
                'one':18,'infinite':3,'none':3}
            self.assertEqual(counts,Counter(expected))
            self.assertGreaterEqual(entropy,0.8 if key.endswith('kp1') else 1.0)
        else:
            self.assertEqual(len(counts),len(outputs))
            self.assertGreaterEqual(entropy,math.log2(len(outputs))-1e-12)
        for axis in range(len(domains)):
            pairs = [(p,q) for p in outputs for q in outputs if p[axis] != q[axis]
                     and all(p[i] == q[i] for i in range(len(p)) if i != axis)]
            self.assertTrue(pairs,(key,axis))
            self.assertTrue(any(outputs[p] != outputs[q] for p,q in pairs),(key,axis))
            if not key.startswith('systems-special-cases/'):
                self.assertTrue(all(outputs[p] != outputs[q] for p,q in pairs),(key,axis))

    def collision_boundary(self):
        seen = {}
        for key,kp in self.kps.items():
            for row in kp['exemplars']:
                if key in KEYS:
                    sig = verify(key,row['problem'],row['answer'])[0]
                else:
                    sig = self.optional_signature(row['problem'])
                if sig is None:
                    continue
                if key in KEYS or seen.get(sig) in KEYS:
                    self.assertTrue(sig not in seen,(key,seen.get(sig),row['problem']))
                seen[sig] = key
        count = 0
        for key,rows in pending_templates(ROOT/'docs/content-foundations').items():
            if key in KEYS:
                continue
            for row in rows:
                args = row['document'].get('arguments',{})
                for sample in args.get('samples',[]):
                    problem = args['statement'].format(**sample['params'])
                    sig = self.optional_signature(problem)
                    if sig is not None:
                        self.assertNotIn(seen.get(sig),KEYS)
                        seen[sig] = key
                        count += 1
        self.assertGreaterEqual(count,96)
        return seen

    def optional_signature(self,problem):
        try:
            return external_signature(problem)
        except (AssertionError,ValueError,KeyError,SyntaxError,ZeroDivisionError,StopIteration):
            return None

    def test_each_authored_and_pending_answer_rejects_perturbations(self):
        tasks = [(key,r['problem'],r['answer']) for key in KEYS for r in self.kps[key]['exemplars']]
        tasks += [(r['kp_id'],r['arguments']['statement'].format(**s['params']),s['expected'])
                  for r in self.recipes for s in r['arguments']['samples']]
        for key,problem,answer in tasks:
            for wrong in wrong_answers(answer):
                with self.assertRaises(AssertionError,msg=(key,problem,wrong)):
                    verify(key,problem,wrong)
            with self.assertRaises(AssertionError,msg=(key,problem,answer)):
                verify(key,perturbed_problem(key,problem,answer),answer)

    def test_entropy_checks_refuse_constant_and_cancelling_families(self):
        for recipe in self.recipes:
            key,args = recipe['kp_id'],recipe['arguments']
            domains = {k:v['values'] for k,v in args['params'].items()}
            tuples = self.expected_tuples(key,domains)
            for bad in ({p:(0,0) for p in tuples},
                        {p:(p[0]-p[0],p[1]-p[1]) for p in tuples}):
                with self.assertRaises(AssertionError):
                    self.check_entropy_and_active_axes(key,domains,bad)

    def test_scope_refusals_and_equivalent_geometry(self):
        for problem in ['Classify $0x+0y=0$ and $x+y=3$.',
                        'Classify $x*y=1$ and $x+y=2$.']:
            with self.assertRaises(AssertionError):
                verify('systems-special-cases/kp3',problem,'one = no; infinite = yes')
        p = 'For $y<=2x+3$ and $y>-x+1$.'
        good = 'solid = (2,3,-1); dashed = (-1,1,1)'
        sig,_ = verify('systems-of-linear-inequalities/kp2',p,good)
        scaled = 'For $-2y>=-4x-6$ and $3y>-3x+3$.'
        self.assertEqual(verify('systems-of-linear-inequalities/kp2',scaled,good)[0],sig)
        for bad in ['solid = (2,3,0); dashed = (-1,1,1)',
                    'solid = (-1,1,1); dashed = (2,3,-1)',
                    'solid = (2,3,-1); dashed = (-1,1,-1)']:
            with self.assertRaises(AssertionError):
                verify('systems-of-linear-inequalities/kp2',p,bad)
        for bad in ['For $y<2x+3$ and $y>-x+1$.',
                    'For $x<=3$ and $y>-x+1$.']:
            with self.assertRaises(AssertionError):
                verify('systems-of-linear-inequalities/kp2',bad,good)


if __name__ == '__main__':
    FACTS = sys.argv.pop(1)
    unittest.main()
