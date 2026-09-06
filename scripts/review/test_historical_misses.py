#!/usr/bin/env python3
"""Conservative historical recovery report regressions; no database writes."""
import copy
import hashlib
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import historical_misses as scan

USER = "12345678-1234-1234-1234-123456789abc"


def attempt(ident="a1", version=1, correct=False, **extra):
    body = dict(type="attempt", v=version, attempt_id=ident, task_id="t1",
                topic="absolute-value", correct=correct,
                problem={"text": "Find the value.", "expected": "2"},
                given_answer="about two")
    return body | extra


def export(events):
    header = dict(record_type="snapshot", user_id=USER,
                  event_count=len(events), through_seq=len(events))
    rows = [dict(record_type="event", user_id=USER, seq=i + 1,
                 schema_version=e.get("v", 1), event_json=json.dumps(e, ensure_ascii=False))
            for i, e in enumerate(events)]
    return [header, *rows]


def run(rows):
    return scan.report(map(json.dumps, rows))


class HistoricalMisses(unittest.TestCase):
    def test_all_unproven_v1_misses_need_review_even_if_arithmetic_looks_wrong(self):
        result = run(export([attempt(), attempt("a2", given_answer="3")]))
        self.assertEqual(result["review_required"], 2)
        self.assertTrue(all(c["review_status"] == "human_review_required"
                            for c in result["candidates"]))

    def test_correct_v1_v2_misses_and_explicit_uncertainty_are_distinct(self):
        result = run(export([
            attempt(correct=True), attempt("a2", version=2),
            attempt("a3", outcome={"ungraded": {"reason": "cannot decide"}}),
            attempt("a4", outcome="incorrect"), attempt("a5")]))
        self.assertEqual([c["attempt_id"] for c in result["candidates"]], ["a5"])
        self.assertEqual(result["counts"]["v1_incorrect_with_explicit_outcome"], 2)
        self.assertEqual(result["counts"]["v2_attempts"], 1)

    def test_lexical_export_bytes_and_unicode_are_preserved(self):
        rows = export([attempt()])
        raw = ' { "type":"attempt", "v":1, "attempt_id":"a1", "task_id":"t1", "topic":"x", "correct":false, "given_answer":"≈ two" } '
        rows[1]["event_json"] = raw
        before = copy.deepcopy(rows)
        candidate = run(rows)["candidates"][0]
        self.assertEqual(candidate["stored_payload"], raw)
        self.assertEqual(candidate["stored_payload_sha256"],
                         hashlib.sha256(raw.encode()).hexdigest())
        self.assertEqual(rows, before)

    def test_missing_payload_version_uses_stored_column_without_inventing_it(self):
        row = attempt()
        del row["v"]
        candidate = run(export([row]))["candidates"][0]
        self.assertEqual(candidate["stored_schema_version"], 1)
        self.assertFalse(candidate["payload_version_present"])

    def test_corrections_are_preserved_without_clearing_review_requirement(self):
        correction = dict(type="regraded", v=2, task_id="t1", topic="absolute-value",
                          reason="explicit later review", attempts=[
                              {"attempt_id": "a1", "outcome": {"ungraded": {"reason": "uncertain"}}}])
        rows = export([attempt(), correction])
        candidate = run(rows)["candidates"][0]
        self.assertEqual(candidate["review_status"], "human_review_required")
        self.assertEqual(candidate["correction_evidence"][0]["seq"], 2)
        correction["attempts"][0]["outcome"] = "correct"
        self.assertEqual(run(export([attempt(), correction]))["review_required"], 1)

    def test_empty_snapshot_is_valid_but_headerless_input_is_not(self):
        self.assertEqual(run(export([]))["review_required"], 0)
        with self.assertRaisesRegex(ValueError, "header"):
            scan.report([])

    def test_event_order_cannot_change_report_or_digest(self):
        rows = export([attempt(), attempt("a2"), {"type": "session_end", "v": 1}])
        shuffled = [rows[0], rows[3], rows[1], rows[2]]
        self.assertEqual(scan.canonical(run(rows)), scan.canonical(run(shuffled)))

    def test_truncated_gapped_duplicate_and_cross_tenant_exports_fail_closed(self):
        base = export([attempt(), attempt("a2")])
        bad = [base[:-1]]
        duplicate = copy.deepcopy(base)
        duplicate[2]["seq"] = 1
        bad.append(duplicate)
        foreign = copy.deepcopy(base)
        foreign[2]["user_id"] = "00000000-0000-0000-0000-000000000001"
        bad.append(foreign)
        gap = copy.deepcopy(base)
        gap[2]["seq"] = 3
        bad.append(gap)
        for rows in bad:
            with self.subTest(rows=rows), self.assertRaises(ValueError):
                run(rows)

    def test_unsupported_or_conflicting_schema_versions_fail_closed(self):
        for column, payload in [(3, 3), (1, 2), (True, 1)]:
            rows = export([attempt(version=payload)])
            rows[1]["schema_version"] = column
            with self.subTest(column=column), self.assertRaises(ValueError):
                run(rows)

    def test_malformed_payloads_outcomes_and_attempt_identity_fail_closed(self):
        for event in [attempt(outcome=None), attempt(correct=0),
                      attempt(outcome={"ungraded": {}})]:
            with self.subTest(event=event), self.assertRaises(ValueError):
                run(export([event]))
        with self.assertRaisesRegex(ValueError, "duplicate attempt"):
            run(export([attempt(), attempt()]))

    def test_duplicate_json_keys_and_non_json_numbers_are_refused(self):
        for text in ['{"v":1,"v":2}', '{"correct":NaN}']:
            with self.subTest(text=text), self.assertRaises(ValueError):
                scan.decode(text)

    def test_snapshot_hash_binds_corrections_and_tenant(self):
        rows = export([attempt(), {"type": "session_end", "v": 1}])
        before = run(rows)["source_sha256"]
        rows[2]["event_json"] = '{"type":"session_end","v":1,"session":"different"}'
        self.assertNotEqual(run(rows)["source_sha256"], before)

    def test_cli_emits_no_partial_report_on_failure_and_is_repeatable(self):
        script = Path(scan.__file__)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "export.jsonl"
            path.write_text("\n".join(map(json.dumps, export([attempt()]))))
            command = [sys.executable, str(script), str(path)]
            first = subprocess.run(command, capture_output=True, check=True)
            second = subprocess.run(command, capture_output=True, check=True)
            self.assertEqual(first.stdout, second.stdout)
            self.assertEqual(json.loads(first.stdout)["review_required"], 1)
            path.write_text("{}\n")
            failed = subprocess.run(command, capture_output=True)
            self.assertNotEqual(failed.returncode, 0)
            self.assertEqual(failed.stdout, b"")
            self.assertIn(b"refused", failed.stderr)

    def test_committed_historical_fixture_scan_is_complete_and_deterministic(self):
        root = Path(__file__).resolve().parents[2]
        fixture = root / "crates/core/tests/fixtures/events/stream_12.jsonl"
        events = [json.loads(line) for line in fixture.read_text().splitlines() if line.strip()]
        result = run(export(events))
        expected = [e["attempt_id"] for e in events if e["type"] == "attempt"
                    and e.get("v", 1) == 1 and e["correct"] is False and "outcome" not in e]
        self.assertEqual([c["attempt_id"] for c in result["candidates"]], expected)
        self.assertEqual(result["review_required"], 4)
        self.assertEqual(hashlib.sha256(fixture.read_bytes()).hexdigest(),
                         "824d35e2ed810ba5aea0b3e7404a03635cd488de37f4e6c37885456e8629b028")
        self.assertEqual(scan.digest(scan.canonical(result)),
                         "71550c00e5cede9b378adfd333850b82d6bff1eb9257d380cb61cc4e70566972")
        self.assertEqual(result["scope"]["event_count"], len(events))


if __name__ == "__main__":
    unittest.main()
