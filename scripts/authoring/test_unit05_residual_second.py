#!/usr/bin/env python3
"""Exact, prompt-derived semantic checks for the second Unit05 checkpoint."""
from fractions import Fraction as Q
from itertools import product
import json
from pathlib import Path
import re
import sys
import unittest

from test_unit05_residual import linear, scalar, signature, verify as verify_system

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT/'scripts/review'))
from foundations_content_audit import pending_templates
KEYS = {f'{topic}/kp{i}' for topic in (
    'systems-elimination', 'systems-money-problems', 'systems-rate-problems')
    for i in range(1, 4)}


def numbers(text):
    return [Q(n) for n in re.findall(r'(?<![\w])\d+(?:/\d+)?', text)]


def model(key, problem):
    """Translate quantities in the actual prompt into two independent equations."""
    if key.startswith('systems-elimination/'):
        equations = [s for s in re.findall(r'\$([^$]+)\$', problem) if '=' in s]
        assert len(equations) == 2
        return [linear(eq) for eq in equations], 'integer'
    n = numbers(problem)
    if key in {'systems-money-problems/kp1', 'systems-money-problems/kp3'}:
        p, q, count, value = n
        assert 'respectively' in problem and 'total value' in problem
        return [(1, 1, count), (p, q, value)], 'count'
    if key == 'systems-money-problems/kp2':
        a, b, c, d, e, f = n
        assert 'fixed unit prices' in problem and 'unit prices (first, second)' in problem
        return [(a, b, c), (d, e, f)], 'price'
    if key == 'systems-rate-problems/kp1':
        v, w = n
        if 'flight log' in problem:
            v, w = w, v
        if 'metres/min' in problem:
            v, w = v*60/1000, w*60/1000
        return [(1, 1, v), (1, -1, w)], 'current'
    if key == 'systems-rate-problems/kp2':
        d, t, e, u = n
        if 'upstream leg' in problem:
            d, t, e, u = e, u, d, t
        if 'minutes' in problem:
            t, u = t/60, u/60
        assert 'stay constant' in problem
        assert d % t == e % u == 0, 'Distance/time must divide evenly'
        return [(1, 1, d/t), (1, -1, e/u)], 'current'
    if key == 'systems-rate-problems/kp3':
        d, t, delta = n[:3]
        assert 'toward each other' in problem
        if 'minutes' in problem:
            t /= 60
        if 'covers' in problem:
            assert n[3] == t
            delta /= t
        if 'times the slower' in problem:
            return [(1, 1, d/t), (1, -delta, 0)], 'travel'
        return [(1, 1, d/t), (1, -1, delta)], 'travel'
    raise AssertionError(key)


def solved(rows):
    (a, b, c), (d, e, f) = rows
    determinant = a*e-b*d
    assert determinant, 'System must have a unique solution'
    return Q(c*e-b*f, determinant), Q(a*f-c*d, determinant)


def canonical_rows(rows):
    result = []
    for row in rows:
        pivot = next(v for v in row if v)
        result.append(tuple(Q(v, pivot) for v in row))
    return tuple(sorted(result))


def verify(key, problem, answer, sketch=None):
    rows, kind = model(key, problem)
    x, y = solved(rows)
    supplied = tuple(scalar(s, {}) for s in answer.strip('() ').split(','))
    assert supplied == ((x,) if key == 'systems-money-problems/kp3' else (x, y)), (problem, answer)
    if kind in {'integer', 'count'}:
        assert x.denominator == y.denominator == 1
    if kind == 'count':
        assert x >= 0 and y >= 0
    elif kind in {'price', 'current', 'travel'}:
        assert x > 0 and y > 0
    if kind in {'current', 'travel'}:
        assert x > y
    if sketch is not None:
        assert len(sketch) >= 90 and 'Check:' in sketch
        for math in re.findall(r'\$([^$]+)\$', sketch):
            if '=' in math:
                values = [scalar(s, {'x': x, 'y': y}) for s in math.split('=')]
                assert len(set(values)) == 1, (key, math)
    return canonical_rows(rows), supplied


class SecondCheckpointTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.facts = json.loads(Path(FACTS).read_text())
        cls.kps = {r['kp_key']: r for r in cls.facts['kps']}
        cls.recipes = json.loads((ROOT/'docs/content-foundations/unit05-residual/templates-second.json').read_text())

    def test_scope_and_authored_semantics(self):
        self.assertEqual({r['kp_id'] for r in self.recipes}, KEYS)
        for key in KEYS:
            rows = self.kps[key]['exemplars']
            self.assertEqual(len(rows), 4)
            signatures = set()
            for row in rows:
                sig, _ = verify(key, row['problem'], row['answer'], row['solution_sketch'])
                self.assertTrue(row['authored_answer_decidable'])
                self.assertNotIn(sig, signatures)
                signatures.add(sig)

    def test_exhaustive_semantics_and_active_parameters(self):
        seen = self.authored_signatures()
        for recipe in self.recipes:
            key, args = recipe['kp_id'], recipe['arguments']
            self.assertEqual(recipe['status'], 'pending')
            self.assertNotIn('space_size', args)
            self.assertEqual(args['constraints'], [])
            domains = {k: v['values'] for k, v in args['params'].items()}
            self.assertEqual(set(domains), {'a', 'b'})
            tuples = set(product(domains['a'], domains['b']))
            samples = {(r['params']['a'], r['params']['b']): r for r in args['samples']}
            self.assertEqual(set(samples), tuples)
            self.assertEqual(len(samples), 12)
            outputs = {}
            for params, sample in samples.items():
                problem = args['statement'].format(**sample['params'])
                sig, value = verify(key, problem, sample['expected'])
                self.assertNotIn(sig, seen, (key, seen.get(sig), problem))
                seen[sig] = key
                outputs[params] = value
            self.assertEqual(len(set(outputs.values())), 12, key)
            for a, b in tuples:
                for other in domains['a']:
                    if other != a:
                        self.assertNotEqual(outputs[a, b], outputs[other, b])
                for other in domains['b']:
                    if other != b:
                        self.assertNotEqual(outputs[a, b], outputs[a, other])
        self.check_other_pending_systems(seen)

    def check_other_pending_systems(self, seen):
        checked = 0
        for key, recipes in pending_templates(ROOT/'docs/content-foundations').items():
            if key in KEYS:
                continue
            for recipe in recipes:
                args = recipe['document'].get('arguments', {})
                for sample in args.get('samples', []):
                    try:
                        problem = args['statement'].format(**sample['params'])
                        sig = signature(problem)
                    except (AssertionError, SyntaxError, ValueError, KeyError, StopIteration, ZeroDivisionError):
                        continue
                    self.assertNotIn(seen.get(sig), KEYS, (key, seen.get(sig)))
                    checked += 1
        self.assertGreaterEqual(checked, 96)

    def authored_signatures(self):
        seen = {}
        for key, row in self.kps.items():
            for exemplar in row['exemplars']:
                if key in KEYS:
                    sig, _ = verify(key, exemplar['problem'], exemplar['answer'])
                else:
                    try:
                        sig = signature(exemplar['problem'])
                    except (AssertionError, SyntaxError, ValueError, KeyError, StopIteration, ZeroDivisionError):
                        continue
                if sig in seen and (key in KEYS or seen[sig] in KEYS):
                    self.fail(f'Authored cross-KP collision: {key}, {seen[sig]}')
                seen[sig] = key
        # Previous checkpoint recipes are part of the collision boundary.
        previous = json.loads((ROOT/'docs/content-foundations/unit05-residual/templates.json').read_text())
        for recipe in previous:
            args = recipe['arguments']
            for sample in args['samples']:
                sig = signature(args['statement'].format(**sample['params']))
                self.assertNotIn(sig, seen, (recipe['kp_id'], seen.get(sig)))
                seen[sig] = recipe['kp_id']
        return seen

    def test_negative_controls_on_every_family(self):
        for recipe in self.recipes:
            key, args = recipe['kp_id'], recipe['arguments']
            sample = args['samples'][0]
            problem = args['statement'].format(**sample['params'])
            answer = sample['expected']
            for bad in ['999', '(999,999)', '(0,0)']:
                with self.assertRaises(AssertionError):
                    verify(key, problem, bad)
            # Mutate actual problem data while retaining the otherwise valid answer.
            mutated = re.sub(r'\d+', lambda m: str(int(m[0])+1), problem, count=1)
            if key.startswith('systems-elimination/'):
                mutated = re.sub(r'=(\d+)', lambda m: '='+str(int(m[1])+1), problem, count=1)
            with self.assertRaises(AssertionError):
                verify(key, mutated, answer)
            with self.assertRaises(AssertionError):
                verify(key, problem, answer, 'Use the appropriate rule.')
        with self.assertRaises(AssertionError):
            verify('systems-money-problems/kp1',
                   'Items have values 7 and 4 euros, respectively. There are 3 items with total value 30 euros.', '(6,-3)')
        with self.assertRaises(AssertionError):
            verify('systems-rate-problems/kp1', 'The ground speeds are 8 and 12 km/h.', '(10,-2)')
        with self.assertRaises(AssertionError):
            verify_system('Solve $x=4$ and $x=4$.', '(4,0)')


if __name__ == '__main__':
    FACTS = sys.argv.pop(1)
    unittest.main()
