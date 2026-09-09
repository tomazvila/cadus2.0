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


def read_json(path):
    return json.loads(path.read_text())


def raw(path):
    return "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()


class RefreshWholeCourseTeachV2Test(unittest.TestCase):
    def make_root(self):
        temp = tempfile.TemporaryDirectory()
        root = Path(temp.name)
        sidecar = root / "docs/content-foundations/whole-course-teach"
        (root / "curriculum/foundations").mkdir(parents=True)
        (root / "curriculum/foundations/unit.yaml").write_text("course: foundations\n")
        rows = [{"kp_id": f"topic-{n // 25:02}/kp{n % 25:02}", "kind": "teach", "arguments": {"n": n}}
                for n in range(735)]
        history = {}
        archive_files = []
        for part in range(1, 31):
            chunk = rows[(part - 1) * 25:part * 25]
            write_json(sidecar / f"drafts/part-{part:02}.json", chunk)
            historic = [{"kp_id": row["kp_id"], "legacy": n} for n, row in enumerate(chunk, (part - 1) * 25)]
            historic_path = sidecar / f"historical-archive/reviews/part-{part:02}.json"
            write_json(historic_path, historic)
            for item in historic:
                history[item["kp_id"]] = "sha256:" + refresh.canonical_sha(item)
            archive_files.append({"path": f"reviews/part-{part:02}.json", "sha256": raw(historic_path)})
            reviews = [{"kp_id": row["kp_id"], "teach_digest": "old", "template_digest": "old",
                        "verification": {"collision": "clear", "production_gate": "accepted", "context_coverage": "sampled_template_instances"},
                        "ai_review": "pending", "historical_review": {"path": f"historical-archive/reviews/part-{part:02}.json", "sha256": history[row["kp_id"]]}}
                       for row in chunk]
            write_json(sidecar / f"reviews/part-{part:02}.json", reviews)
        for name in ("import-manifest.v1.json", "manifest.v1.json"):
            write_json(sidecar / "historical-archive" / name, {"old": name})
            archive_files.append({"path": name, "sha256": raw(sidecar / "historical-archive" / name)})
        write_json(sidecar / "historical-archive/index.json", {"status": "historical-as-encountered", "files": archive_files,
                   "reviews": archive_files[:30]})
        write_json(sidecar / "inputs/templates.json", [])
        write_json(sidecar / "inputs/coverage.json", {"rows": []})
        manifest = {"schema_version": 2, "status": "pending-ai-review", "files": refresh.inventory(sidecar),
                    "canonical_arrays": {"drafts_sha256": "old", "reviews_sha256": "old"},
                    "historical_archive": {"path": "historical-archive/index.json", "sha256": raw(sidecar / "historical-archive/index.json")}}
        write_json(sidecar / "manifest.json", manifest)
        evidence_path = root / "fresh.json"
        self.write_evidence(root, sidecar, rows, history, evidence_path)
        return temp, root, sidecar, evidence_path, rows, history

    def write_evidence(self, root, sidecar, rows, history, path):
        evidence_rows = [{"kp_id": row["kp_id"], "teach_digest": f"teach-{n}", "template_digest": f"template-{n}",
                          "collision": "clear", "production_gate": "accepted", "context_coverage": "sampled_template_instances", "ai_review": "pending"}
                         for n, row in enumerate(rows)]
        evidence = {"schema_version": 2, "status": "pending-ai-review", "curriculum_hash": "generator-only",
                    "raw_input_sha256": refresh.raw_hashes(sidecar, ["inputs/templates.json", *refresh.part_paths("drafts")]),
                    "curriculum_raw_input_sha256": refresh.curriculum_hashes(root),
                    "historical_row_sha256": history, "rows": evidence_rows}
        write_json(path, evidence)

    def test_stale_input_refuses_before_any_write(self):
        temp, root, sidecar, evidence, _, _ = self.make_root()
        self.addCleanup(temp.cleanup)
        before = {p.relative_to(sidecar): p.read_bytes() for p in sidecar.rglob("*") if p.is_file()}
        (sidecar / "inputs/templates.json").write_text("[]\nchanged")
        with self.assertRaisesRegex(refresh.Refused, "stale document inputs"):
            refresh.update(root, evidence)
        after = {p.relative_to(sidecar): p.read_bytes() for p in sidecar.rglob("*") if p.is_file()}
        self.assertEqual(after, {key: value for key, value in before.items() if key != Path("inputs/templates.json")} | {Path("inputs/templates.json"): b"[]\nchanged"})

    def test_changed_teach_and_template_refreshes_complete_inventory(self):
        temp, root, sidecar, evidence, rows, history = self.make_root()
        self.addCleanup(temp.cleanup)
        draft = sidecar / "drafts/part-01.json"
        changed = read_json(draft)
        changed[0]["arguments"]["n"] = "changed"
        write_json(draft, changed)
        write_json(sidecar / "inputs/templates.json", [{"fresh": True}])
        self.write_evidence(root, sidecar, rows, history, evidence)
        refresh.update(root, evidence)
        manifest = read_json(sidecar / "manifest.json")
        self.assertEqual(manifest["files"], refresh.inventory(sidecar))
        self.assertEqual(read_json(sidecar / "reviews/part-01.json")[0]["teach_digest"], "teach-0")
        self.assertEqual(manifest["technical_evidence"]["sha256"], raw(sidecar / "technical-evidence-v2.json"))

    def test_tampered_archive_refuses_before_any_write(self):
        temp, root, sidecar, evidence, _, _ = self.make_root()
        self.addCleanup(temp.cleanup)
        before = {p.relative_to(sidecar): p.read_bytes() for p in sidecar.rglob("*") if p.is_file()}
        (sidecar / "historical-archive/reviews/part-01.json").write_text("[]\n")
        with self.assertRaisesRegex(refresh.Refused, "historical archive file changed"):
            refresh.update(root, evidence)
        after = {p.relative_to(sidecar): p.read_bytes() for p in sidecar.rglob("*") if p.is_file()}
        self.assertEqual(after, {key: value for key, value in before.items() if key != Path("historical-archive/reviews/part-01.json")} | {Path("historical-archive/reviews/part-01.json"): b"[]\n"})


if __name__ == "__main__":
    unittest.main()
