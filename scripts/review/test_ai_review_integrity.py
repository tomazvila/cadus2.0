import hashlib
import json
import test_ai_content_review_lifecycle as fixture
import content_review_packet as packet


class IntegrityTest(fixture.ApplyFixture):
    def receipt(self):
        return json.loads((self.root / "receipt.json").read_text())

    def test_failed_regate_stops_later_writes(self):
        original = self.decide
        def decide(digest, *args):
            return original(digest, *args) | {"rejected_documents": None}
        self.decide = decide
        with self.assertRaises(packet.Refused):
            self.execute()
        self.assertEqual(self.writes, ["a"])
        self.assertFalse(self.receipt()["complete"])

    def test_existing_receipt_is_preserved_before_any_write(self):
        path = self.root / "receipt.json"
        path.write_text("prior evidence")
        with self.assertRaises(packet.Refused):
            self.execute()
        self.assertEqual(self.writes, [])
        self.assertEqual(path.read_text(), "prior evidence")

    def test_uncertain_second_write_retains_first_and_reviewer(self):
        original = self.decide
        def decide(digest, *args):
            if digest == "b":
                raise OSError("response lost")
            return original(digest, *args)
        self.decide = decide
        with self.assertRaises(OSError):
            self.execute()
        receipt = self.receipt()
        self.assertEqual(receipt["receipts"][0]["digest"], "a")
        self.assertEqual(receipt["uncertain"]["digest"], "b")
        self.assertFalse(receipt["complete"])
        self.assertEqual(receipt["ai_reviewer"]["identity"], "i")
        self.assertEqual(receipt["review_sha256"], hashlib.sha256((self.root / "review.json").read_bytes()).hexdigest())

    def test_instruction_without_approved_template_refused(self):
        self.rows = {"a": fixture.make_doc("a", "k")}
        self.rows["a"]["kind"] = "teach"
        item = self.rows["a"] | {"fingerprint_sha256": packet.fingerprint(self.rows["a"])}
        core = {"packet_version": 1, "scope": "all_pending_content", "items": [item]}
        self.packet = core | {"packet_sha256": packet.sha256(core)}
        self.packet_path.write_text(json.dumps(self.packet))
        with self.assertRaises(packet.Refused):
            self.execute()
        self.assertEqual([], self.writes)

    def test_serving_set_drift_blocks_second_write(self):
        self.rows = {"t1": fixture.make_doc("t1", "k1"), "t2": fixture.make_doc("t2", "k2"),
                     "a": fixture.make_doc("a", "k1"), "b": fixture.make_doc("b", "k2")}
        self.rows["t1"]["status"] = self.rows["t2"]["status"] = "approved"
        self.rows["a"]["kind"] = self.rows["b"]["kind"] = "teach"
        items = [self.rows[d] | {"fingerprint_sha256": packet.fingerprint(self.rows[d])} for d in ("a", "b")]
        core = {"packet_version": 1, "scope": "all_pending_content", "items": items}
        self.packet = core | {"packet_sha256": packet.sha256(core)}
        self.packet_path.write_text(json.dumps(self.packet))
        original = self.decide
        def decide(digest, *args):
            answer = original(digest, *args)
            if digest == "a": self.rows["t2"]["body"] = {"changed": True}
            return answer
        self.decide = decide
        with self.assertRaises(packet.Refused): self.execute()
        self.assertEqual(["a"], self.writes)
        receipt = self.receipt()
        self.assertEqual("a", receipt["receipts"][0]["digest"])
        self.assertEqual("i", receipt["ai_reviewer"]["identity"])
        self.assertEqual(receipt["review_sha256"], hashlib.sha256((self.root / "review.json").read_bytes()).hexdigest())
