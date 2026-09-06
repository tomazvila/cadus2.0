"""Tests of `foundations_compute`: the oracle is the curriculum itself.

`testdata/foundations_topics.json` is the trimmed `dump_curriculum` output of
every Foundations topic (`dump_foundations_topics.py`). For every exemplar of
the `Compute $expr$.` family whose expression holds no unbound letter, this
module's `evaluate` must reach the exact value the author's own `answer`
field names — proved against the authored ground truth, not against a value
this module made up.
"""
import json
import random
import re
import unittest
from fractions import Fraction
from pathlib import Path

import foundations_compute as fc

TESTDATA = Path(__file__).parent / "testdata" / "foundations_topics.json"


parse_authored_answer = fc.parse_answer_text


def load_topics():
    return json.loads(TESTDATA.read_text())


def numeric_exemplars():
    """Every `(topic_id, kp_id, expr, answer_text)` of the pure-numeric family."""
    out = []
    for topic in load_topics():
        for kp in topic["knowledge_points"]:
            for exemplar in kp["exemplars"]:
                match = fc.match_problem(exemplar["problem"])
                if not match:
                    continue
                expr = match.group(2)
                if not fc.is_pure_numeric(expr):
                    continue
                try:
                    parse_authored_answer(exemplar["answer"])
                except (ValueError, ZeroDivisionError):
                    # An answer written in words (e.g. "6 x 10^7") sits outside
                    # this module's numeric-answer family; the KP classifier
                    # excludes it from generation the same way.
                    continue
                out.append((topic["id"], kp["id"], expr, exemplar["answer"]))
    return out


class OracleTest(unittest.TestCase):
    def test_fixture_is_present_and_sized(self):
        topics = load_topics()
        self.assertEqual(len(topics), 285)
        kps = sum(len(t["knowledge_points"]) for t in topics)
        self.assertEqual(kps, 809)

    def test_every_pure_numeric_exemplar_matches_its_authored_answer(self):
        cases = numeric_exemplars()
        self.assertGreater(len(cases), 200, "the fixture should hold hundreds of compute exemplars")
        mismatches = []
        for topic_id, kp_id, expr, answer_text in cases:
            expected = parse_authored_answer(answer_text)
            try:
                actual = fc.evaluate(expr)
            except fc.NotArithmetic as error:
                mismatches.append(f"{topic_id}/{kp_id}: {expr!r} raised {error}")
                continue
            if actual != expected:
                mismatches.append(f"{topic_id}/{kp_id}: {expr!r} = {actual}, authored {expected}")
        self.assertEqual(mismatches, [])


class UnitTest(unittest.TestCase):
    def test_basic_arithmetic(self):
        self.assertEqual(fc.evaluate("3 + 4"), Fraction(7))
        self.assertEqual(fc.evaluate("6 \\times 7"), Fraction(42))
        self.assertEqual(fc.evaluate("12 \\div 3"), Fraction(4))
        self.assertEqual(fc.evaluate("9 - 4"), Fraction(5))

    def test_fractions_and_mixed_numbers(self):
        self.assertEqual(fc.evaluate("\\frac{1}{2} + \\frac{3}{8}"), Fraction(7, 8))
        self.assertEqual(fc.evaluate("2\\frac{1}{2} \\div \\frac{1}{2}"), Fraction(5))
        self.assertEqual(fc.evaluate("-1\\frac{1}{2} + 2\\frac{1}{4}"), Fraction(3, 4))

    def test_decimals(self):
        self.assertEqual(fc.evaluate("0.47 \\times 10"), Fraction("4.7"))
        self.assertEqual(fc.evaluate("7.5 \\div 1000"), Fraction("0.0075"))
        self.assertEqual(fc.evaluate("-2.5 + 1.5"), Fraction(-1))

    def test_absolute_value_and_signs(self):
        self.assertEqual(fc.evaluate("|-6|"), Fraction(6))
        self.assertEqual(fc.evaluate("3|-4| - 10"), Fraction(2))
        self.assertEqual(fc.evaluate("-(-(-9))"), Fraction(-9))
        self.assertEqual(fc.evaluate("|2 - 9| + (-3)"), Fraction(4))

    def test_exponents_and_implicit_multiplication(self):
        self.assertEqual(fc.evaluate("-2^4"), Fraction(-16))
        self.assertEqual(fc.evaluate("(-2)^4"), Fraction(16))
        self.assertEqual(fc.evaluate("(-1)(-2)(-3)"), Fraction(-6))
        self.assertEqual(fc.evaluate("-2(5 - 9)"), Fraction(8))
        self.assertEqual(fc.evaluate("2 \\times (-3)^2"), Fraction(18))

    def test_thousands_separator(self):
        self.assertEqual(fc.evaluate("1{,}867 + 3{,}589"), Fraction(5456))
        self.assertEqual(fc.evaluate("5{,}208 \\div 6"), Fraction(868))

    def test_grouped_thousands_number_is_one_operand_not_two(self):
        operands = fc.integer_operands("4{,}675 + 2{,}848")
        self.assertEqual([o.value for o in operands], [4675, 2848])
        self.assertEqual([o.text for o in operands], ["4{,}675", "2{,}848"])

    def test_redrawing_a_grouped_operand_never_produces_a_malformed_group(self):
        # A regression of a caught defect: independently redrawing the two
        # halves of "4{,}675" (`4` and `675`) as separate operands could
        # produce a second group that is not three digits, e.g. `8{,}80`.
        rng = random.Random("grouped-regression")
        for _ in range(200):
            candidate, value = fc.same_shape_new_operands("4{,}675 + 2{,}848", rng)
            self.assertEqual(fc.evaluate(candidate), value)
            for group in re.findall(r"\d{1,3}(?:\{,\}\d+)+", candidate):
                for piece in group.split("{,}")[1:]:
                    self.assertEqual(len(piece), 3, candidate)

    def test_format_operand_groups_at_1000_and_not_below(self):
        self.assertEqual(fc._format_operand(999), "999")
        self.assertEqual(fc._format_operand(1000), "1{,}000")
        self.assertEqual(fc._format_operand(-12345), "-12{,}345")

    def test_exact_rational_exponents(self):
        self.assertEqual(fc.evaluate("25^{1/2}"), Fraction(5))
        self.assertEqual(fc.evaluate("8^{2/3}"), Fraction(4))
        self.assertEqual(fc.evaluate("4^{-1/2}"), Fraction(1, 2))
        with self.assertRaises(fc.NotArithmetic):
            fc.evaluate("10^{1/2}")

    def test_render_answer(self):
        self.assertEqual(fc.render_answer(Fraction(6)), "6")
        self.assertEqual(fc.render_answer(Fraction(-7, 2)), "-7/2")

    def test_render_answer_prefer_decimal(self):
        self.assertEqual(fc.render_answer(Fraction("2.65"), prefer_decimal=True), "2.65")
        self.assertEqual(fc.render_answer(Fraction(7, 20), prefer_decimal=True), "0.35")
        self.assertEqual(fc.render_answer(Fraction(-3, 8), prefer_decimal=True), "-0.375")
        self.assertEqual(fc.render_answer(Fraction(6), prefer_decimal=True), "6")
        # a non-terminating value still falls back to a/b even when preferred
        self.assertEqual(fc.render_answer(Fraction(1, 3), prefer_decimal=True), "1/3")

    def test_rejects_non_arithmetic(self):
        with self.assertRaises(fc.NotArithmetic):
            fc.evaluate("x + 2")

    def test_same_shape_new_operands_is_deterministic_and_differs(self):
        rng_a = random.Random("single-digit-addition/kp1")
        rng_b = random.Random("single-digit-addition/kp1")
        result_a = fc.same_shape_new_operands("3 + 4", rng_a)
        result_b = fc.same_shape_new_operands("3 + 4", rng_b)
        self.assertEqual(result_a, result_b)
        candidate, value = result_a
        self.assertNotEqual(candidate, "3 + 4")
        self.assertEqual(fc.evaluate(candidate), value)

    def test_same_shape_new_operands_avoids_forbidden_zero(self):
        rng = random.Random("seed")
        for _ in range(20):
            _, value = fc.same_shape_new_operands("-8 + 3 + 5", rng, forbid_zero_result=True)
            self.assertNotEqual(value, 0)

    def test_integer_operands_skips_decimal_digits(self):
        operands = [op.text for op in fc.integer_operands("2.5 + 1.3")]
        self.assertEqual(operands, [])


if __name__ == "__main__":
    unittest.main()
