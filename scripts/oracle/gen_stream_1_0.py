#!/usr/bin/env python
"""M3 parity fixture generator: build ONE synthetic 1.0 event stream.

Read-only against the 1.0 code base. Constructs events directly through the 1.0
pydantic models (no store, no database, no wall clock), so a stream is a pure
function of its seed and of the constants in this file.

Two stream families live here.

``--seed 1`` (the default) rebuilds the ORIGINAL hand-designed stream #1,
byte-for-byte. `docs/plans/M3.md` pins its fold digest, so its bytes are frozen.
Spec section 9 names the seeded generator "stream #2 onward" for that reason.

``--seed N`` for ``N >= 2`` builds the SEEDED COVERAGE stream of spec section 9:
one stream that reaches every event type, every reachable FIRe branch, both
lesson-close XP paths, the diagnostic initial and refresh paths, `profile_reset`,
the rounding edges, the quiz threshold, the streak edges in a non-UTC time zone,
and several corrections. The seed varies the prerequisite chain, the tiers, the
gaps, the balances, and the corrected tasks; the coverage skeleton is the same
for every seed.

Coverage of stream #1 (`m3_stream_1.jsonl`):

* ``enrolled`` (course scope for velocity / course_progress / mastery floor)
* two sessions: ``session_start`` / ``session_end`` x2
* ``task_served`` (a projector no-op -- pinned here so the port also treats it
  as a no-op)
* 20 ``attempt`` events over 5 topics of ONE real prerequisite chain, with
  mixed ``correct`` and mixed ``work_quality``, including two attempts where
  ``correct`` and the tier DISAGREE (pins "the projector never reads
  work_quality of an attempt")
* 5 ``lesson_result`` (4 passing, 1 failing with ``failed_at_kp``) and
  5 ``review_result`` (mixed pass/fail, one ``assisted=True``)
* ``remediation_triggered`` (the lesson-fail journal)
* ``regraded`` correcting one attempt's tier AND superseding one
  ``review_result``'s ``quality_tier`` / ``xp``
* an 8-day gap between the lesson block and the review block, so the review
  block lands OVERDUE and exercises ``fire.decay_for`` > 1.

XP is priced with the real 1.0 ``cadus.xp.task_xp``, deliberately UNROUNDED on
``lesson_result`` (mirroring ``service._advance_lesson``, which writes the raw
float) and rounded to 2dp on ``review_result`` (mirroring
``service.review_result``). That reproduces the ``8.924999999999999`` float
artifact in the log on purpose -- it is a parity trap the port must match.

Two branches of spec section 9 are UNREACHABLE and are therefore absent from
every seeded stream. Both are pinned by direct unit tests instead:

* ``interval_for`` never reaches its ``730.0`` cap. The default
  ``interval_table`` ends at ``480.0``, and interpolation never leaves the
  table, so ``min(value, 730.0)`` is always the interpolated value.
* ``speed_for`` never reaches either clamp. ``speed = (0.5 + a) / (0.5 + d)``
  with ``a`` in ``[0, 1]`` (``_clamp01``) and ``d`` in ``[0.05, 0.75]`` (the
  curriculum's range) spans ``[0.4, 3.0)`` open at the top, so neither ``0.33``
  nor ``3.0`` binds.

Usage:
  gen_stream_1_0.py [--seed N] [--out FILE] [--out-dir DIR]
                    [--curriculum DIR] [--config FILE]
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os

from _common import FIXTURES, add_code_base_arguments, load_1_0, point_at_code_base


def build(seed: int, cfg, graph, ap: argparse.ArgumentParser) -> list[object]:
    """The stream of `seed`: stream #1 for seed 1, the seeded family for seed >= 2.

    The builders import the 1.0 models at load time, so they are imported here,
    after the loader is pointed at its trees.
    """
    if seed == 1:
        from _gen_stream_1 import build_stream_1

        return build_stream_1(cfg, graph)
    if seed >= 2:
        from _gen_stream_seeded import build_seeded_stream

        return build_seeded_stream(seed, cfg, graph)
    ap.error("--seed must be 1 or greater")
    return []


def canonical_lines(events: list[object]) -> list[str]:
    """One canonical JSON line per event, exactly as the fixtures store them."""
    lines = []
    for event in events:
        payload = json.loads(event.model_dump_json())
        lines.append(
            json.dumps(payload, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
        )
    return lines


def type_counts(events: list[object]) -> dict[str, int]:
    """How many events of each type the stream holds."""
    counts: dict[str, int] = {}
    for event in events:
        counts[event.type] = counts.get(event.type, 0) + 1
    return counts


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--out", default=None, help="the stream file to write")
    ap.add_argument("--out-dir", default=FIXTURES)
    add_code_base_arguments(ap)
    args = ap.parse_args()
    point_at_code_base(args)

    cfg, graph = load_1_0()
    events = build(args.seed, cfg, graph, ap)

    if args.out:
        stream_path = args.out
        os.makedirs(os.path.dirname(os.path.abspath(stream_path)), exist_ok=True)
    else:
        os.makedirs(args.out_dir, exist_ok=True)
        stream_path = os.path.join(args.out_dir, f"stream_{args.seed}.jsonl")

    blob = "\n".join(canonical_lines(events)) + "\n"
    with open(stream_path, "w", encoding="utf-8") as handle:
        handle.write(blob)

    print(f"wrote {stream_path} ({len(events)} events)")
    print(f"stream sha256 = {hashlib.sha256(blob.encode('utf-8')).hexdigest()}")
    counts = type_counts(events)
    for key in sorted(counts):
        print(f"  {key}: {counts[key]}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
