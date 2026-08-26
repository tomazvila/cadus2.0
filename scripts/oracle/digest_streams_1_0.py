#!/usr/bin/env python
"""M3 parity oracle: record the 1.0 fold digest of every committed stream.

Read-only against the 1.0 code base. It folds each `stream_N.jsonl` fixture with
the 1.0 projector, once per time zone, and writes one `digests_1_0.json` index.
`crates/core/tests/parity_events.rs` reads that index and asserts the Rust fold
reproduces every digest.

The digest is the sha256 of the canonical model blob with `built_from_ts`
removed -- the same bytes `dump_projector_1_0.py` prints. `now` is pinned, so no
wall clock enters the result.

Usage:
  digest_streams_1_0.py [--fixtures DIR] [--out FILE]
                        [--curriculum DIR] [--config FILE] [--now ISO8601]
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
from datetime import UTC, datetime

#: The zones every stream is folded in. `null` is UTC, which is 1.0's default.
ZONES = [None, "America/New_York"]

#: The fixture directory of this repository, found from this file's own path.
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
    from cadus.projector import PROJECTOR_VERSION, config_hash, project

    cfg = load_config()
    graph = load_graph()
    now = datetime.fromisoformat(args.now)
    if now.tzinfo is None:
        now = now.replace(tzinfo=UTC)

    names = sorted(
        (name for name in os.listdir(args.fixtures) if re.fullmatch(r"stream_\d+\.jsonl", name)),
        key=lambda name: int(name.removeprefix("stream_").removesuffix(".jsonl")),
    )

    streams = []
    for name in names:
        events = []
        with open(os.path.join(args.fixtures, name), encoding="utf-8") as handle:
            for line in handle:
                line = line.strip()
                if line:
                    events.append(validate_event(json.loads(line)))

        def digest_of(stream_events, zone):
            model = project(stream_events, graph, cfg, now=now, tz=zone, goal=args.goal)
            payload = json.loads(model.model_dump_json())
            payload.pop("built_from_ts", None)
            return hashlib.sha256(canonical(payload).encode("utf-8")).hexdigest()

        digests = {zone or "UTC": digest_of(events, zone) for zone in ZONES}
        # The PRE-CORRECTION fold: every `regraded` event deleted from the stream.
        # `apply_regrades` works on copies, so deleting the corrections must give
        # this model back -- which pins that the corrected events stay intact.
        bare = [event for event in events if event.type != "regraded"]
        digests["UTC_no_regrades"] = digest_of(bare, None)
        streams.append(
            {
                "stream": name,
                "events": len(events),
                "events_without_regrades": len(bare),
                "digests": digests,
            }
        )
        print(f"{name}: {len(events)} events {digests}")

    index = {
        "oracle": "scripts/oracle/digest_streams_1_0.py",
        "generator": "scripts/oracle/gen_stream_1_0.py",
        "now": args.now,
        "goal": args.goal,
        "projector_version": PROJECTOR_VERSION,
        "config_hash": config_hash(cfg),
        "zones": ["UTC", "America/New_York", "UTC_no_regrades"],
        "streams": streams,
    }
    out = args.out or os.path.join(args.fixtures, "digests_1_0.json")
    with open(out, "w", encoding="utf-8") as handle:
        handle.write(json.dumps(index, indent=2, sort_keys=True) + "\n")
    print(f"wrote {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
