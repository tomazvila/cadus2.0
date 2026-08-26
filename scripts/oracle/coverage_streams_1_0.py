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

import argparse
import json
import math
import os
import re
from datetime import UTC, datetime, timedelta

#: The non-UTC zone the streak block is designed around (spec section 9 item 9).
COVERAGE_TZ = "America/New_York"

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


class Probe:
    """The measured hit set of one stream."""

    def __init__(self) -> None:
        self.hits: set[str] = set()

    def hit(self, name: str) -> None:
        self.hits.add(name)


def close(a: float, b: float, tol: float = 1e-12) -> bool:
    """Whether two floats agree to within ``tol`` in absolute value."""
    return abs(a - b) <= tol


def install(probe: Probe, cfg):
    """Wrap the 1.0 FIRe and projector entry points with the PROBES branch probes.

    Returns a no-argument ``restore`` function. The caller MUST call it after each
    stream. Without the restore, a second ``install`` wraps the first wrapper, and
    then every later fold also feeds the earlier stream's probe -- which reports
    one stream's branches as another stream's coverage.
    """
    from cadus import fire as fire_mod
    from cadus import projector as proj_mod
    from cadus.model import TopicState, TopicStatus

    raw_raw_delta = fire_mod.raw_delta
    raw_decay_for = fire_mod.decay_for
    raw_interval_for = fire_mod.interval_for
    raw_speed_for = fire_mod.speed_for
    raw_apply_update = fire_mod._apply_update
    raw_apply_attempt = fire_mod.apply_attempt
    raw_memory_at = fire_mod.memory_at

    def probed_raw_delta(q, memory_now, passed, config, *, assisted=False):
        if passed:
            span = 1.0 - config.fire.due_threshold
            if span <= 0.0:
                early = 1.0
            else:
                early = max(config.fire.early_floor, min((1.0 - memory_now) / span, 1.0))
            if close(early, config.fire.early_floor):
                probe.hit("fire.early_floor")
            elif close(early, 1.0):
                probe.hit("fire.early_clamp_1")
            else:
                probe.hit("fire.early_mid")
        elif close(q, 0.0):
            probe.hit("fire.fail_q_0")
        elif close(q, 0.15):
            probe.hit("fire.fail_q_015")
        return raw_raw_delta(q, memory_now, passed, config, assisted=assisted)

    def probed_decay_for(state, t, config):
        value = raw_decay_for(state, t, config)
        if close(value, 1.0):
            probe.hit("fire.decay_1")
        elif close(value, config.fire.decay_cap):
            probe.hit("fire.decay_cap_3")
        else:
            probe.hit("fire.decay_mid")
        return value

    def probed_interval_for(rep_num, config):
        table = config.fire.interval_table
        r = max(0.0, rep_num)
        last = len(table) - 1
        index = math.floor(r)
        if r <= 0.0:
            probe.hit("fire.interval_index_0")
        elif index >= last:
            probe.hit("fire.interval_last")
        else:
            probe.hit("fire.interval_interp")
        value = raw_interval_for(rep_num, config)
        if close(value, fire_mod.INTERVAL_CAP_DAYS):
            probe.hit("fire.interval_cap_730")
        return value

    def probed_speed_for(ability, difficulty, config):
        lo, hi = config.fire.speed_clamp
        rate = (0.5 + ability) / (0.5 + difficulty)
        if rate < lo:
            probe.hit("fire.speed_clamp_lo")
        elif rate > hi:
            probe.hit("fire.speed_clamp_hi")
        return raw_speed_for(ability, difficulty, config)

    def probed_apply_update(state, raw, t, *, failed, cfg):
        factor = raw_decay_for(state, t, cfg) if failed else 1.0
        if state.repNum + state.speed * factor * raw < 0.0:
            probe.hit("fire.repnum_floor_0")
        if raw_memory_at(state, t) + raw < 0.0:
            probe.hit("fire.membase_floor_0")
        return raw_apply_update(state, raw, t, failed=failed, cfg=cfg)

    def probed_apply_attempt(states, attempt_result, graph, config, t):
        passed = attempt_result.passed
        if attempt_result.assisted:
            probe.hit("fire.assisted_pass" if passed else "fire.assisted_miss")
        topic = attempt_result.topic
        grade = fire_mod.quality_q(attempt_result.quality)
        explicit = states.get(topic, TopicState())
        raw = raw_raw_delta(
            grade,
            raw_memory_at(explicit, t),
            passed,
            config,
            assisted=attempt_result.assisted,
        )
        # Re-walk the gates, because a DROPPED neighbor is absent from the report
        # the real function returns and must be measured here.
        if passed and raw > 0.0:
            for target, weight in sorted(graph.reach_weights(topic).items()):
                if target == topic or weight <= 0.0:
                    continue
                recipient = states.get(target, TopicState())
                if recipient.speed < config.fire.explicit_speed_threshold:
                    probe.hit("fire.forced_explicit_skip")
                    continue
                credit = (
                    raw_raw_delta(
                        grade,
                        raw_memory_at(recipient, t),
                        True,
                        config,
                        assisted=attempt_result.assisted,
                    )
                    * weight
                )
                if abs(credit) < config.fire.min_credit:
                    probe.hit("fire.min_credit_drop")
        elif not passed and raw < 0.0:
            for target, weight in sorted(graph.upward_weights(topic).items()):
                if target == topic or weight <= 0.0:
                    continue
                recipient = states.get(target, TopicState())
                if recipient.t0 is None:
                    probe.hit("fire.t0_none_penalty_skip")
                    continue
                if abs(raw * weight) < config.fire.min_credit:
                    probe.hit("fire.min_credit_drop")
        return raw_apply_attempt(states, attempt_result, graph, config, t)

    fire_mod.raw_delta = probed_raw_delta
    fire_mod.decay_for = probed_decay_for
    fire_mod.interval_for = probed_interval_for
    fire_mod.speed_for = probed_speed_for
    fire_mod._apply_update = probed_apply_update
    fire_mod.apply_attempt = probed_apply_attempt
    proj_mod.interval_for = probed_interval_for
    proj_mod.speed_for = probed_speed_for
    proj_mod.apply_attempt = probed_apply_attempt

    state_cls = proj_mod.Projector
    raw_placed = state_cls._on_diagnostic_placed
    raw_refresh = state_cls._refresh_placement
    raw_peel = state_cls._peel_back_conditional
    raw_reset = state_cls._on_profile_reset

    def probed_placed(self, event, apply_fire):
        probe.hit("diag.refresh" if event.refresh else "diag.initial")
        if not event.refresh:
            # The INITIAL placement filter is `balance > 0.0` (projector.py:340-344),
            # a different guard from the refresh promote guard below. A `>=` port
            # folds identically unless some row sits exactly on 0.0.
            for tid, balance in event.balances.items():
                if tid in self.graph.topics and balance == 0.0:
                    probe.hit("diag.placed_balance_zero")
        return raw_placed(self, event, apply_fire)

    def probed_refresh(self, event, diag_answers):
        for tid, balance in event.balances.items():
            if tid not in self.graph.topics:
                continue
            old = self.topics.get(tid, TopicState())
            if balance <= 0.0 and old.status is TopicStatus.untouched:
                # The boundary is recorded apart from the strictly-negative case: a
                # `balance < 0.0` guard folds identically to 1.0's `balance <= 0.0`
                # guard unless some row sits exactly on 0.0.
                probe.hit("diag.promote_guard_zero" if balance == 0.0 else "diag.promote_guard")
        return raw_refresh(self, event, diag_answers)

    def probed_peel(self, topic, t):
        candidates = {topic} | self.graph.dependents.get(topic, set())
        for cid in candidates:
            state = self.topics.get(cid)
            if state is not None and state.conditional:
                probe.hit("diag.conditional_peel")
        return raw_peel(self, topic, t)

    def probed_reset(self, event, apply_fire):
        if apply_fire:
            for tid in event.topics:
                if tid not in self.graph.topics:
                    continue
                if self.topics.get(tid, TopicState()) != TopicState():
                    probe.hit("reset.applied")
        return raw_reset(self, event, apply_fire)

    state_cls._on_diagnostic_placed = probed_placed
    state_cls._refresh_placement = probed_refresh
    state_cls._peel_back_conditional = probed_peel
    state_cls._on_profile_reset = probed_reset

    def restore() -> None:
        """Put the unwrapped 1.0 functions back."""
        fire_mod.raw_delta = raw_raw_delta
        fire_mod.decay_for = raw_decay_for
        fire_mod.interval_for = raw_interval_for
        fire_mod.speed_for = raw_speed_for
        fire_mod._apply_update = raw_apply_update
        fire_mod.apply_attempt = raw_apply_attempt
        proj_mod.interval_for = raw_interval_for
        proj_mod.speed_for = raw_speed_for
        proj_mod.apply_attempt = raw_apply_attempt
        state_cls._on_diagnostic_placed = raw_placed
        state_cls._refresh_placement = raw_refresh
        state_cls._peel_back_conditional = raw_peel
        state_cls._on_profile_reset = raw_reset

    return restore


def scan_events(probe: Probe, rows: list[dict], graph) -> None:
    """Record the probes that read the raw event rows, not the fold."""
    types = {row["type"] for row in rows}
    if len(types) == 16:
        probe.hit("types.all_16")
    if NOOP_TYPES <= types:
        probe.hit("types.noop_6")

    for row in rows:
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
        for day, total in daily.items():
            if close(total - math.floor(total), 0.5, 1e-9):
                probe.hit("round.day_half_tie")
            if close(total, float(goal)):
                probe.hit("streak.day_at_goal")
            if close(total, float(goal) - 1.0):
                probe.hit("streak.day_one_below")
            if day + timedelta(days=1) not in daily and day + timedelta(days=2) in daily:
                probe.hit("streak.gap_day")

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
    from cadus.projector import Projector, apply_regrades, config_hash

    cfg = load_config()
    graph = load_graph()

    names = sorted(
        (n for n in os.listdir(args.fixtures) if re.fullmatch(r"stream_\d+\.jsonl", n)),
        key=lambda n: int(n.removeprefix("stream_").removesuffix(".jsonl")),
    )

    per_stream: dict[str, set[str]] = {}
    for name in names:
        rows = []
        events = []
        with open(os.path.join(args.fixtures, name), encoding="utf-8") as handle:
            for line in handle:
                line = line.strip()
                if line:
                    row = json.loads(line)
                    rows.append(row)
                    events.append(validate_event(row))

        probe = Probe()
        restore = install(probe, cfg)
        try:
            state = Projector(graph, cfg)
            for event in apply_regrades(events):
                state.apply(event)
            scan_events(probe, rows, graph)
            scan_xp(probe, state, args.goal)
        finally:
            restore()
        # Copy the set: `probe` is discarded, but a shared reference would let a
        # later stream's hits leak into this stream's row.
        per_stream[name] = set(probe.hits)
        print(f"{name}: {len(probe.hits)} probes hit")

    covered: set[str] = set()
    for hits in per_stream.values():
        covered |= hits

    seeded = names[1:]
    lines = [
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
    for probe_id, item, text in PROBES:
        first = "yes" if probe_id in per_stream["stream_1.jsonl"] else "no"
        hitters = [n for n in seeded if probe_id in per_stream[n]]
        if len(hitters) == len(seeded):
            column = "all"
        elif not hitters:
            column = "none"
        else:
            column = ", ".join(
                n.removeprefix("stream_").removesuffix(".jsonl") for n in hitters
            )
        lines.append(f"| {item} | `{probe_id}` | {text} | {first} | {column} |")

    missing = [p for p, _, _ in PROBES if p not in covered]
    lines += ["", "## Branches no stream reaches", ""]
    if missing:
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
    else:
        lines.append("Every probe of spec section 9 is reached by a committed stream.")
    lines.append("")

    out = args.out or os.path.join(args.fixtures, "coverage.md")
    with open(out, "w", encoding="utf-8") as handle:
        handle.write("\n".join(lines))
    print(f"wrote {out}")
    print(f"config_hash={config_hash(cfg)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
