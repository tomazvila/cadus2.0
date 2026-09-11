"""Independent reconstruction of rendered U07 statements; no recipe solver imports."""
from collections import Counter
import itertools
import hashlib
import json
import math
from pathlib import Path
import re
import sys
import unittest

import sympy as s
from sympy.parsing.sympy_parser import (parse_expr, standard_transformations,
                                      implicit_multiplication_application)

ROOT = Path(__file__).resolve().parents[3]
KEYS = ['polynomial-basics/kp2', 'difference-of-squares/kp1',
        'choosing-factoring-strategy/kp1', 'parabola-vertex-form/kp2',
        'quadratic-graphs-vertex/kp3']
X = s.Symbol('x')


def expression(text):
    text = re.sub(r'(?<=\d)(?=x)', '*', text)
    return parse_expr(text.replace('^', '**'), local_dict={'x': X},
                      transformations=standard_transformations + (implicit_multiplication_application,))


def polynomial(problem):
    """Read mathematical input from prose/area/table; ignore all recipe parameters."""
    if 'coefficient table' in problem.lower():
        columns, values = re.search(r'columns \$\(([^$]+)\)\$ has row \$\(([^$]+)\)\$', problem).groups()
        result = sum(expression(c)*expression(v) for c,v in zip(columns.split(','), values.split(','), strict=True))
    elif 'tile inventory' in problem:
        assert 'nine positive cubic pieces and no other pieces' in problem
        result = 9*X**3
    elif 'square of side' in problem:
        sides = re.findall(r'square of side \$([^$]+)\$', problem)
        assert len(sides) == 2
        result = expression(sides[0])**2-expression(sides[1])**2
    else:
        maths = re.findall(r'\$([^$]+)\$', problem)
        equations = [m.split('=',1)[1] for m in maths if m.startswith('y=')]
        if equations:
            result = expression(equations[-1])
        else:
            candidates = [m for m in maths if 'x' in m and not any(c in m for c in '=<>')]
            result = expression(candidates[-1] if 'proposes' in problem else candidates[0])
    return s.Poly(s.expand(result), X)


def fields(answer):
    pairs = [part.strip().split(' = ',1) for part in answer.split(';')]
    assert all(len(pair)==2 for pair in pairs), answer
    assert len({pair[0] for pair in pairs}) == len(pairs), answer
    return dict(pairs)


def point(text):
    value = s.sympify(text)
    assert isinstance(value, tuple) and len(value)==2, text
    return value


def strategy(poly):
    coeffs = poly.all_coeffs()
    if abs(s.gcd_list(coeffs)) > 1 or poly.eval(0)==0:
        return 'GCF'
    a,b,c = coeffs
    if b==0 and c<0 and s.sqrt(a).is_Integer and s.sqrt(-c).is_Integer:
        return 'difference of squares'
    assert all(v != 0 for v in coeffs), poly
    # Every selected trinomial actually has two integer linear factors.
    factors = s.factor_list(poly.as_expr())[1]
    assert sum(s.degree(f,X)*n for f,n in factors)==2
    assert all(s.degree(f,X)==1 for f,n in factors)
    return 'trinomial factoring'


def verify(key, problem, answer):
    poly = polynomial(problem)
    if key == KEYS[0]:
        assert 1 <= len(poly.terms()) <= 3
        expected = ['monomial','binomial','trinomial'][len(poly.terms())-1]
        assert answer == expected, (problem,answer,expected)
    elif key == KEYS[1]:
        parts = fields(answer)
        assert set(parts)=={'lower_factor','upper_factor'}
        lower,upper = [s.Poly(expression(parts[k]),X) for k in ('lower_factor','upper_factor')]
        assert all(f.degree()==1 and f.LC()==1 for f in [lower,upper])
        assert lower.TC()<upper.TC()
        assert lower*upper==poly, (problem,answer)
    elif key == KEYS[2]:
        assert answer == strategy(poly), (problem,answer,strategy(poly))
    else:
        verify_graph(key,poly,answer)
    return tuple(str(c) for c in poly.all_coeffs())


def verify_graph(key, poly, answer):
    assert poly.degree()==2
    a,b,c = poly.all_coeffs()
    h = -b/(2*a)
    k = poly.eval(h)
    parts = fields(answer)
    assert parts['direction']==('upward' if a>0 else 'downward')
    # Reconstruct completed square and symmetry, without the recipe's root formula.
    assert s.expand(a*(X-h)**2+k)==poly.as_expr()
    if key == KEYS[3]:
        assert set(parts)=={'direction','extreme_value'}
        assert expression(parts['extreme_value'])==k
        return
    assert set(parts)=={'direction','vertex','left_intercept','right_intercept','y_intercept'}
    assert point(parts['vertex'])==(h,k)
    left,right = point(parts['left_intercept']),point(parts['right_intercept'])
    assert left[0]<right[0] and left[1]==right[1]==0
    assert poly.eval(left[0])==poly.eval(right[0])==0
    assert left[0]+right[0]==2*h
    assert point(parts['y_intercept'])==(0,c)


def perturbed(key, problem, answer):
    """Change the printed math while preserving the stored answer: it must fail."""
    poly = polynomial(problem).as_expr()
    if key == KEYS[0]:
        replacement = poly + X**5 if len(s.Poly(poly,X).terms())<3 else poly-s.Poly(poly,X).terms()[-1][1]*X**s.Poly(poly,X).terms()[-1][0][0]
    elif key == KEYS[2]:
        replacement = X**2-169 if answer!='difference of squares' else 3*poly
    else:
        replacement = poly+1
    return f'For $y={replacement}$, solve the requested task.'


def diversity(signatures, answers):
    assert len(signatures)>=12 and len(set(signatures))==len(signatures)
    counts = Counter(answers)
    total = len(answers)
    entropy = -sum((n/total)*math.log2(n/total) for n in counts.values())
    assert entropy>=1.5
    return entropy


def stored_problems(value):
    if isinstance(value,list):
        for item in value:
            yield from stored_problems(item)
    elif isinstance(value,dict):
        if isinstance(value.get('problem'),str):
            yield value['problem']
        if isinstance(value.get('statement'),str):
            for sample in value.get('samples',[]):
                try:
                    yield value['statement'].format(**sample['params'])
                except (KeyError,ValueError):
                    continue
        for item in value.values():
            yield from stored_problems(item)


class ComplementSemantics(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.facts = json.loads((ROOT/'target/unit07-complement/facts.json').read_text())
        cls.by_key = {r['kp_key']:r for r in cls.facts['kps']}
        cls.evidence = json.loads((ROOT/'target/unit07-complement/production-evidence.json').read_text())
        cls.recipes = json.loads((ROOT/'docs/content-foundations/unit07-complement/templates.json').read_text())
        cls.metrics = []

    def test_authored_semantics_and_material_representations(self):
        for key in KEYS:
            rows = self.by_key[key]['exemplars']
            self.assertEqual(len(rows),4)
            structures = set()
            signatures = set()
            for row in rows:
                self.assertTrue(row['authored_answer_decidable'])
                self.assertGreater(len(row['solution_sketch']),65)
                sig = verify(key,row['problem'],row['answer'])
                self.assertNotIn(sig,signatures)
                signatures.add(sig)
                structures.add(re.sub(r'\d+','#',row['problem']))
            self.assertEqual(len(structures),4)

    def test_exhaustive_rendered_semantics_entropy_collisions_and_perturbations(self):
        self.assertEqual({r['kp_id'] for r in self.evidence},set(KEYS))
        seen = set()
        for key in KEYS:
            seen.update((key,verify(key,e['problem'],e['answer'])) for e in self.by_key[key]['exemplars'])
        known_text = {e['problem'] for kp in self.facts['kps'] for e in kp['exemplars']}
        for directory in ['docs/content-foundations','docs/reports']:
            for path in (ROOT/directory).rglob('*.json'):
                if 'unit07-complement' not in path.parts:
                    known_text.update(stored_problems(json.loads(path.read_text())))
        for row in self.evidence:
            key = row['kp_id']
            self.assertEqual(row['status'],'pending')
            count = 14 if key==KEYS[0] else 12
            self.assertEqual(len(row['instances']),count)
            answers = Counter()
            for item in row['instances']:
                sig = verify(key,item['problem'],item['answer'])
                self.assertNotIn((key,sig),seen)
                seen.add((key,sig))
                self.assertNotIn(item['problem'],known_text)
                known_text.add(item['problem'])
                answers[item['answer']]+=1
                self.assertGreater(len(item['solution_sketch']),100)
                with self.assertRaises(AssertionError):
                    verify(key,perturbed(key,item['problem'],item['answer']),item['answer'])
            signatures = [verify(key,i['problem'],i['answer']) for i in row['instances']]
            entropy = diversity(signatures,[i['answer'] for i in row['instances']])
            self.metrics.append(dict(kp_id=key,instances=count,semantic_signatures=count,
                distinct_answers=len(answers),answer_entropy_bits=entropy,
                authored_collisions=0,sibling_collisions=0,perturbed_prompt_rejections=count))


    def test_all_declared_tuples_are_sampled_and_rendered(self):
        for row,evidence in zip(self.recipes,self.evidence,strict=True):
            self.assertEqual(row['kp_id'],evidence['kp_id'])
            args = row['arguments']
            domains = {k:v['values'] for k,v in args['params'].items()}
            wanted = set(itertools.product(*domains.values()))
            if row['kp_id']==KEYS[0]:
                wanted = {t for t in wanted if 1 <= sum(dict(zip(domains,t))[k]!=0 for k in 'abcd') <= 3}
            if row['kp_id']==KEYS[2]:
                wanted = {t for t in wanted if dict(zip(domains,t))['b'] in (0,dict(zip(domains,t))['c']-1)}
            actual = {tuple(sample['params'][k] for k in domains) for sample in args['samples']}
            self.assertEqual(wanted,actual)
            self.assertEqual(len(actual),14 if row['kp_id']==KEYS[0] else 12)
            rendered = {verify(row['kp_id'],i['problem'],i['answer']) for i in evidence['instances']}
            for sample in args['samples']:
                problem = args['statement'].format(**sample['params'])
                self.assertIn(verify(row['kp_id'],problem,sample['expected']),rendered)

    def test_diversity_rejects_collisions_constants_and_number_only_support_variants(self):
        with self.assertRaises(AssertionError):
            diversity([1]*12,range(12))
        with self.assertRaises(AssertionError):
            diversity(range(12),['constant']*12)
        with self.assertRaises(AssertionError):
            diversity(range(11),range(11))
        row = next(r for r in self.evidence if r['kp_id']==KEYS[0])
        supports = [tuple(m for m,c in polynomial(i['problem']).terms()) for i in row['instances']]
        self.assertEqual(len(set(supports)),14)
        self.assertEqual(Counter(len(s) for s in supports),{1:4,2:6,3:4})

    def test_semantic_negative_controls(self):
        for row in self.evidence:
            key = row['kp_id']
            for item in row['instances']:
                answer = item['answer']
                if key in KEYS[:3:2]:
                    wrong = 'monomial' if answer!='monomial' else 'trinomial'
                elif key==KEYS[1]:
                    f=fields(answer)
                    wrong=f'lower_factor = {f["upper_factor"]}; upper_factor = {f["lower_factor"]}'
                else:
                    wrong=answer.replace('upward','WRONG').replace('downward','upward').replace('WRONG','downward')
                with self.assertRaises((AssertionError,KeyError)):
                    verify(key,item['problem'],wrong)


if __name__ == '__main__':
    output = ROOT/'target/unit07-complement/semantic-evidence.json'
    output.unlink(missing_ok=True)
    run = unittest.main(exit=False)
    if not run.result.wasSuccessful():
        raise SystemExit(1)
    paths = ['target/unit07-complement/facts.json', 'target/unit07-complement/production-evidence.json',
             'docs/content-foundations/unit07-complement/templates.json']
    packet = dict(kps=ComplementSemantics.metrics,
                  inputs_sha256={p:hashlib.sha256((ROOT/p).read_bytes()).hexdigest() for p in paths})
    output.write_text(json.dumps(packet,indent=2)+'\n')
