import hashlib
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("refresh_whole_course_teach_v2.py")
spec = importlib.util.spec_from_file_location("refresh", SCRIPT)
refresh = importlib.util.module_from_spec(spec)
spec.loader.exec_module(refresh)


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + "\n")


def digest(path):
    return "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()


class RefreshWholeCourseTeachV2Test(unittest.TestCase):
    def make_root(self):
        temp = tempfile.TemporaryDirectory()
        root = Path(temp.name)
        sidecar = root / "docs/content-foundations/whole-course-teach"
        (root / "curriculum/foundations").mkdir(parents=True)
        (root / "curriculum/foundations/unit.yaml").write_text("course: foundations\n")
        rows = []
        for number in range(735):
            key = f"topic-{number // 25:02}/kp{number % 25:02}"
            rows.append({"kp_id": key, "kind": "teach", "arguments": {"n": number}})
        for part in range(1, 31):
            chunk = rows[(part - 1) * 25:part * 25]
            write_json(sidecar / f"drafts/part-{part:02}.json", chunk)
            reviews = [{"kp_id": row["kp_id"], "teach_digest": "old", "template_digest": "old",
                        "verification": {"collision": "clear", "production_gate": "accepted", "context_coverage": "sampled_template_instances"},
                        "ai_review": "pending", "historical_review": {"path": f"historical-archive/reviews/part-{part:02}.json", "sha256": f"sha256:history-{number}"}}
                       for number, row in enumerate(chunk, (part - 1) * 25)]
            write_json(sidecar / f"reviews/part-{part:02}.json", reviews)
        write_json(sidecar / "inputs/templates.json", [])
        write_json(sidecar / "historical-archive/index.json", {"preserved": True})
        files = [{"path": path, "rows": len(read_json(sidecar / path)), "sha256": hashlib.sha256((sidecar / path).read_bytes()).hexdigest()}
                 for kind in ("drafts", "reviews") for path in refresh.part_paths(kind)]
        files.append({"path": "inputs/templates.json", "rows": 0, "sha256": hashlib.sha256((sidecar / "inputs/templates.json").read_bytes()).hexdigest()})
        write_json(sidecar / "manifest.json", {"schema_version": 2, "status": "pending-ai-review", "files": files,
                   "canonical_arrays": {"drafts_sha256": "old", "reviews_sha256": "old"}})
        evidence_rows = [{"kp_id": row["kp_id"], "teach_digest": f"teach-{number}", "template_digest": f"template-{number}",
                          "collision": "clear", "production_gate": "accepted", "context_coverage": "sampled_template_instances", "ai_review": "pending"}
                         for number, row in enumerate(rows)]
        evidence = {"schema_version": 2, "status": "pending-ai-review", "curriculum_hash": "generator-only",
                    "raw_input_sha256": refresh.raw_hashes(sidecar, ["inputs/templates.json", *refresh.part_paths("drafts")]),
                    "curriculum_raw_input_sha256": refresh.curriculum_hashes(root),
                    "historical_row_sha256": {row["kp_id"]: f"sha256:history-{number}" for number, row in enumerate(rows)},
                    "rows": evidence_rows}
        evidence_path = root / "fresh.json"
        write_json(evidence_path, evidence)
        return temp, root, sidecar, evidence_path

    def test_stale_input_refuses_before_any_write(self):
        temp, root, sidecar, evidence = self.make_root()
        self.addCleanup(temp.cleanup)
        before = {path.relative_to(sidecar): path.read_bytes() for path in sidecar.rglob("*") if path.is_file()}
        (sidecar / "inputs/templates.json").write_text("[]\nchanged")
        with self.assertRaisesRegex(refresh.Refused, "stale document inputs"):
            refresh.update(root, evidence)
        after = {path.relative_to(sidecar): path.read_bytes() for path in sidecar.rglob("*") if path.is_file()}
        self.assertEqual(after, {key: value for key, value in before.items() if key != Path("inputs/templates.json")} | {Path("inputs/templates.json"): b"[]\nchanged"})

    def test_refresh_updates_current_records_and_preserves_archive(self):
        temp, root, sidecar, evidence = self.make_root()
        self.addCleanup(temp.cleanup)
        archive_before = {path.relative_to(sidecar): path.read_bytes() for path in (sidecar / "historical-archive").rglob("*") if path.is_file()}
        refresh.update(root, evidence)
        archive_after = {path.relative_to(sidecar): path.read_bytes() for path in (sidecar / "historical-archive").rglob("*") if path.is_file()}
        self.assertEqual(archive_after, archive_before)
        technical = read_json(sidecar / "technical-evidence-v2.json")
        self.assertEqual(len(technical["rows"]), 735)
        review = read_json(sidecar / "reviews/part-01.json")[0]
        self.assertEqual(review["teach_digest"], "teach-0")
        self.assertEqual(review["ai_review"], "pending")
        self.assertNotIn("approved", review)
        manifest = read_json(sidecar / "manifest.json")
        self.assertEqual(manifest["technical_evidence"]["sha256"], digest(sidecar / "technical-evidence-v2.json"))


def read_json(path):
    return json.loads(path.read_text())


if __name__ == "__main__":
    unittest.main()
