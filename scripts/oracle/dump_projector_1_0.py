#!/usr/bin/env python
"""M3 parity oracle: fold a 1.0 event stream and print the learner model.

Read-only. Reads a JSONL event stream, validates every line through the 1.0
discriminated union (``cadus.events.validate_event``), folds it with the 1.0
projector (``cadus.projector.project``), and prints the resulting
``LearnerModel`` as canonical JSON (sorted keys, no whitespace padding) on
stdout. The sha256 of that exact byte string goes to stderr.

The 2.0 Rust port must produce the SAME bytes for the same stream.

Determinism rules this script enforces (a wall-clock value in the output would
break parity):

* ``now`` is pinned to ``--now`` (default 2000-01-01T00:00:00Z), so
  ``built_from_ts`` is fixed. ``built_from_ts`` is then STRIPPED from the
  compared model, because it is the one field that carries wall clock.
* ``tz`` defaults to UTC (``None``), ``goal`` defaults to 40.

Usage:
  dump_projector_1_0.py STREAM.jsonl [--curriculum DIR] [--config FILE]
                        [--now ISO8601] [--tz TZNAME] [--goal N]
                        [--keep-built-from-ts]
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
from datetime import UTC, datetime


def canonical(obj: object) -> str:
    """Canonical JSON: sorted keys, compact separators, UTF-8, no NaN."""
    return json.dumps(
        obj, sort_keys=True, separators=(",", ":"), ensure_ascii=False, allow_nan=False
    )


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("stream", help="path to the JSONL event stream")
    ap.add_argument("--curriculum", default="/home/deploy/dev/cadus2.0/curriculum")
    ap.add_argument("--config", default="/home/deploy/dev/cadus/config.yaml")
    ap.add_argument("--now", default="2000-01-01T00:00:00+00:00")
    ap.add_argument("--tz", default=None)
    ap.add_argument("--goal", type=int, default=40)
    ap.add_argument("--keep-built-from-ts", action="store_true")
    args = ap.parse_args()

    os.environ["CADUS_CURRICULUM"] = args.curriculum
    os.environ["CADUS_CONFIG"] = args.config

    from cadus.events import validate_event
    from cadus.loader import load_config, load_graph
    from cadus.projector import PROJECTOR_VERSION, config_hash, project

    cfg = load_config()
    graph = load_graph()

    events = []
    with open(args.stream, encoding="utf-8") as handle:
        for lineno, line in enumerate(handle, 1):
            line = line.strip()
            if not line:
                continue
            try:
                events.append(validate_event(json.loads(line)))
            except Exception as exc:  # noqa: BLE001 - surface the line number
                print(f"{args.stream}:{lineno}: {exc}", file=sys.stderr)
                return 2

    now = datetime.fromisoformat(args.now)
    if now.tzinfo is None:
        now = now.replace(tzinfo=UTC)

    model = project(events, graph, cfg, now=now, tz=args.tz, goal=args.goal)

    # Round-trip through the pydantic JSON encoder so datetimes/dates/enums are
    # rendered exactly as 1.0 persists them, then re-canonicalize.
    payload = json.loads(model.model_dump_json())
    if not args.keep_built_from_ts:
        payload.pop("built_from_ts", None)

    blob = canonical(payload)
    digest = hashlib.sha256(blob.encode("utf-8")).hexdigest()

    print(blob)
    print(f"events={len(events)}", file=sys.stderr)
    print(f"projector_version={PROJECTOR_VERSION}", file=sys.stderr)
    print(f"config_hash={config_hash(cfg)}", file=sys.stderr)
    print(f"sha256={digest}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
