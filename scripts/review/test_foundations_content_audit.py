#!/usr/bin/env python3
"""Negative controls for the fail-closed Foundations content audit."""
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import foundations_content_audit as audit


class FoundationsContentAuditTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.here = Path(__file__).resolve().parent
        cls.facts = json.loads((cls.here / "testdata/content_audit_facts.json").read_text())
        cls.documents = cls.here / "testdata/content_audit_documents"
        cls.templates = audit.pending_templates(cls.documents)
        cls.report = audit.build_report(cls.facts, cls.templates)
        cls.rows = {row["kp_key"]: row for row in cls.report["kps"]}

    def codes(self, key):
        return {row["code"] for row in self.rows[key]["issues"]}

    def test_clean_control_has_no_findings(self):
        self.assertEqual(self.rows["clean/kp1"]["issues"], [])

    def test_each_failure_class_is_attributed_to_its_exact_kp(self):
        expected = {
            "short/kp1": "fewer_than_four_exemplars",
            "missing/kp1": "missing_solution_sketch",
            "undecidable/kp1": "undecidable_authored_answer",
            "singleton/kp1": "singleton_label_contract",
            "duplicate/kp1": "duplicate_problem_answer_family",
            "absent-template/kp1": "absent_pending_template_recipe",
            "generic/kp1": "generic_or_tautological_sketch",
            "tautology/kp1": "generic_or_tautological_sketch",
            "template-generic/kp1": "generic_or_tautological_sketch",
        }
        for key, code in expected.items():
            with self.subTest(key=key):
                self.assertIn(code, self.codes(key))

    def test_missing_and_empty_sketches_are_both_reported(self):
        issue = next(row for row in self.rows["missing/kp1"]["issues"]
                     if row["code"] == "missing_solution_sketch")
        self.assertEqual(issue["evidence"], [1, 2])

    def test_numeric_variants_share_a_family_but_distinct_operations_do_not(self):
        issue = next(row for row in self.rows["duplicate/kp1"]["issues"]
                     if row["code"] == "duplicate_problem_answer_family")
        self.assertEqual(issue["evidence"][0]["indices"], [0, 1])
        self.assertNotEqual(audit.family("Compute 2 + 3"), audit.family("Compute 2 - 3"))

    def test_generic_and_tautological_markers_are_distinct(self):
        generic = next(row for row in self.rows["generic/kp1"]["issues"]
                       if row["code"] == "generic_or_tautological_sketch")
        tautology = next(row for row in self.rows["tautology/kp1"]["issues"]
                         if row["code"] == "generic_or_tautological_sketch")
        self.assertIn("generic_order_of_operations", generic["evidence"][0]["markers"])
        self.assertIn("tautological_answer_restatement", tautology["evidence"][0]["markers"])

    def test_reflexive_equality_is_marked_but_a_real_computation_is_not(self):
        marker = audit._markers("$6 \\times 7 = 42$; $42 = 42$.", "42")
        control = audit._markers("$6 \\times 7 = 42$.", "42")
        self.assertIn("tautological_reflexive_equality", marker)
        self.assertNotIn("tautological_reflexive_equality", control)

    def test_orphan_template_key_keeps_report_failed(self):
        templates = {**self.templates, "ghost/kp1": []}
        report = audit.build_report(self.facts, templates)
        self.assertFalse(report["ok"])
        self.assertEqual(report["orphan_pending_template_keys"], ["ghost/kp1"])

    def test_incomplete_facts_fail_closed(self):
        broken = json.loads(json.dumps(self.facts))
        del broken["kps"][0]["exemplars"][0]["authored_answer_decidable"]
        with self.assertRaisesRegex(audit.Refused, "incomplete exemplar"):
            audit.build_report(broken, {})

    def test_malformed_content_evidence_fails_closed(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "broken.json").write_text("{")
            with self.assertRaisesRegex(audit.Refused, "cannot read"):
                audit.pending_templates(root)


    def test_incomplete_template_arguments_do_not_satisfy_pending_evidence(self):
        complete = {
            "statement": "Compute {a} + 1.",
            "params": {"a": {"kind": "choice", "values": [1]}},
            "answer_expr": "a + 1",
            "solution_sketch": "Add one to {a}.",
            "hints": ["Increase the value by one."],
            "samples": [{"params": {"a": 1}, "expected": "2"}],
        }
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            rows = [
                {"kp_id": "valid/kp1", "kind": "template", "arguments": complete},
                {"kp_id": "stored/kp1", "kind": "template", "body": complete},
            ]
            for field in complete:
                incomplete = dict(complete)
                del incomplete[field]
                rows.append({
                    "kp_id": f"missing-{field}/kp1",
                    "kind": "template",
                    "arguments": incomplete,
                })
            rows.append({
                "kp_id": "legacy-placeholder/kp1",
                "kind": "template",
                "arguments": {"problem": "Compute 1 + 1.", "answer_expr": "2"},
            })
            (root / "templates.json").write_text(json.dumps(rows))
            templates = audit.pending_templates(root)
        self.assertEqual(set(templates), {"stored/kp1", "valid/kp1"})

if __name__ == "__main__":
    unittest.main()
