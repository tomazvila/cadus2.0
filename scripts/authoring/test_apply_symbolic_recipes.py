"""Regression tests for complete symbolic recipe application."""
from __future__ import annotations
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
import apply_symbolic_linear_graphs as linear
import apply_symbolic_systems_inequalities as systems
from foundations_curriculum_patch import ExemplarKey

def minimal_yaml(topic: str, kp: str) -> str:
    return (
        "topics:\n"
        f"  - id: {topic}\n"
        "    knowledge_points:\n"
        f"      - id: {kp}\n"
        "        exemplars:\n"
        "          - problem: 'Example?'\n"
        '            answer: "1"\n'
    )

class ApplyRecipeSketchTests(unittest.TestCase):
    def assert_driver_installs_sketch(self, module, key: ExemplarKey) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "unit.yaml"
            path.write_text(minimal_yaml(key.topic_id, key.kp_id))
            argv = [module.__file__, "--curriculum", str(path), "--write"]
            with (
                patch.object(module, "CONTRACTS", {}),
                patch.object(module, "NEW", {}),
                patch.object(module, "REVIEW_PATCHES", {}),
                patch.object(module, "SKETCHES", {key: "Reviewed reasoning."}),
                patch.object(sys, "argv", argv),
            ):
                self.assertEqual(module.main(), 0)
            self.assertIn(
                "            solution_sketch: 'Reviewed reasoning.'\n",
                path.read_text(),
            )

    def test_linear_driver_installs_solution_sketches(self) -> None:
        self.assert_driver_installs_sketch(
            linear, ExemplarKey("linear-replay-probe", "kp1", 0)
        )

    def test_systems_driver_installs_solution_sketches(self) -> None:
        self.assert_driver_installs_sketch(
            systems, ExemplarKey("systems-replay-probe", "kp1", 0)
        )

if __name__ == "__main__":
    unittest.main()
