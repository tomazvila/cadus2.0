#!/usr/bin/env python3
"""Negative controls for the fail-closed Foundations content audit."""
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import foundations_content_audit as audit
from template_gate_audit import ROOT, curriculum_source_hash, gate_source_hash


class FoundationsContentAuditTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.here = Path(__file__).resolve().parent
        cls.facts = json.loads((cls.here / "testdata/content_audit_facts.json").read_text())
        cls.facts["curriculum_hash"] = "synthetic-test-curriculum"
        cls.documents = cls.here / "testdata/content_audit_documents"
        cls.templates = audit.pending_templates(cls.documents)
        # Synthetic grammar facts isolate Python reporting from the Rust adapter.
        cls.gates = {"schema_version": 1, "curriculum_hash": cls.facts["curriculum_hash"],
                     "gate_source_hash": gate_source_hash(),
                     "curriculum_source_hash": curriculum_source_hash(ROOT / "curriculum"),
                     "recipes": [{"document": row["document"], "accepted": True, "reason": None}
                                 for rows in cls.templates.values() for row in rows]}
        cls.report = audit.build_report(cls.facts, cls.templates, cls.gates)
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

    def clean_inputs(self):
        facts = {**self.facts, "kps": [self.facts["kps"][0]]}
        self.assertEqual(facts["kps"][0]["kp_key"], "clean/kp1")
        return facts, {"clean/kp1": self.templates["clean/kp1"]}

    def test_missing_production_evidence_cannot_make_a_clean_kp_green(self):
        facts, templates = self.clean_inputs()
        report = audit.build_report(facts, templates)
        self.assertFalse(report["ok"])
        self.assertEqual(report["kps"][0]["issues"][0]["code"],
                         "pending_template_production_gate_declined")

    def test_an_empty_recipe_bucket_does_not_count_as_pending_content(self):
        facts, _ = self.clean_inputs()
        report = audit.build_report(facts, {"clean/kp1": []}, self.gates)
        self.assertFalse(report["ok"])
        self.assertEqual(report["kps"][0]["issues"][0]["code"],
                         "absent_pending_template_recipe")

    def test_only_complete_stored_bodies_are_sent_to_the_gate(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "drafts.json").write_text(json.dumps([
                {"kp_id": "metadata/kp1", "kind": "template", "status": "pending"},
                {"kp_id": "incomplete/kp1", "kind": "template", "status": "pending",
                 "body": {"v": 1, "space_size": 12, "answer_expr": "a", "params": {}}},
                {"kp_id": "body/kp1", "kind": "template", "status": "pending",
                 "body": {"v": 1, "topic_id": "body", "answer_kind": "numeric",
                          "space_size": 12, "statement": "Compute {a}.",
                          "answer_expr": "a", "params": {"a": {"kind": "int", "low": 1, "high": 2}},
                          "solution_sketch": "The value is {a}.", "hints": ["Read the value."],
                          "samples": [{"params": {"a": 1}, "expected": "1"}]}},
            ]))
            templates = audit.pending_templates(root)
        self.assertNotIn("metadata/kp1", templates)
        self.assertNotIn("incomplete/kp1", templates)
        self.assertEqual(templates["body/kp1"][0]["document"]["arguments"], {
            "statement": "Compute {a}.", "answer_expr": "a",
            "params": {"a": {"kind": "int", "low": 1, "high": 2}},
            "constraints": [], "solution_sketch": "The value is {a}.",
            "hints": ["Read the value."], "distractors": [],
            "samples": [{"params": {"a": 1}, "expected": "1"}],
        })

    def test_actual_worker_preflight_reason_is_reported_without_a_model_or_db(self):
        facts, templates = self.clean_inputs()
        declined = json.loads(json.dumps(self.gates))
        declined["recipes"][0].update(accepted=False,
            reason="answer-kind: answer kind multi-step is not symbolically decidable")
        with patch("subprocess.run", side_effect=AssertionError("unit test started a process")):
            report = audit.build_report(facts, templates, declined)
        self.assertFalse(report["ok"])
        self.assertIn("not symbolically decidable",
                      report["kps"][0]["issues"][0]["evidence"][0]["reason"])

    def test_recipe_mutation_invalidates_cached_gate_acceptance(self):
        facts, templates = self.clean_inputs()
        changed = json.loads(json.dumps(templates))
        changed["clean/kp1"][0]["document"]["arguments"]["answer_contract"] = {"kind": "none"}
        self.assertFalse(audit.build_report(facts, changed, self.gates)["ok"])

    def test_wrong_curriculum_or_malformed_gate_facts_fail_closed(self):
        for mutate in (
            lambda value: value.update(curriculum_hash="stale"),
            lambda value: value.update(gate_source_hash="stale"),
            lambda value: value.update(curriculum_source_hash="stale"),
            lambda value: value["recipes"][0].update(accepted="true"),
            lambda value: value["recipes"].append(value["recipes"][0]),
            lambda value: value["recipes"][0].update(accepted=False, reason=None),
        ):
            value = json.loads(json.dumps(self.gates))
            mutate(value)
            with self.assertRaises(audit.Refused):
                audit.build_report(self.facts, self.templates, value)

    def test_one_accepted_recipe_does_not_hide_an_unverified_sibling(self):
        facts, templates = self.clean_inputs()
        changed = json.loads(json.dumps(templates))
        sibling = json.loads(json.dumps(changed["clean/kp1"][0]))
        sibling["document"]["arguments"]["answer_expr"] = "999"
        changed["clean/kp1"].append(sibling)
        self.assertFalse(audit.build_report(facts, changed, self.gates)["ok"])

    def test_gate_or_curriculum_changes_invalidate_both_cached_fact_files(self):
        for name in ("gate_source_hash", "curriculum_source_hash"):
            with patch(f"template_gate_audit.{name}", return_value="modified-current-source"):
                with self.assertRaises(audit.Refused):
                    audit.build_report(self.facts, self.templates, self.gates)

    def test_python_cli_with_cached_facts_never_starts_a_process(self):
        facts, templates = self.clean_inputs()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "facts.json").write_text(json.dumps(facts))
            (root / "gates.json").write_text(json.dumps(self.gates))
            content = root / "content"
            content.mkdir()
            (content / "drafts.json").write_text(json.dumps([templates["clean/kp1"][0]["document"]]))
            with patch("subprocess.run", side_effect=AssertionError("unit test started a process")):
                code = audit.main(["--facts", str(root / "facts.json"),
                                   "--template-gate-facts", str(root / "gates.json"),
                                   "--content-root", str(content), "--output", str(root / "report.json")])
            self.assertEqual(code, 0)


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
