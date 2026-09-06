"""The reviewed symbolic slice stays clean under the authoritative audit."""
import json
import sys
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import foundations_content_audit as audit


class SymbolicReviewedAuditTest(unittest.TestCase):
    def test_all_reviewed_kps_have_zero_content_integrity_findings(self) -> None:
        facts = json.loads(
            (HERE / "testdata/symbolic_reviewed_facts.json").read_text()
        )
        templates = audit.pending_templates(HERE.parents[1] / "docs/content-foundations")
        report = audit.build_report(facts, templates)
        for row in report["kps"]:
            self.assertEqual(row["issues"], [], row["kp_key"])


if __name__ == "__main__":
    unittest.main()
