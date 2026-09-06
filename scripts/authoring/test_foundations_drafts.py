"""Tests of `foundations_drafts`."""
import json
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


class ClassifyKpTest(unittest.TestCase):
    def test_arithmetic_kp_is_a_candidate(self):
        topic, kp = find_kp("single-digit-addition", "kp1")
        rng = random.Random(f"{topic['id']}/{kp['id']}")
        candidate = fd.classify_kp(topic, kp, rng)
        self.assertIsNotNone(candidate)
        self.assertEqual(candidate.kp_key, "single-digit-addition/kp1")
        self.assertEqual(fc.evaluate(candidate.candidate_expr), candidate.answer)
        served = {fc.parse_answer_text(e["answer"]) for e in kp["exemplars"]}
        self.assertNotIn(candidate.answer, served)
        for exemplar in kp["exemplars"]:
            self.assertNotEqual(f"Compute ${candidate.candidate_expr}$.", exemplar["problem"])

    def test_word_problem_kp_is_not_a_candidate(self):
        topic, kp = find_kp("fraction-basics", "kp1")
        rng = random.Random("seed")
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
        self.assertGreater(checked, 50)


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


if __name__ == "__main__":
    unittest.main()
