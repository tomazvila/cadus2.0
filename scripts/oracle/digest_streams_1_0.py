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

import os

from _common import (
    load_1_0,
    parity_digest,
    parse_now,
    parse_stream_oracle_args,
    read_events,
    stream_names,
    write_index,
)

#: The zones every stream is folded in. `null` is UTC, which is 1.0's default.
ZONES = [None, "America/New_York"]


def fold_digest(events, graph, cfg, now, zone, goal: int) -> str:
    """The parity digest of the 1.0 fold of `events` in `zone`."""
    from cadus.projector import project

    return parity_digest(project(events, graph, cfg, now=now, tz=zone, goal=goal))


def main() -> int:
    args = parse_stream_oracle_args(__doc__)

    from cadus.events import validate_event
    from cadus.projector import PROJECTOR_VERSION, config_hash

    cfg, graph = load_1_0()
    now = parse_now(args.now)

    streams = []
    for name in stream_names(args.fixtures):
        events = read_events(os.path.join(args.fixtures, name), validate_event)
        digests = {
            zone or "UTC": fold_digest(events, graph, cfg, now, zone, args.goal)
            for zone in ZONES
        }
        # The PRE-CORRECTION fold: every `regraded` event deleted from the stream.
        # `apply_regrades` works on copies, so deleting the corrections must give
        # this model back -- which pins that the corrected events stay intact.
        bare = [event for event in events if event.type != "regraded"]
        digests["UTC_no_regrades"] = fold_digest(bare, graph, cfg, now, None, args.goal)
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
    write_index(args.out or os.path.join(args.fixtures, "digests_1_0.json"), index)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
