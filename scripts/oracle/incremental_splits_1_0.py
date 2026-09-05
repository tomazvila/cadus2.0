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

A `profile_reset` is a SECOND divergence class with the same cause and no
correction in it. `finalize` drops a topic state that equals a default one, so a
reset topic leaves the cached model; `ability_update` (`fire.py:399`) then skips a
target that is absent from the states, and the resume stops propagating onto that
topic. `cadus.service.project_and_save` (`service.py:272`) forces the full replay
on a `Regraded` event or a `projector_version` mismatch and on nothing else, so a
reset stream stays on the incremental path in 1.0. `stream_u3_coverage.jsonl`
carries that class, so this oracle reads it beside the numbered streams.

The 2.0 port must reproduce 1.0 EXACTLY: it must agree with the full replay at
the same splits, and it must produce the same divergent model at the same splits.
So this file records both -- the mismatching split indices and the 1.0 digest of
each mismatching incremental fold.

Usage:
  incremental_splits_1_0.py [--fixtures DIR] [--out FILE]
                            [--curriculum DIR] [--config FILE] [--now ISO8601]
"""

from __future__ import annotations

import os

from _common import (
    load_1_0,
    parity_blob,
    parse_now,
    parse_stream_oracle_args,
    read_events,
    sha256_of,
    stream_names,
    write_index,
)

#: The streams outside the `stream_N.jsonl` family that this oracle also reads.
EXTRA_STREAMS = ["stream_u3_coverage.jsonl"]

#: Why the incremental fold diverges, as the index records it.
WHY = (
    "project_incremental seeds FIRe from the cached model, so a `regraded` "
    "event in the new half that supersedes a grade the prior half already "
    "folded never reaches FIRe. 1.0 documents this and routes such a stream "
    "down the full-replay path (service.py:272 -- a Regraded event or a "
    "projector_version mismatch, and nothing else). A profile_reset is the "
    "second class: finalize drops the reset topic's default state, so the "
    "resume stops propagating onto it, and service.py:272 leaves such a "
    "stream on the incremental path. The port must diverge at the SAME "
    "splits and to the SAME model in both classes."
)


def stream_list(fixtures: str) -> list[str]:
    """The numbered streams, then every extra stream that exists."""
    names = stream_names(fixtures)
    # The coverage stream carries the `profile_reset` divergence class, which no
    # numbered stream reaches. It sorts last, after the numbered streams.
    for extra in EXTRA_STREAMS:
        if os.path.exists(os.path.join(fixtures, extra)):
            names.append(extra)
    return names


def mismatching_splits(events, full: str, graph, cfg, now, goal: int):
    """The splits whose incremental fold leaves the full replay, with their digests."""
    from cadus.projector import project, project_incremental

    mismatching: list[int] = []
    digests: dict[str, str] = {}
    for split in range(len(events) + 1):
        prior, fresh = events[:split], events[split:]
        cached = project(prior, graph, cfg, now=now, tz=None, goal=goal)
        model = project_incremental(
            cached, prior, fresh, graph, cfg, now=now, tz=None, goal=goal
        )
        blob = parity_blob(model)
        if blob != full:
            mismatching.append(split)
            digests[str(split)] = sha256_of(blob)
    return mismatching, digests


def main() -> int:
    args = parse_stream_oracle_args(__doc__)

    from cadus.events import validate_event
    from cadus.projector import PROJECTOR_VERSION, config_hash, project

    cfg, graph = load_1_0()
    now = parse_now(args.now)

    streams = []
    for name in stream_list(args.fixtures):
        events = read_events(os.path.join(args.fixtures, name), validate_event)
        full = parity_blob(project(events, graph, cfg, now=now, tz=None, goal=args.goal))
        mismatching, digests = mismatching_splits(events, full, graph, cfg, now, args.goal)
        streams.append(
            {
                "stream": name,
                "events": len(events),
                "full_digest": sha256_of(full),
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
        "why": WHY,
        "streams": streams,
    }
    write_index(args.out or os.path.join(args.fixtures, "incremental_1_0.json"), index)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
