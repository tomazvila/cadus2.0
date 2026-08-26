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
import math
import os
import random
from datetime import UTC, datetime, timedelta

# --------------------------------------------------------------------------- #
# Deterministic constants -- change nothing here without re-taking the oracle.
# --------------------------------------------------------------------------- #

SEED = 20260302  # recorded on task_served; nothing in the fold reads it

#: A real prerequisite chain from the `foundations` course, so the FIRe
#: encompassing propagation (downward credit / upward penalty) actually fires.
#:   absolute-value
#:     <- adding-integers                 (w 0.5, key)
#:     <- basic-absolute-value-equations  (w 0.7, key)
#:          <- absolute-value-equations        (w 0.7, key)
#:               <- absolute-value-inequalities (w 0.5, key)
TOPICS = [
    "absolute-value",
    "adding-integers",
    "basic-absolute-value-equations",
    "absolute-value-equations",
    "absolute-value-inequalities",
]

COURSE = "foundations"

T_START = datetime(2026, 3, 2, 9, 0, 0, tzinfo=UTC)
ATTEMPT_GAP = timedelta(seconds=137)  # deliberately not a round number
TASK_GAP = timedelta(hours=3, minutes=17)
REVIEW_DAY_GAP = timedelta(days=8)  # pushes the review block OVERDUE

#: (work_quality, correct) per attempt slot. Slots 3 and 12 deliberately
#: DISAGREE (correct=True at a failing tier; correct=False at a passing tier)
#: so a port that quietly derives one from the other diverges.
LESSON_ATTEMPTS = [
    ("perfect", True),
    ("nearly_perfect", True),
    ("passable", True),
    ("poor", True),  # slot 3: correct BUT poor tier
    ("nearly_perfect", True),
    ("nearly_passable", False),
    ("passable", True),
    ("perfect", True),
    ("poor", False),
    ("blowoff", False),
]
REVIEW_ATTEMPTS = [
    ("nearly_perfect", True),
    ("perfect", True),
    ("passable", False),  # slot 12: incorrect BUT passable tier
    ("nearly_passable", False),
    ("perfect", True),
    ("nearly_perfect", True),
    ("poor", False),
    ("passable", True),
    ("nearly_passable", False),
    ("nearly_perfect", True),
]

#: Which lesson fails (index into TOPICS). The rest pass.
FAILING_LESSON = 4
#: Which review is reference-assisted (index into TOPICS).
ASSISTED_REVIEW = 1
#: Which review the `regraded` event supersedes (index into TOPICS).
REGRADED_REVIEW = 2


def problem_text(topic: str, n: int) -> str:
    """A stable, human-readable problem statement (its sha1[:12] feeds the
    ``last_problems`` dedup window, so it must be byte-stable)."""
    return f"[{topic}] synthetic problem #{n}: evaluate the expression."


# --------------------------------------------------------------------------- #
# Stream #1 -- the frozen hand-designed stream (seed 1)
# --------------------------------------------------------------------------- #


def build_stream_1(cfg, graph) -> list[object]:
    """The original stream #1. Its bytes are frozen: `docs/plans/M3.md` pins the
    digest of its fold."""
    from cadus.fire import grade_review
    from cadus.model import (
        Attempt,
        AttemptProblem,
        Enrolled,
        LessonResult,
        RegradedAttempt,
        Regraded,
        RemediationTriggered,
        ReviewResult,
        ServedProblem,
        SessionEnd,
        SessionStart,
        TaskServed,
        TaskType,
        WorkQuality,
    )
    from cadus.projector import problem_text_hash
    from cadus.xp import task_xp

    for tid in TOPICS:
        assert tid in graph.topics, f"topic {tid!r} left the curriculum"

    events: list[object] = []
    clock = T_START

    def at() -> datetime:
        return clock

    # ---- enrollment (out of session: `session` stays None) ---------------- #
    events.append(Enrolled(ts=clock, session=None, course=COURSE))
    clock += ATTEMPT_GAP

    # ================= session 1: the lesson block ========================= #
    s1 = "s_2026-03-02a"
    events.append(SessionStart(ts=clock, session=s1))
    clock += ATTEMPT_GAP

    slot = 0
    for i, tid in enumerate(TOPICS):
        topic = graph.topics[tid]
        kp_ids = [kp.id for kp in topic.knowledge_points]
        task_id = f"{s1}-lesson-{tid}"

        events.append(
            TaskServed(
                ts=clock,
                session=s1,
                task_id=task_id,
                task_type=TaskType.lesson,
                topic=tid,
                kp=kp_ids[0] if kp_ids else None,
                problems=[
                    ServedProblem(
                        id=f"{task_id}-p{n}",
                        text_hash=problem_text_hash(problem_text(tid, n)),
                        expected_time_secs=topic.expected_time_secs,
                    )
                    for n in (0, 1)
                ],
                seed=SEED + i,
            )
        )
        clock += ATTEMPT_GAP

        last_tier = None
        for n in (0, 1):
            tier_name, correct = LESSON_ATTEMPTS[slot]
            last_tier = WorkQuality(tier_name)
            events.append(
                Attempt(
                    ts=clock,
                    session=s1,
                    attempt_id=f"{task_id}-a{n}",
                    task_id=task_id,
                    topic=tid,
                    kp=kp_ids[min(n, len(kp_ids) - 1)] if kp_ids else None,
                    task_type=TaskType.lesson,
                    problem=AttemptProblem(
                        text=problem_text(tid, n), expected=f"expected-{tid}-{n}"
                    ),
                    given_answer=f"given-{tid}-{n}",
                    correct=correct,
                    secs=20 + 7 * slot,
                    error_tags=[] if correct else ["arithmetic-slip"],
                    work_quality=last_tier,
                    grader_note=None,
                    assisted=False,
                )
            )
            clock += ATTEMPT_GAP
            slot += 1

        passed = i != FAILING_LESSON
        # `service._advance_lesson` prices the lesson from the LAST attempt's
        # tier and writes the RAW (unrounded) float onto the event.
        xp = task_xp(TaskType.lesson, last_tier, cfg, kp_count=len(kp_ids))
        events.append(
            LessonResult(
                ts=clock,
                session=s1,
                topic=tid,
                passed=passed,
                failed_at_kp=None if passed else (kp_ids[-1] if kp_ids else None),
                xp=xp,
                quality_tier=last_tier,
                assisted=False,
            )
        )
        clock += ATTEMPT_GAP

        if not passed:
            events.append(
                RemediationTriggered(
                    ts=clock,
                    session=s1,
                    kind="lesson_fail",
                    source_topic=tid,
                    targets=[],
                )
            )
            clock += ATTEMPT_GAP

        clock += TASK_GAP

    events.append(SessionEnd(ts=clock, session=s1, xp_earned=0.0, minutes=48.5))

    # ================= session 2: the review block, 8 days later =========== #
    clock = T_START + REVIEW_DAY_GAP
    s2 = "s_2026-03-10a"
    events.append(SessionStart(ts=clock, session=s2))
    clock += ATTEMPT_GAP

    review_task_ids: dict[str, str] = {}
    slot = 0
    for i, tid in enumerate(TOPICS):
        topic = graph.topics[tid]
        task_id = f"{s2}-review-{tid}"
        review_task_ids[tid] = task_id
        assisted = i == ASSISTED_REVIEW

        events.append(
            TaskServed(
                ts=clock,
                session=s2,
                task_id=task_id,
                task_type=TaskType.review,
                topic=tid,
                kp=None,
                problems=[],
                seed=SEED + 100 + i,
            )
        )
        clock += ATTEMPT_GAP

        seq: list[bool] = []
        last_tier = None
        for n in (0, 1):
            tier_name, correct = REVIEW_ATTEMPTS[slot]
            last_tier = WorkQuality(tier_name)
            seq.append(correct)
            events.append(
                Attempt(
                    ts=clock,
                    session=s2,
                    attempt_id=f"{task_id}-a{n}",
                    task_id=task_id,
                    topic=tid,
                    kp=None,
                    task_type=TaskType.review,
                    problem=AttemptProblem(
                        text=problem_text(tid, 10 + n),
                        expected=f"expected-{tid}-{10 + n}",
                    ),
                    given_answer=f"given-{tid}-{10 + n}",
                    correct=correct,
                    secs=30 + 11 * slot,
                    error_tags=[] if correct else ["sign-error"],
                    work_quality=last_tier,
                    grader_note=None,
                    assisted=assisted and n == 1,
                )
            )
            clock += ATTEMPT_GAP
            slot += 1

        # `service.review_result`: order-sensitive grade, tier from the LAST
        # attempt, xp ROUNDED to 2dp on the event.
        passed, score = grade_review(seq, cfg)
        xp = round(task_xp(TaskType.review, last_tier, cfg), 2)
        events.append(
            ReviewResult(
                ts=clock,
                session=s2,
                topic=tid,
                passed=passed,
                weighted_score=score,
                xp=xp,
                quality_tier=last_tier,
                assisted=assisted,
                task_id=None,
            )
        )
        clock += ATTEMPT_GAP
        clock += TASK_GAP

    # ---- the correction ---------------------------------------------------- #
    # Supersedes ONE attempt's tier and the review_result that closes that task.
    # The review_result carries task_id=None, so `apply_regrades` matches it via
    # the most recent preceding attempt on the same topic -- pinning that rule.
    regraded_topic = TOPICS[REGRADED_REVIEW]
    regraded_task = review_task_ids[regraded_topic]
    events.append(
        Regraded(
            ts=clock,
            session=s2,
            task_id=regraded_task,
            topic=regraded_topic,
            attempts=[
                RegradedAttempt(
                    attempt_id=f"{regraded_task}-a0",
                    work_quality=WorkQuality.passable,
                    error_tags=["notation"],
                    grader_note="regraded: notation only, method sound",
                )
            ],
            quality_tier=WorkQuality.nearly_perfect,
            xp=5.0,
            reason="grader mis-tiered a correct method as a miss",
        )
    )
    clock += ATTEMPT_GAP

    events.append(SessionEnd(ts=clock, session=s2, xp_earned=0.0, minutes=36.25))

    return events


# --------------------------------------------------------------------------- #
# Stream #2 onward -- the seeded coverage stream (spec section 9)
# --------------------------------------------------------------------------- #

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


def prerequisite_chains(graph, course: str) -> list[list[str]]:
    """Every ``CHAIN_LENGTH``-topic prerequisite chain of ``course``.

    A chain starts at a topic and walks down its lowest-id prerequisite inside
    the course at each step. Only chains whose every topic has three knowledge
    points qualify, because the seeded stream needs a 3-KP lesson pass to write
    the ``8.924999999999999`` XP artifact (trap T7). The result is sorted, so the
    pool is a pure function of the curriculum."""
    course_topics = set(graph.topics_in_course(course))
    chains: list[list[str]] = []
    for tid in sorted(course_topics):
        chain = [tid]
        while len(chain) < CHAIN_LENGTH:
            below = sorted(
                edge.id
                for edge in graph.topics[chain[-1]].prerequisites
                if edge.id in course_topics and edge.id not in chain
            )
            if not below:
                break
            chain.append(below[0])
        if len(chain) == CHAIN_LENGTH and all(
            len(graph.topics[step].knowledge_points) == 3 for step in chain
        ):
            chains.append(chain)
    assert chains, "the curriculum holds no 5-topic all-3-KP chain"
    return chains


def _even_half(total: float) -> float:
    """The nearest ``K + 0.5`` at or below ``total`` with ``K`` EVEN.

    ``xp.total`` is ``int(round(sum(...)))`` and Python's ``round`` is half-even
    (trap T3), so an even ``K`` makes ``round`` return ``K`` where a
    half-away-from-zero rounding returns ``K + 1``. The seeded stream lands its
    grand XP total exactly here."""
    whole = math.floor(total)
    if whole % 2 != 0:
        whole -= 1
    return whole + 0.5


def build_seeded_stream(seed: int, cfg, graph) -> list[object]:
    """The spec-section-9 coverage stream for ``seed`` (``seed >= 2``).

    The phases below are the coverage skeleton; the seed varies the chain, the
    spare topics, the tiers, the balance order, and the corrected tasks."""
    from cadus.model import (
        AnkiCardCreated,
        Attempt,
        AttemptProblem,
        ConfigChanged,
        CurriculumChanged,
        DiagnosticAnswer,
        DiagnosticPlaced,
        Enrolled,
        LessonResult,
        ProfileReset,
        QuizResult,
        QuizTopicResult,
        RegradedAttempt,
        Regraded,
        RemediationTriggered,
        ReviewResult,
        ServedProblem,
        SessionEnd,
        SessionStart,
        TaskServed,
        TaskType,
        WorkQuality,
    )
    from cadus.projector import problem_text_hash
    from cadus.xp import task_xp

    rng = random.Random(1_000_003 * seed + 7)
    events: list[object] = []

    def at(day: int, hour: int, minute: int = 0) -> datetime:
        return COVERAGE_ORIGIN + timedelta(days=day, hours=hour, minutes=minute)

    def kps(tid: str) -> list[str]:
        return [kp.id for kp in graph.topics[tid].knowledge_points]

    def served(ts, session, task_id, task_type, topic, kp=None, count=2) -> None:
        events.append(
            TaskServed(
                ts=ts,
                session=session,
                task_id=task_id,
                task_type=task_type,
                topic=topic,
                kp=kp,
                problems=[
                    ServedProblem(
                        id=f"{task_id}-p{n}",
                        text_hash=problem_text_hash(problem_text(topic, n)),
                        expected_time_secs=graph.topics[topic].expected_time_secs,
                    )
                    for n in range(count)
                ],
                seed=SEED + seed * 100 + len(events),
            )
        )

    def attempt(
        ts, session, task_id, topic, index, task_type, correct, tier, kp=None, assisted=False
    ) -> None:
        events.append(
            Attempt(
                ts=ts,
                session=session,
                attempt_id=f"{task_id}-a{index}",
                task_id=task_id,
                topic=topic,
                kp=kp,
                task_type=task_type,
                problem=AttemptProblem(
                    text=problem_text(topic, index), expected=f"expected-{topic}-{index}"
                ),
                given_answer=f"given-{topic}-{index}",
                correct=correct,
                secs=rng.randrange(12, 240),
                error_tags=[] if correct else [rng.choice(["sign-error", "notation"])],
                work_quality=WorkQuality(tier),
                grader_note=None,
                assisted=assisted,
            )
        )

    def review(ts, session, topic, passed, tier, xp, assisted=False, task_id=None) -> None:
        events.append(
            ReviewResult(
                ts=ts,
                session=session,
                topic=topic,
                passed=passed,
                weighted_score=rng.choice([0.0, 7 / 15, 0.65, 11 / 15, 1.0]),
                xp=xp,
                quality_tier=WorkQuality(tier),
                assisted=assisted,
                task_id=task_id,
            )
        )

    course_topics = sorted(graph.topics_in_course(COURSE))
    top, step_a, step_b, step_c, deep = rng.choice(prerequisite_chains(graph, COURSE))
    chain = [top, step_a, step_b, step_c, deep]
    spares = rng.sample([tid for tid in course_topics if tid not in set(chain)], 8)
    other_courses = sorted(
        course.id for course in graph.catalog.courses if course.id != COURSE
    )

    # ---- phase A: enrollment, the diagnostic, the initial placement -------- #
    s0 = "s_cov_0"
    if seed % 2 == 0:
        events.append(Enrolled(ts=at(0, 9), session=None, course=COURSE))
    else:
        events.append(
            Enrolled(
                ts=at(0, 9),
                session=None,
                course=COURSE,
                reason="gap-fill",
                return_to=rng.choice(other_courses),
            )
        )
    events.append(SessionStart(ts=at(0, 9, 5), session=s0))
    for index, (tid, correct, weight) in enumerate(
        [(deep, True, 1.0), (deep, False, 0.6), (step_b, True, 0.35)]
    ):
        events.append(
            DiagnosticAnswer(
                ts=at(0, 9, 10 + index),
                session=s0,
                topic=tid,
                correct=correct,
                secs=30 + 5 * index,
                weight=weight,
            )
        )
    # `step_c` misses eight probes, so its seeded ability is 0.5 * 0.7**8 = 0.029 --
    # below every difficulty in the tree. Its `speed` is then below 1.0, which is the
    # forced-explicit gate that makes `fire.apply_attempt` skip it for credit.
    for index in range(8):
        events.append(
            DiagnosticAnswer(
                ts=at(0, 9, 20 + index),
                session=s0,
                topic=step_c,
                correct=False,
                secs=95 + index,
                weight=1.0,
            )
        )
    balances = [
        (deep, 5.5),  # above PLACEMENT_REPNUM_CAP, so `repNum` caps at 4.0
        (step_c, 2.5),  # answered, and answered badly
        (step_b, 1.5),  # answered once
        (top, 0.75),  # conditional, so a later miss on `step_a` peels it
        (step_a, 1.25),  # never answered: the inferred pass seeds it
        (OFF_CURRICULUM, 3.0),  # not a topic: skipped
        (spares[0], -1.0),  # non-positive: the initial pass drops it
    ]
    rng.shuffle(balances)  # dict order is load-bearing (trap T6)
    events.append(
        DiagnosticPlaced(
            ts=at(0, 10), session=s0, balances=dict(balances), conditional=[top], refresh=False
        )
    )
    events.append(SessionEnd(ts=at(0, 10, 30), session=s0, xp_earned=0.0, minutes=90.0))

    # ---- phase B: the two lesson-close XP paths (trap T7) ------------------ #
    s1 = "s_cov_1"
    raw_xp = task_xp(TaskType.lesson, WorkQuality.passable, cfg, kp_count=3)
    assert repr(raw_xp) == "8.924999999999999", repr(raw_xp)
    events.append(SessionStart(ts=at(2, 9), session=s1))
    for index, (tid, xp) in enumerate([(top, raw_xp), (step_a, round(raw_xp, 2))]):
        task = f"{s1}-lesson-{tid}"
        served(at(2, 9, 1 + 20 * index), s1, task, TaskType.lesson, tid, kp=kps(tid)[0])
        attempt(
            at(2, 9, 3 + 20 * index),
            s1,
            task,
            tid,
            0,
            TaskType.lesson,
            True,
            "perfect",
            kp=kps(tid)[0],
        )
        attempt(
            at(2, 9, 6 + 20 * index),
            s1,
            task,
            tid,
            1,
            TaskType.lesson,
            True,
            "passable",
            kp=kps(tid)[1],
        )
        events.append(
            LessonResult(
                ts=at(2, 9, 8 + 20 * index),
                session=s1,
                topic=tid,
                passed=True,
                failed_at_kp=None,
                xp=xp,
                quality_tier=WorkQuality.passable,
                assisted=False,
            )
        )
    events.append(SessionEnd(ts=at(2, 10), session=s1, xp_earned=0.0, minutes=60.0))

    # ---- phase C: a lesson that fails twice, and a repeated trigger -------- #
    s2 = "s_cov_2"
    events.append(SessionStart(ts=at(3, 9), session=s2))
    fail_topic = spares[1]
    for round_index in range(2):
        task = f"{s2}-lesson-{fail_topic}-{round_index}"
        served(at(3, 9, 1 + 10 * round_index), s2, task, TaskType.lesson, fail_topic)
        attempt(
            at(3, 9, 3 + 10 * round_index),
            s2,
            task,
            fail_topic,
            0,
            TaskType.lesson,
            False,
            "poor",
            kp=kps(fail_topic)[1],
        )
        events.append(
            LessonResult(
                ts=at(3, 9, 5 + 10 * round_index),
                session=s2,
                topic=fail_topic,
                passed=False,
                failed_at_kp=kps(fail_topic)[1],
                xp=0.0,
                quality_tier=WorkQuality.poor,
                assisted=False,
            )
        )
        # The SAME trigger twice: `pending_remediation` dedups on (kind, targets).
        events.append(
            RemediationTriggered(
                ts=at(3, 9, 6 + 10 * round_index),
                session=s2,
                kind="lesson_fail",
                source_topic=fail_topic,
                targets=[spares[2], OFF_CURRICULUM],
            )
        )
    events.append(SessionEnd(ts=at(3, 10), session=s2, xp_earned=0.0, minutes=45.0))

    # ---- phase D: first touch, the blow-off floor, the t0-is-None skip ----- #
    # `spares[3]` is untouched with ability 0.0, so the result seeds it first
    # (`_apply_fire_result` step 2). A `blowoff` miss is `raw_delta == -1.0`, which
    # floors `repNum` at 0.0 and `memoryBase` at 0.0. Its `t0` is None, so
    # `decay_for` is 1.0, and every upward target is unlearned, so the penalty is
    # skipped everywhere.
    review(
        at(4, 9),
        None,
        spares[3],
        False,
        "blowoff",
        task_xp(TaskType.review, WorkQuality.blowoff, cfg),
    )

    # ---- phase E: the three `early` bands of `raw_delta` -------------------- #
    # `step_b` was placed at day 0 with memoryBase 1.0, so by day 5 its memory sits
    # between 0.5 and 0.925: `early` is interpolated. The second pass twenty minutes
    # later reads a memory above 1.0, so `early` clamps at the 0.15 floor.
    review(at(5, 9), None, step_b, True, "perfect", 6.5)
    review(at(5, 9, 20), None, step_b, True, "perfect", 6.5)
    # `spares[3]` has memoryBase 0.0 after the blow-off, so `early` clamps at 1.0.
    review(at(6, 9), None, spares[3], True, "nearly_perfect", 5.0, assisted=True)

    # ---- phase F: the conditional peel-back -------------------------------- #
    # `step_a` is a direct prerequisite of `top`, and `top` was placed conditional,
    # so a MISSED attempt on `step_a` halves `top`'s repNum and clears the flag.
    s3 = "s_cov_3"
    events.append(SessionStart(ts=at(8, 9), session=s3))
    peel_task = f"{s3}-review-{step_a}"
    served(at(8, 9, 1), s3, peel_task, TaskType.review, step_a)
    attempt(at(8, 9, 3), s3, peel_task, step_a, 0, TaskType.review, False, "nearly_passable")
    events.append(SessionEnd(ts=at(8, 10), session=s3, xp_earned=0.0, minutes=30.0))

    # ---- phase G: the three `decay_for` bands ------------------------------ #
    # Each miss reads a different overdue ratio against the topic's own interval:
    #   day 14 on `step_a`  -- between 1 and 3, so the factor is interpolated
    #   day 20 on `deep`    -- below 1, so the factor is 1.0
    #   day 40 on `top`     -- far past 3, so the factor caps at 3.0 and `repNum`
    #                          floors at 0.0
    review(
        at(14, 9),
        None,
        step_a,
        False,
        "poor",
        task_xp(TaskType.review, WorkQuality.poor, cfg),
        assisted=True,
    )
    review(
        at(20, 9),
        None,
        deep,
        False,
        "nearly_passable",
        task_xp(TaskType.review, WorkQuality.nearly_passable, cfg),
    )
    review(
        at(40, 9),
        None,
        top,
        False,
        "blowoff",
        task_xp(TaskType.review, WorkQuality.blowoff, cfg),
    )

    # ---- phase H: the quiz threshold --------------------------------------- #
    # The first quiz is above `retake_below` (0.8) and the second is below it, so
    # `quiz.retake_pending` is False and then True. Both carry an off-curriculum row
    # and a missed row.
    for index, (day, score) in enumerate([(15, 0.875), (16, 0.75)]):
        events.append(
            QuizResult(
                ts=at(day, 9),
                session=None,
                quiz_id=f"q{index + 1}-seed{seed}",
                score=score,
                per_topic=[
                    QuizTopicResult(topic=deep, correct=True, secs=20 + index),
                    QuizTopicResult(topic=OFF_CURRICULUM, correct=True, secs=15),
                    QuizTopicResult(topic=step_c, correct=False, secs=95 + index),
                ],
                xp=15.0,
            )
        )

    # ---- phase I: the profile reset ---------------------------------------- #
    # `step_b` carries accumulated state by now, so the reset is observable.
    events.append(ProfileReset(ts=at(18, 9), session=None, topics=[step_b, OFF_CURRICULUM]))

    # ---- phase J: the interval ladder -------------------------------------- #
    # Twelve monthly passes on `deep`. The first few read a memory near zero, so
    # `early` clamps at 1.0 and `repNum` climbs by a full `speed` step; the later
    # ones read a saturated memory and climb by the `early` floor. `repNum` ends
    # past the last index of `interval_table`, which is that table's flat tail.
    # Each carries ZERO XP, so the ladder cannot disturb the streak block below.
    for day in range(22, 22 + 30 * 12, 30):
        review(at(day, 9), None, deep, True, "perfect", 0.0)

    # ---- phase K: the refresh diagnostic ----------------------------------- #
    # `top` is answered, so its ability blends; `step_c` is not, so it keeps its own.
    # `step_b` was reset to untouched but carries a POSITIVE balance, so the refresh
    # promotes it. `spares[5]` is untouched with a non-positive balance, which is the
    # H2 promote-guard: it is skipped.
    for index, weight in enumerate([1.0, 0.8]):
        events.append(
            DiagnosticAnswer(
                ts=at(400, 9, index),
                session=None,
                topic=top,
                correct=False,
                secs=140 + index,
                weight=weight,
            )
        )
    refresh = [
        (top, -1.0),
        (step_b, 1.0),
        (spares[5], -1.0),
        (OFF_CURRICULUM, 2.0),
        (step_c, 3.0),
    ]
    rng.shuffle(refresh)
    events.append(
        DiagnosticPlaced(
            ts=at(400, 10),
            session=None,
            balances=dict(refresh),
            conditional=[step_c],
            refresh=True,
        )
    )

    # ---- phase L: the no-op types ------------------------------------------ #
    events.append(
        AnkiCardCreated(
            ts=at(402, 9),
            session=None,
            topic=top,
            deck=f"Cadus::{COURSE}",
            note_id=1_600_000_000 + seed,
            front_hash=problem_text_hash(problem_text(top, 99)),
        )
    )
    events.append(
        ConfigChanged(
            ts=at(402, 9, 1), session=None, summary="retuned the quiz cadence", git_ref=None
        )
    )
    events.append(
        CurriculumChanged(
            ts=at(402, 9, 2), session=None, summary="edited one exemplar", git_ref=f"{seed:07x}"
        )
    )
    events.append(
        TaskServed(
            ts=at(402, 9, 3),
            session=None,
            task_id=f"cov-multistep-{seed}",
            task_type=TaskType.multi_step,
            topic=None,
            kp=None,
            problems=[],
            component_topics=[deep, step_c, step_b],
            seed=None,
        )
    )

    # ---- phase M: the corrections ------------------------------------------ #
    s4 = "s_cov_4"
    events.append(SessionStart(ts=at(404, 9), session=s4))
    # 1. A lesson task corrected TWICE. The later correction wins, and its
    #    `lesson_result` binds through the most recent preceding attempt.
    lesson_topic = spares[6]
    lesson_task = f"{s4}-lesson-{lesson_topic}"
    served(at(404, 9, 1), s4, lesson_task, TaskType.lesson, lesson_topic)
    attempt(
        at(404, 9, 3),
        s4,
        lesson_task,
        lesson_topic,
        0,
        TaskType.lesson,
        True,
        "nearly_passable",
    )
    attempt(
        at(404, 9, 5),
        s4,
        lesson_task,
        lesson_topic,
        1,
        TaskType.lesson,
        True,
        "nearly_passable",
    )
    events.append(
        LessonResult(
            ts=at(404, 9, 7),
            session=s4,
            topic=lesson_topic,
            passed=True,
            failed_at_kp=None,
            xp=task_xp(TaskType.lesson, WorkQuality.nearly_passable, cfg, kp_count=3),
            quality_tier=WorkQuality.nearly_passable,
            assisted=False,
        )
    )
    # 2. A review task whose `review_result` carries its own `task_id`.
    review_topic = spares[7]
    review_task = f"{s4}-review-{review_topic}"
    served(at(404, 9, 9), s4, review_task, TaskType.review, review_topic)
    attempt(at(404, 9, 11), s4, review_task, review_topic, 0, TaskType.review, False, "poor")
    review(
        at(404, 9, 13),
        s4,
        review_topic,
        False,
        "poor",
        task_xp(TaskType.review, WorkQuality.poor, cfg),
        task_id=review_task,
    )
    # 3. A `lesson_result` with NO preceding attempt on its topic. Its correction
    #    cannot bind to it, so it keeps its own tier and XP.
    events.append(
        LessonResult(
            ts=at(404, 9, 15),
            session=s4,
            topic=spares[2],
            passed=True,
            failed_at_kp=None,
            xp=task_xp(TaskType.lesson, WorkQuality.poor, cfg, kp_count=3),
            quality_tier=WorkQuality.poor,
            assisted=False,
        )
    )
    events.append(
        Regraded(
            ts=at(404, 10),
            session=s4,
            task_id=lesson_task,
            topic=lesson_topic,
            attempts=[
                RegradedAttempt(
                    attempt_id=f"{lesson_task}-a1",
                    work_quality=WorkQuality.passable,
                    error_tags=["notation"],
                    grader_note="regraded: first pass",
                )
            ],
            quality_tier=WorkQuality.passable,
            xp=task_xp(TaskType.lesson, WorkQuality.passable, cfg, kp_count=3),
            reason="the first correction, superseded below",
        )
    )
    events.append(
        Regraded(
            ts=at(404, 10, 5),
            session=s4,
            task_id=lesson_task,
            topic=lesson_topic,
            attempts=[
                RegradedAttempt(
                    attempt_id=f"{lesson_task}-a1",
                    work_quality=WorkQuality.perfect,
                    error_tags=[],
                    grader_note="regraded: second pass, method sound",
                )
            ],
            quality_tier=WorkQuality.perfect,
            xp=task_xp(TaskType.lesson, WorkQuality.perfect, cfg, kp_count=3),
            reason="a later correction of the same task wins",
        )
    )
    events.append(
        Regraded(
            ts=at(404, 10, 10),
            session=s4,
            task_id=review_task,
            topic=review_topic,
            attempts=[
                RegradedAttempt(
                    attempt_id=f"{review_task}-a0",
                    work_quality=WorkQuality.nearly_perfect,
                    error_tags=["timing-unreliable"],
                    grader_note="regraded",
                )
            ],
            quality_tier=WorkQuality.nearly_perfect,
            xp=task_xp(TaskType.review, WorkQuality.nearly_perfect, cfg),
            reason="the review carries its own task_id",
        )
    )
    events.append(
        Regraded(
            ts=at(404, 10, 15),
            session=s4,
            task_id=f"{s4}-lesson-{spares[2]}",
            topic=spares[2],
            attempts=[],
            quality_tier=WorkQuality.perfect,
            xp=99.0,
            reason="no attempt precedes that lesson_result, so nothing binds",
        )
    )
    events.append(SessionEnd(ts=at(404, 11), session=s4, xp_earned=0.0, minutes=120.0))

    # ---- phase N1: a remediation target that stays open -------------------- #
    # `spares[5]` is never practiced after this instant, so the target survives to
    # `pending_remediation`. The off-curriculum id beside it is dropped.
    events.append(
        RemediationTriggered(
            ts=at(406, 9),
            session=None,
            kind="quiz-miss",
            source_topic=step_c,
            targets=[spares[5], OFF_CURRICULUM],
        )
    )

    # ---- phase N: the XP total tuner --------------------------------------- #
    # A failing `lesson_result` records XP and fires no FIRe, so it moves the grand
    # total without touching a memory state. Its day is far outside the trailing
    # 28-day window, so it moves neither the streak nor the velocity. Its XP is
    # filled in below, once the rest of the stream is priced.
    tuner_index = len(events)
    events.append(None)

    # ---- phase O: the streak block, in COVERAGE_TZ ------------------------- #
    # Local days in America/New_York, against the goal of 40:
    #   Apr 10  16.375  below the goal
    #   Apr 11  --      a gap day
    #   Apr 12  --      a gap day
    #   Apr 13  39.0    ONE XP below the goal: the day that stops the streak
    #   Apr 14  40.0    EXACTLY at the goal
    #   Apr 15  40.0    EXACTLY at the goal
    #   Apr 16  40.5    today, and a half-even rounding tie for `xp.today`
    # so `streak_days` is 3, and a `> goal` test instead of `>= goal` gives 1.
    # The last entry is stamped 02:30Z on Apr 17, which is 22:30 on Apr 16 in New
    # York, so the whole block moves by a day between the two zones (trap T9).
    # The window total is 175.875 exactly, so `xp_per_day_28d` is 6.28125 and
    # `round(x, 4)` is a half-even tie: 6.2812, where a scale-round-divide gives
    # 6.2813 (trap T4).
    streak_block = [
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
    streak_topics = chain + spares[:5]
    for index, (ts, xp) in enumerate(streak_block):
        review(
            ts,
            None,
            streak_topics[index % len(streak_topics)],
            True,
            rng.choice(["perfect", "nearly_perfect", "passable"]),
            xp,
        )

    # ---- price the tuner --------------------------------------------------- #
    priced = [
        event.xp
        for event in events
        if isinstance(event, (LessonResult, ReviewResult, QuizResult))
    ]
    target = _even_half(sum(priced))
    delta = target - sum(priced)
    events[tuner_index] = LessonResult(
        ts=at(420, 9),
        session=None,
        topic=spares[4],
        passed=False,
        failed_at_kp=kps(spares[4])[0],
        xp=delta,
        quality_tier=WorkQuality.blowoff,
        assisted=False,
    )
    total = sum(
        event.xp
        for event in events
        if isinstance(event, (LessonResult, ReviewResult, QuizResult))
    )
    assert total == target, f"seed {seed}: the XP total {total!r} is not {target!r}"
    assert math.floor(total) % 2 == 0, f"seed {seed}: {total!r} does not round half-even down"

    events.sort(key=lambda event: event.ts)
    return events


# --------------------------------------------------------------------------- #
# Entry point
# --------------------------------------------------------------------------- #


def canonical_lines(events: list[object]) -> list[str]:
    """One canonical JSON line per event, exactly as the fixtures store them."""
    lines = []
    for event in events:
        payload = json.loads(event.model_dump_json())
        lines.append(
            json.dumps(payload, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
        )
    return lines


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--out", default=None, help="the stream file to write")
    ap.add_argument(
        "--out-dir",
        default="/tmp/claude-1000/-home-deploy-dev-cadus2-0/"
        "423a634f-40c8-4ad8-9fdd-67df2281434b/scratchpad",
    )
    ap.add_argument("--curriculum", default="/home/deploy/dev/cadus2.0/curriculum")
    ap.add_argument("--config", default="/home/deploy/dev/cadus/config.yaml")
    args = ap.parse_args()

    os.environ["CADUS_CURRICULUM"] = args.curriculum
    os.environ["CADUS_CONFIG"] = args.config

    from cadus.loader import load_config, load_graph

    cfg = load_config()
    graph = load_graph()

    if args.seed == 1:
        events = build_stream_1(cfg, graph)
    elif args.seed >= 2:
        events = build_seeded_stream(args.seed, cfg, graph)
    else:
        ap.error("--seed must be 1 or greater")

    if args.out:
        stream_path = args.out
        os.makedirs(os.path.dirname(os.path.abspath(stream_path)), exist_ok=True)
    else:
        os.makedirs(args.out_dir, exist_ok=True)
        stream_path = os.path.join(args.out_dir, f"m3_stream_{args.seed}.jsonl")

    blob = "\n".join(canonical_lines(events)) + "\n"
    with open(stream_path, "w", encoding="utf-8") as handle:
        handle.write(blob)

    print(f"wrote {stream_path} ({len(events)} events)")
    print(f"stream sha256 = {hashlib.sha256(blob.encode('utf-8')).hexdigest()}")
    counts: dict[str, int] = {}
    for event in events:
        counts[event.type] = counts.get(event.type, 0) + 1
    for key in sorted(counts):
        print(f"  {key}: {counts[key]}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
