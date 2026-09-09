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
