"""Semantic checks that the production template gate does not infer from KP prose."""
import json
import math
from pathlib import Path
import unittest
import sys

from build import build

ROOT = Path(__file__).resolve().parents[3]


def carries(a,b):
    count=carry=0
    while a or b:
        carry=(a%10+b%10+carry)//10
        count+=carry
        a//=10
        b//=10
    return count


def borrowing(a,b):
    count=borrow=0
    while a or b:
        borrow=int(a%10-borrow < b%10)
        count+=borrow
        a//=10
        b//=10
    return count


ARITHMETIC_RULES = {
        "single-digit-addition/kp1":lambda a,b:0<=a<=9 and 0<=b<=9 and a+b<=10,
        "single-digit-addition/kp2":lambda a,b:0<=a<=9 and 0<=b<=9 and 11<=a+b<=18,
        "subtraction-facts/kp1":lambda a,b:0<=b<=a<=10,
        "subtraction-facts/kp2":lambda a,b:11<=a<=18 and 2<=b<=9 and 0<=a-b<10,
        "multiplication-tables/kp1":lambda a,b:0<=a<=10 and 0<=b<=10,
        "multiplication-tables/kp2":lambda a,b:a in (11,12) and 2<=b<=12,
        "division-facts/kp1":lambda a,b:2<=b<=10 and a%b==0 and 2<=a//b<=10,
        "division-facts/kp2":lambda a,b:2<=b<=10 and a%b==0 and 2<=a//b<=10,
        "addition-with-carrying/kp1":lambda a,b:10<=a<=99 and 10<=b<=99 and carries(a,b)==1,
        "addition-with-carrying/kp2":lambda a,b:100<=a<=999 and 100<=b<=999 and carries(a,b)>=2,
        "addition-with-carrying/kp3":lambda a,b:0<=a<=999 and 0<=b<=999,
        "subtraction-with-borrowing/kp1":lambda a,b:10<=b<=a<=99 and borrowing(a,b)==1,
        "subtraction-with-borrowing/kp2":lambda a,b:100<=b<=a<=999 and '0' not in str(a) and borrowing(a,b)>=1,
        "subtraction-with-borrowing/kp3":lambda a,b:10<=b<=a<=999,
        "multi-digit-addition-subtraction/kp1":lambda a,b:1000<=a<=9999 and 1000<=b<=9999 and carries(a,b)>=2,
        "multi-digit-addition-subtraction/kp2":lambda a:0<=a<=999 and borrowing(2003,a)>=2,
        "multi-digit-addition-subtraction/kp3":lambda a:0<=a<=9999,
}


def arithmetic_rules():
    return ARITHMETIC_RULES.copy()


def place_operation_rules():
    return {
        "place-value/kp1":lambda a:0<=a<=9999,
        "place-value/kp2":lambda a:0<=a<=99999,
        "rounding-whole-numbers/kp1":lambda a:10<=a<=999,
        "rounding-whole-numbers/kp2":lambda a:0<=a<=9999,
        "multiplying-by-powers-of-ten/kp1":lambda a:1<=a<=99,
        "multiplying-by-powers-of-ten/kp2":lambda a,b:a%10==b%10==0 and 1<=a//10<=10 and 1<=b//10<=10,
        "multiplying-by-one-digit/kp1":lambda a:10<=a<=99,
        "multiplying-by-one-digit/kp2":lambda a:100<=a<=999,
        "multiplying-by-one-digit/kp3":lambda a,b:a%10==9 and 1<=b<=9,
        "multi-digit-multiplication/kp1":lambda a:10<=a<=99,
        "multi-digit-multiplication/kp2":lambda a:100<=a<=999,
        "multi-digit-multiplication/kp3":lambda a:10<=a<=99,
        "long-division-one-digit/kp1":lambda a:10<=a<=99 and a%4==0,
        "long-division-one-digit/kp2":lambda a:100<=a<=999 and a%7==0,
        "long-division/kp1":lambda a:1000<=a<=9999 and a%8==0,
        "long-division/kp2":lambda a:100<=a<=999 and a%12==0,
        "whole-number-exponents/kp2":lambda a:a*a<=1000,
        "whole-number-exponents/kp3":lambda a,b:a*a<=200 and b**3<=200,
        "expressions-with-parentheses/kp1":lambda a,b:0<=a<=9 and 0<=b<=9,
        "expressions-with-parentheses/kp2":lambda a:a>=4,
        "expressions-with-parentheses/kp3":lambda a:a>=0,
        "order-of-operations/kp1":lambda a,b:0<=a<=9 and 0<=b<=9,
        "order-of-operations/kp2":lambda a,b:0<=a<=9 and 0<=b<=9,
        "order-of-operations/kp3":lambda a,b:(a+3)**2>=b*2,
    }


def context_number_rules():
    return {
        "addition-subtraction-word-problems/kp1":lambda a:0<=a<=9999,
        "addition-subtraction-word-problems/kp2":lambda a:86<=a<=999,
        "addition-subtraction-word-problems/kp3":lambda a:57<=a<=9999,
        "multiplication-division-word-problems/kp1":lambda a:1<=a<=99,
        "multiplication-division-word-problems/kp2":lambda a:100<=a<=999 and a%7==0,
        "multiplication-division-word-problems/kp3":lambda a:a>0,
        "division-with-remainders/kp3":lambda a:a>0,
        "comparing-ordering-whole-numbers/kp1":lambda a,b:0<=a<=99999 and 0<=b<=99999 and a!=b,
        "comparing-ordering-whole-numbers/kp2":lambda a:0<=a<=9999 and a not in (312,427),
        "greatest-common-factor/kp1":lambda a,b:1<=a<=30 and 1<=b<=30,
        "greatest-common-factor/kp2":lambda a,b:1<=a<=60 and 1<=b<=60,
        "greatest-common-factor/kp3":lambda a,b:math.gcd(a,b)==1 or b%a==0 or a%b==0,
        "least-common-multiple/kp1":lambda a,b:1<=a<=12 and 1<=b<=12,
        "least-common-multiple/kp2":lambda a,b:math.gcd(a,b)==1 or b%a==0 or a%b==0,
        "least-common-multiple/kp3":lambda a,b:1<=a<=10 and 1<=b<=10,
        "gcf-lcm/kp1":lambda a:40<=a<=150,
        "gcf-lcm/kp2":lambda a:1<=a<=60,
        "gcf-lcm/kp3":lambda a:a>0,
        "rounding-estimation/kp1":lambda a:10<=a<=999,
        "rounding-estimation/kp2":lambda a:a>0,
        "rounding-estimation/kp3":lambda a:600<=a+298<=700 and a+298!=650,
        "prime-composite-numbers/kp2":lambda a:2<=a<47,
        "perfect-squares/kp2":lambda a,b:0<=a<=150 and 0<=b<=150 and (math.isqrt(a)**2==a)!=(math.isqrt(b)**2==b),
    }


class Domains(unittest.TestCase):
    def test_import_manifest_and_published_arguments_are_reproducible(self):
        sys.path.insert(0,str(ROOT/'scripts/authoring'))
        from import_local_drafts import validate
        rows,_=build(ROOT)
        self.assertEqual(len(validate({},rows)),64)
        published=json.loads((ROOT/'docs/content-foundations/unit00-templates/drafts.json').read_text())
        self.assertEqual(rows,published)

    def test_every_candidate_has_a_semantic_domain_check(self):
        rows,missing=build(ROOT)
        rules=arithmetic_rules() | place_operation_rules() | context_number_rules()
        self.assertEqual(set(rules),{r['kp_id'] for r in rows})
        self.assertEqual(len(missing),17)
        for row in rows:
            samples=row['arguments']['samples']
            self.assertGreaterEqual(len(samples),12,row['kp_id'])
            for sample in samples:
                with self.subTest(kp=row['kp_id'],params=sample['params']):
                    self.assertTrue(rules[row['kp_id']](**sample['params']))

    def test_rejected_degenerate_and_cross_topic_families_stay_out(self):
        rows,_=build(ROOT)
        by_key={row["kp_id"]:row for row in rows}
        self.assertNotIn("factors-and-multiples/kp1",by_key)
        division={(s["params"]["a"],s["expected"])
                  for s in by_key["division-facts/kp1"]["arguments"]["samples"]
                  if s["params"]["b"]==4}
        long_division={(s["params"]["a"],s["expected"])
                       for s in by_key["long-division-one-digit/kp1"]["arguments"]["samples"]}
        self.assertFalse(division & long_division)

    def test_boundary_and_special_cases_remain_present(self):
        rows,_=build(ROOT)
        by_key={}
        for row in rows:
            by_key.setdefault(row['kp_id'],[]).extend(row['arguments']['samples'])
        self.assertIn(2500,[r['params']['a'] for r in by_key['rounding-whole-numbers/kp2']])
        for key in ('greatest-common-factor/kp3','least-common-multiple/kp2'):
            pairs=[(s['params']['a'],s['params']['b']) for s in by_key[key]]
            self.assertTrue(any(math.gcd(a,b)==1 for a,b in pairs))
            self.assertTrue(any(b%a==0 or a%b==0 for a,b in pairs))
        estimates={s['expected'] for s in by_key['rounding-estimation/kp3']}
        self.assertEqual(estimates,{'600','700'})


if __name__ == '__main__':
    unittest.main()
