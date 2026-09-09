import copy, json, sys, tempfile, unittest
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
import ai_content_review as ai
import content_review_packet as p

def make_doc(digest, kp):
    return {"digest": digest, "kp_id": kp, "kind": "template", "status": "pending", "body": {"x": digest}, "gate": {"gated": True, "instances_checked": 8}, "instances": [], "instances_note": None}

class ApplyFixture(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.root = Path(self.temp.name); self.rows = {"a": make_doc("a", "k1"), "b": make_doc("b", "k2")}; self.cookie = self.root / "cookie"; self.cookie.write_text("x"); (self.root / "curriculum").mkdir(); (self.root / "curriculum/a").write_text("v1"); self.writes = []
        items = [row | {"fingerprint_sha256": p.fingerprint(row)} for row in self.rows.values()]; core = {"packet_version": 1, "scope": "all_pending_content", "items": items}; self.packet = core | {"packet_sha256": p.sha256(core)}; self.packet_path = self.root / "packet.json"; self.packet_path.write_text(json.dumps(self.packet))
    def tearDown(self): self.temp.cleanup()
    def request(self, method, suffix):
        rows = [row for row in self.rows.values() if ("kp=" not in suffix or ("kp=" + row["kp_id"]) in suffix)]
        return {"items": [copy.deepcopy(row) for row in rows], "limit": 200}
    def document(self, digest): return copy.deepcopy(self.rows[digest])
    def decide(self, digest, *_): self.writes.append(digest); self.rows[digest]["status"] = "approved"; return {"digest": digest, "status": "approved"}
    def execute(self, drift=False):
        contexts = ai.context(self.packet, self, self.root / "curriculum"); core = {"ai_review_version": 1, "packet_sha256": self.packet["packet_sha256"], "items": contexts}; review = core | {"context_sha256": p.sha256(core), "reviewer": {"identity":"i","model":"m","policy_version":"v"}, "decisions": [{"digest": digest, "decision":"approve", "reason":"reviewed", "checks": {name:{"status":"pass","evidence":"independent check"} for name in ai.CHECKS}} for digest in self.rows]}; review_path = self.root / "review.json"; review_path.write_text(json.dumps(review)); original = p.Api; p.Api = lambda *_: self
        if drift:
            original_decide = self.decide
            def change(digest, *args):
                answer = original_decide(digest, *args)
                if digest == "a": (self.root / "curriculum/a").write_text("v2")
                return answer
            self.decide = change
        args = type("Args", (), {"packet":self.packet_path,"review":review_path,"base_url":"x","cookie_file":self.cookie,"curriculum_root":self.root / "curriculum","decisions":self.root / "decisions.json","receipt":self.root / "receipt.json","commit":True})()
        try: ai.apply(args)
        finally: p.Api = original
        return args
    def test_successful_two_template_apply(self):
        self.execute(); self.assertEqual(["a", "b"], self.writes)
    def test_curriculum_drift_blocks_second_and_keeps_first_receipt(self):
        with self.assertRaises(p.Refused): args = self.execute(True)
        self.assertEqual(["a"], self.writes)

if __name__ == "__main__": unittest.main()
