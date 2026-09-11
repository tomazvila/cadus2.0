"""Semantic table for `apply_arithmetic_core_recipes_multidigit`: every new
column-arithmetic exemplar is independently recomputed from its own problem
text; every word-problem and mental-strategy exemplar is checked against the
real quantities its own problem states.
"""
import json
import re
import unittest
from pathlib import Path

import apply_arithmetic_core_recipes_multidigit as recipes
import foundations_compute as fc
from foundations_curriculum_patch import KpKey

TESTDATA = Path(__file__).parent / "testdata" / "foundations_topics.json"

#: The exemplar count each KP held before this module's own recipe ran.
ORIGINAL_COUNTS: dict[tuple[str, str], int] = {
    ("addition-with-carrying", "kp1"): 2, ("addition-with-carrying", "kp2"): 2,
    ("addition-with-carrying", "kp3"): 2,
    ("subtraction-with-borrowing", "kp1"): 2, ("subtraction-with-borrowing", "kp2"): 2,
    ("subtraction-with-borrowing", "kp3"): 2,
    ("multi-digit-addition-subtraction", "kp1"): 2, ("multi-digit-addition-subtraction", "kp2"): 3,
    ("multi-digit-addition-subtraction", "kp3"): 2,
    ("addition-subtraction-word-problems", "kp1"): 2, ("addition-subtraction-word-problems", "kp2"): 2,
    ("addition-subtraction-word-problems", "kp3"): 2,
    ("multiplying-by-one-digit", "kp1"): 2, ("multiplying-by-one-digit", "kp2"): 2,
    ("multiplying-by-one-digit", "kp3"): 2,
    ("multi-digit-multiplication", "kp1"): 2, ("multi-digit-multiplication", "kp2"): 3,
    ("multi-digit-multiplication", "kp3"): 2,
}


def find_kp(topic_id: str, kp_id: str) -> dict:
    for topic in json.loads(TESTDATA.read_text()):
        if topic["id"] == topic_id:
            for kp in topic["knowledge_points"]:
                if kp["id"] == kp_id:
                    return kp
    raise KeyError((topic_id, kp_id))


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
    def test_every_pure_arithmetic_recipe_matches_its_own_evaluated_expression(self):
        checked = 0
        for key, plans in recipes.RECIPES.items():
            for plan in plans:
                match = fc.match_problem(plan.problem)
                if not match or "and check" in plan.problem or "mentally" in plan.problem:
                    continue
                with self.subTest(kp=str(key), problem=plan.problem):
                    value = fc.evaluate(match.group(2))
                    self.assertEqual(fc.render_answer(value), plan.answer)
                    self.assertNotIn(plan.problem, served_problems(key.topic_id, key.kp_id))
                checked += 1
        self.assertEqual(checked, 22)


_MULT_RE = re.compile(r"\$(\d[\d.]*) \\times (\d[\d.]*)\$")


class MentalStrategyTest(unittest.TestCase):
    def test_near_round_recipes_state_the_true_product(self):
        for plan in recipes.RECIPES[KpKey("multiplying-by-one-digit", "kp3")]:
            match = _MULT_RE.search(plan.problem)
            self.assertIsNotNone(match, plan.problem)
            a, b = int(match[1]), int(match[2])
            self.assertEqual(int(plan.answer), a * b)

    def test_estimate_check_recipe_states_the_true_product_and_a_true_estimate(self):
        matches = [_MULT_RE.findall(p.problem) for p in recipes.RECIPES[KpKey("multi-digit-multiplication", "kp3")]]
        for plan, pairs in zip(recipes.RECIPES[KpKey("multi-digit-multiplication", "kp3")], matches):
            (a, b), *rest = [(int(x), int(y)) for x, y in pairs]
            with self.subTest(problem=plan.problem):
                self.assertEqual(int(plan.answer), a * b)
                if rest:
                    ra, rb = rest[0]
                    self.assertIn(str(ra * rb), plan.solution_sketch)


class WordProblemArithmeticTest(unittest.TestCase):
    """Every quantity named in a word-problem recipe's own text sums to its answer."""

    NUMBER_RE = re.compile(r"\$([\d{,}]+)\$")

    def _numbers(self, text: str) -> list[int]:
        return [int(n.replace("{,}", "")) for n in self.NUMBER_RE.findall(text)]

    def test_one_step_word_problem_recipes_combine_their_own_stated_numbers(self):
        for topic in ("addition-subtraction-word-problems",):
            for kp_id in ("kp1", "kp2", "kp3"):
                for plan in recipes.RECIPES[KpKey(topic, kp_id)]:
                    numbers = self._numbers(plan.problem)
                    answer = int(plan.answer)
                    with self.subTest(problem=plan.problem):
                        # the answer is reachable by some +/- combination of the
                        # problem's own stated numbers (never an unrelated value)
                        reachable = {numbers[0]}
                        for n in numbers[1:]:
                            reachable = {r + n for r in reachable} | {r - n for r in reachable}
                        reachable |= {abs(r) for r in reachable}
                        self.assertIn(answer, reachable, (plan.problem, numbers))


class NoDuplicateWithinBatchTest(unittest.TestCase):
    def test_no_recipe_batch_repeats_a_problem_text(self):
        for key, plans in recipes.RECIPES.items():
            problems = [p.problem for p in plans]
            with self.subTest(kp=str(key)):
                self.assertEqual(len(problems), len(set(problems)))


if __name__ == "__main__":
    unittest.main()
