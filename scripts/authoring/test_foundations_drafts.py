"""Tests of `foundations_drafts`."""
import json
import math
import random
import unittest
from pathlib import Path

import foundations_compute as fc
import foundations_drafts as fd

TESTDATA = Path(__file__).parent / "testdata" / "foundations_topics.json"


def load_topics():
    return json.loads(TESTDATA.read_text())


def find_kp(topic_id, kp_id):
    for topic in load_topics():
        if topic["id"] == topic_id:
            for kp in topic["knowledge_points"]:
                if kp["id"] == kp_id:
                    return topic, kp
    raise KeyError((topic_id, kp_id))


class ClassifyFamilyTest(unittest.TestCase):
    def test_families(self):
        cases = {
            "3 + 4": "integer_add_sub",
            "-4 \\times 3": "integer_mul_div",
            "(-1)(-2)(-3)": "integer_mul_div",
            "\\frac{6}{9}": "fraction_reduce",
            "\\frac{1}{2} + \\frac{3}{8}": "fraction_add_sub",
            "\\frac{2}{3} \\times \\frac{3}{4}": "fraction_mul_div",
            "2.5 + 1.3": "decimal_add_sub",
            "0.47 \\times 10": "decimal_mul_div",
            "|-6|": "absolute_value",
            "3|-4| - 10": "absolute_value",
            "(-2)^4": "exponent",
            "8^{2/3}": "exponent",
        }
        for expr, expected in cases.items():
            self.assertEqual(fd.classify_family(expr), expected, expr)


class WithinKpFamilyTest(unittest.TestCase):
    def test_rejects_a_bare_fraction_operand_equal_to_one(self):
        check = fd._within_kp_family(frozenset({fc.Fraction(2)}), "integer_add_sub")
        self.assertFalse(check(r"3 \times \frac{2}{2}", fc.Fraction(2)))

    def test_rejects_a_trivial_times_or_div_one_factor(self):
        check = fd._within_kp_family(frozenset({fc.Fraction(4)}), "fraction_mul_div")
        self.assertFalse(check(r"4\frac{1}{4} \times 1", fc.Fraction(4)))
        self.assertFalse(check(r"1 \times \frac{4}{1}", fc.Fraction(4)))
        self.assertFalse(check(r"4 \div 1", fc.Fraction(4)))

    def test_fraction_reduce_rejects_an_already_reduced_draw(self):
        check = fd._within_kp_family(frozenset({fc.Fraction(2, 3)}), "fraction_reduce")
        self.assertFalse(check(r"\frac{7}{17}", fc.Fraction(7, 17)))
        self.assertTrue(check(r"\frac{4}{6}", fc.Fraction(2, 3)))

    def test_no_generated_candidate_across_the_curriculum_is_degenerate(self):
        """No fraction operand anywhere a generated candidate uses equals
        exactly one, and no `fraction_reduce` candidate is already reduced."""
        checked = 0
        for topic in load_topics():
            for kp in topic["knowledge_points"]:
                rng = random.Random(f"{topic['id']}/{kp['id']}")
                candidate = fd.classify_kp(topic, kp, rng)
                if candidate is None:
                    continue
                checked += 1
                for num, den in fd._BARE_FRACTION.findall(candidate.candidate_expr):
                    self.assertNotEqual(num, den, candidate.kp_key)
                if candidate.family == "fraction_reduce":
                    match = fd._BARE_FRACTION.search(candidate.candidate_expr)
                    self.assertIsNotNone(match)
                    self.assertGreater(math.gcd(int(match.group(1)), int(match.group(2))), 1)
        self.assertGreater(checked, 20)


class ClassifyKpTest(unittest.TestCase):
    def test_arithmetic_kp_is_a_candidate(self):
        # kp1's three exemplars densely cover 7, 8 and 9: no fourth in-band
        # integer is left, so kp1 correctly declines. kp2 (12, 15, 12) still
        # has 13 and 14 free.
        topic, kp = find_kp("single-digit-addition", "kp2")
        rng = random.Random(f"{topic['id']}/{kp['id']}")
        candidate = fd.classify_kp(topic, kp, rng)
        self.assertIsNotNone(candidate)
        self.assertEqual(candidate.kp_key, "single-digit-addition/kp2")
        self.assertEqual(fc.evaluate(candidate.candidate_expr), candidate.answer)
        served = {fc.parse_answer_text(e["answer"]) for e in kp["exemplars"]}
        self.assertNotIn(candidate.answer, served)
        self.assertTrue(min(served) <= candidate.answer <= max(served))
        for exemplar in kp["exemplars"]:
            self.assertNotEqual(f"Compute ${candidate.candidate_expr}$.", exemplar["problem"])

    def test_a_densely_served_kp_correctly_declines(self):
        topic, kp = find_kp("single-digit-addition", "kp1")
        rng = random.Random(f"{topic['id']}/{kp['id']}")
        self.assertIsNone(fd.classify_kp(topic, kp, rng))

    def test_word_problem_kp_is_not_a_candidate(self):
        topic, kp = find_kp("fraction-basics", "kp1")
        rng = random.Random("seed")
        self.assertIsNone(fd.classify_kp(topic, kp, rng))

    def test_radical_quotient_is_not_taught_as_bare_fraction_reduction(self):
        topic, kp = find_kp("dividing-radicals", "kp3")
        rng = random.Random(f"{topic['id']}/{kp['id']}")
        self.assertEqual(fd.classify_family(r"\frac{10\sqrt{27}}{5\sqrt{3}}"), "fraction_reduce")
        self.assertIsNone(fd.classify_kp(topic, kp, rng))

    def test_classify_is_deterministic(self):
        topic, kp = find_kp("adding-integers", "kp1")
        first = fd.classify_kp(topic, kp, random.Random(f"{topic['id']}/{kp['id']}"))
        second = fd.classify_kp(topic, kp, random.Random(f"{topic['id']}/{kp['id']}"))
        self.assertEqual(first, second)

    def test_every_candidate_across_the_curriculum_is_internally_consistent(self):
        checked = 0
        for topic in load_topics():
            for kp in topic["knowledge_points"]:
                rng = random.Random(f"{topic['id']}/{kp['id']}")
                candidate = fd.classify_kp(topic, kp, rng)
                if candidate is None:
                    continue
                checked += 1
                self.assertEqual(fc.evaluate(candidate.candidate_expr), candidate.answer)
                served = {fc.parse_answer_text(e["answer"]) for e in kp["exemplars"]}
                self.assertNotIn(candidate.answer, served)
                self.assertIn(candidate.family, fd.FAMILIES)
                # the candidate never asks for an easier or a harder item
                # than this knowledge point's own author already authored
                self.assertTrue(min(served) <= candidate.answer <= max(served), kp["id"])
                if all(value.denominator == 1 for value in served):
                    self.assertEqual(candidate.answer.denominator, 1, kp["id"])
        self.assertGreater(checked, 20)


class DecimalFamilyRenderingTest(unittest.TestCase):
    def test_every_decimal_family_candidate_renders_as_a_decimal_when_terminating(self):
        checked = 0
        for topic in load_topics():
            for kp in topic["knowledge_points"]:
                rng = random.Random(f"{topic['id']}/{kp['id']}")
                candidate = fd.classify_kp(topic, kp, rng)
                if candidate is None or not candidate.family.startswith("decimal"):
                    continue
                draft = fd.teach_draft(candidate)
                final_step = draft["arguments"]["worked_example"]["steps"][-1]
                if fc._terminating_decimal(candidate.answer) is not None:
                    self.assertIn(".", final_step, candidate.kp_key)
                    self.assertNotIn("/", final_step, candidate.kp_key)
                checked += 1
        self.assertGreater(checked, 3)


class DraftShapeTest(unittest.TestCase):
    def _candidate(self):
        topic, kp = find_kp("single-digit-addition", "kp2")
        rng = random.Random(f"{topic['id']}/{kp['id']}")
        return fd.classify_kp(topic, kp, rng)

    def test_teach_draft_shape(self):
        candidate = self._candidate()
        draft = fd.teach_draft(candidate)
        self.assertEqual(draft["kind"], "teach")
        self.assertEqual(draft["kp_id"], candidate.kp_key)
        args = draft["arguments"]
        self.assertTrue(args["concept"].strip())
        steps = args["worked_example"]["steps"]
        self.assertGreaterEqual(len(steps), 2)
        self.assertIn(candidate.candidate_expr, args["worked_example"]["problem"])
        self.assertIn(fc.render_answer(candidate.answer), steps[-1])

    def test_hint_draft_shape_has_three_rungs_and_no_digit(self):
        candidate = self._candidate()
        draft = fd.hint_draft(candidate)
        self.assertEqual(draft["kind"], "hint_ladder")
        hints = draft["arguments"]["hints"]
        self.assertEqual(len(hints), 3)
        self.assertEqual(len(set(hints)), 3)
        for rung in hints:
            self.assertFalse(any(ch.isdigit() for ch in rung))

    def test_no_hint_rung_anywhere_names_a_digit(self):
        for family_hints in fd.HINTS.values():
            for rung in family_hints:
                self.assertFalse(any(ch.isdigit() for ch in rung))


class SolutionSketchTest(unittest.TestCase):
    def test_generates_a_sketch_naming_the_exemplars_own_numbers(self):
        sketch = fd.solution_sketch_for("-7 + 4", "-3")
        self.assertIn("$-7 + 4 = -3$.", sketch)

    def test_coarse_family_never_claims_a_solution_sketch_is_semantically_complete(self):
        _, kp = find_kp("single-digit-addition", "kp1")
        self.assertIsNone(fd.missing_solution_sketches(kp))

    def test_missing_solution_sketches_is_none_for_a_non_numeric_kp(self):
        _, kp = find_kp("fraction-basics", "kp1")
        self.assertIsNone(fd.missing_solution_sketches(kp))

    def test_every_pure_numeric_exemplars_own_sketch_is_arithmetically_correct(self):
        """`solution_sketch_for` stays correct for every qualifying exemplar,
        independent of whether the curriculum already carries a sketch."""
        checked = 0
        for topic in load_topics():
            for kp in topic["knowledge_points"]:
                matches = fd.pure_numeric_exemplars(kp)
                if not matches:
                    continue
                for exemplar, match in zip(kp["exemplars"], matches):
                    expr = match.group(2)
                    answer_text = exemplar["answer"]
                    sketch = fd.solution_sketch_for(expr, answer_text)
                    self.assertIn(f"${expr} = {answer_text}$.", sketch)
                    self.assertEqual(fc.evaluate(expr), fc.parse_answer_text(answer_text))
                    checked += 1
        self.assertGreater(checked, 50)

    def test_coarse_family_never_generates_a_held_out_curriculum_item(self):
        refused = 0
        for topic in load_topics():
            for kp in topic["knowledge_points"]:
                self.assertIsNone(fd.generate_held_out_exemplars(topic, kp))
                refused += 1
        self.assertEqual(refused, 809)


if __name__ == "__main__":
    unittest.main()
