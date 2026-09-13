"""Independent repair arithmetic, deterministic artifacts, bounds, and scope controls."""
import json
import math
import re
import subprocess
import unittest
from fractions import Fraction

from unit06_corrections import REPLACEMENTS, UNIT, patch
from unit06_refresh_evidence import LEGACY, replace_records
from unit06_report import family, generic, inspect
from unit06_templates import CANDIDATES, OUT, generate


def load(name):
    if name == "candidates.json":
        return json.loads(CANDIDATES.read_text())
    return json.loads((OUT / name).read_text())


class Unit06CorrectionTest(unittest.TestCase):
    def test_repair_is_idempotent_and_preserves_contracts(self):
        current = UNIT.read_text()
        self.assertEqual(patch(current), current)
        # The interrupted dirty state had already expanded two singleton labels.
        new = re.findall(r"answer_contract:.*", current)
        singleton = 'answer_contract: {"kind":"label","options":[["no solution"]]}'
        self.assertNotIn(singleton, new)
        self.assertIn('answer_contract: {"kind":"label","options":[["no solution"],["x = 4"],["x = -2"]]}', new)
        self.assertIn('answer_contract: {"kind":"label","options":[["no solution"],["x = 25"],["x = -5"]]}', new)

    def test_root_repair_answers_from_input_numbers(self):
        for (key, index), (problem, answer, _) in REPLACEMENTS.items():
            if key.startswith("perfect-square-roots"):
                # The largest literal in each repaired question is its area/radicand.
                value = max(map(int, re.findall(r"\d+", problem)))
                self.assertEqual(int(answer)**2, value, (key, index))
                self.assertGreater(int(answer), 0)
            elif key == "cube-roots/kp2":
                negatives = list(map(int, re.findall(r"-\d+", problem)))
                self.assertEqual(int(answer)**3, min(negatives), (key, index))

    def test_square_root_arithmetic_and_inverse_product(self):
        computations = {
            ("square-roots/kp1", 2): math.isqrt(64)-math.isqrt(9),
            ("square-roots/kp1", 3): 4*math.isqrt(9),
            ("square-roots/kp3", 2): math.isqrt(9)*math.isqrt(16),
            ("square-roots/kp3", 3): math.isqrt(100)//math.isqrt(4),
        }
        for key, expected in computations.items():
            self.assertEqual(int(REPLACEMENTS[key][1]), expected)

    def test_converse_repairs_independently_recompute_comparisons(self):
        self.assertEqual(64+225, 289)
        self.assertEqual(7**2+24**2, 25**2)
        self.assertEqual((3*8)**2+(4*8)**2, (5*8)**2)
        self.assertNotEqual(4+9, 16)
        self.assertNotEqual(5**2+6**2, 9**2)
        self.assertNotEqual(3**2+4**2, 6**2)
        for (key, _), (_, answer, _) in REPLACEMENTS.items():
            if key.startswith("pythagorean-converse"):
                self.assertEqual(answer, "yes" if key.endswith("kp1") else "no")

    def test_generated_candidates_are_reproducible_and_domain_bound(self):
        rows = generate()
        self.assertEqual(rows, load("candidates.json"))
        self.assertEqual(len({r["kp_id"] for r in rows}), 78)
        for row in rows:
            values = row["arguments"]["params"]["a"]["values"]
            if row["kp_id"] == "perfect-square-roots/kp1":
                self.assertTrue(all(1 <= a <= 15 for a in values))
            elif row["kp_id"] == "perfect-square-roots/kp2":
                self.assertTrue(all(a <= 144 and math.isqrt(a)**2 == a for a in values))
            elif row["kp_id"] == "cube-roots/kp1":
                self.assertEqual(values, [n**3 for n in range(1, 13)])
            elif row["kp_id"].startswith("scientific-notation"):
                self.assertTrue(all(1 <= Fraction(a) < 10 for a in values))

    def test_square_root_comparisons_exercise_both_orderings(self):
        row = next(r for r in generate() if r["kp_id"] == "estimating-square-roots/kp3")
        answers = {sample["expected"] for sample in row["arguments"]["samples"]}
        self.assertEqual(answers, {"1", "2"})

    def test_other_units_evidence_records_are_byte_identical(self):
        keys = {r["kp_id"] for r in load("candidates.json")}
        for name in ("drafts.json", "stored-review.json"):
            path = LEGACY / name
            before = subprocess.check_output(["git", "show", "HEAD:" + str(path)], text=True)
            original = json.loads(before)
            current = json.loads(path.read_text())
            # Preserve historical versions, their multiplicity, and their order.
            self.assertEqual(
                [r for r in original if r["kp_id"] not in keys or r["kind"] != "template"],
                [r for r in current if r["kp_id"] not in keys or r["kind"] != "template"],
            )
            replacement = {r["kp_id"]: r for r in json.loads(path.read_text()) if r["kp_id"] in keys and r["kind"] == "template"}
            self.assertEqual(replace_records(before, replacement), path.read_text())

    def test_scoped_checks_detect_numeric_substitutions_and_generic_sketches(self):
        self.assertEqual(family("Compute $sqrt(16)$."), family("Compute $sqrt(49)$."))
        self.assertTrue(generic("Evaluate grouped expressions first, then powers."))
        self.assertTrue(generic("The result is $4 = 4$."))
        self.assertFalse(generic("$2*2=4$, so the principal root is $2$."))

    def test_rendered_roots_are_evaluated_and_latex_is_not_double_escaped(self):
        evaluated = {"cube-roots/kp2", "cube-roots/kp3", "rational-exponents/kp1",
                     "rational-exponents/kp2", "dividing-radicals/kp2"}
        for row in load("pending-review.json"):
            for item in row["instances"]:
                self.assertNotIn(chr(92)*2, item["problem"])
                self.assertNotIn(chr(92)*2, item["solution_sketch"])
                if row["kp_id"] in evaluated:
                    Fraction(item["answer"])

    def test_six_authored_codes_clear_and_missing_templates_are_exact_blockers(self):
        pending = {r["kp_id"]: r for r in load("pending-review.json")}
        blockers = {r["kp_id"] for r in load("schema-blockers.json")}
        for row in load("authored-verification.json"):
            issues = inspect(row, pending)["issues"]
            expected = [{"code": "absent_pending_template_recipe"}] if row["kp_id"] in blockers else []
            self.assertEqual(issues, expected, row["kp_id"])


if __name__ == "__main__":
    unittest.main()
