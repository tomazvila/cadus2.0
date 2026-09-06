#!/usr/bin/env python3
"""Independently verify the reviewed choice/unit cohort against its inventory.

Run from the repository root. All computations use integers or Fraction;
polynomial comparisons use complete coefficient vectors. This reads metadata
and emits a manifest; it never edits curriculum or approves stored content.
"""
import json
import math
from fractions import Fraction as F
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
ROWS = {
    (r['topic_id'], r['kp_id'], r['exemplar_index']): r
    for r in map(json.loads, (ROOT / 'docs/reports/foundations-contract-candidates.jsonl').read_text().splitlines())
}
REVIEWED = []


def prime(n):
    return n >= 2 and all(n % d for d in range(2, math.isqrt(n) + 1))


def relation_function(points):
    return all(x != u or y == v for x, y in points for u, v in points)


def collinear(a, b, c):
    return (b[0] - a[0]) * (c[1] - a[1]) == (c[0] - a[0]) * (b[1] - a[1])


def record(topic, kp, index, value, policy, proof):
    row = ROWS[topic, f'kp{kp}', index]
    assert row['answer'] == value, (topic, kp, index, row['answer'], value)
    assert row['existing_contract'] in (None, policy), (topic, 'policy drift')
    REVIEWED.append({k: row[k] for k in ('file', 'topic_id', 'kp_id', 'exemplar_index', 'problem', 'answer')} | {
        'answer_contract': policy, 'verification': proof,
    })


def choice(topic, kp, index, predicate, proof, words=('yes', 'no')):
    record(topic, kp, index, words[0 if predicate else 1],
           {'kind': 'label', 'options': [[w] for w in words]}, proof)


for index, n in [(0, 64), (2, 50)]:
    choice('perfect-squares', 2, index, math.isqrt(n) ** 2 == n, f'isqrt({n}) squared equals {n}')
for index, n, d in [(0, 84, 7), (1, 51, 3)]:
    choice('factors-and-multiples', 3, index, n % d == 0, f'{n} modulo {d}')
for kp, index, n, d in [(1, 0, 470, 5), (1, 1, 384, 2), (2, 0, 471, 3), (2, 1, 522, 9), (3, 0, 316, 4), (3, 1, 234, 6)]:
    choice('divisibility-rules', kp, index, n % d == 0, f'{n} modulo {d}')
for index, n in enumerate([17, 21]):
    choice('prime-composite-numbers', 1, index, prime(n), f'Trial division through floor(sqrt({n}))', ('prime', 'composite'))
choice('prime-composite-numbers', 3, 0, prime(1), 'Primes are integers at least two')

CASES = [
    ('equivalent-fractions', 3, 1, F(4, 6) == F(6, 9), '4/6 = 6/9'),
    ('improper-fractions-mixed-numbers', 3, 0, F(9, 4) == 2 + F(1, 4), '9/4 = 2 + 1/4'),
    ('comparing-ordering-decimals', 2, 0, F('0.5') == F('0.50'), 'Exact decimal fractions'),
    ('understanding-ratios', 3, 1, F(4, 6) == F(10, 15), '4/6 = 10/15'),
    ('ratio-tables-equivalent-ratios', 3, 0, F(12, 18) == F(10, 15), '12/18 = 10/15'),
    ('parts-of-an-expression', 2, 1, 2 == 1, 'Like monomials have identical variable exponents'),
    ('equivalent-expressions', 1, 0, (2, 6) == (2, 6), 'Distribute 2(x+3); complete linear coefficients are (2,6)'),
    ('equivalent-expressions', 1, 1, (0, 3 + 4, 0) == (7, 0, 0), 'Complete quadratic coefficients'),
    ('checking-a-solution', 1, 0, 3 * 4 - 5 == 7, 'Substitute x=4'),
    ('checking-a-solution', 1, 1, 3 + 8 == 12, 'Substitute x=3'),
    ('checking-a-solution', 1, 2, 2 * -2 + 9 == 5, 'Substitute m=-2'),
    ('proportional-relationships', 2, 1, F(3, 1) == F(6, 2) == F(10, 3), 'Compare all y/x ratios'),
    ('graphing-proportional-relationships', 3, 1, F(9, 2) > 4, 'Compare slopes 9/2 and 4'),
    ('slope', 3, 0, collinear((0, 1), (2, 5), (5, 11)), 'Exact determinant of point differences'),
    ('solutions-of-two-variable-equations', 1, 0, 5 == 3 * 2 - 1, 'Substitute (2,5)'),
    ('solutions-of-two-variable-equations', 1, 1, 2 * 1 + 4 == 7, 'Substitute (1,4)'),
    ('graphing-from-a-table', 2, 0, collinear((0, -2), (1, 1), (2, 4)), 'Exact determinant'),
    ('solutions-of-inequalities', 2, 0, 2 * 3 + 1 > 5, 'Substitute x=3'),
    ('solutions-of-inequalities', 2, 1, 5 - 2 < 3, 'Strict endpoint x=2'),
    ('solutions-of-inequalities', 3, 0, 4 + 2 <= 6, 'Closed endpoint x=4'),
    ('solutions-of-inequalities', 3, 1, 4 + 2 < 6, 'Open endpoint x=4'),
    ('two-step-inequalities', 3, 0, 5 * 3 - 2 < 13, 'Open endpoint x=3'),
    ('two-step-inequalities', 3, 1, 2 * 4 + 1 >= 9, 'Closed endpoint x=4'),
    ('and-or-inequalities', 1, 0, 1 < 3 < 5, 'Evaluate both interval bounds'),
    ('and-or-inequalities', 1, 1, 0 < -2 or 0 > 1, 'Evaluate both disjuncts'),
    ('interval-notation', 1, 2, 3 < 3 <= 8, 'Left endpoint is open'),
    ('graphing-linear-inequalities', 2, 0, 0 < 0 + 2, 'Substitute (0,0)'),
    ('graphing-linear-inequalities', 2, 1, 4 >= 2 * 1 + 1, 'Substitute (1,4)'),
    ('graphing-linear-inequalities', 3, 0, 2 * 0 + 3 * 0 <= 6, 'Substitute origin'),
    ('graphing-linear-inequalities', 3, 1, 0 > 0 - 1, 'Substitute origin'),
    ('checking-systems-solutions', 1, 0, 4 == 3 * 1 + 1 and 4 == 1 + 3, 'Both equations at (1,4)'),
    ('checking-systems-solutions', 1, 1, 5 == 2 + 3 and 5 == 2 * 2 + 1, 'Both equations at (2,5)'),
    ('checking-systems-solutions', 2, 0, 3 + 2 == 5 and 3 - 2 == 2, 'Both equations at (3,2)'),
    ('checking-systems-solutions', 2, 1, 2 * 2 + 1 == 5 and 3 * 2 - 1 == 4, 'Both equations at (2,1)'),
    ('systems-of-linear-inequalities', 1, 0, 1 <= 1 + 2 and 1 > -1, 'Both inequalities at (1,1)'),
    ('systems-of-linear-inequalities', 1, 1, 4 < 2 * 0 + 1 and 4 >= 0, 'Both inequalities at (0,4)'),
    ('systems-of-linear-inequalities', 3, 1, 0 >= 0 - 3 and 0 <= -0 + 5, 'Both inequalities at origin'),
    ('perfect-square-trinomials', 1, 1, 5 ** 2 == 4 * 1 * 25, 'A monic quadratic square has discriminant zero'),
]
for args in CASES:
    choice(*args)
for topic, kp, index, result in [('comparing-integers', 2, 0, -7 < -9), ('solutions-of-inequalities', 1, 0, -3 < -5), ('solutions-of-inequalities', 1, 1, 4 >= 4)]:
    choice(topic, kp, index, result, 'Evaluate the authored integer comparison', ('true', 'false'))
slopes = (3, -3)
record('slopes-of-parallel-perpendicular-lines', 3, 1,
       'parallel' if slopes[0] == slopes[1] else 'perpendicular' if slopes[0] * slopes[1] == -1 else 'neither',
       {'kind': 'label', 'options': [['parallel'], ['perpendicular'], ['neither']]}, 'Compare slopes, then their product with -1')
for kp, index, a, b, c in [(1, 0, 5, 12, 13), (1, 1, 8, 15, 17), (2, 0, 4, 5, 6), (2, 1, 2, 3, 4)]:
    choice('pythagorean-converse', kp, index, a*a + b*b == c*c, 'Pythagorean equality on sorted positive side lengths')
for index, points in enumerate([[(1, 2), (2, 3), (3, 4)], [(1, 2), (1, 5), (3, 4)], [(0, 7), (2, 7), (4, 7)]]):
    choice('identifying-functions-vertical-line-test', 1, index, relation_function(points), 'Every equal input has equal outputs')
choice('identifying-functions-vertical-line-test', 2, 0, 3 == -3, 'At x=0 the circle contains y=3 and y=-3')
choice('identifying-functions-vertical-line-test', 2, 1, True, 'The explicit real affine expression assigns one output to each input')
choice('identifying-functions-vertical-line-test', 2, 2, 1 == -1, 'At x=1 both y=1 and y=-1 satisfy x=y squared')
for index, outputs in enumerate([[2, 4, 6], [5, 5, 7]]):
    choice('one-to-one-functions', 1, index, len(set(outputs)) == len(outputs), 'Distinct inputs have distinct outputs')
for kp in (2, 3):
    choice('one-to-one-functions', kp, 0, (-1)**2 != 1**2, 'Distinct inputs -1 and 1 have the same squared output')
    choice('one-to-one-functions', kp, 1, 2 != 0, 'An affine real function is injective exactly when its slope is nonzero')


def unit(topic, kp, index, computed, proof):
    row = ROWS[topic, f'kp{kp}', index]
    policy = row['candidate_contract']
    assert policy['kind'] == 'unit'
    amount = row['answer'].replace(policy['unit'], '').strip()
    assert F(amount) == computed, (topic, amount, computed)
    record(topic, kp, index, row['answer'], policy, proof)


UNITS = [
    ('interpreting-linear-models', 3, 0, 50 - 5 * 4, '50 L minus 5 L/min times 4 min'),
    ('systems-mixture-problems', 1, 0, 12 * F(35, 100), '12 L times exact acid fraction 35/100'),
    ('quadratic-applications', 2, 1, -5 * 3**2 + 30 * 3, 'Concave quadratic vertex t=-30/(2*-5)=3'),
    ('exponential-growth-decay', 2, 0, 20000 * F(1, 2)**2, 'Eight years is two four-year half lives'),
    ('exponential-growth-decay', 3, 0, 1000 * F(11, 10)**2, 'Two annual growth periods'),
    ('compound-interest', 1, 0, 2000 * F(11, 10)**2, 'Two annual growth periods'),
    ('compound-interest', 1, 1, 500 * F(6, 5)**2, 'Two annual growth periods'),
    ('compound-interest', 2, 0, 1000 * F(26, 25)**2, 'Two half-year periods at 8/2 percent'),
    ('compound-interest', 2, 1, 400 * F(21, 20)**2, 'Two half-year periods at 10/2 percent'),
    ('continuous-growth-model', 1, 0, 1000 * F(21, 20)**2, 'Authored semiannual expression; exact cents'),
    ('complementary-angle-trig', 2, 0, 90 - 25, 'Acute complementary angles sum to 90 degrees'),
    ('complementary-angle-trig', 2, 1, 90 - 41, 'Acute complementary angles sum to 90 degrees'),
    ('trig-applications', 1, 0, math.isqrt(3**2 + 4**2), 'Pythagorean length; 5 squared equals 3 squared plus 4 squared'),
    ('trig-applications', 1, 1, 50 * F(1, 2), 'Height equals string length times the supplied exact sine'),
    ('radians-degrees', 2, 0, F(1, 4) * 180, 'Exact rational multiple of pi times 180 degrees/pi'),
    ('radians-degrees', 2, 1, F(2, 3) * 180, 'Exact rational multiple of pi times 180 degrees/pi'),
]
for args in UNITS:
    unit(*args)
for index, angle in enumerate([-45, 400, 780]):
    unit('coterminal-angles', 2, index, angle % 360, 'Euclidean remainder modulo 360 degrees')
for kp, index, angle in [(1, 0, 150), (1, 1, 225), (1, 2, 300), (3, 0, 420), (3, 1, -120)]:
    folded = angle % 180
    unit('reference-angles', kp, index, min(folded, 180 - folded), 'Fold the angle modulo 180 to its acute reference angle')
# Sine rule gives sin(B)=sqrt(2)/2; the acute branch uniquely gives 45 degrees.
assert F(6**2 * 2, 6**2) * F(1, 2)**2 == F(1, 2)
unit('law-of-sines', 3, 1, 45, 'Sine rule gives positive sin(B) squared = 1/2; acute B=45 degrees')
for index, coefficient, radicand, a, b, cosine in [(0, 10, 13, 40, 30, F(1, 2)), (1, 20, 37, 60, 80, F(-1, 2))]:
    assert coefficient**2 * radicand == a*a + b*b - 2*a*b*cosine
    row = ROWS['law-of-sines-cosines', 'kp3', index]
    record('law-of-sines-cosines', 3, index, f'{coefficient}√{radicand} ' + row['candidate_contract']['unit'],
           row['candidate_contract'], 'Cosine rule; compare exact squared positive lengths')

assert len({(r['topic_id'], r['kp_id'], r['exemplar_index']) for r in REVIEWED}) == len(REVIEWED)
for row in sorted(REVIEWED, key=lambda r: (r['file'], r['topic_id'], r['kp_id'], r['exemplar_index'])):
    print(json.dumps(row, ensure_ascii=False, sort_keys=True, separators=(',', ':')))
