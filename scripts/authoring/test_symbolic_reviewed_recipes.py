"""Tests for the fail-closed symbolic recipe scope."""
from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from apply_symbolic_reviewed import apply_unit
from apply_symbolic_reviewed import UNITS


class ReviewedRecipeTest(unittest.TestCase):
    def test_recipe_contains_only_audit_clean_kps(self) -> None:
        self.assertEqual(
            {unit: len(data["additions"]) for unit, data in UNITS.items()},
            {"03": 16, "04": 9, "05": 11},
        )
        for data in UNITS.values():
            import json
            for contract in data["contracts"].values():
                value = json.loads(contract)
                if value["kind"] == "label":
                    self.assertGreaterEqual(len(value["options"]), 2)


if __name__ == "__main__":
    unittest.main()
