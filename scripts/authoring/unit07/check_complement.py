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

APPROVED_DOMAINS = {'polynomial-basics/kp2': {'params': {'a': {'kind': 'choice', 'values': [0, 2]}, 'b': {'kind': 'choice', 'values': [0, 3]}, 'c': {'kind': 'choice', 'values': [0, 5]}, 'd': {'kind': 'choice', 'values': [0, -9]}, 'f': {'kind': 'choice', 'values': ['monomial']}, 'g': {'kind': 'choice', 'values': ['binomial']}, 'h': {'kind': 'choice', 'values': ['trinomial']}}, 'constraints': [{'op': 'ge', 'left': {'add': [{'mul': [{'lit': '1/2'}, 'a']}, {'mul': [{'lit': '1/3'}, 'b']}, {'mul': [{'lit': '1/5'}, 'c']}, {'mul': [{'lit': '-1/9'}, 'd']}]}, 'right': {'lit': 1}}, {'op': 'le', 'left': {'add': [{'mul': [{'lit': '1/2'}, 'a']}, {'mul': [{'lit': '1/3'}, 'b']}, {'mul': [{'lit': '1/5'}, 'c']}, {'mul': [{'lit': '-1/9'}, 'd']}]}, 'right': {'lit': 3}}], 'count': 14}, 'difference-of-squares/kp1': {'params': {'a': {'kind': 'choice', 'values': [1, 4, 9, 16, 25, 49, 64, 81, 100]}}, 'constraints': [], 'count': 9}, 'choosing-factoring-strategy/kp1': {'params': {'g': {'kind': 'choice', 'values': [1, 2]}, 'b': {'kind': 'choice', 'values': [0, 48, 80, 120]}, 'c': {'kind': 'choice', 'values': [49, 81, 121]}, 'f': {'kind': 'choice', 'values': ['GCF']}, 's': {'kind': 'choice', 'values': ['difference of squares']}, 'h': {'kind': 'choice', 'values': ['trinomial factoring']}}, 'constraints': [{'op': 'eq', 'left': {'mul': ['b', {'add': ['b', {'mul': [{'lit': -1}, 'c']}, {'lit': 1}]}]}, 'right': {'lit': 0}}], 'count': 12}, 'parabola-vertex-form/kp2': {'params': {'a': {'kind': 'choice', 'values': [-2, 3]}, 'k': {'kind': 'choice', 'values': [-8, -5, -2, 7, 10, 12]}, 'd': {'kind': 'choice', 'values': ['downward']}, 'u': {'kind': 'choice', 'values': ['upward']}}, 'constraints': [], 'count': 12}, 'quadratic-graphs-vertex/kp3': {'params': {'a': {'kind': 'choice', 'values': [-1, 1]}, 'l': {'kind': 'choice', 'values': [-8, -6]}, 'r': {'kind': 'choice', 'values': [4, 8, 12]}, 'd': {'kind': 'choice', 'values': ['downward']}, 'u': {'kind': 'choice', 'values': ['upward']}}, 'constraints': [], 'count': 12}}
SOURCE_ALIASES = {
    'docs/content-foundations/whole-course-teach/inputs/templates.json': set(KEYS),
    'docs/content-foundations/teach-reviews/rational-final-55/remaining-55-finite7.instruction-gate.input-v2.json': {KEYS[0], KEYS[2], KEYS[3]},
    'docs/content-foundations/hint-reviews/exponents-remaining-17/radical17.instruction-gate.input-v2.json': {KEYS[0], KEYS[2], KEYS[3]},
    'docs/content-foundations/finite-objective-domains/adoption-2026-09-13/repair-plan.json': {KEYS[1]},
}


LEGACY_CONTRACT_ALIAS_PATHS = {
    "docs/content-foundations/teach-reviews/rational-final-55/remaining-55-finite7.instruction-gate.input-v2.json",
    "docs/content-foundations/hint-reviews/exponents-remaining-17/radical17.instruction-gate.input-v2.json",
}

def expected_count(recipe):
    expected = APPROVED_DOMAINS[recipe['kp_id']]
    args = recipe['arguments']
    assert args['params'] == expected['params']
    assert args['constraints'] == expected['constraints']
    return expected['count']


def native_case(row, problem, answer=None):
    policy = row.get('finite_policy')
    if policy is None:
        return None
    matches = [case for case in policy['cases'] if any(
        variant['problem'].strip() == problem.strip()
        and (answer is None or variant['answer'] == answer)
        for variant in case['variants'])]
    assert len(matches) <= 1
    return matches[0] if matches else None


def permitted_finite_collision(row, item, prior_problem):
    case = native_case(row, item['problem'], item['answer'])
    prior = native_case(row, prior_problem)
    if case is None or prior is None:
        return False
    assert item['native_finite_case'] == {'id': case['id'], 'role': case['role']}
    return case['id'] == prior['id'] and case['role'] == 'practice_fresh'


def validate_finite_cases(row):
    if row['kp_id'] != KEYS[1]:
        assert row.get('finite_policy') is None
        return None
    policy = row['finite_policy']
    assert policy['schema_version'] == 1 and policy['review_ref']
    assert row['finite_policy_fingerprint']
    assert len(policy['cases']) == 10
    radicands = set()
    fresh = set()
    for case in policy['cases']:
        signatures = {verify(KEYS[1], variant['problem'], variant['answer']) for variant in case['variants']}
        assert len(signatures) == 1
        signature = next(iter(signatures))
        assert signature[:2] == ('1', '0')
        radicand = -int(signature[2])
        assert radicand not in radicands
        radicands.add(radicand)
        assert case['role'] == ('teach_only' if radicand == 36 else 'practice_fresh')
        if case['role'] == 'practice_fresh':
            fresh.add(case['id'])
    assert radicands == {n*n for n in range(1, 11)}
    served = []
    for item in row['instances']:
        case = native_case(row, item['problem'], item['answer'])
        assert case and case['role'] == 'practice_fresh'
        assert item['native_finite_case'] == {'id':case['id'], 'role':case['role']}
        served.append(case['id'])
    assert len(served) == len(set(served)) == 9 and set(served) == fresh
    return served


def verify_factor_sketch(problem, sketch):
    # Require a mathematically valid displayed factorization, rather than a character quota.
    target = polynomial(problem).as_expr()
    chains = [text.split('=') for text in re.findall(r'\$([^$]+)\$', sketch) if '=' in text]
    assert any(len(chain) >= 2 and all(s.expand(expression(part)) == target for part in chain) for chain in chains)



def expression(text, evaluate=True):
    text = re.sub(r'(?<=\d)(?=x)', '*', text)
    return parse_expr(text.replace('^', '**'), local_dict={'x': X},
                      transformations=standard_transformations + (implicit_multiplication_application,), evaluate=evaluate)


def polynomial(problem):
    """Read mathematical input from prose/area/table; ignore all recipe parameters."""
    if 'nonzero coefficients' in problem:
        coefficients = re.findall(r'power \$(\d+)\$: coefficient \$([^$]+)\$', problem)
        assert coefficients
        result = sum(expression(value)*X**int(power) for power,value in coefficients)
    elif 'Correct the factorization' in problem:
        relation = next(text for text in re.findall(r'\$([^$]+)\$', problem) if '=' in text)
        result = expression(relation.split('=',1)[0])
    elif 'nonzero entries' in problem:
        match = re.search(r'nonzero entries \$([^$]+)\$ at powers \$([^$]+)\$', problem)
        assert match, problem
        coefficients = [expression(value) for value in match[1].split(',')]
        powers = [int(value) for value in match[2].split(',')]
        assert len(coefficients) == len(powers) and len(set(powers)) == len(powers)
        assert all(power >= 0 for power in powers) and all(value != 0 for value in coefficients)
        result = sum(value*X**power for value,power in zip(coefficients,powers,strict=True))
    elif 'coefficient row' in problem:
        match = re.search(r'coefficient row is \$\(([^$]+)\)\$', problem)
        assert match, problem
        values = [expression(value) for value in match[1].split(',')]
        assert len(values) == 3
        result = values[0]*X**2 + values[1]*X + values[2]
    elif 'coefficient table' in problem.lower():
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
        equations = [m.split('=',1)[1] for m in maths if re.match(r'^y\s*=', m)]
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


def verify(key, problem, answer, contract=None):
    poly = polynomial(problem)
    if key == KEYS[0]:
        assert 1 <= len(poly.terms()) <= 3
        expected = ['monomial','binomial','trinomial'][len(poly.terms())-1]
        assert answer == expected, (problem,answer,expected)
    elif key == KEYS[1]:
        if ';' in answer:
            parts = fields(answer)
            assert set(parts)=={'lower_factor','upper_factor'}
            lower,upper = [s.Poly(expression(parts[k]),X) for k in ('lower_factor','upper_factor')]
            assert lower.TC()<upper.TC()
        else:
            product = expression(answer)
            assert product.is_Mul and len(product.args) == 2, answer
            lower,upper = [s.Poly(factor,X) for factor in product.args]
        assert all(f.degree()==1 and f.LC()==1 for f in [lower,upper])
        assert lower*upper==poly, (problem,answer)
    elif key == KEYS[2]:
        factor_request = any(word in problem.lower() for word in (
            'factor it', 'complete factorization', 'factored pattern', 'factored form'))
        if factor_request:
            assert contract is None or contract['kind'] == 'exact'
            verify_complete_factorization(poly, answer)
        else:
            assert contract is None or contract['kind'] == 'label'
            assert answer == strategy(poly), (problem,answer,strategy(poly))
    else:
        verify_graph(key,poly,answer,problem,contract)
    return tuple(str(c) for c in poly.all_coeffs())


def verify_complete_factorization(poly, answer):
    product = expression(answer, evaluate=False)
    assert product.is_Mul or product.is_Pow, answer
    def multiplicative_factors(value):
        if value.is_Mul:
            for child in value.args:
                yield from multiplicative_factors(child)
        else:
            yield value
    factors = list(multiplicative_factors(product))
    degree = 0
    coefficient = s.Integer(1)
    for factor in factors:
        if factor.is_number:
            assert factor.is_Integer and factor != 0
            coefficient *= factor
            continue
        base, power = factor.as_base_exp()
        assert power.is_Integer and power > 0
        linear = s.Poly(base,X)
        assert linear.degree() == 1 and all(value.is_Integer for value in linear.all_coeffs())
        assert abs(s.gcd_list(linear.all_coeffs())) == 1
        degree += int(power)
    assert degree == poly.degree()
    assert abs(coefficient) == abs(s.gcd_list(poly.all_coeffs()))
    assert s.Poly(s.expand(product),X) == poly, (poly,answer)


def verify_graph(key, poly, answer, problem, contract=None):
    assert poly.degree()==2
    a,b,c = poly.all_coeffs()
    h = -b/(2*a)
    k = poly.eval(h)
    parts = fields(answer)
    assert parts['direction']==('upward' if a>0 else 'downward')
    # Reconstruct completed square and symmetry, without the recipe's root formula.
    assert s.expand(a*(X-h)**2+k)==poly.as_expr()
    if key == KEYS[3]:
        required = {'direction','extreme_value'}
        if contract is not None:
            assert contract['kind'] == 'multipart'
            assert {part['name'] for part in contract['parts']} == required
        assert set(parts)==required
        assert expression(parts['extreme_value'])==k
        return
    required = {'direction','vertex','left_intercept','right_intercept'}
    asks_y = 'y_intercept' in problem or re.search(r'\$?y\$?-intercept', problem)
    if asks_y:
        required.add('y_intercept')
    if contract is not None:
        assert contract['kind'] == 'multipart'
        assert {part['name'] for part in contract['parts']} == required
    assert set(parts)==required
    assert point(parts['vertex'])==(h,k)
    left,right = point(parts['left_intercept']),point(parts['right_intercept'])
    assert left[0]<right[0] and left[1]==right[1]==0
    assert poly.eval(left[0])==poly.eval(right[0])==0
    assert left[0]+right[0]==2*h
    if asks_y:
        assert point(parts['y_intercept'])==(0,c)


def verify_graph_sketch(key, problem, sketch):
    poly = polynomial(problem)
    a,b,c = poly.all_coeffs()
    if key == KEYS[3]:
        match = re.search(r'a\s*=\s*([+-]?\d+)\s*([<>])\s*0', sketch)
        assert match and s.Integer(match[1]) == a
        assert match[2] == ('>' if a > 0 else '<')
        assert 'vertex' in sketch and ('min' if a > 0 else 'max') in sketch
    else:
        chunks = re.findall(r'\$([^$]+)\$', sketch)
        assert chunks and s.Poly(s.expand(expression(chunks[0])),X) == poly
        axis = re.search(r'axis \$x\s*=\s*([^$]+)\$', sketch)
        ordinate = re.search(r'\$y\(([^)]+)\)\s*=\s*([^$]+)\$', sketch)
        assert axis and ordinate
        h = -b/(2*a)
        assert expression(axis[1]) == expression(ordinate[1]) == h
        assert expression(ordinate[2]) == poly.eval(h)


def perturbed(key, problem, answer):
    """Change the printed math while preserving the stored answer: it must fail."""
    poly = polynomial(problem).as_expr()
    if key == KEYS[0]:
        replacement = poly + X**5 if len(s.Poly(poly,X).terms())<3 else poly-s.Poly(poly,X).terms()[-1][1]*X**s.Poly(poly,X).terms()[-1][0][0]
    elif key == KEYS[2]:
        replacement = X**2-169 if answer!='difference of squares' else 3*poly
    else:
        replacement = poly+1
    suffix = ' Include y_intercept.' if ('y_intercept' in problem or re.search(r'\$?y\$?-intercept', problem)) else ''
    return f'For $y={replacement}$, solve the requested task.' + suffix


def diversity(signatures, answers, finite_cases=None):
    if finite_cases is None:
        assert len(signatures)>=12
    else:
        assert len(finite_cases) == len(set(finite_cases)) == len(signatures) == 9
    assert len(set(signatures))==len(signatures)
    counts = Counter(answers)
    total = len(answers)
    entropy = -sum((n/total)*math.log2(n/total) for n in counts.values())
    assert entropy>=1.5
    return entropy


def stored_problems(value, aliases=None, legacy_allowed=False):
    if isinstance(value,list):
        for item in value:
            yield from stored_problems(item, aliases, legacy_allowed)
    elif isinstance(value,dict):
        if aliases and value.get('kind') == 'template' and value.get('kp_id') in aliases:
            arguments = value.get('arguments')
            accepted = aliases[value['kp_id']]
            if arguments == accepted:
                return  # Exact same-candidate source alias; retain all other records.
            if legacy_allowed and value['kp_id'] == KEYS[2] and isinstance(arguments,dict) and 'answer_contract' not in arguments:
                digest = hashlib.sha256(json.dumps(arguments,sort_keys=True,separators=(',',':'),ensure_ascii=False).encode()).hexdigest()
                contract = {'kind':'label','options':[['GCF'],['difference of squares'],['trinomial factoring']]}
                if digest == '98ca0a4fc215b79f2beb0c39cd21558e91015bb03121d6c33dce1d01c57d9d92' and accepted.get('answer_contract') == contract:
                    if dict(arguments,answer_contract=contract) == accepted:
                        return  # Pinned legacy contract serialization of this same source candidate.
        if isinstance(value.get('problem'),str):
            yield value['problem']
        if isinstance(value.get('statement'),str):
            for sample in value.get('samples',[]):
                try:
                    yield value['statement'].format(**sample['params'])
                except (KeyError,ValueError):
                    continue
        for item in value.values():
            yield from stored_problems(item, aliases, legacy_allowed)


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
                if key == KEYS[1]:
                    verify_factor_sketch(row['problem'], row['solution_sketch'])
                elif key in KEYS[3:]:
                    verify_graph_sketch(key, row['problem'], row['solution_sketch'])
                else:
                    self.assertGreater(len(row['solution_sketch']),65)
                sig = verify(key,row['problem'],row['answer'],row.get('answer_contract'))
                self.assertNotIn(sig,signatures)
                signatures.add(sig)
                structures.add(re.sub(r'\d+','#',row['problem']))
            self.assertEqual(len(structures),4)

    def test_exhaustive_rendered_semantics_entropy_collisions_and_perturbations(self):
        self.assertEqual({r['kp_id'] for r in self.evidence},set(KEYS))
        seen = {}
        practice_seen = set()
        recipe_by_key = {recipe['kp_id']:recipe for recipe in self.recipes}
        for key in KEYS:
            for exemplar in self.by_key[key]['exemplars']:
                signature = (key,verify(key,exemplar['problem'],exemplar['answer']))
                seen.setdefault(signature,[]).append(exemplar['problem'])
        known_text = {e['problem'] for kp in self.facts['kps'] for e in kp['exemplars']}
        for directory in ['docs/content-foundations','docs/reports']:
            for path in (ROOT/directory).rglob('*.json'):
                if 'unit07-complement' not in path.parts:
                    allowed = SOURCE_ALIASES.get(path.relative_to(ROOT).as_posix(), set())
                    aliases = {key:recipe_by_key[key]['arguments'] for key in allowed}
                    legacy_allowed = path.relative_to(ROOT).as_posix() in LEGACY_CONTRACT_ALIAS_PATHS
                    known_text.update(stored_problems(json.loads(path.read_text()), aliases, legacy_allowed))
        for row in self.evidence:
            key = row['kp_id']
            self.assertEqual(row['status'],'pending')
            count = expected_count(recipe_by_key[key])
            finite_cases = validate_finite_cases(row)
            self.assertEqual(len(row['instances']),count)
            answers = Counter()
            for item in row['instances']:
                sig = verify(key,item['problem'],item['answer'])
                self.assertNotIn((key,sig),practice_seen)
                practice_seen.add((key,sig))
                for prior in seen.get((key,sig),[]):
                    self.assertTrue(permitted_finite_collision(row,item,prior), (key,item['problem'],prior))
                if item['problem'] in known_text:
                    self.assertTrue(permitted_finite_collision(row,item,item['problem']), (key,item['problem']))
                known_text.add(item['problem'])
                answers[item['answer']]+=1
                self.assertGreater(len(item['solution_sketch']),100)
                with self.assertRaises(AssertionError):
                    verify(key,perturbed(key,item['problem'],item['answer']),item['answer'])
            signatures = [verify(key,i['problem'],i['answer']) for i in row['instances']]
            entropy = diversity(signatures,[i['answer'] for i in row['instances']], finite_cases)
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
            self.assertEqual(len(actual),expected_count(row))
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

    def test_authored_answer_contract_negative_controls(self):
        count = 0
        for key in KEYS:
            for exemplar in self.by_key[key]['exemplars']:
                answer = exemplar['answer']
                if ';' in answer:
                    wrong = answer.replace('upward','WRONG').replace('downward','upward').replace('WRONG','downward')
                    if wrong == answer:
                        pairs = fields(answer)
                        wrong = '; '.join(f'{name} = ({value})+1' for name,value in pairs.items())
                elif key == KEYS[0] or (key == KEYS[2] and exemplar.get('answer_contract',{} ) and exemplar['answer_contract']['kind'] == 'label'):
                    wrong = 'monomial' if answer != 'monomial' else 'trinomial'
                else:
                    wrong = f'({answer})+1'
                with self.assertRaises((AssertionError,KeyError)):
                    verify(key,exemplar['problem'],wrong,exemplar.get('answer_contract'))
                count += 1
        self.assertEqual(count,20)

    def test_source_alias_provenance_negative_controls(self):
        recipe = next(row for row in self.recipes if row['kp_id'] == KEYS[2])
        accepted = recipe['arguments']
        legacy = json.loads(json.dumps(recipe))
        del legacy['arguments']['answer_contract']
        allowed_paths = {
            'docs/content-foundations/teach-reviews/rational-final-55/remaining-55-finite7.instruction-gate.input-v2.json',
            'docs/content-foundations/hint-reviews/exponents-remaining-17/radical17.instruction-gate.input-v2.json',
        }
        for path in allowed_paths:
            self.assertIn(KEYS[2],SOURCE_ALIASES[path])
            self.assertIn(path, LEGACY_CONTRACT_ALIAS_PATHS)
            self.assertEqual(list(stored_problems(legacy,{KEYS[2]:accepted},legacy_allowed=path in LEGACY_CONTRACT_ALIAS_PATHS)),[])
        primary = 'docs/content-foundations/whole-course-teach/inputs/templates.json'
        self.assertIn(KEYS[2], SOURCE_ALIASES[primary])
        self.assertNotIn(primary, LEGACY_CONTRACT_ALIAS_PATHS)
        self.assertTrue(list(stored_problems(legacy,{KEYS[2]:accepted},legacy_allowed=primary in LEGACY_CONTRACT_ALIAS_PATHS)))
        self.assertEqual(list(stored_problems(recipe,{KEYS[2]:accepted},legacy_allowed=False)),[])
        self.assertTrue(list(stored_problems(legacy,{})))
        changed = json.loads(json.dumps(legacy))
        changed['arguments']['statement'] += ' Changed assessment.'
        self.assertTrue(list(stored_problems(changed,{KEYS[2]:accepted},legacy_allowed=True)))
        changed = json.loads(json.dumps(legacy))
        changed['arguments']['answer_contract'] = {'kind':'exact'}
        self.assertTrue(list(stored_problems(changed,{KEYS[2]:accepted},legacy_allowed=True)))
        changed = json.loads(json.dumps(legacy))
        changed['arguments']['samples'][0]['params']['a'] = 99
        self.assertTrue(list(stored_problems(changed,{KEYS[2]:accepted},legacy_allowed=True)))

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
