"""Static exemplar evidence cannot substitute for current worker gate evidence."""
import json
import sys
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import foundations_content_audit as audit


class SymbolicReviewedAuditTest(unittest.TestCase):
    def test_reviewed_exemplars_stay_clean_but_require_worker_gate_evidence(self) -> None:
        facts = json.loads(
            (HERE / "testdata/symbolic_reviewed_facts.json").read_text()
        )
        templates = audit.pending_templates(HERE.parents[1] / "docs/content-foundations")
        report = audit.build_report(facts, templates)
        self.assertFalse(report["ok"])
        for row in report["kps"]:
            self.assertEqual([issue["code"] for issue in row["issues"]],
                             ["pending_template_production_gate_declined"], row["kp_key"])


if __name__ == "__main__":
    unittest.main()
