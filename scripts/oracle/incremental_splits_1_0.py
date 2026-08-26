#!/usr/bin/env python
"""M3 parity oracle: record where the 1.0 incremental fold leaves the full replay.

Read-only against the 1.0 code base. For every committed stream and every split
`k` it folds `project_incremental(project(events[:k]), events[:k], events[k:])`
and compares it with `project(events)`.

**The two are NOT equal at every split, and that is 1.0 behavior, not a defect.**
`project_incremental` seeds the FIRe topic states from the cached model. When a
`regraded` event arrives in the NEW half and supersedes a grade that the PRIOR
half already folded, the cache carries the uncorrected FIRe state, and the
corrected prior events replay light (`apply_fire=False`), so the correction never
reaches FIRe. `cadus.projector.project_incremental` documents this, and
`cadus.service.project_and_save` sends such a stream down the full-replay path
instead.

The 2.0 port must reproduce 1.0 EXACTLY: it must agree with the full replay at
the same splits, and it must produce the same divergent model at the same splits.
So this file records both -- the mismatching split indices and the 1.0 digest of
each mismatching incremental fold.

Usage:
  incremental_splits_1_0.py [--fixtures DIR] [--out FILE]
                            [--curriculum DIR] [--config FILE] [--now ISO8601]
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
from datetime import UTC, datetime

FIXTURES = os.path.normpath(
    os.path.join(
        os.path.dirname(os.path.abspath(__file__)),
        "..",
        "..",
        "crates",
        "core",
        "tests",
        "fixtures",
        "events",
    )
)


def canonical(obj: object) -> str:
    """Canonical JSON: sorted keys, compact separators, UTF-8, no NaN."""
    return json.dumps(
        obj, sort_keys=True, separators=(",", ":"), ensure_ascii=False, allow_nan=False
    )


def blob_of(model: object) -> str:
    """The parity blob: the canonical model with `built_from_ts` removed."""
    payload = json.loads(model.model_dump_json())
    payload.pop("built_from_ts", None)
    return canonical(payload)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--fixtures", default=FIXTURES)
    ap.add_argument("--out", default=None)
    ap.add_argument("--curriculum", default="/home/deploy/dev/cadus2.0/curriculum")
    ap.add_argument("--config", default="/home/deploy/dev/cadus/config.yaml")
    ap.add_argument("--now", default="2000-01-01T00:00:00+00:00")
    ap.add_argument("--goal", type=int, default=40)
    args = ap.parse_args()

    os.environ["CADUS_CURRICULUM"] = args.curriculum
    os.environ["CADUS_CONFIG"] = args.config

    from cadus.events import validate_event
    from cadus.loader import load_config, load_graph
    from cadus.projector import PROJECTOR_VERSION, config_hash, project, project_incremental

    cfg = load_config()
    graph = load_graph()
    now = datetime.fromisoformat(args.now)
    if now.tzinfo is None:
        now = now.replace(tzinfo=UTC)

    names = sorted(
        (n for n in os.listdir(args.fixtures) if re.fullmatch(r"stream_\d+\.jsonl", n)),
        key=lambda n: int(n.removeprefix("stream_").removesuffix(".jsonl")),
    )

    streams = []
    for name in names:
        events = []
        with open(os.path.join(args.fixtures, name), encoding="utf-8") as handle:
            for line in handle:
                line = line.strip()
                if line:
                    events.append(validate_event(json.loads(line)))

        full = blob_of(project(events, graph, cfg, now=now, tz=None, goal=args.goal))
        full_digest = hashlib.sha256(full.encode("utf-8")).hexdigest()

        mismatching: list[int] = []
        digests: dict[str, str] = {}
        for split in range(len(events) + 1):
            prior, fresh = events[:split], events[split:]
            cached = project(prior, graph, cfg, now=now, tz=None, goal=args.goal)
            model = project_incremental(
                cached, prior, fresh, graph, cfg, now=now, tz=None, goal=args.goal
            )
            blob = blob_of(model)
            if blob != full:
                mismatching.append(split)
                digests[str(split)] = hashlib.sha256(blob.encode("utf-8")).hexdigest()

        streams.append(
            {
                "stream": name,
                "events": len(events),
                "full_digest": full_digest,
                "mismatching_splits": mismatching,
                "mismatching_digests": digests,
            }
        )
        print(f"{name}: {len(events)} events, mismatching splits {mismatching}")

    index = {
        "oracle": "scripts/oracle/incremental_splits_1_0.py",
        "now": args.now,
        "goal": args.goal,
        "projector_version": PROJECTOR_VERSION,
        "config_hash": config_hash(cfg),
        "why": (
            "project_incremental seeds FIRe from the cached model, so a `regraded` "
            "event in the new half that supersedes a grade the prior half already "
            "folded never reaches FIRe. 1.0 documents this and routes such a stream "
            "down the full-replay path. The port must diverge at the SAME splits and "
            "to the SAME model."
        ),
        "streams": streams,
    }
    out = args.out or os.path.join(args.fixtures, "incremental_1_0.json")
    with open(out, "w", encoding="utf-8") as handle:
        handle.write(json.dumps(index, indent=2, sort_keys=True) + "\n")
    print(f"wrote {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
