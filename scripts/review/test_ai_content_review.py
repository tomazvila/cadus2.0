#!/usr/bin/env python3
import copy
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import ai_content_review as ai
import content_review_packet as packet


def item(digest="d", kind="template"):
    return {"digest": digest, "kp_id": "t/k", "kind": kind, "status": "pending",
            "body": {"x": digest}, "gate": None, "instances": [], "instances_note": None, "policy_digest": None, "approved_policy_digest": None, "template_context_digest": None, "approved_template_context_digest": None, "eligible_template_digests": [], "curriculum_digest": "curriculum-v1", "approved_curriculum_digest": None, "review_engine_digest": "engine-v1", "approved_review_engine_digest": None}


def checks(status="pass", evidence="recomputed independently"):
    return {name: {"status": status, "evidence": evidence} for name in ai.CHECKS}


class EvidenceTest(unittest.TestCase):
    def setUp(self):
        self.expected = {"d": {"digest": "d"}}
        self.base = {"ai_review_version": 1, "reviewer": {"identity": "agent-1", "model": "test", "policy_version": "v1"}, "decisions": [{"digest": "d", "decision": "approve", "reason": "all reviewed", "checks": checks()}]}

    def test_approval_requires_all_substantive_passes(self):
        self.assertEqual(ai.review_rows(self.base, self.expected)[0][1], "approve")
        for change in (lambda x: x["decisions"][0]["checks"].pop("explanations"), lambda x: x["decisions"][0]["checks"]["explanations"].update(evidence=""), lambda x: x["decisions"][0]["checks"]["explanations"].update(status="fail")):
            bad = copy.deepcopy(self.base); change(bad)
            with self.assertRaises(packet.Refused): ai.review_rows(bad, self.expected)

    def test_duplicate_missing_and_unsupported_rows_refuse(self):
        for rows in ([], self.base["decisions"] * 2, [{"digest": "x", "decision": "approve", "reason": "x", "checks": checks()}]):
            bad = copy.deepcopy(self.base); bad["decisions"] = rows
            with self.assertRaises(packet.Refused): ai.review_rows(bad, self.expected)

    def test_quarantine_is_explicit_and_never_an_approval(self):
        batch = copy.deepcopy(self.base); batch["decisions"][0]["decision"] = "quarantine"; batch["decisions"][0]["checks"] = checks("unsupported", "no supported method")
        self.assertEqual(ai.review_rows(batch, self.expected)[0][1], "quarantine")

    def test_context_rejects_stale_body_and_mixed_template_instruction(self):
        one = item(); core = {"packet_version": 1, "scope": "all_pending_content", "items": [{**one, "fingerprint_sha256": packet.fingerprint(one)}]}; doc = core | {"packet_sha256": packet.sha256(core)}
        class Api:
            def document(self, digest): return copy.deepcopy(one)
            def request(self, method, suffix): return {"items": [], "limit": 200}
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp); (root / "a").write_text("a")
            self.assertEqual(len(ai.context(doc, Api(), root)), 1)
            one["body"] = {"x": "drift"}
            with self.assertRaises(packet.Refused): ai.context(doc, Api(), root)


if __name__ == "__main__": unittest.main()
