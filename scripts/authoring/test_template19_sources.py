"""No external runtime is needed to protect the canonical recipe source mirrors."""
import unittest

from sync_template19_recipes import synchronize


class Template19SourcesTests(unittest.TestCase):
    def test_legacy_generators_have_not_reintroduced_superseded_recipes(self):
        self.assertEqual(synchronize(check=True), [])


if __name__ == "__main__":
    unittest.main()
