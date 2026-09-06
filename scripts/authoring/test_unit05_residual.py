#!/usr/bin/env python3
"""Independent exact-arithmetic checks; pass fresh content_audit_facts JSON."""
import ast
from fractions import Fraction as Q
from itertools import product
import json
from pathlib import Path
import re
import sys
import unittest

ROOT = Path(__file__).resolve().parents[2]
KEYS = {
    'substitution-with-isolated-variable/kp1',
    'substitution-with-isolated-variable/kp3',
    'systems-substitution/kp1', 'systems-substitution/kp2', 'systems-substitution/kp3',
    'elimination-with-addition/kp1', 'elimination-with-addition/kp2',
    'elimination-with-addition/kp3',
}


def scalar(text, values):
    """Evaluate only rational arithmetic, independently of the Rust evaluator."""
    text = re.sub(r'(\d|\))(?=[xy(])', r'\1*', text.replace(' ', ''))
    return node_value(ast.parse(text, mode='eval').body, values)


def node_value(node, values):
    if isinstance(node, ast.Constant) and type(node.value) is int:
        return Q(node.value)
    if isinstance(node, ast.Name):
        return Q(values[node.id])
    if isinstance(node, ast.UnaryOp):
        value = node_value(node.operand, values)
        if isinstance(node.op, ast.USub):
            return -value
        if isinstance(node.op, ast.UAdd):
            return value
    if isinstance(node, ast.BinOp):
        a, b = node_value(node.left, values), node_value(node.right, values)
        if isinstance(node.op, ast.Add):
            return a + b
        if isinstance(node.op, ast.Sub):
            return a - b
        if isinstance(node.op, ast.Mult):
            return a * b
        if isinstance(node.op, ast.Div):
            return a / b
    raise ValueError(f'Unsupported arithmetic: {ast.dump(node)}')


def equations(problem):
    result = [s for s in re.findall(r'\$([^$]+)\$', problem) if '=' in s]
    assert len(result) == 2, problem
    assert all(s.count('=') == 1 for s in result), problem
    return result


def linear(equation):
    left, right = equation.split('=')
    def residual(x, y):
        return scalar(left, {'x': x, 'y': y}) - scalar(right, {'x': x, 'y': y})
    c = residual(0, 0)
    a, b = residual(1, 0) - c, residual(0, 1) - c
    assert residual(2, 3) == 2*a + 3*b + c
    return a, b, -c


def signature(problem):
    rows = []
    for eq in equations(problem):
        row = linear(eq)
        pivot = next(v for v in row if v)
        rows.append(tuple(v/pivot for v in row))
    return tuple(sorted(rows))


def verify(problem, answer, sketch=None):
    pair = [scalar(t, {}) for t in answer.strip('() ').split(',')]
    assert len(pair) == 2
    x, y = pair
    rows = [linear(eq) for eq in equations(problem)]
    (a, b, _), (d, e, _) = rows
    assert a*e != b*d, 'System must have a unique solution'
    assert all(a*x+b*y == c for a, b, c in rows), (problem, answer)
    # Reject statements that directly supply either requested coordinate.
    assert not any(re.fullmatch(r'\s*[xy]\s*=\s*[-+]?\d+\s*', eq)
                   for eq in equations(problem)), problem
    if sketch is not None:
        assert len(sketch) >= 70 and 'Check:' in sketch
        for math in re.findall(r'\$([^$]+)\$', sketch):
            if '=' in math:
                parts = [scalar(part, {'x': x, 'y': y}) for part in math.split('=')]
                assert len(set(parts)) == 1, (problem, math)
    return tuple(pair)


class Unit05SemanticTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.facts = json.loads(Path(FACTS).read_text())
        cls.rows = {r['kp_key']: r for r in cls.facts['kps']}
        cls.recipes = json.loads((ROOT/'docs/content-foundations/unit05-residual/templates.json').read_text())

    def test_scope_and_four_independent_authored_examples(self):
        self.assertEqual({r['kp_id'] for r in self.recipes}, KEYS)
        for key in KEYS:
            exemplars = self.rows[key]['exemplars']
            self.assertEqual(len(exemplars), 4, key)
            structures = set()
            for row in exemplars:
                verify(row['problem'], row['answer'], row['solution_sketch'])
                self.assertTrue(row['authored_answer_decidable'])
                structure = re.sub(r'\d+', '#', row['problem'])
                self.assertNotIn(structure, structures, key)
                structures.add(structure)

    def test_exhaustive_meaningful_domains_and_no_collisions(self):
        seen = self.authored_signatures()
        for row in self.recipes:
            self.assertEqual(row['status'], 'pending')
            args = row['arguments']
            self.assertNotIn('space_size', args)
            self.assertEqual(args['constraints'], [])
            domains = {k: v['values'] for k, v in args['params'].items()}
            self.assertEqual(set(domains), {'a', 'b'})
            wanted = set(product(domains['a'], domains['b']))
            samples = {(s['params']['a'], s['params']['b']): s for s in args['samples']}
            self.assertEqual(set(samples), wanted)
            self.assertEqual(len(wanted), 12)
            outputs = {}
            for params, sample in samples.items():
                problem = args['statement'].format(**sample['params'])
                outputs[params] = verify(problem, sample['expected'])
                sig = signature(problem)
                self.assertNotIn(sig, seen, (row['kp_id'], seen.get(sig), problem))
                seen[sig] = row['kp_id']
            self.assertEqual(len(set(outputs.values())), 12)
            for a, b in wanted:
                for other in domains['a']:
                    if other != a:
                        self.assertNotEqual(outputs[a, b], outputs[other, b])
                for other in domains['b']:
                    if other != b:
                        self.assertNotEqual(outputs[a, b], outputs[a, other])

    def authored_signatures(self):
        seen = {}
        # Compare with every parseable two-equation exemplar in Foundations.
        for key, row in self.rows.items():
            for exemplar in row['exemplars']:
                try:
                    sig = signature(exemplar['problem'])
                except (AssertionError, SyntaxError, ValueError, KeyError, StopIteration, ZeroDivisionError):
                    if key in KEYS:
                        raise
                    continue
                if sig in seen and (key in KEYS or seen[sig] in KEYS):
                    self.fail(f'Cross-KP/authored collision: {key}, {seen[sig]}')
                seen[sig] = key
        return seen

    def test_semantic_negative_controls(self):
        sample = self.recipes[0]['arguments']['samples'][0]
        problem = self.recipes[0]['arguments']['statement'].format(**sample['params'])
        for bad in ['(0,0)', '(999,999)', '(1,-1)']:
            with self.assertRaises(AssertionError):
                verify(problem, bad)
        # Correct answers to a different system must fail against the actual prompt.
        with self.assertRaises(AssertionError):
            verify(problem.replace('2x', '3x'), sample['expected'])
        with self.assertRaises(AssertionError):
            verify('Solve $x=4$ and $x=4$.', '(4,0)')
        with self.assertRaises(AssertionError):
            verify('Solve $x=4$ and $y=3$.', '(4,3)')
        with self.assertRaises(AssertionError):
            verify(problem, sample['expected'], 'Use the appropriate rule.')
        self.assertEqual(signature('Solve $2x+y=7$ and $x-y=2$.'),
                         signature('Solve $2x-2y=4$ and $4x+2y=14$.'))


if __name__ == '__main__':
    FACTS = sys.argv.pop(1)
    unittest.main()
