"""Policy changes require fresh AI evidence and an atomic approval binding."""
import json
import tempfile
import unittest
from pathlib import Path

import ai_content_review as ai
import content_review_packet as packet
from test_content_review_packet import FakeApi, document


class PolicyApi(FakeApi):
    def list_status(self, status):
        return [row for row in super().list_pending() if row["status"] == status]

    def decide(self, digest, decision, reason=None, policy_digest=None, template_context_digest=None):
        row = self.documents[digest]
        if decision == "approve" and policy_digest != row["policy_digest"]:
            raise packet.Refused("review_context_changed")
        result = super().decide(digest, decision, reason, policy_digest, template_context_digest)
        row["approved_policy_digest"] = policy_digest
        return result | {"approved_policy_digest": policy_digest}

    def request(self, method, suffix):
        return {"items": list(self.documents.values()), "limit": 200}


def stale():
    return document("finite", kind="template") | {
        "status": "approved", "policy_digest": "new-policy",
        "approved_policy_digest": "old-policy"}


class PolicyContextTest(unittest.TestCase):
    def test_both_current_and_approved_policy_are_fingerprint_inputs(self):
        row = stale()
        baseline = packet.fingerprint(row)
        for field in ("policy_digest", "approved_policy_digest"):
            self.assertNotEqual(baseline, packet.fingerprint(row | {field: "changed"}))
        self.assertTrue(packet.reviewable(row))
        self.assertFalse(packet.reviewable(row | {"approved_policy_digest": "new-policy"}))

    def test_stale_approval_can_be_exported_and_reapproved_against_current_policy(self):
        api = PolicyApi([stale(), document("ordinary") | {"status": "approved"}])
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); output = root / "packet.json"
            count, decisions, _ = packet.export_packet(api, output, None, 1, stale_policy=True)
            self.assertEqual(count, 1)
            exported = packet.load_json(output)
            decisions.write_text(json.dumps({"decision_version": 1, "packet_sha256": exported["packet_sha256"],
                "decisions": [{"digest": "finite", "decision": "approve"}]}))
            result = packet.apply_decisions(api, output, decisions, True)
            self.assertTrue(result["complete"])
            self.assertEqual(api.documents["finite"]["approved_policy_digest"], "new-policy")
            self.assertEqual(packet.stale_policy_queue(api), [])

    def test_policy_race_at_the_write_is_refused_without_approving_new_policy(self):
        api = PolicyApi([stale()])
        item = packet.packet_item(api.document("finite"), {})
        original = api.decide
        def racing_decide(*args):
            api.documents["finite"]["policy_digest"] = "next-policy"
            return original(*args)
        api.decide = racing_decide
        with self.assertRaisesRegex(packet.Refused, "review_context_changed"):
            packet.apply_one_decision(api, item, "approve", "reviewed", {}, None, None)
        self.assertEqual(api.writes, [])
        self.assertEqual(api.documents["finite"]["approved_policy_digest"], "old-policy")

    def test_stale_template_cannot_supply_instruction_review_context(self):
        api = PolicyApi([stale(), document("teach") | {"policy_digest": "new-policy"}])
        row = packet.packet_item(api.document("teach"), {})
        core = {"packet_version": 1, "scope": "all_pending_content", "items": [row]}
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaisesRegex(packet.Refused, "refreshed approved template"):
                ai.context(core | {"packet_sha256": packet.sha256(core)}, api, Path(directory))

    def test_api_posts_the_reviewed_policy_digest_with_approval(self):
        api = packet.Api("http://localhost", "test-cookie")
        calls = []
        api.request = lambda *args: calls.append(args)
        api.decide("finite", "approve", policy_digest="reviewed-policy")
        self.assertEqual(calls, [("POST", "/finite/approve", {"policy_digest": "reviewed-policy", "template_context_digest": None})])


if __name__ == "__main__":
    unittest.main()
