"""Exhaustive semantics, collision, entropy, materiality and negative controls."""
from collections import Counter
from itertools import product
import json
from pathlib import Path
import re
import sys
import unittest

from build import ROOT, OUT, generate, is_set, reconstruct
from semantic import affine, math, verify as verify_linear
import graphs
import semantic_sets
import semantic_labels
from math import log2


def verify(problem,answer,solution=None):
    if 'Give boundary and graph' in problem: return graphs.verify(problem,answer,solution)
    if semantic_labels.is_label(problem): return semantic_labels.verify(problem,answer,solution)
    if is_set(problem): return semantic_sets.verify(problem,answer,solution)
    return verify_linear(problem,answer,solution)


def family(problem):
    """Preserve the actual task's two sides, ignoring presentation and whitespace."""
    if 'Give boundary and graph' in problem: return ('graph',math(problem)[0])
    if semantic_labels.is_label(problem): return semantic_labels.family(problem)
    if is_set(problem): return ('bounded',semantic_sets.parts(problem).replace(' ',''))
    equation=next(s for s in math(problem) if re.search(r'<=|>=|<|>',s))
    left,op,right=re.split(r'(<=|>=|<|>)',equation)
    return (affine(left),op,affine(right))


def material(args):
    domains={k:v['values'] for k,v in args['params'].items()}
    expected=list(product(*domains.values()))
    samples={tuple(s['params'][k] for k in domains):s for s in args['samples']}
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
    if args['answer_contract']['kind']=='label':
        truth_materiality(answers,domains)
        return signatures
    assert len(set(answers.values()))>=12, 'Collapsed answer entropy'
    for params in expected:
        for axis,domain in enumerate(domains.values()):
            for other in domain:
                changed=list(params)
                changed[axis]=other
                if tuple(changed)!=params:
                    assert answers[params]!=answers[tuple(changed)]
    return signatures


def truth_materiality(answers,domains):
    counts=Counter(answers.values())
    assert set(counts)=={'yes','no'}, 'Constant decision family'
    total=sum(counts.values())
    assert -sum((n/total)*log2(n/total) for n in counts.values())>=0.8
    for axis,domain in enumerate(domains.values()):
        changes=[]
        for params,answer in answers.items():
            for other in domain:
                changed=list(params)
                changed[axis]=other
                changes.append(answer!=answers[tuple(changed)])
        assert any(changes), 'Nonmaterial decision axis'


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
        for p,bad in [('Solve $-8<-2x<=6$.','-4 < x <= 3'),
                      ('Solve $|2x+1|<=7$.','-3 <= x <= 4'),
                      ('Write $-2<x<=5$ in interval notation.','[-2, 5]')]:
            with self.assertRaises(AssertionError): verify(p,bad)
        for p,bad in [('Is $x=4$ a solution of $2x+1<9$?','yes'),
                      ('For $2x+3y<=-5$, does the shaded region include the origin?','yes'),
                      ('Is $x=5$ a solution of $x>2$ and $x<4$?','yes')]:
            with self.assertRaises(AssertionError): verify(p,bad)
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
