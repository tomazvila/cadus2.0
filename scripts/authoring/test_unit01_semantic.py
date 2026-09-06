"""Unit01 boundary regressions and compatibility with the actual draft importer."""
import json
import unittest
from fractions import Fraction as F
from math import gcd
from pathlib import Path
from generate_unit01_templates import drafts
from import_local_drafts import load_document, validate

ROOT = Path('docs/content-foundations/fractions-decimals')


def pending():
    return [r for p in (ROOT/'templates').glob('*.json') for r in json.loads(p.read_text())]


def terminates_within(value, places):
    return (value * 10**places).denominator == 1


class Unit01SemanticTests(unittest.TestCase):
    def test_manifest_is_accepted_by_real_importer_without_execution(self):
        document, rows = load_document(ROOT/'manifest.json')
        validated = validate(document, rows)
        self.assertEqual(len(validated), 119)
        self.assertEqual(sum(kind == 'template' for _, kind in validated), 79)

    def test_committed_templates_are_exact_outputs_of_reviewed_recipes(self):
        generated = {r['kp_id']: r for r in drafts()}
        self.assertEqual(len(generated), 79)
        self.assertEqual({r['kp_id'] for r in pending()}, set(generated))
        for row in pending():
            self.assertEqual(row, generated[row['kp_id']])

    def test_all_fraction_operands_meet_the_restrictive_kp_boundaries(self):
        checks = {
            'fraction-basics/kp1': lambda a,b: 2 <= b <= 12 and 0 < a < b,
            'fraction-basics/kp2': lambda a,b: a+b <= 30,
            'fraction-basics/kp3': lambda a,b: 2 <= min(a,b) <= max(a,b) <= 10,
            'fractions-on-number-line/kp1': lambda a,b: 2 <= b <= 10 and 0 < b-a < b,
            'fractions-on-number-line/kp2': lambda a: a > 3 and a % 3 != 0,
            'fractions-on-number-line/kp3': lambda a,b: 2 <= b <= 10 and a.denominator == 1,
            'equivalent-fractions/kp1': lambda a,b: 2 <= b <= 5,
            'equivalent-fractions/kp2': lambda a: 0 < a <= 30 and gcd(int(a),30) > 1,
            'equivalent-fractions/kp3': lambda a: 0 < a <= 60 and gcd(int(a),60) > 1,
            'adding-subtracting-like-fractions/kp1': lambda a,b: 0 < a+b < 11 and gcd(int(a+b),11) == 1,
            'adding-subtracting-like-fractions/kp2': lambda a,b: 0 < b < a <= 12,
            'adding-subtracting-like-fractions/kp3': lambda a,b: a+b > 6,
            'fraction-of-a-number/kp1': lambda a: a % 2 == 0,
            'fraction-of-a-number/kp2': lambda a: a % 4 == 0,
            'fraction-of-a-number/kp3': lambda a: a <= 60 and a % 2 == 0,
            'multiplying-fractions/kp1': lambda a,b: max(a,b) <= 9 and gcd(int(a*b),63) == 1,
            'multiplying-fractions/kp2': lambda a,b: max(a,b) <= 12 and gcd(int(a*b),48) > 1,
            'multiplying-fractions/kp3': lambda a,b: max(a,b) <= 12 and gcd(int(a),6) > 1,
            'improper-fractions-mixed-numbers/kp1': lambda a: 1 <= a//5 <= 9 and a % 5 != 0,
            'improper-fractions-mixed-numbers/kp2': lambda a,b: 1 <= a <= 9 and 0 < b < 5,
            'mixed-numbers/kp2': lambda a,b: b > 1 and a+F(1,6) > 1+b/6,
            'fraction-word-problems/kp2': lambda a: a % 4 == 0,
        }
        self.check_boundaries(checks)

    def test_decimal_percent_and_ratio_operand_contracts(self):
        checks = {
            'decimal-multiplication-powers-of-ten/kp1': lambda a: terminates_within(a,2),
            'decimal-multiplication-powers-of-ten/kp2': lambda a: terminates_within(a/100,3),
            'decimal-operations/kp1': lambda a: terminates_within(a,1),
            'decimal-operations/kp2': lambda a: terminates_within(a/4,2),
            'decimal-operations/kp3': lambda a: (a/F(1,5)).denominator == 1,
            'fraction-decimal-conversion/kp1': lambda a,b: b in [2,4,5,8,10],
            'fraction-decimal-conversion/kp2': lambda a: terminates_within(a,2),
            'percent-conversions/kp3': lambda a: 0 < 100*a <= 200,
            'percent-of-a-number/kp1': lambda a: (a*F(3,10)).denominator == 1,
            'percent-of-a-number/kp3': lambda a: (a*F(3,2)).denominator == 1,
            'understanding-ratios/kp1': lambda a: a <= 20,
            'understanding-ratios/kp2': lambda a: a <= 40,
            'understanding-ratios/kp3': lambda a,b: 2 <= b <= 5,
            'ratio-tables-equivalent-ratios/kp1': lambda a,b: (b/2).denominator == 1,
            'ratio-tables-equivalent-ratios/kp2': lambda a,b: 2 <= b <= 8,
            'ratios-proportions/kp2': lambda a: (a/5).denominator == 1,
            'percent-applications/kp1': lambda a: (a*F(4,5)).denominator == 1,
            'percent-applications/kp2': lambda a: (a*F(5,6)).denominator == 1,
            'percent-applications/kp3': lambda a: terminates_within(a*F(27,25),2),
            'simple-interest/kp1': lambda a: (a/20).denominator == 1,
            'simple-interest/kp2': lambda a: (3*a/20).denominator == 1,
        }
        self.check_boundaries(checks)

    def check_boundaries(self, checks):
        generated = {r['kp_id']: r for r in drafts()}
        for key, check in checks.items():
            if key not in generated:
                continue
            for sample in generated[key]['arguments']['samples']:
                with self.subTest(kp=key, params=sample['params']):
                    self.assertTrue(check(**{k:F(v) for k,v in sample['params'].items()}))


if __name__ == '__main__':
    unittest.main()
