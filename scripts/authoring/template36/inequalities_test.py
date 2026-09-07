"""Bounded-domain and statement-only adversarial checks for inequality recipes."""
import itertools
import json
import unittest

from .inequalities import ROOT, build
from .inequalities_answers import reconstruct, semantic_signature
from .inequalities_collisions import signature


class InequalityTemplates(unittest.TestCase):
    def test_exact_twelve_key_inventory(self):
        keys = {row["kp_id"] for row in build()}
        expected = {
            "checking-a-solution/kp1", "compound-inequalities/kp1",
            "graphing-inequalities-number-line/kp1", "graphing-inequalities-number-line/kp3",
            "graphing-linear-inequalities/kp1", "graphing-linear-inequalities/kp2",
            "interval-notation/kp2", "interval-notation/kp3", "solutions-of-inequalities/kp1",
            "substitution-with-isolated-variable/kp2", "writing-inequalities-from-statements/kp1",
            "writing-inequalities-from-statements/kp3"}
        self.assertEqual(keys, expected)

    def test_all_cartesian_cases_have_independent_answers_and_unique_premises(self):
        for row in build():
            args = row["arguments"]
            axes = args["params"]
            combinations = list(itertools.product(*(p["values"] for p in axes.values())))
            self.assertEqual(len(combinations), len(args["samples"]))
            seen = set()
            for sample in args["samples"]:
                problem = args["statement"].format(**sample["params"])
                self.assertEqual(reconstruct(row["kp_id"], problem), sample["expected"])
                signature = semantic_signature(row["kp_id"], problem)
                self.assertNotIn(signature, seen, (row["kp_id"], problem))
                seen.add(signature)
            self.assertGreaterEqual(len(seen), 12)
            self.assertGreaterEqual(len({s["expected"] for s in args["samples"]}), 2)

    def test_strictness_and_substitution_are_reconstructed_from_premises(self):
        key = "solutions-of-inequalities/kp1"
        self.assertEqual(reconstruct(key, "Is $-12 < -12$ true or false?"), "false")
        self.assertEqual(reconstruct(key, "Is $-13 < -12$ true or false?"), "true")
        key = "checking-a-solution/kp1"
        self.assertEqual(reconstruct(key, "Is $x=-6$ a solution of $7x-11=-53$?"), "yes")
        self.assertEqual(reconstruct(key, "Is $x=-6$ a solution of $7x-11=-52$?"), "no")
        key = "graphing-linear-inequalities/kp2"
        self.assertEqual(reconstruct(key, "Point $(2,-5)$ in $y <= 3x-11$?"), "yes")
        self.assertEqual(reconstruct(key, "Point $(2,-4)$ in $y <= 3x-11$?"), "no")

    def test_system_answer_satisfies_both_original_equations(self):
        row = next(r for r in build() if r["kp_id"].startswith("substitution-"))
        for sample in row["arguments"]["samples"]:
            a, b = sample["params"]["a"], sample["params"]["b"]
            # Independent finite search, separate from the algebraic reconstruction.
            pairs = [(x, 5*x+a) for x in range(-100, 101) if 5*x+a == 2*x+b]
            self.assertEqual(len(pairs), 1)
            self.assertEqual(str(pairs[0]), sample["expected"])

    def test_interval_domains_cover_both_conversion_directions(self):
        for row in build():
            if not row["kp_id"].startswith("interval-notation/"):
                continue
            samples = row["arguments"]["samples"]
            self.assertEqual(sum(s["params"]["s"].startswith("x") for s in samples), 6)
            self.assertEqual(sum("∞" in s["params"]["s"] for s in samples), 6)
        key = "interval-notation/kp2"
        self.assertEqual(reconstruct(key, "Convert $x <= -19$."), "(-∞, -19]")
        self.assertEqual(reconstruct(key, "Convert $x < -19$."), "(-∞, -19)")
        self.assertEqual(reconstruct(key, "Convert $[-19, ∞)$."), "x >= -19")
        self.assertEqual(reconstruct(key, "Convert $(-19, ∞)$."), "x > -19")
        key = "interval-notation/kp3"
        self.assertEqual(reconstruct(key, "Convert $(-∞, -13) ∪ [11, ∞)$."),
                         "x < -13 or x >= 11")

    def test_checked_in_manifest_is_reproducible(self):
        path = ROOT / "docs/content-foundations/template36/inequalities.json"
        self.assertEqual(json.loads(path.read_text()), build())

    def test_no_semantic_collision_against_entire_exported_corpus(self):
        path = ROOT / "target/template36-corpus.json"
        self.assertTrue(path.exists(), "Run root's exhaustive curriculum/pending corpus export first")
        corpus = json.loads(path.read_text())
        keys = {row["kp_id"] for row in build()}
        existing = set()
        recognized = skipped = 0
        for item in corpus:
            if item["source"].endswith("template36/inequalities.json"):
                continue
            candidate, reason = signature(item["problem"])
            if candidate is None:
                skipped += 1
                self.assertTrue(reason)
                self.assertNotIn(item["kp_id"], keys, item)
            else:
                recognized += 1
                existing.add(candidate)
        for row in build():
            for sample in row["arguments"]["samples"]:
                problem = row["arguments"]["statement"].format(**sample["params"])
                candidate, _ = signature(problem)
                self.assertIsNotNone(candidate, problem)
                self.assertNotIn(candidate, existing, (row["kp_id"], problem))
        print(f"inequality semantic corpus: recognized={recognized}, "
              f"other-family={skipped}, unsupported-same-key=0, collisions=0")

    def test_no_cross_lane_task_collisions(self):
        ours = set()
        for row in build():
            args = row["arguments"]
            for sample in args["samples"]:
                ours.add(signature(args["statement"].format(**sample["params"]))[0])
        rows = []
        for name in ("expressions", "graphs"):
            path = ROOT / f"docs/content-foundations/template36/{name}.json"
            rows.extend(json.loads(path.read_text()))
        for row in rows:
            args = row["arguments"]
            for sample in args["samples"]:
                problem = args["statement"].format(**sample["params"])
                candidate, _ = signature(problem)
                self.assertNotIn(candidate, ours, (row["kp_id"], problem))

    def test_cross_key_paraphrases_have_identical_task_signatures(self):
        pairs = [
            ("To graph $x \u2265 17$, is the circle at $17$ open or closed?",
             "For $x >= 17$, is its number-line endpoint circle open or closed?"),
            ('Write an inequality: "$x$ is at least $23$".',
             'Translate the phrase "$x$ is at least $23$" into an inequality.'),
            ("True or false: $-18 < -12$?", "Is $-18 < -12$ true or false?"),
            ("Solve the system $y=2x+18$, $y=5x-15$.",
             "Solve $y=5x+-15$ and $y=2x+18$ by setting the expressions for y equal.")]
        for first, second in pairs:
            self.assertIsNotNone(signature(first)[0])
            self.assertEqual(signature(first), signature(second))


if __name__ == "__main__":
    unittest.main()
