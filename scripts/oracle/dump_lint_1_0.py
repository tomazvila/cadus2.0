#!/usr/bin/env python
"""1.0 lint findings of one curriculum tree, as canonical JSON — the U3 oracle.

Usage: python dump_lint_1_0.py FIXTURE_DIR > expected.json
"""
from __future__ import annotations
import json, sys
sys.path.insert(0, "/home/deploy/dev/cadus")
from cadus.graph import CurriculumNotFound, Finding, lint_curriculum

#: The tail of a `yaml` message is the YAML library's own exception text, and it
#: embeds the absolute file path. PyYAML and the Rust YAML crate never write the
#: same text, so both sides replace the tail with this placeholder before the
#: comparison. The `{rel}: ` prefix, the code, the file and the fatal flag stay
#: exact (spec section 5, rule 1).
YAML_TAIL = "<yaml parser message>"


def normalize(entry: dict) -> dict:
    if entry["code"] == "yaml":
        entry["message"] = entry["message"].split(": ", 1)[0] + ": " + YAML_TAIL
    return entry


def main(path: str) -> int:
    try:
        findings = lint_curriculum(path)
    except CurriculumNotFound:
        # 1.0 `lint_curriculum` raises here; `Graph.load` reports the same tree with
        # this finding (`cadus/graph.py:206`). The port returns it (spec section 5).
        findings = [Finding("empty", "no curriculum found")]
    json.dump([normalize(f.as_dict()) for f in findings], sys.stdout,
              sort_keys=True, ensure_ascii=False, indent=2)
    print()
    return 0

if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1]))
