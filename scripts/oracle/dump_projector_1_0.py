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
import json
import sys

from _common import (
    add_code_base_arguments,
    add_goal_argument,
    add_now_argument,
    load_1_0,
    parity_blob,
    parse_now,
    point_at_code_base,
    sha256_of,
)


class StreamError(Exception):
    """A line of the stream that 1.0 refuses, with its line number."""


def read_stream(path: str, validate_event) -> list:
    """The validated events of the stream. Raise `StreamError` on a bad line."""
    events = []
    with open(path, encoding="utf-8") as handle:
        for lineno, line in enumerate(handle, 1):
            line = line.strip()
            if not line:
                continue
            try:
                events.append(validate_event(json.loads(line)))
            except Exception as exc:  # noqa: BLE001 - surface the line number
                raise StreamError(f"{path}:{lineno}: {exc}") from exc
    return events


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("stream", help="path to the JSONL event stream")
    add_code_base_arguments(ap)
    add_now_argument(ap)
    ap.add_argument("--tz", default=None)
    add_goal_argument(ap)
    ap.add_argument("--keep-built-from-ts", action="store_true")
    args = ap.parse_args()
    point_at_code_base(args)

    from cadus.events import validate_event
    from cadus.projector import PROJECTOR_VERSION, config_hash, project

    cfg, graph = load_1_0()

    try:
        events = read_stream(args.stream, validate_event)
    except StreamError as exc:
        print(exc, file=sys.stderr)
        return 2

    now = parse_now(args.now)
    model = project(events, graph, cfg, now=now, tz=args.tz, goal=args.goal)

    # Round-trip through the pydantic JSON encoder so datetimes/dates/enums are
    # rendered exactly as 1.0 persists them, then re-canonicalize.
    blob = parity_blob(model, keep_built_from_ts=args.keep_built_from_ts)

    print(blob)
    print(f"events={len(events)}", file=sys.stderr)
    print(f"projector_version={PROJECTOR_VERSION}", file=sys.stderr)
    print(f"config_hash={config_hash(cfg)}", file=sys.stderr)
    print(f"sha256={sha256_of(blob)}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
