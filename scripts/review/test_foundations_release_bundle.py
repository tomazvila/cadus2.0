#!/usr/bin/env python3
"""Fail-closed unit tests for the final Foundations release bundle."""
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

MODULE = Path(__file__).with_name("foundations_release_bundle.py")
SPEC = importlib.util.spec_from_file_location("bundle", MODULE)
bundle = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(bundle)


def row(number, kind):
    return {"kp_id": f"topic/kp{number}", "kind": kind, "arguments": {"value": number}}


class ReleaseBundleTest(unittest.TestCase):
    def write(self, name, rows):
        path = self.root / name
        path.write_text(json.dumps(rows))
        return path

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)

    def tearDown(self):
        self.temp.cleanup()

    def valid(self):
        templates = [row(number, "template") for number in range(809)]
        teach = [row(number, "teach") for number in range(735)]
        instruction = ([row(number, "teach") for number in range(735, 809)] +
                       [row(number, "hint_ladder") for number in range(809)])
        return templates, teach, instruction

    def test_exact_three_way_composition_is_2427_unique_rows(self):
        groups, _sources, counts = bundle.prepare(
            self.write("templates.json", self.valid()[0]),
            self.write("teach.json", self.valid()[1]),
            self.write("instruction.json", self.valid()[2]))
        self.assertEqual(counts, bundle.EXPECTED)
        self.assertEqual(sum(map(len, groups)), 2427)

    def test_curated_teach_composition_has_no_generated_teach(self):
        templates, teach, instruction = self.valid()
        teach += [row for row in instruction if row["kind"] == "teach"]
        hints = [row for row in instruction if row["kind"] == "hint_ladder"]
        groups, _sources, counts = bundle.prepare(
            self.write("templates.json", templates), self.write("teach.json", teach),
            self.write("hints.json", hints))
        self.assertEqual([809, 809, 809], list(map(len, groups)))
        self.assertEqual(counts, bundle.EXPECTED)

    def test_new_bundle_is_pending_ai_review_and_legacy_stays_verifiable(self):
        templates, teach, instruction = self.valid()
        args = type("Args", (), {"release_root": self.root, "release_commit": "head",
            "templates": self.write("templates.json", templates),
            "teach": self.write("teach.json", teach),
            "instruction": self.write("hints.json", instruction),
            "output": self.root / "bundle"})
        original = bundle.git_head
        bundle.git_head = lambda _root: "head"
        try:
            bundle.build(args)
            receipt = bundle.read_json(args.output / "bundle.json")
            self.assertEqual(2, receipt["version"])
            self.assertEqual("pending-ai-review", receipt["status"])
            self.assertNotIn("human_approval", receipt)
            bundle.validate_receipt(receipt, self.root)
            receipt.update(version=1, status="pending-human-review", human_approval="pending")
            receipt.pop("ai_approval")
            receipt["bundle_sha256"] = bundle.digest_json({k: v for k, v in receipt.items() if k != "bundle_sha256"})
            bundle.validate_receipt(receipt, self.root)
            receipt["human_approval"] = "approved"
            receipt["bundle_sha256"] = bundle.digest_json({k: v for k, v in receipt.items() if k != "bundle_sha256"})
            with self.assertRaisesRegex(bundle.Refused, "lifecycle"):
                bundle.validate_receipt(receipt, self.root)
        finally:
            bundle.git_head = original

    def test_overlap_is_refused(self):
        templates, teach, instruction = self.valid()
        instruction[0] = row(0, "teach")
        with self.assertRaisesRegex(bundle.Refused, "overlap"):
            bundle.prepare(self.write("templates.json", templates),
                           self.write("teach.json", teach),
                           self.write("instruction.json", instruction))

    def test_missing_row_is_refused(self):
        templates, teach, instruction = self.valid()
        with self.assertRaisesRegex(bundle.Refused, "counts"):
            bundle.prepare(self.write("templates.json", templates[:-1]),
                           self.write("teach.json", teach),
                           self.write("instruction.json", instruction))

    def test_recovery_receipt_must_bind_head_and_byte_identical_restore(self):
        archive = self.root / "backup.dump.age"
        archive.write_bytes(b"encrypted")
        receipt = {
            "candidate_head": "head", "archive": {"encrypted": True,
                "plaintext_dump_written": False, "path": str(archive),
                "sha256": bundle.digest_bytes(archive.read_bytes())},
            "disposable_database_removed": True, "outbound_model_keys": "empty",
            "production_read_only_fingerprint": "same",
            "restored_fingerprint_before": "same", "restored_fingerprint_after": "same",
        }
        path = self.write("recovery.json", receipt)
        args = type("Args", (), {"receipt": path, "release_root": self.root})
        original = bundle.git_head
        bundle.git_head = lambda _root: "head"
        try:
            bundle.verify_recovery(args)
            receipt["restored_fingerprint_after"] = "changed"
            path.write_text(json.dumps(receipt))
            with self.assertRaisesRegex(bundle.Refused, "byte-identical"):
                bundle.verify_recovery(args)
        finally:
            bundle.git_head = original


if __name__ == "__main__":
    unittest.main()
