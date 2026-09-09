#!/usr/bin/env python3
"""Write-lifecycle regressions for the AI C6 protocol; no network or model calls."""
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import content_review_packet as review


def doc(digest):
    return {"digest": digest, "kp_id": digest, "kind": "template", "status": "pending",
            "body": {"statement": digest}, "gate": {"gated": True}, "instances": [], "instances_note": None}


class Api:
    def __init__(self, rows, fail=None): self.rows, self.fail, self.writes = {x["digest"]: x for x in rows}, fail, []
    def document(self, digest): return json.loads(json.dumps(self.rows[digest]))
    def decide(self, digest, decision, reason=None):
        self.writes.append(digest)
        if digest == self.fail: raise OSError("connection lost after request")
        self.rows[digest]["status"] = "approved"; return {"digest": digest, "status": "approved"}


class LifecycleTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.root = Path(self.temp.name)
        rows = [doc("a"), doc("b")]
        items = [{**x, "fingerprint_sha256": review.fingerprint(x)} for x in rows]
        core = {"packet_version": 1, "scope": "all_pending_content", "items": items}
        self.packet = self.root / "packet.json"; self.packet.write_text(json.dumps(core | {"packet_sha256": review.sha256(core)}))
        self.decisions = self.root / "decisions.json"; self.decisions.write_text(json.dumps({"decision_version": 1, "packet_sha256": review.sha256(core), "decisions": [{"digest":"a","decision":"approve"},{"digest":"b","decision":"approve"}]}))
        self.receipt = self.root / "receipt.json"
    def tearDown(self): self.temp.cleanup()
    def test_successful_two_document_batch_persists_receipts(self):
        result = review.apply_decisions(Api([doc("a"), doc("b")]), self.packet, self.decisions, True, self.receipt)
        self.assertTrue(result["complete"]); self.assertEqual(2, len(json.loads(self.receipt.read_text())["receipts"]))
    def test_first_uncertain_write_has_inflight_receipt(self):
        with self.assertRaises(OSError): review.apply_decisions(Api([doc("a"), doc("b")], "a"), self.packet, self.decisions, True, self.receipt)
        saved = json.loads(self.receipt.read_text()); self.assertEqual("a", saved["uncertain"]["digest"]); self.assertEqual([], saved["receipts"])
    def test_second_uncertain_write_retains_first_receipt(self):
        with self.assertRaises(OSError): review.apply_decisions(Api([doc("a"), doc("b")], "b"), self.packet, self.decisions, True, self.receipt)
        saved = json.loads(self.receipt.read_text()); self.assertEqual(["a"], [x["digest"] for x in saved["receipts"]]); self.assertEqual("b", saved["uncertain"]["digest"])
    def test_dry_run_receipt_is_saved_without_writes(self):
        api = Api([doc("a"), doc("b")]); review.apply_decisions(api, self.packet, self.decisions, False, self.receipt)
        self.assertEqual([], api.writes); self.assertTrue(json.loads(self.receipt.read_text())["complete"])


if __name__ == "__main__": unittest.main()
