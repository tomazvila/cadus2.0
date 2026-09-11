#!/usr/bin/env python3
"""Digest-bound content review packet regressions; no real network or writes."""
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import content_review_packet as review


def document(digest, kp="topic/kp1", kind="teach", body=None):
    return {
        "digest": digest, "kp_id": kp, "kind": kind, "status": "pending",
        "created_at": "2026-09-06T12:00:00Z", "authoring_attempts": 1,
        "authoring_cost_usd": "0.000000", "body": body or {"concept": digest},
        "gate": None, "instances": [], "instances_note": None, "policy_digest": None, "approved_policy_digest": None, "template_context_digest": None, "approved_template_context_digest": None, "eligible_template_digests": [],
        "curriculum_digest": "curriculum-v1", "approved_curriculum_digest": None,
        "review_engine_digest": "engine-v1", "approved_review_engine_digest": None,
    }


class FakeApi:
    def __init__(self, documents):
        self.documents = {row["digest"]: row for row in documents}
        self.writes = []

    def list_pending(self):
        return [{key: row[key] for key in ("digest", "kp_id", "kind", "status")}
                for row in self.documents.values()]

    def document(self, digest):
        return json.loads(json.dumps(self.documents[digest]))

    def decide(self, digest, decision, reason=None, policy_digest=None,
               template_context_digest=None, curriculum_digest=None,
               review_engine_digest=None):
        self.writes.append((digest, decision, reason))
        status = "approved" if decision == "approve" else "rejected"
        self.documents[digest]["status"] = status
        self.documents[digest]["approved_template_context_digest"] = template_context_digest
        self.documents[digest]["approved_curriculum_digest"] = curriculum_digest
        self.documents[digest]["approved_review_engine_digest"] = review_engine_digest
        return {"digest": digest, "status": status, "rejected_documents": [],
                "approved_policy_digest": policy_digest,
                "approved_template_context_digest": template_context_digest,
                "approved_curriculum_digest": curriculum_digest,
                "approved_review_engine_digest": review_engine_digest}


class ChangingApi(FakeApi):
    def __init__(self, documents):
        super().__init__(documents)
        self.reads = 0

    def list_pending(self):
        self.reads += 1
        rows = super().list_pending()
        return rows if self.reads == 1 else rows[:-1]


class PacketTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.packet = self.root / "packet.json"
        self.api = FakeApi([document("sha256:a"), document("sha256:b", kind="template")])
        review.export_packet(self.api, self.packet, None, 2)
        self.decisions = self.root / "decisions.json"

    def tearDown(self):
        self.temp.cleanup()

    def decide(self, rows):
        packet = json.loads(self.packet.read_text())
        self.decisions.write_text(json.dumps({
            "decision_version": 1, "packet_sha256": packet["packet_sha256"],
            "decisions": rows,
        }))

    def test_export_binds_full_documents_and_creates_empty_decision_file(self):
        packet = json.loads(self.packet.read_text())
        self.assertEqual([row["digest"] for row in packet["items"]], ["sha256:a", "sha256:b"])
        self.assertEqual(review.validate_packet(packet)["sha256:a"]["body"], {"concept": "sha256:a"})
        template = json.loads((self.root / "packet.decisions.json").read_text())
        self.assertEqual(template["decisions"], [])

    def test_queue_change_during_export_fails_before_writing_a_packet(self):
        output = self.root / "changing.json"
        with self.assertRaisesRegex(review.Refused, "queue changed"):
            review.export_packet(ChangingApi(list(self.api.documents.values())), output, None, 2)
        self.assertFalse(output.exists())

    def test_only_explicitly_selected_digests_are_written(self):
        self.decide([{"digest": "sha256:b", "decision": "approve"}])
        receipt = review.apply_decisions(self.api, self.packet, self.decisions, True)
        self.assertTrue(receipt["committed"])
        self.assertEqual(self.api.writes, [("sha256:b", "approve", None)])
        self.assertEqual(self.api.documents["sha256:a"]["status"], "pending")
        self.assertEqual(self.api.documents["sha256:b"]["status"], "approved")

    def test_dry_run_rechecks_but_writes_nothing(self):
        self.decide([{"digest": "sha256:a", "decision": "reject", "reason": "wrong example"}])
        receipt = review.apply_decisions(self.api, self.packet, self.decisions, False)
        self.assertFalse(receipt["committed"])
        self.assertEqual(self.api.writes, [])

    def test_unknown_duplicate_implicit_and_reasonless_decisions_fail_closed(self):
        cases = [
            [{"digest": "sha256:x", "decision": "approve"}],
            [{"digest": "sha256:a", "decision": "approve"},
             {"digest": "sha256:a", "decision": "approve"}],
            [{"digest": "sha256:a", "decision": "undecided"}],
            [{"digest": "sha256:a", "decision": "reject", "reason": ""}],
        ]
        for rows in cases:
            with self.subTest(rows=rows):
                self.decide(rows)
                with self.assertRaises(review.Refused):
                    review.apply_decisions(self.api, self.packet, self.decisions, True)
                self.assertEqual(self.api.writes, [])

    def test_tampered_packet_or_decisions_for_another_packet_fail_closed(self):
        self.decide([{"digest": "sha256:a", "decision": "approve"}])
        packet = json.loads(self.packet.read_text())
        packet["items"][0]["body"] = {"concept": "tampered"}
        self.packet.write_text(json.dumps(packet))
        with self.assertRaisesRegex(review.Refused, "packet fingerprint"):
            review.apply_decisions(self.api, self.packet, self.decisions, True)
        self.assertEqual(self.api.writes, [])

    def test_live_body_status_or_gate_drift_aborts_before_any_write(self):
        self.decide([{"digest": "sha256:a", "decision": "approve"},
                     {"digest": "sha256:b", "decision": "approve"}])
        self.api.documents["sha256:b"]["gate"] = {"ok": False}
        with self.assertRaisesRegex(review.Refused, "differs"):
            review.apply_decisions(self.api, self.packet, self.decisions, True)
        self.assertEqual(self.api.writes, [])

    def test_exact_source_body_is_reported_as_provenance(self):
        source = self.root / "docs/content-foundations/unit"
        source.mkdir(parents=True)
        (source / "teach.json").write_text(json.dumps([{
            "kp_id": "topic/kp1", "kind": "teach", "arguments": {"concept": "sha256:a"}
        }]))
        (source / "manifest.json").write_text(json.dumps({"files": ["teach.json"]}))
        index = review.source_index(self.root / "docs/content-foundations")
        item = review.packet_item(document("sha256:a"), index)
        self.assertEqual(item["source_matches"][0]["row"], 0)
        self.assertTrue(item["source_matches"][0]["file"].endswith("teach.json"))


if __name__ == "__main__":
    unittest.main()
