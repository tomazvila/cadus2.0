#!/usr/bin/env python3
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

SPEC = importlib.util.spec_from_file_location("cohorts", Path(__file__).with_name("review_cohorts.py"))
cohorts = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(cohorts)

def packet(items):
    core = {"packet_version": 1, "scope": "all_pending_content", "items": items}
    return core | {"packet_sha256": cohorts.sha256(core)}

def item(key, kind, digest):
    return {"kp_id": key, "kind": kind, "digest": digest}

class CohortsTest(unittest.TestCase):
    def test_complete_partition_hashes_and_limit(self):
        items = [item(f"topic/kp{n:02}", "template" if n % 2 else "teach", f"d{n}") for n in range(20)]
        result = cohorts.split(packet(items), {row["kp_id"]: "unit" for row in items}, 16)
        self.assertEqual({row["digest"] for _, _, _, batch in result for row in batch["items"]}, {row["digest"] for row in items})
        for _, _, _, batch in result:
            self.assertEqual(batch["packet_sha256"], cohorts.sha256({key: value for key, value in batch.items() if key != "packet_sha256"}))
            self.assertLessEqual(len({row["kp_id"] for row in batch["items"]}), 16)
    def test_duplicate_digest_is_rejected(self):
        rows = [item("topic/kp1", "teach", "same"), item("topic/kp2", "teach", "same")]
        with self.assertRaises(cohorts.Refused): cohorts.split(packet(rows), {"topic/kp1":"u","topic/kp2":"u"})
    def test_unknown_kp_is_rejected(self):
        with self.assertRaises(cohorts.Refused): cohorts.split(packet([item("topic/kp1","teach","d")]), {})
    def test_multiple_templates_per_kp_are_rejected(self):
        rows = [item("topic/kp1","template","a"), item("topic/kp1","template","b")]
        with self.assertRaises(cohorts.Refused): cohorts.split(packet(rows), {"topic/kp1":"u"})
    def test_existing_output_is_unchanged(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory); output=root/"out"; output.mkdir(); marker=output/"keep"; marker.write_text("keep")
            source=root/"packet.json"; source.write_text(json.dumps(packet([item("topic/kp1","teach","d")])))
            units=root/"units.json"; units.write_text(json.dumps({"topic/kp1":"u"}))
            with self.assertRaises(cohorts.Refused): cohorts.write(source, units, output)
            self.assertEqual(marker.read_text(),"keep")

    def test_extras_do_not_hide_missing_mapping(self):
        with self.assertRaises(cohorts.Refused):
            cohorts.split(packet([item("topic/kp1", "teach", "d")]), {"other/kp": "unit"})

    def test_path_traversal_refuses_before_output_creation(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, units, output = root / "packet.json", root / "units.json", root / "out"
            source.write_text(json.dumps(packet([item("topic/kp1", "teach", "d")])))
            units.write_text(json.dumps({"topic/kp1": "../escape"}))
            with self.assertRaises(cohorts.Refused):
                cohorts.write(source, units, output)
            self.assertFalse(output.exists())

    def test_empty_packet_is_refused(self):
        with self.assertRaises(cohorts.Refused):
            cohorts.split(packet([]), {})
if __name__ == "__main__": unittest.main()
