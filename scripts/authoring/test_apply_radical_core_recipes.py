"""Independent semantic checks for the five explicit radical recipes."""
import math
import re
import unittest
from fractions import Fraction
from pathlib import Path

import apply_radical_core_recipes as recipes
import foundations_compute as compute
from foundations_curriculum_patch import KpKey
from foundations_testdata import find_kp

TESTDATA = Path(__file__).parent / "testdata" / "foundations_topics.json"
ROOT_RE = re.compile(r"^Compute \$(?P<prefix>\d*)\\sqrt(?:\[(?P<degree>\d+)\])?\{(?P<body>.+)\}\$\.$")


class RadicalRecipeSemanticTest(unittest.TestCase):
    def test_exact_recipe_matrix(self):
        expected = {
            KpKey("perfect-square-roots", "kp2"): [(100, 2, 10), (81, 2, 9)],
            KpKey("perfect-square-roots", "kp3"): [(361, 2, 19), (324, 2, 18)],
            KpKey("square-roots", "kp1"): [(64, 2, 16), (9, 2, 15)],
            KpKey("square-roots", "kp3"): [(144, 2, 12), (100, 2, 10)],
            KpKey("cube-roots", "kp2"): [(-64, 3, -4), (-27, 3, -3)],
        }
        self.assertEqual(set(recipes.RECIPES), set(expected))
        for key, rows in expected.items():
            plans = recipes.RECIPES[key]
            self.assertEqual(len(plans), 2)
            for plan, (radicand, degree, answer) in zip(plans, rows):
                with self.subTest(kp=key, problem=plan.problem):
                    match = ROOT_RE.match(plan.problem)
                    self.assertIsNotNone(match)
                    prefix = int(match["prefix"] or "1")
                    factors = [int(part.strip()) for part in match["body"].split(r"\cdot")]
                    self.assertEqual(compute.evaluate(plan.problem.split("$")[1]), answer)
                    self.assertEqual(plan.answer, str(answer))
                    self.assertEqual(int(match["degree"] or "2"), degree)
                    self.assertEqual(self._product(factors), radicand)
                    self.assertEqual(abs(answer // prefix) ** degree, abs(radicand))
                    if key == KpKey("square-roots", "kp3"):
                        self.assertTrue(
                            all(math.isqrt(factor) ** 2 == factor for factor in factors),
                            "every displayed factor must itself be a perfect square",
                        )
                    self.assertIn(str(answer if key.topic_id == "square-roots" else abs(answer // prefix)), plan.solution_sketch)

    def test_constraints_and_distinctness(self):
        for key, plans in recipes.RECIPES.items():
            answers = [int(plan.answer) for plan in plans]
            self.assertEqual(len(answers), len(set(answers)), key)
            kp = find_kp(TESTDATA,key.topic_id,key.kp_id)
            self.assertEqual(len(kp["exemplars"]), 4, key)
            self.assertTrue(all(item.get("solution_sketch") for item in kp["exemplars"]), key)
        self.assertTrue(all(0 < int(plan.answer) <= 12 for plan in recipes.RECIPES[KpKey("perfect-square-roots", "kp2")]))
        self.assertTrue(all(0 < int(plan.answer) <= 20 for plan in recipes.RECIPES[KpKey("perfect-square-roots", "kp3")]))
        self.assertTrue(all(int(plan.answer) < 0 for plan in recipes.RECIPES[KpKey("cube-roots", "kp2")]))

    @staticmethod
    def _product(values: list[int]) -> int:
        result = 1
        for value in values:
            result *= value
        return result


class RadicalEvaluatorSafetyTest(unittest.TestCase):
    def test_exact_real_roots_and_compatible_products(self):
        cases = {
            r"\sqrt{49}": 7,
            r"\sqrt{\frac{49}{100}}": Fraction(7, 10),
            r"\sqrt[3]{-125}": -5,
            r"\sqrt{50} / \sqrt{2}": 5,
            r"\sqrt{12} \cdot \sqrt{3}": 6,
        }
        for expression, expected in cases.items():
            with self.subTest(expression=expression):
                self.assertEqual(compute.evaluate(expression), expected)

    def test_non_real_or_inexact_results_fail_closed(self):
        for expression in (r"\sqrt{-2}", r"\sqrt{-2} \cdot \sqrt{-2}", r"\sqrt{2}"):
            with self.subTest(expression=expression):
                with self.assertRaises(compute.NotArithmetic):
                    compute.evaluate(expression)


if __name__ == "__main__":
    unittest.main()
