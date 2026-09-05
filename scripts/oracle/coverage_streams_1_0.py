#!/usr/bin/env python
"""M3 coverage oracle: measure what each committed stream actually exercises.

Read-only against the 1.0 code base. It wraps the 1.0 FIRe and projector
functions with counting probes, folds every `stream_N.jsonl` fixture, and writes
the spec section 9 coverage table to
`crates/core/tests/fixtures/events/coverage.md`.

Every row is a MEASURED branch hit, not a claim from the generator. A probe adds
no state to the fold: each wrapper calls the real function and records what that
call did, so the digests stay the digests of `digest_streams_1_0.py`.

Usage:
  coverage_streams_1_0.py [--fixtures DIR] [--out FILE]
                          [--curriculum DIR] [--config FILE] [--now ISO8601]
"""

from __future__ import annotations

import math
import os
from datetime import timedelta

from _common import (
    iter_rows,
    load_1_0,
    parse_stream_oracle_args,
    stream_names,
)
from _coverage_probes import Probe, close, install

#: The non-UTC zone the streak block is designed around (spec section 9 item 9).
COVERAGE_TZ = "America/New_York"

#: (probe id, spec section 9 item, what the probe records).
PROBES = [
    ("types.all_16", 1, "all 16 event types present"),
    ("types.noop_6", 1, "the six no-op types present"),
    ("fire.early_floor", 2, "pass early factor clamped at early_floor 0.15"),
    ("fire.early_mid", 2, "pass early factor interpolated"),
    ("fire.early_clamp_1", 2, "pass early factor clamped at 1.0"),
    ("fire.fail_q_0", 2, "fail at q = 0.0, raw_delta -1.0"),
    ("fire.fail_q_015", 2, "fail at q = 0.15"),
    ("fire.decay_1", 2, "decay_for 1.0, not overdue"),
    ("fire.decay_mid", 2, "decay_for interpolated"),
    ("fire.decay_cap_3", 2, "decay_for at the 3.0 cap"),
    ("fire.repnum_floor_0", 2, "repNum clamped at 0 by a negative delta"),
    ("fire.membase_floor_0", 2, "memoryBase clamped at 0"),
    ("fire.interval_index_0", 2, "interval_for at or below index 0"),
    ("fire.interval_interp", 2, "interval_for interpolated"),
    ("fire.interval_last", 2, "interval_for at the last index"),
    ("fire.interval_cap_730", 2, "interval_for at the 730.0 cap"),
    ("fire.speed_clamp_lo", 2, "speed_for clamped at 0.33"),
    ("fire.speed_clamp_hi", 2, "speed_for clamped at 3.0"),
    ("fire.min_credit_drop", 2, "an implicit credit dropped below min_credit"),
    ("fire.forced_explicit_skip", 2, "a recipient below explicit_speed_threshold skipped"),
    ("fire.t0_none_penalty_skip", 2, "an upward penalty skipped on t0 is None"),
    ("fire.assisted_pass", 2, "assisted discount on a pass"),
    ("fire.assisted_miss", 2, "assisted flag on a miss, no discount"),
    ("xp.raw_8_924999999999999", 3, "a 3-KP passable lesson writes the raw float"),
    ("xp.rounded_8_92", 4, "the same value rounded to 2dp on the close path"),
    ("diag.initial", 5, "diagnostic_placed, initial placement"),
    ("diag.refresh", 5, "diagnostic_placed with refresh true"),
    ("diag.promote_guard", 5, "H2 guard: negative balance on an untouched topic"),
    (
        "diag.promote_guard_zero",
        5,
        "H2 REFRESH guard at its boundary: a balance of exactly 0.0",
    ),
    (
        "diag.placed_balance_zero",
        5,
        "the INITIAL placement filter at its boundary: a balance of exactly 0.0",
    ),
    ("diag.conditional_peel", 5, "a conditional placement peeled back by a miss"),
    ("reset.applied", 6, "profile_reset clears accumulated state"),
    ("round.day_half_tie", 7, "a local-day XP total on an exact .5 tie"),
    ("round.velocity_tie", 7, "xp_per_day_28d on a round(x, 4) half-even tie"),
    ("quiz.row_on_curriculum", 8, "a quiz per_topic row on the curriculum"),
    ("quiz.row_off_curriculum", 8, "a quiz per_topic row off the curriculum"),
    ("quiz.retake_false", 8, "a quiz score at or above retake_below"),
    ("quiz.retake_true", 8, "a quiz score below retake_below"),
    ("streak.day_at_goal", 9, "ANY local day exactly at the goal"),
    (
        "streak.reference_day_at_goal",
        9,
        "the REFERENCE day exactly at the goal, which the first comparison reads",
    ),
    ("streak.day_one_below", 9, "a local day one XP below the goal"),
    ("streak.gap_day", 9, "a gap day inside the streak block"),
    ("streak.tz_shifts_day", 9, "the block moves a day between UTC and the tz"),
    ("regrade.multiple", 10, "more than one regraded event"),
    ("regrade.superseding", 10, "a later correction on an already-corrected target"),
    ("regrade.no_preceding_attempt", 10, "a correction whose target has no attempt"),
]

#: Where a probe that no committed stream reaches IS pinned. The report prints one
#: of these lines per unreached probe, so an unreached and unpinned probe is loud.
PINNED_ELSEWHERE = {
    "fire.interval_cap_730": "`crates/core/tests/fire.rs`, a direct unit test",
    "fire.speed_clamp_lo": "`crates/core/tests/fire.rs`, a direct unit test",
    "fire.speed_clamp_hi": "`crates/core/tests/fire.rs`, a direct unit test",
    "diag.placed_balance_zero": (
        "`crates/core/tests/projector.rs`, on "
        "`fixtures/events/boundary/placed_balance_zero.jsonl`"
    ),
    "streak.reference_day_at_goal": (
        "`crates/core/tests/projector.rs`, on "
        "`fixtures/events/boundary/streak_reference_day_at_goal.jsonl`"
    ),
}

NOOP_TYPES = frozenset(
    {
        "task_served",
        "session_start",
        "session_end",
        "anki_card_created",
        "config_changed",
        "curriculum_changed",
    }
)


def scan_events(probe: Probe, rows: list[dict], graph) -> None:
    """Record the probes that read the raw event rows, not the fold."""
    types = {row["type"] for row in rows}
    if len(types) == 16:
        probe.hit("types.all_16")
    if NOOP_TYPES <= types:
        probe.hit("types.noop_6")
    for row in rows:
        scan_row(probe, row, graph)
    scan_corrections(probe, rows)


def scan_row(probe: Probe, row: dict, graph) -> None:
    """Record the XP and quiz probes of one raw event row."""
    if row["type"] == "lesson_result" and repr(float(row["xp"])) == "8.924999999999999":
        probe.hit("xp.raw_8_924999999999999")
    if row["type"] in {"lesson_result", "review_result"} and close(float(row["xp"]), 8.92):
        probe.hit("xp.rounded_8_92")
    if row["type"] == "quiz_result":
        probe.hit("quiz.retake_true" if row["score"] < 0.8 else "quiz.retake_false")
        for entry in row["per_topic"]:
            probe.hit(
                "quiz.row_on_curriculum"
                if entry["topic"] in graph.topics
                else "quiz.row_off_curriculum"
            )


def scan_corrections(probe: Probe, rows: list[dict]) -> None:
    """Record the `regraded` probes: count, supersession, and a missing attempt."""
    corrections = [row for row in rows if row["type"] == "regraded"]
    if len(corrections) > 1:
        probe.hit("regrade.multiple")
    seen: set[str] = set()
    for row in corrections:
        target = row["task_id"]
        if target in seen:
            probe.hit("regrade.superseding")
        seen.add(target)
        attempts = [
            other
            for other in rows
            if other["type"] == "attempt" and other.get("task_id") == target
        ]
        if not attempts:
            probe.hit("regrade.no_preceding_attempt")


def scan_days(probe: Probe, daily: dict, goal: int) -> None:
    """Record the streak and rounding probes of one zone's daily totals."""
    for day, total in daily.items():
        if close(total - math.floor(total), 0.5, 1e-9):
            probe.hit("round.day_half_tie")
        if close(total, float(goal)):
            probe.hit("streak.day_at_goal")
        if close(total, float(goal) - 1.0):
            probe.hit("streak.day_one_below")
        if day + timedelta(days=1) not in daily and day + timedelta(days=2) in daily:
            probe.hit("streak.gap_day")


def scan_xp(probe: Probe, state, goal: int) -> None:
    """Record the streak and rounding probes off the folded XP ledger."""
    from cadus.xp import daily_totals, local_day, xp_per_day

    for tz in (None, COVERAGE_TZ):
        daily = daily_totals(state.xp_events, tz)
        # `current_streak` reads the REFERENCE day first, and that comparison decides
        # whether today counts toward the streak. A past day at the goal only ever
        # reaches the `while` loop, so the two probes are separate.
        reference_day = local_day(state.last_ts, tz)
        if close(daily.get(reference_day, 0.0), float(goal)):
            probe.hit("streak.reference_day_at_goal")
        scan_days(probe, daily, goal)

    if set(daily_totals(state.xp_events, None)) != set(
        daily_totals(state.xp_events, COVERAGE_TZ)
    ):
        probe.hit("streak.tz_shifts_day")

    # A `round(x, 4)` half-even tie is a value whose scaled fraction is exactly 0.5.
    for tz in (None, COVERAGE_TZ):
        rate = xp_per_day(state.xp_events, state.last_ts, tz, window_days=28)
        scaled = rate * 1e4
        if close(scaled - math.floor(scaled), 0.5, 1e-9):
            probe.hit("round.velocity_tie")


def fold_stream(path: str, cfg, graph, goal: int) -> set[str]:
    """Fold one stream under the probes and return the probes it hit."""
    from cadus.events import validate_event
    from cadus.projector import Projector, apply_regrades

    rows = []
    events = []
    for row in iter_rows(path):
        rows.append(row)
        events.append(validate_event(row))

    probe = Probe()
    restore = install(probe, cfg)
    try:
        state = Projector(graph, cfg)
        for event in apply_regrades(events):
            state.apply(event)
        scan_events(probe, rows, graph)
        scan_xp(probe, state, goal)
    finally:
        restore()
    # Copy the set: `probe` is discarded, but a shared reference would let a
    # later stream's hits leak into this stream's row.
    return set(probe.hits)


def seed_column(hitters: list[str], seeded: list[str]) -> str:
    """The `s2..s20` cell: `all`, `none`, or the seed numbers that hit the probe."""
    if len(hitters) == len(seeded):
        return "all"
    if not hitters:
        return "none"
    return ", ".join(n.removeprefix("stream_").removesuffix(".jsonl") for n in hitters)


def report_header(names: list[str]) -> list[str]:
    """The lines above the coverage table."""
    return [
        "# Stream coverage - spec `projector-1.0-spec.md` section 9",
        "",
        "Generated by `scripts/oracle/coverage_streams_1_0.py` against the live 1.0",
        "code base. Every cell is a MEASURED branch hit. The script wraps the 1.0 FIRe",
        "and projector functions with counting probes and folds each committed stream.",
        "To regenerate this file, run:",
        "",
        "```sh",
        "cd /home/deploy/dev/cadus",
        "/home/deploy/dev/cadus/.venv/bin/python \\",
        "    <2.0-repo>/scripts/oracle/coverage_streams_1_0.py \\",
        "    --fixtures <2.0-repo>/crates/core/tests/fixtures/events",
        "```",
        "",
        f"Streams: {len(names)}. `stream_1.jsonl` is the frozen hand-designed stream of",
        "`docs/plans/M3.md`. `stream_2.jsonl` through `stream_20.jsonl` are the seeded",
        "coverage family: one 15-phase skeleton, with the chain, the tiers, the gaps,",
        "the balances, and the corrected tasks varied per seed.",
        "",
        "Column `s1` is `stream_1.jsonl`. Column `s2..s20` is the seeded family: it reads",
        "`all` when every one of the 19 seeds hits the probe, `none` when no seed hits",
        "it, and lists the seed numbers otherwise.",
        "",
        "| item | probe | what it records | s1 | s2..s20 |",
        "|---|---|---|---|---|",
    ]


def report_rows(names: list[str], per_stream: dict[str, set[str]]) -> list[str]:
    """One table row per probe."""
    seeded = names[1:]
    lines = []
    for probe_id, item, text in PROBES:
        first = "yes" if probe_id in per_stream["stream_1.jsonl"] else "no"
        hitters = [n for n in seeded if probe_id in per_stream[n]]
        column = seed_column(hitters, seeded)
        lines.append(f"| {item} | `{probe_id}` | {text} | {first} | {column} |")
    return lines


def report_missing(covered: set[str]) -> list[str]:
    """The section on the probes no committed stream reaches."""
    missing = [p for p, _, _ in PROBES if p not in covered]
    lines = ["", "## Branches no stream reaches", ""]
    if not missing:
        lines.append("Every probe of spec section 9 is reached by a committed stream.")
        return lines
    lines += [
        "No committed stream of the family above reaches these, so each one is",
        "pinned by a test of its own:",
        "",
    ]
    lines += [f"- `{p}` -- {PINNED_ELSEWHERE.get(p, 'UNPINNED: this row needs a test')}" for p in missing]
    lines += [
        "",
        "`interval_for` never reaches its 730.0 cap, because the default",
        "`interval_table` ends at 480.0 and interpolation never leaves the table.",
        "`speed_for` never reaches either clamp, because `(0.5 + a) / (0.5 + d)` with",
        "`a` in [0, 1] and `d` in the curriculum's [0.05, 0.75] spans [0.4, 3.0),",
        "open at the top, so neither 0.33 nor 3.0 binds.",
        "",
        "The two boundary rows are reachable from a stream, and the streams that",
        "reach them are `tests/fixtures/events/boundary/`: the seeded family emits",
        "no balance of exactly 0.0 on the initial placement path and no reference",
        "day exactly at the goal (M3 review round 1, findings #7 and #15).",
    ]
    return lines


def main() -> int:
    args = parse_stream_oracle_args(__doc__)

    from cadus.projector import config_hash

    cfg, graph = load_1_0()

    names = stream_names(args.fixtures)
    per_stream: dict[str, set[str]] = {}
    for name in names:
        per_stream[name] = fold_stream(os.path.join(args.fixtures, name), cfg, graph, args.goal)
        print(f"{name}: {len(per_stream[name])} probes hit")

    covered: set[str] = set()
    for hits in per_stream.values():
        covered |= hits

    lines = report_header(names) + report_rows(names, per_stream) + report_missing(covered)
    lines.append("")

    out = args.out or os.path.join(args.fixtures, "coverage.md")
    with open(out, "w", encoding="utf-8") as handle:
        handle.write("\n".join(lines))
    print(f"wrote {out}")
    print(f"config_hash={config_hash(cfg)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
