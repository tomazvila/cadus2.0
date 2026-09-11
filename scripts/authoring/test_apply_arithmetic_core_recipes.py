"""The semantic table: one row per `apply_arithmetic_core_recipes` KP, naming
its own authored constraint and checking every new exemplar against it by
an INDEPENDENT recompute from the parsed problem text — never by trusting
this module's own arithmetic, which would be circular.
"""
import re
import unittest
from pathlib import Path

import apply_arithmetic_core_recipes as recipes
from foundations_curriculum_patch import KpKey
from foundations_testdata import find_kp

TESTDATA = Path(__file__).parent / "testdata" / "foundations_topics.json"

_DIV_RE = re.compile(r"^Compute \$(?P<a>\d+) \\div (?P<b>\d+)\$")
_MUL_RE = re.compile(r"^Compute \$(?P<a>\d+) \\times (?P<b>\d+)\$\.$")


def served_division_pairs(kp: dict) -> set[tuple[int, int]]:
    pairs = set()
    for exemplar in kp["exemplars"]:
        match = _DIV_RE.match(exemplar["problem"])
        if match:
            pairs.add((int(match["a"]), int(match["b"])))
    return pairs


def served_multiplication_pairs(kp: dict) -> set[tuple[int, int]]:
    pairs = set()
    for exemplar in kp["exemplars"]:
        match = _MUL_RE.match(exemplar["problem"])
        if match:
            pairs.add((int(match["a"]), int(match["b"])))
    return pairs


def digits(n: int) -> int:
    return len(str(n))


def trailing_zeros(n: int) -> int:
    count = 0
    while n % 10 == 0:
        n //= 10
        count += 1
    return count


_ZERO_WORDS = {1: "one", 2: "two", 3: "three"}


class QuotientRemainderSemanticTest(unittest.TestCase):
    """Table: (topic, kp, dividend digits, divisor digits, count, original exemplar count)."""

    CASES = [
        ("division-with-remainders", "kp1", 2, 1, 2, 2),
        ("division-with-remainders", "kp2", 2, 1, 2, 2),
        ("long-division", "kp3", None, 2, 2, 2),
        ("long-division-one-digit", "kp3", 3, 1, 2, 2),
    ]

    def test_every_new_quotient_remainder_exemplar_matches_its_kps_own_constraint(self):
        for topic_id, kp_id, dividend_digits, divisor_digits, count, original_count in self.CASES:
            with self.subTest(kp=f"{topic_id}/{kp_id}"):
                # `testdata/foundations_topics.json` may already reflect THIS
                # script's own `--write` run, so "already served" is judged
                # against the KP's original exemplars only, not its current
                # (possibly already-raised) count.
                kp = find_kp(TESTDATA,topic_id,kp_id)
                kp = {**kp, "exemplars": kp["exemplars"][:original_count]}
                served = served_division_pairs(kp)
                plans = recipes.RECIPES[KpKey(topic_id, kp_id)]
                self.assertEqual(len(plans), count)
                for plan in plans:
                    match = _DIV_RE.match(plan.problem)
                    self.assertIsNotNone(match, plan.problem)
                    dividend, divisor = int(match["a"]), int(match["b"])
                    quotient, remainder = divmod(dividend, divisor)
                    self.assertEqual(plan.answer, f"{quotient} R{remainder}")
                    self.assertGreater(remainder, 0, "nonzero remainder")
                    self.assertLess(remainder, divisor, "remainder below the divisor")
                    if dividend_digits is not None:
                        self.assertEqual(digits(dividend), dividend_digits, "dividend digit count")
                    self.assertEqual(digits(divisor), divisor_digits, "divisor digit count")
                    self.assertNotIn((dividend, divisor), served, "not an already-served pair")
                    self.assertIn(f'"divisor":{divisor}', plan.contract_override)


class PowersOfTenSemanticTest(unittest.TestCase):
    def test_kp1_one_factor_is_a_power_of_ten_the_other_is_one_or_two_digits(self):
        kp = find_kp(TESTDATA,"multiplying-by-powers-of-ten", "kp1")
        kp = {**kp, "exemplars": kp["exemplars"][:3]}
        served = served_multiplication_pairs(kp)
        plans = recipes.RECIPES[KpKey("multiplying-by-powers-of-ten", "kp1")]
        self.assertEqual(len(plans), 1)
        match = _MUL_RE.match(plans[0].problem)
        self.assertIsNotNone(match)
        factor, power = int(match["a"]), int(match["b"])
        self.assertIn(power, (10, 100, 1000))
        self.assertIn(digits(factor), (1, 2))
        self.assertEqual(plans[0].answer, str(factor * power))
        self.assertNotIn((factor, power), served)
        expected_zeros = trailing_zeros(power)
        expected_word = _ZERO_WORDS[expected_zeros]
        self.assertIn(f"{expected_word} zero", plans[0].solution_sketch)
        # no false intermediate equality: `factor * power` is never `factor`
        self.assertNotIn(f"= {factor}$", plans[0].solution_sketch)
        self.assertIn(f"= {factor * power}$", plans[0].solution_sketch)

    def test_kp2_both_factors_end_in_zero_and_the_stripped_fact_is_correct(self):
        kp = find_kp(TESTDATA,"multiplying-by-powers-of-ten", "kp2")
        kp = {**kp, "exemplars": kp["exemplars"][:3]}
        served = served_multiplication_pairs(kp)
        plans = recipes.RECIPES[KpKey("multiplying-by-powers-of-ten", "kp2")]
        self.assertEqual(len(plans), 1)
        match = _MUL_RE.match(plans[0].problem)
        self.assertIsNotNone(match)
        a, b = int(match["a"]), int(match["b"])
        self.assertEqual(a % 10, 0, "first factor ends in zero")
        self.assertEqual(b % 10, 0, "second factor ends in zero")
        self.assertEqual(plans[0].answer, str(a * b))
        self.assertNotIn((a, b), served)
        expected_zeros = trailing_zeros(a) + trailing_zeros(b)
        expected_word = _ZERO_WORDS[expected_zeros]
        self.assertIn(f"append the {expected_word} zero", plans[0].solution_sketch)


class PracticeSketchSemanticTest(unittest.TestCase):
    def test_every_explicit_missing_sketch_matches_the_applied_curriculum(self):
        self.assertEqual(len(recipes.MISSING_SKETCHES), 8)
        for key, sketch in recipes.MISSING_SKETCHES.items():
            exemplar = find_kp(TESTDATA,key.topic_id,key.kp_id)["exemplars"][key.exemplar_index]
            with self.subTest(key=key):
                self.assertEqual(exemplar["solution_sketch"], sketch)

    def test_restored_powers_of_ten_practice_sketches_state_true_products(self):
        for key, sketch in recipes.MISSING_SKETCHES.items():
            if key.topic_id != "multiplying-by-powers-of-ten":
                continue
            exemplar = find_kp(TESTDATA,key.topic_id,key.kp_id)["exemplars"][key.exemplar_index]
            match = _MUL_RE.match(exemplar["problem"])
            self.assertIsNotNone(match, exemplar["problem"])
            a, b = int(match["a"]), int(match["b"])
            self.assertEqual(exemplar["answer"], str(a * b))
            if key.kp_id == "kp1":
                self.assertIn(f"= {a * b}$", sketch)
            else:
                self.assertIn(f"${a * b}$", sketch)


class RejectionTest(unittest.TestCase):
    def test_every_recipe_key_names_a_real_knowledge_point(self):
        for topic_id, kp_id in [(k.topic_id, k.kp_id) for k in recipes.RECIPES]:
            find_kp(TESTDATA,topic_id,kp_id)  # raises KeyError if the KP is not real

    def test_no_recipe_introduces_a_duplicate_answer_within_its_own_batch(self):
        for key, plans in recipes.RECIPES.items():
            answers = [p.answer for p in plans]
            with self.subTest(kp=str(key)):
                self.assertEqual(len(answers), len(set(answers)), "distinct answers within the batch")


if __name__ == "__main__":
    unittest.main()
