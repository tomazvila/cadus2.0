"""The constants both stream families of `gen_stream_1_0.py` share.

Change nothing here without re-taking the oracle: every committed stream is a
pure function of its seed and of these values.
"""

from __future__ import annotations

from datetime import UTC, datetime

SEED = 20260302  # recorded on task_served; nothing in the fold reads it

COURSE = "foundations"


def problem_text(topic: str, n: int) -> str:
    """A stable, human-readable problem statement (its sha1[:12] feeds the
    ``last_problems`` dedup window, so it must be byte-stable)."""
    return f"[{topic}] synthetic problem #{n}: evaluate the expression."


#: The time zone the streak block of a seeded stream is designed for. The fold
#: takes the zone as a parameter, so the same stream is also folded in UTC; only
#: `local_day` moves (trap T9), which is exactly what the block exercises.
COVERAGE_TZ = "America/New_York"

#: The daily XP goal the streak block is measured against (`config.yaml`).
COVERAGE_GOAL = 40

#: The length of the prerequisite chain a seeded stream works over.
CHAIN_LENGTH = 5

#: The origin of the seeded timeline. Every phase is an offset in days from it.
COVERAGE_ORIGIN = datetime(2026, 1, 5, tzinfo=UTC)

#: An id that is deliberately NOT a topic. Every list that carries topic ids
#: carries it, so the port's "skip an unknown topic" guards are all exercised.
OFF_CURRICULUM = "not-a-real-topic"

#: The streak block, in COVERAGE_TZ. Local days in America/New_York, against
#: the goal of 40:
#:   Apr 10  16.375  below the goal
#:   Apr 11  --      a gap day
#:   Apr 12  --      a gap day
#:   Apr 13  39.0    ONE XP below the goal: the day that stops the streak
#:   Apr 14  40.0    EXACTLY at the goal
#:   Apr 15  40.0    EXACTLY at the goal
#:   Apr 16  40.5    today, and a half-even rounding tie for `xp.today`
#: so `streak_days` is 3, and a `> goal` test instead of `>= goal` gives 1.
#: The last entry is stamped 02:30Z on Apr 17, which is 22:30 on Apr 16 in New
#: York, so the whole block moves by a day between the two zones (trap T9).
#: The window total is 175.875 exactly, so `xp_per_day_28d` is 6.28125 and
#: `round(x, 4)` is a half-even tie: 6.2812, where a scale-round-divide gives
#: 6.2813 (trap T4).
STREAK_BLOCK = [
    (datetime(2027, 4, 10, 15, 0, tzinfo=UTC), 16.375),
    (datetime(2027, 4, 13, 15, 0, tzinfo=UTC), 19.5),
    (datetime(2027, 4, 13, 17, 0, tzinfo=UTC), 19.5),
    (datetime(2027, 4, 14, 15, 0, tzinfo=UTC), 20.0),
    (datetime(2027, 4, 14, 17, 0, tzinfo=UTC), 20.0),
    (datetime(2027, 4, 15, 13, 0, tzinfo=UTC), 12.5),
    (datetime(2027, 4, 15, 15, 0, tzinfo=UTC), 15.0),
    (datetime(2027, 4, 15, 17, 0, tzinfo=UTC), 12.5),
    (datetime(2027, 4, 16, 15, 0, tzinfo=UTC), 20.25),
    (datetime(2027, 4, 17, 2, 30, tzinfo=UTC), 20.25),
]
