"""Tests of `foundations_curriculum_patch`."""
import tempfile
import unittest
from pathlib import Path

from foundations_curriculum_patch import ExemplarKey, Rejection, apply_solution_sketches

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


class ApplyTest(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.path = Path(self.dir.name) / "fixture.yaml"
        self.path.write_text(FIXTURE)

    def tearDown(self):
        self.dir.cleanup()

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


if __name__ == "__main__":
    unittest.main()
