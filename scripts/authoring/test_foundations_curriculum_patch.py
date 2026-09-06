"""Tests of `foundations_curriculum_patch`."""
import tempfile
import unittest
from pathlib import Path

from foundations_curriculum_patch import (
    ExemplarKey,
    ExemplarPatch,
    KpKey,
    NewExemplar,
    Rejection,
    apply_solution_sketches,
    insert_answer_contracts,
    insert_exemplars,
    patch_exemplars,
)

FIXTURE = """\
topics:
  - id: adding-integers
    name: Adding Integers
    knowledge_points:
      - id: kp1
        name: Same sign
        exemplars:
          - problem: 'Compute $-3 + (-5)$.'
            answer: "-8"
            answer_contract: {"kind":"exact"}
          - problem: 'Compute $-7 + (-2)$.'
            answer: "-9"
            answer_contract: {"kind":"exact"}
            solution_sketch: 'Already authored.'
        constraints: "small integers"
      - id: kp2
        name: Opposite sign
        exemplars:
          - problem: 'Compute $-7 + 4$.'
            answer: "-3"
        constraints: "small integers"
  - id: other-topic
    knowledge_points:
      - id: kp1
        name: Unrelated
        exemplars:
          - problem: 'Compute $1 + 1$.'
            answer: "2"
        constraints: "n/a"
"""


class FixtureFileCase(unittest.TestCase):
    """A fresh copy of `FIXTURE` at `self.path`, for one test."""

    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.path = Path(self.dir.name) / "fixture.yaml"
        self.path.write_text(FIXTURE)

    def tearDown(self):
        self.dir.cleanup()


class ApplyTest(FixtureFileCase):
    def test_inserts_a_missing_sketch_after_the_last_field(self):
        key = ExemplarKey("adding-integers", "kp1", 0)
        text, applied = apply_solution_sketches(
            self.path, {key: "Combine same-signed values: $-3 + (-5) = -8$."}, write=False
        )
        self.assertEqual(applied, [key])
        lines = text.splitlines()
        problem_line = next(i for i, l in enumerate(lines) if "'Compute $-3 + (-5)$.'" in l)
        self.assertIn("answer_contract", lines[problem_line + 2])
        self.assertIn("solution_sketch: 'Combine same-signed", lines[problem_line + 3])
        # the file's byte length grows by exactly one inserted line
        self.assertEqual(len(text.splitlines()), len(FIXTURE.splitlines()) + 1)

    def test_second_exemplar_of_a_kp_is_addressed_by_index(self):
        key = ExemplarKey("adding-integers", "kp2", 0)
        text, applied = apply_solution_sketches(
            self.path, {key: "Add the opposite: $-7 + 4 = -3$."}, write=False
        )
        self.assertEqual(applied, [key])
        self.assertIn("solution_sketch: 'Add the opposite: $-7 + 4 = -3$.'", text)

    def test_write_true_persists_the_change(self):
        key = ExemplarKey("other-topic", "kp1", 0)
        apply_solution_sketches(self.path, {key: "One plus one is two: $1 + 1 = 2$."}, write=True)
        self.assertIn("One plus one is two", self.path.read_text())

    def test_refuses_an_already_sketched_exemplar_and_writes_nothing(self):
        key = ExemplarKey("adding-integers", "kp1", 1)
        before = self.path.read_text()
        with self.assertRaises(Rejection):
            apply_solution_sketches(self.path, {key: "New text"}, write=True)
        self.assertEqual(self.path.read_text(), before)

    def test_refuses_an_unknown_key_and_writes_nothing(self):
        key = ExemplarKey("adding-integers", "kp1", 9)
        before = self.path.read_text()
        with self.assertRaises(Rejection):
            apply_solution_sketches(self.path, {key: "New text"}, write=True)
        self.assertEqual(self.path.read_text(), before)

    def test_refuses_a_sketch_with_a_literal_quote(self):
        key = ExemplarKey("adding-integers", "kp2", 0)
        with self.assertRaises(Rejection):
            apply_solution_sketches(self.path, {key: "It's wrong"}, write=False)

    def test_topics_do_not_leak_into_each_other(self):
        key = ExemplarKey("other-topic", "kp1", 0)
        text, applied = apply_solution_sketches(
            self.path, {key: "Sketch for the unrelated topic."}, write=False
        )
        self.assertEqual(applied, [key])
        # the adding-integers kp2 exemplar (also index 0, same problem shape)
        # must NOT have received the sketch meant for other-topic/kp1
        lines = text.splitlines()
        kp2_line = next(i for i, l in enumerate(lines) if "'Compute $-7 + 4$.'" in l)
        self.assertNotIn("Sketch for the unrelated topic", lines[kp2_line + 1])


class InsertExemplarsTest(FixtureFileCase):
    def test_appends_after_the_kps_last_exemplar_with_contract(self):
        key = KpKey("adding-integers", "kp1")
        new = NewExemplar(
            problem="Compute $-1 + (-1)$.",
            answer="-2",
            solution_sketch="Same sign: add the sizes, keep the sign. $-1 + (-1) = -2$.",
            with_contract=True,
        )
        text, applied = insert_exemplars(self.path, {key: [new]}, write=False)
        self.assertEqual(applied, [key])
        lines = text.splitlines()
        inserted = next(i for i, l in enumerate(lines) if "-1 + (-1)" in l)
        self.assertIn("answer: \"-2\"", lines[inserted + 1])
        self.assertIn('answer_contract: {"kind":"exact"}', lines[inserted + 2])
        self.assertIn("solution_sketch:", lines[inserted + 3])
        # it lands before kp1's constraints line, i.e. still inside kp1
        constraints_line = next(i for i, l in enumerate(lines) if "small integers" in l)
        self.assertLess(inserted, constraints_line)

    def test_appends_without_contract_when_the_kp_has_none(self):
        key = KpKey("adding-integers", "kp2")
        new = NewExemplar(
            problem="Compute $-9 + 2$.",
            answer="-7",
            solution_sketch="Take the difference of the sizes: $-9 + 2 = -7$.",
            with_contract=False,
        )
        text, _ = insert_exemplars(self.path, {key: [new]}, write=False)
        lines = text.splitlines()
        inserted = next(i for i, l in enumerate(lines) if "-9 + 2" in l)
        self.assertNotIn("answer_contract", lines[inserted + 2])

    def test_contract_override_wins_over_with_contract(self):
        key = KpKey("adding-integers", "kp2")
        new = NewExemplar(
            problem="Compute $9 \\div 2$.",
            answer="4 R1",
            solution_sketch="sketch",
            with_contract=True,
            contract_override='{"kind":"quotient_remainder","divisor":2}',
        )
        text, _ = insert_exemplars(self.path, {key: [new]}, write=False)
        lines = text.splitlines()
        inserted = next(i for i, l in enumerate(lines) if "9 \\div 2" in l)
        self.assertIn('answer_contract: {"kind":"quotient_remainder","divisor":2}', lines[inserted + 2])

    def test_multiple_new_exemplars_land_in_order(self):
        key = KpKey("other-topic", "kp1")
        news = [
            NewExemplar("Compute $2 + 2$.", "4", "Add: $2 + 2 = 4$.", False),
            NewExemplar("Compute $3 + 3$.", "6", "Add: $3 + 3 = 6$.", False),
        ]
        text, _ = insert_exemplars(self.path, {key: news}, write=False)
        first = text.index("2 + 2")
        second = text.index("3 + 3")
        self.assertLess(first, second)

    def test_write_true_persists_the_change(self):
        key = KpKey("other-topic", "kp1")
        new = NewExemplar("Compute $9 + 9$.", "18", "Add: $9 + 9 = 18$.", False)
        insert_exemplars(self.path, {key: [new]}, write=True)
        self.assertIn("9 + 9", self.path.read_text())

    def test_refuses_an_unknown_key_and_writes_nothing(self):
        key = KpKey("adding-integers", "kp9")
        before = self.path.read_text()
        new = NewExemplar("Compute $1 + 1$.", "2", "sketch", False)
        with self.assertRaises(Rejection):
            insert_exemplars(self.path, {key: [new]}, write=True)
        self.assertEqual(self.path.read_text(), before)

    def test_kp2_addition_does_not_leak_into_kp1(self):
        key = KpKey("adding-integers", "kp2")
        new = NewExemplar("Compute $-5 + 5$.", "0", "sketch text", False)
        text, _ = insert_exemplars(self.path, {key: [new]}, write=False)
        lines = text.splitlines()
        kp1_constraints = next(i for i, l in enumerate(lines) if "small integers" in l)
        self.assertNotIn("-5 + 5", "\n".join(lines[: kp1_constraints + 1]))


class InsertAnswerContractsTest(FixtureFileCase):
    def assert_contract_rejected(self, key):
        before = self.path.read_text()
        with self.assertRaises(Rejection):
            insert_answer_contracts(self.path, {key: '{"kind":"exact"}'}, write=True)
        self.assertEqual(self.path.read_text(), before)

    def test_inserts_a_contract_right_after_answer(self):
        key = ExemplarKey("adding-integers", "kp2", 0)
        text, applied = insert_answer_contracts(
            self.path, {key: '{"kind":"exact"}'}, write=False
        )
        self.assertEqual(applied, [key])
        lines = text.splitlines()
        answer_line = next(i for i, l in enumerate(lines) if 'answer: "-3"' in l)
        self.assertIn('answer_contract: {"kind":"exact"}', lines[answer_line + 1])

    def test_lands_before_an_existing_solution_sketch(self):
        # kp1/1 already has answer_contract (exact); use a fresh row with a
        # sketch but no contract instead, to test ordering against a sketch.
        text = FIXTURE.replace(
            '          - problem: \'Compute $-7 + 4$.\'\n            answer: "-3"\n',
            '          - problem: \'Compute $-7 + 4$.\'\n            answer: "-3"\n'
            "            solution_sketch: 'existing'\n",
        )
        self.path.write_text(text)
        key = ExemplarKey("adding-integers", "kp2", 0)
        new_text, applied = insert_answer_contracts(
            self.path, {key: '{"kind":"exact"}'}, write=False
        )
        self.assertEqual(applied, [key])
        lines = new_text.splitlines()
        answer_line = next(i for i, l in enumerate(lines) if 'answer: "-3"' in l)
        self.assertIn("answer_contract", lines[answer_line + 1])
        self.assertIn("solution_sketch: 'existing'", lines[answer_line + 2])

    def test_write_true_persists_the_change(self):
        key = ExemplarKey("other-topic", "kp1", 0)
        insert_answer_contracts(self.path, {key: '{"kind":"exact"}'}, write=True)
        self.assertIn('answer_contract: {"kind":"exact"}', self.path.read_text())

    def test_refuses_an_already_contracted_exemplar_and_writes_nothing(self):
        key = ExemplarKey("adding-integers", "kp1", 0)
        self.assert_contract_rejected(key)

    def test_refuses_an_unknown_key_and_writes_nothing(self):
        key = ExemplarKey("adding-integers", "kp1", 9)
        self.assert_contract_rejected(key)

    def test_refuses_an_exemplar_without_an_answer_and_writes_nothing(self):
        self.path.write_text(FIXTURE.replace('            answer: "-3"\n', ""))
        key = ExemplarKey("adding-integers", "kp2", 0)
        before = self.path.read_text()
        with self.assertRaisesRegex(Rejection, "has no answer field"):
            insert_answer_contracts(self.path, {key: '{"kind":"exact"}'}, write=True)
        self.assertEqual(self.path.read_text(), before)


class PatchExemplarsTest(FixtureFileCase):
    def test_replaces_exact_fields_without_touching_the_answer(self):
        key = ExemplarKey("adding-integers", "kp1", 1)
        text, applied = patch_exemplars(
            self.path,
            {key: ExemplarPatch(problem="Compute $-8 + (-1)$.", solution_sketch="Add to get $-9$.")},
            write=False,
        )
        self.assertEqual(applied, [key])
        self.assertIn("problem: 'Compute $-8 + (-1)$.'", text)
        self.assertIn('answer: "-9"', text)
        self.assertIn("solution_sketch: 'Add to get $-9$.'", text)

    def test_unknown_patch_fails_before_writing(self):
        before = self.path.read_text()
        with self.assertRaises(Rejection):
            patch_exemplars(
                self.path,
                {ExemplarKey("missing", "kp1", 0): ExemplarPatch(problem="No")},
                write=True,
            )
        self.assertEqual(self.path.read_text(), before)


if __name__ == "__main__":
    unittest.main()
