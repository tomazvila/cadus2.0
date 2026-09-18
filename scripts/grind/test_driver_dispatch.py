#!/usr/bin/env python3
"""Stdlib-only unittest coverage for orchestrator.plan_action (defect 1).

Tests the pure classification function only — no network, no DB.
"""
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
os.environ.setdefault("CADUS_GRIND_LOG", "/tmp/orchestrator-test.log")

from orchestrator import plan_action  # noqa: E402


class TestPlanAction(unittest.TestCase):
    def test_walk_when_tasks_present(self):
        plan = {"tasks": [{"task_id": "t1"}], "blocked": [], "course_complete": False}
        self.assertEqual(plan_action(plan), "walk")

    def test_walk_wins_over_blocked(self):
        plan = {"tasks": [{"task_id": "t1"}],
                "blocked": [{"topic": "probability-statistics/expectation"}]}
        self.assertEqual(plan_action(plan), "walk")

    def test_author_when_blocked_and_no_tasks(self):
        plan = {"tasks": [], "blocked": [{"topic": "probability-statistics/kp3"}],
                "course_complete": False, "quiz_due": False}
        self.assertEqual(plan_action(plan), "author")

    def test_end_when_course_complete(self):
        plan = {"tasks": [], "blocked": [], "course_complete": True, "quiz_due": False}
        self.assertEqual(plan_action(plan), "end")

    def test_idle_when_frontier_open(self):
        plan = {"tasks": [], "blocked": [], "course_complete": False, "quiz_due": False}
        self.assertEqual(plan_action(plan), "idle")

    def test_quiz_due_with_empty_tasks_is_walk(self):
        plan = {"tasks": [], "blocked": [], "course_complete": False, "quiz_due": True}
        self.assertEqual(plan_action(plan), "walk")

    def test_quiz_due_beats_blocked(self):
        plan = {"tasks": [], "blocked": [{"topic": "x"}], "quiz_due": True}
        self.assertEqual(plan_action(plan), "walk")

    def test_missing_keys_treated_as_empty(self):
        self.assertEqual(plan_action({}), "idle")
        self.assertEqual(plan_action({"tasks": []}), "idle")
        self.assertEqual(plan_action({"blocked": []}), "idle")
        self.assertEqual(plan_action({"course_complete": False}), "idle")
        self.assertEqual(plan_action({"tasks": [], "blocked": [{"topic": "t"}]}), "author")
        self.assertEqual(plan_action({"tasks": [], "course_complete": True}), "end")
        self.assertEqual(plan_action({"quiz_due": True}), "walk")

    def test_none_and_empty_containers(self):
        self.assertEqual(plan_action({"tasks": None, "blocked": None,
                                      "course_complete": None, "quiz_due": None}), "idle")
        self.assertEqual(plan_action({"tasks": [], "blocked": [],
                                      "course_complete": False, "quiz_due": False}), "idle")


if __name__ == "__main__":
    unittest.main()
