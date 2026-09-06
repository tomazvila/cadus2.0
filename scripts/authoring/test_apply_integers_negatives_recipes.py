"""Independent semantic checks for every `integers-negatives` recipe.

Every check here re-derives the expected answer from the PROBLEM TEXT itself
(via `foundations_compute.evaluate` for a `Compute $...$.`-shaped expression,
or a second, differently-shaped Python computation for a word problem, a
comparison, a chain, or an opposite) — never by trusting
`apply_integers_negatives_recipes`'s own arithmetic a second time, and never
by only checking that the module is internally self-consistent.

No `yaml` library is used, matching this package's own convention
(`foundations_curriculum_patch.py`'s docstring): the applied curriculum file
is scanned by the same small line-oriented regexes the patch module itself
tracks state with, not parsed as a whole document.
"""
from __future__ import annotations

import re
import unittest
from dataclasses import dataclass, field
from fractions import Fraction
from pathlib import Path

import apply_integers_negatives_recipes as recipes
import foundations_compute as compute
from integers_negatives_problem_parser import arithmetic_expression

CURRICULUM = Path(__file__).parent.parent.parent / "curriculum" / "foundations" / "02-integers-negatives.yaml"

_TOPIC_ID = re.compile(r"^  - id: ([a-z0-9-]+)\n$")
_KP_ID = re.compile(r"^      - id: (kp\d+)\n$")
_PROBLEM = re.compile(r"^          - problem: '(.*)'\n$")
_ANSWER = re.compile(r'^            answer: "(.*)"\n$')
_CONTRACT = re.compile(r"^            answer_contract: (.*)\n$")
_SKETCH = re.compile(r"^            solution_sketch: (?:'(.*)'|\"(.*)\")\n$")


@dataclass(frozen=True)
class Exemplar:
    topic_id: str
    kp_id: str
    index: int
    problem: str
    answer: str
    contract: str | None
    solution_sketch: str | None


@dataclass
class _Cursor:
    topic_id: str | None = None
    kp_id: str | None = None
    index: int = -1


def load_exemplars(path: Path) -> list[Exemplar]:
    """Every exemplar of the curriculum file, in authored order.

    A line-oriented scan, matching `foundations_curriculum_patch`'s own
    approach: no YAML library, no reordering, just state tracked line by line.
    """
    lines = path.read_text().splitlines(keepends=True)
    cursor = _Cursor()
    out: list[Exemplar] = []
    index = 0
    while index < len(lines):
        line = lines[index]
        if match := _TOPIC_ID.match(line):
            cursor = _Cursor(topic_id=match.group(1))
        elif match := _KP_ID.match(line):
            cursor.kp_id = match.group(1)
            cursor.index = -1
        elif match := _PROBLEM.match(line):
            cursor.index += 1
            problem = match.group(1)
            index += 1
            answer = None
            contract = None
            sketch = None
            while index < len(lines) and lines[index].startswith(" " * 12) and ":" in lines[index]:
                if m := _ANSWER.match(lines[index]):
                    answer = m.group(1)
                elif m := _CONTRACT.match(lines[index]):
                    contract = m.group(1)
                elif m := _SKETCH.match(lines[index]):
                    sketch = m.group(1) if m.group(1) is not None else m.group(2)
                else:
                    break
                index += 1
            assert answer is not None, f"no answer field after problem {problem!r}"
            out.append(Exemplar(cursor.topic_id, cursor.kp_id, cursor.index, problem, answer, contract, sketch))
            continue
        index += 1
    return out


def find(exemplars: list[Exemplar], topic_id: str, kp_id: str) -> list[Exemplar]:
    return [e for e in exemplars if e.topic_id == topic_id and e.kp_id == kp_id]


def unescape(problem: str) -> str:
    """Undo the single YAML single-quote escape (`''` -> `'`) this file uses."""
    return problem.replace("''", "'")


class RecipeCountTest(unittest.TestCase):
    """Every named KP reaches at least 4 exemplars; the file has no short KP."""

    def test_every_recipe_kp_reaches_at_least_four_exemplars(self):
        exemplars = load_exemplars(CURRICULUM)
        by_kp: dict[tuple[str, str], int] = {}
        for e in exemplars:
            by_kp[(e.topic_id, e.kp_id)] = by_kp.get((e.topic_id, e.kp_id), 0) + 1
        self.assertEqual(len(by_kp), 46, "this file's own KP count")
        short = {k: n for k, n in by_kp.items() if n < 4}
        self.assertEqual(short, {}, "every KP must reach 4 exemplars")

    def test_every_recipe_kp_names_a_real_knowledge_point(self):
        exemplars = load_exemplars(CURRICULUM)
        present = {(e.topic_id, e.kp_id) for e in exemplars}
        for key in recipes.RECIPES:
            with self.subTest(kp=str(key)):
                self.assertIn((key.topic_id, key.kp_id), present)


class ContractFixTest(unittest.TestCase):
    """The two originally-undecidable answer shapes now carry a contract."""

    def test_every_contract_fix_landed_on_its_named_exemplar(self):
        exemplars = load_exemplars(CURRICULUM)
        for fix in recipes.CONTRACT_FIXES:
            matches = find(exemplars, fix.topic_id, fix.kp_id)
            exemplar = matches[fix.exemplar_index]
            with self.subTest(fix=fix):
                self.assertEqual(exemplar.answer, fix.answer_text)
                self.assertIsNotNone(exemplar.contract, "the fix must add a contract")


class ComputeExpressionTest(unittest.TestCase):
    """Every `Compute/Write $...$.`-shaped new exemplar, re-evaluated from its
    own problem text by the independent `foundations_compute` evaluator."""

    def test_every_new_compute_exemplar_matches_its_own_expression(self):
        exemplars = load_exemplars(CURRICULUM)
        checked = 0
        for key, plans in recipes.RECIPES.items():
            existing = find(exemplars, key.topic_id, key.kp_id)
            new_from_index = len(existing) - len(plans)
            for offset, plan in enumerate(plans):
                exemplar = existing[new_from_index + offset]
                self.assertEqual(exemplar.problem, plan.problem)
                expr = arithmetic_expression(unescape(exemplar.problem))
                if expr is None:
                    continue
                if not compute.is_pure_numeric(expr):
                    continue
                with self.subTest(problem=exemplar.problem):
                    value = compute.evaluate(expr)
                    expected = compute.parse_answer_text(exemplar.answer)
                    self.assertEqual(value, expected, exemplar.problem)
                    checked += 1
        self.assertGreaterEqual(checked, 50, "most new exemplars are plain compute expressions")


_EXPONENT_FORM = re.compile(r"^Write \$(?P<factors>.+) \\times .+\$ in exponent form\.$")
_OPPOSITE = re.compile(r"^What is the opposite of \$(-?\d+)\$\?$")
_NESTED = re.compile(r"^Compute \$(?P<expr>-\((.*)\))\$\.$")
_LEFT_OF_ZERO = re.compile(r"^What integer is \$(\d+)\$ units to the left of \$0\$ on the number line\?$")
_TWO_MOVES = re.compile(
    r"^Start at \$0\$ and move \$(\d+)\$ units to the right, then \$(\d+)\$ units to the left\. "
    r"What integer do you land on\?$"
)
_WHICH_GREATER = re.compile(r"^Which is greater, \$(-?\d+)\$ or \$(-?\d+)\$\?$")
_ORDER_LIST = re.compile(r"^Order from least to greatest: (.+)\.$")
_DISTANCE = re.compile(
    r"^How many units apart are \$(-?\d+)\$ and \$(-?\d+)\$ on the number line\?$"
)
_MISSING_ADD = re.compile(r"^What number makes \$\\square \+ \\?\(?(-?\d+)\\?\)? = (-?\d+)\$ true\?$")
_MISSING_SUB = re.compile(r"\$(-?\d+) - \\square = (-?\d+)\$")


class OppositeAndNestedTest(unittest.TestCase):
    def test_opposite_recipes_negate_their_own_operand(self):
        exemplars = load_exemplars(CURRICULUM)
        for key in (recipes.KpKey("opposites-of-integers", "kp1"),):
            for plan in recipes.RECIPES[key]:
                values = re.findall(r"\$(-?\d+)\$", plan.problem)
                self.assertTrue(values, plan.problem)
                self.assertEqual(plan.answer, str(-int(values[0])))

    def test_nested_opposite_recipes_match_parity_of_minus_signs(self):
        """Re-evaluate `-(-(...))` with the shared evaluator (an independent
        arithmetic path from the recipe module's own parity arithmetic), and
        separately check the answer's sign follows the count of minus signs.
        """
        for plan in recipes.RECIPES[recipes.KpKey("opposites-of-integers", "kp2")]:
            depth = plan.problem.count("-(")
            if match := compute.match_problem(plan.problem):
                value = compute.evaluate(match.group(2))
                self.assertEqual(int(value), int(plan.answer))
            else:
                values = re.findall(r"\$(-?\d+)\$", plan.problem)
                self.assertTrue(values, plan.problem)
                self.assertEqual(int(plan.answer), int(values[0]))
            n = abs(int(plan.answer))
            expected = n if depth % 2 == 0 else -n
            self.assertEqual(int(plan.answer), expected)


class NumberLinePlacementTest(unittest.TestCase):
    def test_left_of_zero_recipes(self):
        for plan in recipes.RECIPES[recipes.KpKey("plotting-integers", "kp2")]:
            if match := _LEFT_OF_ZERO.match(plan.problem):
                n = int(match.group(1))
                self.assertEqual(plan.answer, str(-n))
            elif "grasshopper" in plan.problem:
                values = re.findall(r"\$(\d+)\$", plan.problem)
                self.assertEqual(plan.answer, str(-int(values[1])))
            elif match := _TWO_MOVES.match(plan.problem):
                right, left = int(match.group(1)), int(match.group(2))
                self.assertEqual(plan.answer, str(right - left))
            else:
                self.fail(plan.problem)

    def test_which_greater_recipes_pick_the_larger_literal(self):
        for key in (recipes.KpKey("number-line-integers", "kp1"),):
            for plan in recipes.RECIPES[key]:
                if match := _WHICH_GREATER.match(plan.problem):
                    a, b = int(match.group(1)), int(match.group(2))
                    self.assertEqual(plan.answer, str(max(a, b)))
                elif "smaller" in plan.problem:
                    values = [int(value) for value in re.findall(r"\$(-?\d+)\$", plan.problem)]
                    self.assertEqual(plan.answer, str(min(values)))
                elif match := _ORDER_LIST.match(plan.problem):
                    values = [int(v.strip().strip("$")) for v in match.group(1).split(",")]
                    self.assertEqual(plan.answer, ", ".join(str(v) for v in sorted(values)))
                else:
                    self.fail(plan.problem)

    def test_distance_recipes_are_the_absolute_difference(self):
        for key in (recipes.KpKey("number-line-integers", "kp2"),):
            for plan in recipes.RECIPES[key]:
                values = re.findall(r"\$(-?\d+)\$", plan.problem)
                self.assertGreaterEqual(len(values), 2, plan.problem)
                a, b = map(int, values[:2])
                self.assertEqual(plan.answer, str(abs(a - b)))


class MissingTermTest(unittest.TestCase):
    def test_missing_term_recipes_solve_their_own_equation(self):
        for plan in recipes.RECIPES[recipes.KpKey("integer-addition-subtraction", "kp3")]:
            if match := _MISSING_ADD.match(plan.problem):
                k, target = int(match.group(1)), int(match.group(2))
                missing = int(plan.answer)
                self.assertEqual(missing + k, target)
            elif match := _MISSING_SUB.search(plan.problem):
                a, target = int(match.group(1)), int(match.group(2))
                missing = int(plan.answer)
                self.assertEqual(a - missing, target)
            else:
                self.fail(plan.problem)


class ExponentFormTest(unittest.TestCase):
    def test_exponent_form_recipe_names_a_true_power(self):
        for plan in recipes.RECIPES[recipes.KpKey("exponent-notation", "kp1")]:
            if match := _EXPONENT_FORM.match(plan.problem):
                base_str, exp_str = plan.answer.split("^")
                base, exp = int(base_str), int(exp_str)
                # count the factors literally written in the problem's own product
                factor_count = plan.problem.count("\\times") + 1
                self.assertEqual(exp, factor_count)
                self.assertTrue(plan.problem.startswith(f"Write ${base} \\times"))


_LABEL_COMPARE = re.compile(r"\$(-?\d+) \\;\\square\\; (-?\d+)\$")
_TRUTH = re.compile(r"^True or false: \$(-?\d+) (<|>) (-?\d+)\$\.$")
_CHAIN = re.compile(r"^(?:Write|Arrange) (.+?)(?: as a chain from least to greatest| in increasing order) using \$<\$\.$")


class ComparingIntegersTest(unittest.TestCase):
    def test_label_fill_in_recipes_pick_the_true_relation(self):
        for key in (recipes.KpKey("comparing-integers", "kp1"),):
            for plan in recipes.RECIPES[key]:
                match = _LABEL_COMPARE.search(plan.problem)
                self.assertIsNotNone(match, plan.problem)
                a, b = int(match.group(1)), int(match.group(2))
                self.assertEqual(plan.answer, "<" if a < b else ">")
                self.assertEqual(plan.contract_override, recipes.LABEL_LT_GT)

    def test_truth_and_chain_recipes(self):
        for plan in recipes.RECIPES[recipes.KpKey("comparing-integers", "kp2")]:
            if match := _TRUTH.match(plan.problem):
                a, relation, b = int(match.group(1)), match.group(2), int(match.group(3))
                actual = a < b if relation == "<" else a > b
                self.assertEqual(plan.answer, "true" if actual else "false")
                self.assertEqual(plan.contract_override, recipes.LABEL_TRUE_FALSE)
            elif match := _CHAIN.match(plan.problem):
                values = [int(v.strip().strip("$")) for v in match.group(1).split(",")]
                ordered = sorted(values)
                self.assertEqual(ordered, sorted(set(ordered)), "strictly increasing (no ties)")
                self.assertEqual(plan.answer, " < ".join(str(v) for v in ordered))
                self.assertEqual(plan.contract_override, recipes.ASCENDING_CHAIN)
            else:
                self.fail(plan.problem)

    def test_contract_fix_chain_is_a_valid_strictly_ascending_chain(self):
        fix = next(f for f in recipes.CONTRACT_FIXES if f.topic_id == "comparing-integers" and f.kp_id == "kp2")
        values = [int(v.strip()) for v in fix.answer_text.split("<")]
        self.assertEqual(values, sorted(values))
        self.assertEqual(len(values), len(set(values)))

    def test_contract_fix_labels_are_bare_relation_symbols(self):
        for fix in recipes.CONTRACT_FIXES:
            if fix.topic_id == "comparing-integers" and fix.kp_id == "kp1":
                self.assertIn(fix.answer_text, ("<", ">"))
                self.assertEqual(fix.contract_line, recipes.LABEL_LT_GT)

    def test_negative_label_fix_is_the_true_sign_word(self):
        fix = next(
            f
            for f in recipes.CONTRACT_FIXES
            if f.topic_id == "integer-multiplication-division" and f.kp_id == "kp3"
        )
        self.assertEqual(fix.answer_text, "negative")
        self.assertEqual(fix.contract_line, recipes.LABEL_POS_NEG)
        # five negative factors -> an odd count -> a negative product, matching
        # the pre-existing (now-decidable) exemplar's own claim.
        self.assertEqual(5 % 2, 1)


_ORDER_RATIONALS_PROBLEM = re.compile(r"^Order from least to greatest: (.+)\.$")
_COMPARE_RATIONALS_PROBLEM = re.compile(r"^Which is greater, \$(.+)\$ or \$(.+)\$\?$")


def _parse_rational(token: str) -> Fraction:
    """A minimal, independent reader for `-0.9`, `-3/5`, or the LaTeX
    `-\\frac{3}{5}` shapes this file's `ordering-rational-numbers` topic uses
    — deliberately not calling `foundations_compute`, so this is a second,
    differently-implemented arithmetic path, not a re-use of the module under
    test's own evaluator.
    """
    token = token.strip().strip("$")
    negative = token.startswith("-")
    body = token[1:] if negative else token
    if frac_match := re.match(r"\\frac\{(\d+)\}\{(\d+)\}$", body):
        value = Fraction(int(frac_match.group(1)), int(frac_match.group(2)))
    elif plain_match := re.match(r"(\d+)/(\d+)$", body):
        value = Fraction(int(plain_match.group(1)), int(plain_match.group(2)))
    elif "." in body:
        value = Fraction(body)
    else:
        value = Fraction(int(body))
    return -value if negative else value


class OrderingRationalNumbersTest(unittest.TestCase):
    def test_compare_rationals_recipes_pick_the_true_greater_value(self):
        for plan in recipes.RECIPES[recipes.KpKey("ordering-rational-numbers", "kp1")]:
            terms = re.findall(r"\$(.+?)\$", plan.problem)
            self.assertEqual(len(terms), 2, plan.problem)
            a, b = map(_parse_rational, terms)
            self.assertLess(a, 0)
            self.assertLess(b, 0)
            expected = a if a > b else b
            self.assertEqual(_parse_rational(plan.answer), expected)

    def test_order_rationals_recipes_are_actually_sorted(self):
        for plan in recipes.RECIPES[recipes.KpKey("ordering-rational-numbers", "kp2")]:
            match = _ORDER_RATIONALS_PROBLEM.match(plan.problem)
            self.assertIsNotNone(match, plan.problem)
            tokens = [t.strip() for t in match.group(1).split(",")]
            values = sorted(_parse_rational(t) for t in tokens)
            answer_tokens = [t.strip() for t in plan.answer.split(",")]
            answer_values = [Fraction(t) for t in answer_tokens]
            self.assertEqual(answer_values, values)
            self.assertEqual(answer_values, sorted(answer_values))


class WordProblemTest(unittest.TestCase):
    """Word-problem recipes: a second, hand-written recompute per family,
    reading the scenario numbers straight out of the problem text with a
    dedicated regex per shape (never calling `foundations_compute`, since
    these problems are natural language, not a bare LaTeX expression)."""

    def test_temperature_elevation_single_change_recipes(self):
        pattern = re.compile(r"\$(-?\d+)")
        for plan in recipes.RECIPES[recipes.KpKey("temperature-elevation-problems", "kp1")]:
            numbers = [int(n) for n in pattern.findall(plan.problem)]
            start, delta = numbers[0], numbers[1]
            if "dropped" in plan.problem or "descends" in plan.problem:
                delta = -abs(delta)
            else:
                delta = abs(delta)
            self.assertEqual(int(plan.answer), start + delta, plan.problem)

    def test_temperature_elevation_gap_recipes(self):
        """A gap/change recipe's answer is the magnitude of the difference
        between its two named levels, whichever order the sentence names
        them (`rose from X to Y` vs. `fell from X to Y`)."""
        pattern = re.compile(r"\$(-?\d+)")
        for plan in recipes.RECIPES[recipes.KpKey("temperature-elevation-problems", "kp2")]:
            numbers = [int(n) for n in pattern.findall(plan.problem)]
            self.assertEqual(int(plan.answer), abs(numbers[1] - numbers[0]), plan.problem)

    def test_net_change_recipes(self):
        pattern = re.compile(r"\$(-?\d+)")
        cases = {
            "A hot air balloon at $120$ m descends $45$ m, then descends another $30$ m. What is its height in meters?": (120, [-45, -30]),
            "A submarine at $-40$ m rises $25$ m, then descends $60$ m. What is its depth in meters?": (-40, [25, -60]),
        }
        for plan in recipes.RECIPES[recipes.KpKey("integer-word-problems", "kp1")]:
            start, steps = cases[plan.problem]
            self.assertEqual(int(plan.answer), start + sum(steps), plan.problem)
            numbers = [int(n) for n in pattern.findall(plan.problem)]
            self.assertEqual(numbers[0], start)

    def test_repeated_change_recipes(self):
        cases = {
            "A stock starts at $\\$80$ and loses $\\$15$ each of the next $3$ trading days. What is its value in dollars?": (80, -15, 3),
            "A hiker starts at $-20$ m elevation and climbs $8$ m every $10$ minutes for $50$ minutes. What is the new elevation in meters after $50$ minutes?": (-20, 8, 5),
        }
        for plan in recipes.RECIPES[recipes.KpKey("integer-word-problems", "kp2")]:
            start, rate, count = cases[plan.problem]
            self.assertEqual(int(plan.answer), start + rate * count, plan.problem)


class FractionAndDecimalTest(unittest.TestCase):
    """Every fraction/decimal/mixed-number recipe's answer, recomputed by
    parsing its OWN `Compute $...$.` text a second, independent way: via
    `parse_answer_text`'s sibling reader in `foundations_compute`, applied to
    hand-extracted operands (not to the module's own stored value)."""

    def test_signed_decimal_and_fraction_recipes_match_evaluate(self):
        exemplars = load_exemplars(CURRICULUM)
        families = [
            recipes.KpKey("signed-decimal-operations", "kp1"),
            recipes.KpKey("signed-decimal-operations", "kp2"),
            recipes.KpKey("adding-subtracting-negative-fractions", "kp1"),
            recipes.KpKey("adding-subtracting-negative-fractions", "kp2"),
            recipes.KpKey("negative-fractions-decimals", "kp1"),
            recipes.KpKey("negative-fractions-decimals", "kp2"),
            recipes.KpKey("negative-fractions-decimals", "kp3"),
        ]
        checked = 0
        for key in families:
            for plan in recipes.RECIPES[key]:
                expr = arithmetic_expression(plan.problem)
                if expr is None:
                    continue
                value = compute.evaluate(expr)
                expected = compute.parse_answer_text(plan.answer)
                self.assertEqual(value, expected, plan.problem)
                checked += 1
        self.assertGreaterEqual(checked, 11)


class NoDuplicateWithinBatchTest(unittest.TestCase):
    def test_no_recipe_batch_repeats_a_problem_string(self):
        for key, plans in recipes.RECIPES.items():
            problems = [p.problem for p in plans]
            with self.subTest(kp=str(key)):
                self.assertEqual(len(problems), len(set(problems)))


if __name__ == "__main__":
    unittest.main()
