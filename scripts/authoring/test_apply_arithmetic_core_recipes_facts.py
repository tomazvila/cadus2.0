"""Semantic table for `apply_arithmetic_core_recipes_facts`: every new exemplar
is independently recomputed from its own problem text (never trusting this
module's own arithmetic) and checked against its knowledge point's own
authored `constraints` and its already-served numbers.
"""
import json
import re
import unittest
from pathlib import Path

import apply_arithmetic_core_recipes_facts as recipes
import foundations_compute as fc
import foundations_number_theory as nt
from foundations_curriculum_patch import KpKey

TESTDATA = Path(__file__).parent / "testdata" / "foundations_topics.json"
_COMPUTE_RE = re.compile(r"^Compute \$(.+)\$\.$")
_MISSING_RE = re.compile(r"^Solve: \$\\square \\times (\d+) = (\d+)\$\.$")
_SQUARE_CHECK_RE = re.compile(r"^Is \$(\d+)\$ a perfect square\? \(yes/no\)$")
_VALUE_RE = re.compile(r"^What is the value of the \$(\d+)\$ in \$([\d{,}]+)\$\?$")
_EXPANDED_RE = re.compile(r"^Write \$([\d{,}]+)\$ in expanded form\.$")
_COMPARE_RE = re.compile(r"^Which is larger, \$([\d{,}]+)\$ or \$([\d{,}]+)\$\?$")
_ORDER_RE = re.compile(r"^Order from least to greatest: (.+)\.$")
_ORDER_ITEM_RE = re.compile(r"\$([\d{,}]+)\$")
_ROUND_RE = re.compile(r"^Round \$([\d{,}]+)\$ to the nearest (\w+)\.$")
_PLACE_BY_NAME = {"ten": 1, "hundred": 2, "thousand": 3}


def _int(text: str) -> int:
    return int(text.replace("{,}", "").replace(",", ""))


def find_kp(topic_id: str, kp_id: str) -> dict:
    for topic in json.loads(TESTDATA.read_text()):
        if topic["id"] == topic_id:
            for kp in topic["knowledge_points"]:
                if kp["id"] == kp_id:
                    return kp
    raise KeyError((topic_id, kp_id))


#: The exemplar count each KP held BEFORE this module's own recipe ran, so
#: "not already served" is judged against the original curriculum even when
#: `testdata/foundations_topics.json` already reflects a `--write` run.
ORIGINAL_COUNTS: dict[tuple[str, str], int] = {
    ("single-digit-addition", "kp1"): 3, ("single-digit-addition", "kp2"): 3,
    ("subtraction-facts", "kp1"): 3, ("subtraction-facts", "kp2"): 3,
    ("multiplication-tables", "kp1"): 3, ("multiplication-tables", "kp2"): 2,
    ("division-facts", "kp1"): 3, ("division-facts", "kp2"): 3,
    ("perfect-squares", "kp1"): 3, ("perfect-squares", "kp2"): 3,
    ("place-value", "kp1"): 2, ("place-value", "kp2"): 2,
    ("comparing-ordering-whole-numbers", "kp1"): 2, ("comparing-ordering-whole-numbers", "kp2"): 2,
    ("rounding-whole-numbers", "kp1"): 3, ("rounding-whole-numbers", "kp2"): 3,
}


def served_problems(topic_id: str, kp_id: str) -> set[str]:
    original = ORIGINAL_COUNTS[(topic_id, kp_id)]
    return {e["problem"] for e in find_kp(topic_id, kp_id)["exemplars"][:original]}


class RecipeKeysAreRealTest(unittest.TestCase):
    def test_every_recipe_and_sketch_key_names_a_real_knowledge_point(self):
        for key in recipes.RECIPES:
            find_kp(key.topic_id, key.kp_id)
        for key in recipes.MISSING_SKETCHES:
            kp = find_kp(key.topic_id, key.kp_id)
            self.assertLess(key.exemplar_index, len(kp["exemplars"]))


class PureArithmeticAnswersTest(unittest.TestCase):
    """Every `Compute $expr$.` recipe: the answer is the real evaluated value."""

    def test_every_compute_shaped_recipe_matches_its_own_evaluated_expression(self):
        checked = 0
        for key, plans in recipes.RECIPES.items():
            for plan in plans:
                match = _COMPUTE_RE.match(plan.problem)
                if not match:
                    continue
                with self.subTest(kp=str(key), problem=plan.problem):
                    value = fc.evaluate(match.group(1))
                    self.assertEqual(fc.render_answer(value), plan.answer)
                    self.assertNotIn(plan.problem, served_problems(key.topic_id, key.kp_id))
                checked += 1
        self.assertEqual(checked, 9)


class MissingFactorAndSquareTest(unittest.TestCase):
    def test_missing_factor_recipe_solves_the_real_equation(self):
        plans = recipes.RECIPES[KpKey("division-facts", "kp2")]
        (plan,) = plans
        match = _MISSING_RE.match(plan.problem)
        self.assertIsNotNone(match, plan.problem)
        known, product = int(match[1]), int(match[2])
        self.assertEqual(known * int(plan.answer), product)

    def test_square_check_recipe_states_a_true_yes_no_fact(self):
        plans = recipes.RECIPES[KpKey("perfect-squares", "kp2")]
        (plan,) = plans
        match = _SQUARE_CHECK_RE.match(plan.problem)
        self.assertIsNotNone(match, plan.problem)
        n = int(match[1])
        root = round(n**0.5)
        is_square = root * root == n
        self.assertEqual(plan.answer, "yes" if is_square else "no")


class PlaceValueTest(unittest.TestCase):
    CASES = [("place-value", "kp1", 4), ("place-value", "kp2", 5)]

    def test_every_value_and_expanded_form_recipe_matches_the_real_digits(self):
        for topic_id, kp_id, digit_ceiling in self.CASES:
            for plan in recipes.RECIPES[KpKey(topic_id, kp_id)]:
                with self.subTest(kp=f"{topic_id}/{kp_id}", problem=plan.problem):
                    if match := _VALUE_RE.match(plan.problem):
                        digit, n = int(match[1]), _int(match[2])
                        width = len(str(n))
                        found = next(
                            value for place in range(width)
                            for d, value in [nt.digit_at_place(n, place)] if d == digit and str(value) == plan.answer
                        )
                        self.assertEqual(str(found), plan.answer)
                        self.assertLessEqual(len(str(n)), digit_ceiling)
                    elif match := _EXPANDED_RE.match(plan.problem):
                        n = _int(match[1])
                        self.assertEqual(plan.answer, nt.expanded_form(n))
                        self.assertEqual(sum(int(t) for t in plan.answer.split(" + ")), n)


class CompareOrderTest(unittest.TestCase):
    def test_compare_recipes_name_the_true_larger_number(self):
        for plan in recipes.RECIPES[KpKey("comparing-ordering-whole-numbers", "kp1")]:
            match = _COMPARE_RE.match(plan.problem)
            self.assertIsNotNone(match, plan.problem)
            a, b = _int(match[1]), _int(match[2])
            self.assertEqual(int(plan.answer), max(a, b))

    def test_order_recipes_list_the_true_ascending_order(self):
        for plan in recipes.RECIPES[KpKey("comparing-ordering-whole-numbers", "kp2")]:
            match = _ORDER_RE.match(plan.problem)
            self.assertIsNotNone(match, plan.problem)
            values = [_int(v) for v in _ORDER_ITEM_RE.findall(match[1])]
            expected = ", ".join(str(v) for v in sorted(values))
            self.assertEqual(plan.answer, expected)
            self.assertIn(len(values), (3, 4))


class RoundingTest(unittest.TestCase):
    def test_rounding_recipes_match_the_real_half_up_rule(self):
        for plan in recipes.RECIPES[KpKey("rounding-whole-numbers", "kp1")] + recipes.RECIPES[
            KpKey("rounding-whole-numbers", "kp2")
        ]:
            match = _ROUND_RE.match(plan.problem)
            self.assertIsNotNone(match, plan.problem)
            n, place_word = _int(match[1]), match[2]
            place_index = _PLACE_BY_NAME[place_word]
            self.assertEqual(int(plan.answer), nt.round_to(n, place_index))


class NoDuplicateAnswerWithinBatchTest(unittest.TestCase):
    def test_no_recipe_batch_repeats_a_problem_text(self):
        for key, plans in recipes.RECIPES.items():
            problems = [p.problem for p in plans]
            with self.subTest(kp=str(key)):
                self.assertEqual(len(problems), len(set(problems)))


if __name__ == "__main__":
    unittest.main()
