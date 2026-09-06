"""The generic local-draft importer validates, serves, and stops on every path."""
import json
import os
import socket
import stat
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace

import import_local_drafts as importer

SCRATCH = Path.home() / ".cache" / "cadus2_tmp"

FAKE_WORKER = r'''#!/usr/bin/env python3
import json, os, sys, urllib.request
kind = sys.argv[sys.argv.index("--kind") + 1]
key = sys.argv[sys.argv.index("--kp") + 1]
topic, kp = key.split("/")
tool = {"teach": "emit_teach", "hint_ladder": "emit_hint_ladder"}[kind]
request = {"messages": [{"role": "user", "content": f"Topic: T (id: {topic})\nKnowledge point: K (id: {kp})"}],
           "tool_choice": {"function": {"name": tool}}}
url = os.environ["OPENAI_BASE_URL"] + "/chat/completions"
with urllib.request.urlopen(urllib.request.Request(url, json.dumps(request).encode(), {"Content-Type": "application/json"}), timeout=2) as reply:
    body = json.load(reply)
print("key", os.environ["OPENAI_API_KEY"], "model", os.environ["OPENAI_MODEL"], "cost", body["usage"]["cost"])
print("port", url.split(":")[2].split("/")[0])
print(f"{kind}: stored 1 skipped 0 declined 0 calls 1 alerts 0")
print("reserved: 0 micro-USD; reported: 0 micro-USD; price-bound breach: false; stored 1; declined 0")
'''


def write_manifest(directory, rows, **extra):
    (directory / "part.json").write_text(json.dumps(rows))
    manifest = {"files": ["part.json"], **extra}
    path = directory / "manifest.json"
    path.write_text(json.dumps(manifest))
    return path


def good_rows():
    return [
        {"kp_id": "single-digit-addition/kp1", "kind": "teach",
         "arguments": {"concept": "Count on.", "worked_example": {"problem": "Compute $2 + 3$.", "steps": ["$2 + 3 = 5$."]}}},
        {"kp_id": "single-digit-addition/kp1", "kind": "hint_ladder", "arguments": {"hints": ["Start where?"]}},
    ]


class ManifestTest(unittest.TestCase):
    def setUp(self):
        SCRATCH.mkdir(parents=True, exist_ok=True)
        self.scratch = tempfile.TemporaryDirectory(dir=SCRATCH)
        self.directory = Path(self.scratch.name)

    def tearDown(self):
        self.scratch.cleanup()

    def load(self, rows, **extra):
        return importer.validate(*importer.load_document(write_manifest(self.directory, rows, **extra)))

    def test_manifest_files_load_and_validate(self):
        drafts = self.load(good_rows(), kinds=["teach", "hint_ladder"], knowledge_points=1)
        self.assertEqual(set(drafts), {("single-digit-addition/kp1", "teach"), ("single-digit-addition/kp1", "hint_ladder")})

    def test_a_plain_draft_list_is_accepted(self):
        path = self.directory / "list.json"
        path.write_text(json.dumps(good_rows()))
        self.assertEqual(len(importer.validate(*importer.load_document(path))), 2)

    def test_invalid_drafts_are_refused(self):
        cases = {
            "duplicate": good_rows() + [good_rows()[0]],
            "serving key": [{"kp_id": "kp1", "kind": "teach", "arguments": {"a": 1}}],
            "kind": [{"kp_id": "t/kp1", "kind": "essay", "arguments": {"a": 1}}],
            "non-empty JSON object": [{"kp_id": "t/kp1", "kind": "teach", "arguments": []}],
            "exactly kp_id": [{"kp_id": "t/kp1", "kind": "teach", "arguments": {"a": 1}, "status": "approved"}],
            "names no draft": [],
        }
        for expected, rows in cases.items():
            with self.assertRaises(importer.DraftError, msg=expected) as error:
                self.load(rows)
            self.assertIn(expected, str(error.exception))

    def test_manifest_declarations_are_checked(self):
        with self.assertRaises(importer.DraftError) as error:
            self.load(good_rows(), kinds=["teach"])
        self.assertIn("outside the manifest 'kinds'", str(error.exception))
        with self.assertRaises(importer.DraftError) as error:
            self.load(good_rows(), knowledge_points=3)
        self.assertIn("declares 3", str(error.exception))
        with self.assertRaises(importer.DraftError):
            importer.load_document(write_manifest(self.directory, good_rows(), files=["../etc/passwd"]))

    def test_report_lines_are_summed(self):
        stdout = ("teach: stored 3 skipped 1 declined 2 calls 9 alerts 0\n"
                  "declined a/kp1 teach after 5 attempts: reason\n"
                  "hint_ladder: stored 4 skipped 0 declined 0 calls 4 alerts 0\n"
                  "reserved: 1 micro-USD; reported: 0 micro-USD; price-bound breach: false; stored 7; declined 2\n")
        totals, declines, reported = importer.parse_report(stdout)
        self.assertEqual(totals, {"stored": 7, "skipped": 1, "declined": 2, "calls": 13})
        self.assertEqual(declines, ["declined a/kp1 teach after 5 attempts: reason"])
        self.assertEqual(reported, [0])


class RunTest(unittest.TestCase):
    def setUp(self):
        SCRATCH.mkdir(parents=True, exist_ok=True)
        self.scratch = tempfile.TemporaryDirectory(dir=SCRATCH)
        self.directory = Path(self.scratch.name)
        self.worker = self.directory / "fake-worker.py"
        self.worker.write_text(FAKE_WORKER)
        self.worker.chmod(self.worker.stat().st_mode | stat.S_IXUSR)

    def tearDown(self):
        self.scratch.cleanup()

    def test_the_run_serves_over_loopback_with_a_placeholder_key_and_stops(self):
        drafts = importer.validate({}, good_rows())
        args = SimpleNamespace(worker=str(self.worker), curriculum=None, budget_usd="5",
                               request_reserve_usd="0.50", concurrency=1, dry_run=False)
        os.environ["OPENAI_API_KEY"] = "sk-real-key-must-not-reach-the-child"
        from io import StringIO
        captured = StringIO()
        real_stdout, sys.stdout = sys.stdout, captured
        try:
            code = importer.run(args, drafts)
        finally:
            sys.stdout = real_stdout
            del os.environ["OPENAI_API_KEY"]
        output = captured.getvalue()
        self.assertEqual(code, 0, output)
        self.assertIn("key operator-draft-local-only model operator-draft-v1 cost 0", output)
        self.assertIn("local drafts: stored 2 skipped 0 declined 0 calls 2", output)
        self.assertIn("model cost reported 0 micro-USD", output)
        port = int(next(line.split()[1] for line in output.splitlines() if line.startswith("port ")))
        with socket.socket() as probe:
            probe.settimeout(1)
            self.assertNotEqual(probe.connect_ex(("127.0.0.1", port)), 0, "the server still listens")


if __name__ == "__main__":
    unittest.main()
