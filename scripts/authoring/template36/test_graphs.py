"""Exhaustive reconstruction, adversarial answers, and whole-corpus screening."""
from itertools import product
import json
import unittest

from graphs import DEST, ROOT, build
from graphs_collisions import collisions, families
from graphs_oracle import signature, verify


def instances(rows):
    for row in rows:
        args = row["arguments"]
        for sample in args["samples"]:
            yield row["kp_id"], args["statement"].format(**sample["params"]), sample["expected"]


class GraphRecipeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.rows = build()

    def test_committed_artifact_is_reproducible_and_scope_exact(self):
        self.assertEqual(self.rows, json.loads(DEST.read_text()))
        self.assertEqual(len(self.rows), 12)
        self.assertEqual(len({r["kp_id"] for r in self.rows}), 12)
        for row in self.rows:
            self.assertEqual(set(row), {"kp_id", "kind", "arguments"})
            self.assertEqual(row["kind"], "template")
            self.assertNotIn("status", row)

    def test_domains_exhaustive_and_every_parameter_changes_the_premise(self):
        for row in self.rows:
            args, key = row["arguments"], row["kp_id"]
            domains = {k: v["values"] for k, v in args["params"].items()}
            expected = set(product(*domains.values()))
            actual = {tuple(s["params"][k] for k in domains) for s in args["samples"]}
            self.assertEqual(expected, actual, key)
            self.assertEqual(len(actual), 12, key)
            self.assertEqual(args["constraints"], [])
            self.assertTrue(all(isinstance(x, int) for d in domains.values() for x in d))
            signatures = {signature(key, args["statement"].format(**s["params"]))
                          for s in args["samples"]}
            self.assertEqual(len(signatures), 12, key)
            self.assertGreater(len({s["expected"] for s in args["samples"]}), 1, key)

    def test_all_144_answers_are_independently_reconstructed(self):
        count = 0
        for key, statement, answer in instances(self.rows):
            verify(key, statement, answer)
            count += 1
        self.assertEqual(count, 144)

    def test_wrong_numeric_values_coordinates_and_boolean_fields_refuse(self):
        for key, statement, answer in instances(self.rows):
            if ";" in answer:
                parts = answer.split(";")
                for index, part in enumerate(parts):
                    name, value = part.split("=")
                    value = value.strip()
                    wrong = "no" if value == "yes" else "yes" if value == "no" else str(int(value)+1)
                    altered = parts.copy()
                    altered[index] = name+"="+wrong
                    with self.assertRaises(AssertionError):
                        verify(key, statement, ";".join(altered))
            elif answer.startswith("("):
                x, y = map(int, answer.strip("()").split(","))
                for wrong in (f"({x+1},{y})", f"({x},{y+1})", f"({y},{x})"):
                    if wrong != answer:
                        with self.assertRaises(AssertionError):
                            verify(key, statement, wrong)
            else:
                with self.assertRaises(AssertionError):
                    verify(key, statement, str(int(answer)+1))

    def test_qualitative_branches_and_context_constraints(self):
        by_key = {r["kp_id"]: r for r in self.rows}
        for kp in ("kp1", "kp3"):
            row = by_key["interpreting-graphs-qualitatively/"+kp]
            self.assertEqual(len({s["expected"] for s in row["arguments"]["samples"]}), 3)
        slope = by_key["interpreting-linear-models/kp1"]["arguments"]["samples"]
        self.assertTrue(any("filling = no" in s["expected"] for s in slope))
        self.assertTrue(any("filling = yes" in s["expected"] for s in slope))
        axis = by_key["plotting-points/kp3"]["arguments"]["samples"]
        self.assertTrue(all(s["expected"].endswith(",0)") for s in axis))
        self.assertTrue(any(s["expected"].startswith("(-") for s in axis))

    def test_hints_and_sketches_are_substantive_and_renderable(self):
        for row in self.rows:
            args = row["arguments"]
            self.assertEqual(len(args["hints"]), 2)
            for hint in args["hints"]:
                self.assertGreaterEqual(len(hint), 45)
                self.assertNotRegex(hint, r"\b\d+\b|answer is|appropriate rule")
            self.assertGreaterEqual(len(args["solution_sketch"]), 180)
            for sample in args["samples"]:
                sketch = args["solution_sketch"].format(**sample["params"])
                self.assertNotRegex(sketch, r"\{[a-z]+\}")

    def test_all_authored_and_pending_corpus_operand_screen(self):
        path = ROOT / "target/template36-corpus.json"
        self.assertTrue(path.exists(), "Run the real worker template36_production test first")
        corpus = json.loads(path.read_text())
        self.assertGreater(len(corpus), 100_000)
        keys = {r["kp_id"] for r in self.rows}
        for key in keys:
            authored = [r for r in corpus if r["kp_id"] == key and r["source"] == "authored"]
            self.assertGreaterEqual(len(authored), 4, key)
            self.assertTrue(all(families(key, r["problem"]) for r in authored))
        candidates = [(key, problem) for key, problem, _ in instances(self.rows)]
        self.assertEqual(collisions(candidates, corpus), [])

    def test_screen_catches_renaming_reordering_and_cross_key_duplicate_models(self):
        candidate = [("linear-word-problems/kp1", "A fee is 29 and rate is 7. Build a cost model.")]
        duplicate = [{"kp_id": "writing-expressions-from-patterns/kp2",
                      "problem": "Write an expression: a museum charges seven per visit plus 29 fixed fee."}]
        self.assertTrue(collisions(candidate, duplicate))
        duplicate[0]["problem"] = "Write an expression: a museum charges seven per visit plus 61 fixed fee."
        self.assertFalse(collisions(candidate, duplicate))


if __name__ == "__main__":
    unittest.main()
