"""No external runtime is needed to protect the canonical recipe source mirrors."""
import json
import unittest

from sync_template19_recipes import ROOT, synchronize


class Template19SourcesTests(unittest.TestCase):
    def test_legacy_generators_have_not_reintroduced_superseded_recipes(self):
        self.assertEqual(synchronize(check=True), [])

    def test_relocated_unit07_recipes_have_current_homes_and_no_retired_mirrors(self):
        keys = (
            "converting-to-vertex-form/kp3",
            "parabola-vertex-form/kp3",
            "quadratic-applications/kp1",
            "quadratic-applications/kp2",
            "quadratic-graphs-vertex/kp2",
        )
        cohort = ROOT / "docs/content-foundations/template19-production-gate"
        canonical = json.loads((cohort / "drafts.json").read_text())
        self.assertEqual(len(canonical), 19)
        self.assertEqual(len({row["kp_id"] for row in canonical}), 19)
        destination = "docs/content-foundations/whole-course-teach/inputs/templates.json"
        mappings = json.loads((cohort / "sources.json").read_text())
        current = json.loads((ROOT / destination).read_text())
        retired = json.loads((ROOT / "docs/content-foundations/unit07/templates.json").read_text())
        for key in keys:
            with self.subTest(kp_id=key):
                expected = [row for row in canonical if row["kp_id"] == key]
                self.assertEqual(len(expected), 1)
                mapping = [row for row in mappings if row["kp_id"] == key]
                self.assertEqual(len(mapping), 1)
                self.assertEqual(mapping[0]["sources"], [destination])
                matching = [row for row in current if row["kp_id"] == key and row["kind"] == "template"]
                self.assertEqual(len(matching), 1)
                self.assertEqual(matching[0]["arguments"], expected[0]["arguments"])
                self.assertFalse(any(row.get("kp_id") == key and row.get("kind") == "template" for row in retired))


if __name__ == "__main__":
    unittest.main()
