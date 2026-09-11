#!/usr/bin/env python3
"""Regenerate `testdata/foundations_topics.json` from the curriculum tree.

Runs the existing `dump_curriculum` binary (the M1 parity oracle of
`crates/core/src/bin/dump_curriculum.rs`), keeps only the `foundations`
course, and trims every topic to the fields this package's generators read:
`id`, `unit`, `name`, and each knowledge point's `id`, `name`, `constraints`
and `exemplars`. No exemplar, no answer and no answer_contract is edited.
"""
import argparse
import json
import subprocess
import sys
from pathlib import Path

FIELDS = ("id", "name", "constraints")


def trim_topic(topic):
    return {
        "id": topic["id"],
        "unit": topic["unit"],
        "name": topic["name"],
        "knowledge_points": [
            {**{key: kp.get(key) for key in FIELDS}, "exemplars": kp["exemplars"]}
            for kp in topic["knowledge_points"]
        ],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dump-curriculum", required=True, help="the dump_curriculum binary")
    parser.add_argument("--curriculum", default="curriculum", help="the curriculum tree")
    parser.add_argument(
        "--out",
        default=str(Path(__file__).parent / "testdata" / "foundations_topics.json"),
    )
    args = parser.parse_args()
    result = subprocess.run(
        [args.dump_curriculum, args.curriculum], capture_output=True, text=True, check=False
    )
    if result.returncode != 0:
        print(result.stderr, file=sys.stderr)
        return result.returncode
    dump = json.loads(result.stdout)
    topics = [trim_topic(t) for t in dump["topics"] if t.get("course") == "foundations"]
    Path(args.out).write_text(json.dumps(topics, indent=1, sort_keys=True) + "\n")
    print(f"wrote {len(topics)} foundations topics to {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
