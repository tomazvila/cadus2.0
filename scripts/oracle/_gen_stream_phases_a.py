"""Phases A to G of the seeded stream of `gen_stream_1_0.py`.

A: the enrollment, the diagnostic, and the initial placement. B: the two
lesson-close XP paths. C: a lesson that fails twice. D and E: the first touch
and the `early` bands. F and G: the conditional peel-back and the `decay_for`
bands. Every function takes the `_SeededStream` builder `s` and appends to
`s.events`. Import this module after the 1.0 loader is pointed at its trees.
"""

from __future__ import annotations

from _gen_stream_base import COURSE, OFF_CURRICULUM
from cadus.model import (
    DiagnosticAnswer,
    DiagnosticPlaced,
    Enrolled,
    RemediationTriggered,
    SessionEnd,
    SessionStart,
    TaskType,
    WorkQuality,
)
from cadus.xp import task_xp


def phase_a(s) -> None:
    """Phase A: enrollment, the diagnostic, the initial placement."""
    at, rng, events = s.at, s.rng, s.events
    s0 = "s_cov_0"
    if s.seed % 2 == 0:
        events.append(Enrolled(ts=at(0, 9), session=None, course=COURSE))
    else:
        events.append(
            Enrolled(
                ts=at(0, 9),
                session=None,
                course=COURSE,
                reason="gap-fill",
                return_to=rng.choice(s.other_courses),
            )
        )
    events.append(SessionStart(ts=at(0, 9, 5), session=s0))
    for index, (tid, correct, weight) in enumerate(
        [(s.deep, True, 1.0), (s.deep, False, 0.6), (s.step_b, True, 0.35)]
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
                topic=s.step_c,
                correct=False,
                secs=95 + index,
                weight=1.0,
            )
        )
    balances = [
        (s.deep, 5.5),  # above PLACEMENT_REPNUM_CAP, so `repNum` caps at 4.0
        (s.step_c, 2.5),  # answered, and answered badly
        (s.step_b, 1.5),  # answered once
        (s.top, 0.75),  # conditional, so a later miss on `step_a` peels it
        (s.step_a, 1.25),  # never answered: the inferred pass seeds it
        (OFF_CURRICULUM, 3.0),  # not a topic: skipped
        (s.spares[0], -1.0),  # non-positive: the initial pass drops it
    ]
    rng.shuffle(balances)  # dict order is load-bearing (trap T6)
    events.append(
        DiagnosticPlaced(
            ts=at(0, 10),
            session=s0,
            balances=dict(balances),
            conditional=[s.top],
            refresh=False,
        )
    )
    events.append(SessionEnd(ts=at(0, 10, 30), session=s0, xp_earned=0.0, minutes=90.0))

def phase_b(s) -> None:
    """Phase B: the two lesson-close XP paths (trap T7)."""
    at, events = s.at, s.events
    s1 = "s_cov_1"
    raw_xp = task_xp(TaskType.lesson, WorkQuality.passable, s.cfg, kp_count=3)
    assert repr(raw_xp) == "8.924999999999999", repr(raw_xp)
    events.append(SessionStart(ts=at(2, 9), session=s1))
    for index, (tid, xp) in enumerate([(s.top, raw_xp), (s.step_a, round(raw_xp, 2))]):
        task = f"{s1}-lesson-{tid}"
        kps = s.kps(tid)
        s.served(at(2, 9, 1 + 20 * index), s1, task, TaskType.lesson, tid, kp=kps[0])
        s.attempt(
            at(2, 9, 3 + 20 * index), s1, task, tid, 0, TaskType.lesson, True, "perfect", kp=kps[0]
        )
        s.attempt(
            at(2, 9, 6 + 20 * index), s1, task, tid, 1, TaskType.lesson, True, "passable", kp=kps[1]
        )
        s.lesson_result(at(2, 9, 8 + 20 * index), s1, tid, True, None, xp, "passable")
    events.append(SessionEnd(ts=at(2, 10), session=s1, xp_earned=0.0, minutes=60.0))

def phase_c(s) -> None:
    """Phase C: a lesson that fails twice, and a repeated trigger."""
    at, events = s.at, s.events
    s2 = "s_cov_2"
    events.append(SessionStart(ts=at(3, 9), session=s2))
    fail_topic = s.spares[1]
    failed_kp = s.kps(fail_topic)[1]
    for round_index in range(2):
        task = f"{s2}-lesson-{fail_topic}-{round_index}"
        s.served(at(3, 9, 1 + 10 * round_index), s2, task, TaskType.lesson, fail_topic)
        s.attempt(
            at(3, 9, 3 + 10 * round_index),
            s2,
            task,
            fail_topic,
            0,
            TaskType.lesson,
            False,
            "poor",
            kp=failed_kp,
        )
        s.lesson_result(
            at(3, 9, 5 + 10 * round_index), s2, fail_topic, False, failed_kp, 0.0, "poor"
        )
        # The SAME trigger twice: `pending_remediation` dedups on (kind, targets).
        events.append(
            RemediationTriggered(
                ts=at(3, 9, 6 + 10 * round_index),
                session=s2,
                kind="lesson_fail",
                source_topic=fail_topic,
                targets=[s.spares[2], OFF_CURRICULUM],
            )
        )
    events.append(SessionEnd(ts=at(3, 10), session=s2, xp_earned=0.0, minutes=45.0))

def phase_d_e(s) -> None:
    """Phases D and E: the first touch and the blow-off floor, then the `early` bands."""
    at, cfg = s.at, s.cfg
    # ---- phase D: first touch, the blow-off floor, the t0-is-None skip ----- #
    # `spares[3]` is untouched with ability 0.0, so the result seeds it first
    # (`_apply_fire_result` step 2). A `blowoff` miss is `raw_delta == -1.0`, which
    # floors `repNum` at 0.0 and `memoryBase` at 0.0. Its `t0` is None, so
    # `decay_for` is 1.0, and every upward target is unlearned, so the penalty is
    # skipped everywhere.
    s.review(
        at(4, 9),
        None,
        s.spares[3],
        False,
        "blowoff",
        task_xp(TaskType.review, WorkQuality.blowoff, cfg),
    )

    # ---- phase E: the three `early` bands of `raw_delta` -------------------- #
    # `step_b` was placed at day 0 with memoryBase 1.0, so by day 5 its memory sits
    # between 0.5 and 0.925: `early` is interpolated. The second pass twenty minutes
    # later reads a memory above 1.0, so `early` clamps at the 0.15 floor.
    s.review(at(5, 9), None, s.step_b, True, "perfect", 6.5)
    s.review(at(5, 9, 20), None, s.step_b, True, "perfect", 6.5)
    # `spares[3]` has memoryBase 0.0 after the blow-off, so `early` clamps at 1.0.
    s.review(at(6, 9), None, s.spares[3], True, "nearly_perfect", 5.0, assisted=True)

def phase_f_g(s) -> None:
    """Phases F and G: the conditional peel-back, then the three `decay_for` bands."""
    at, cfg, events = s.at, s.cfg, s.events
    # ---- phase F: the conditional peel-back -------------------------------- #
    # `step_a` is a direct prerequisite of `top`, and `top` was placed conditional,
    # so a MISSED attempt on `step_a` halves `top`'s repNum and clears the flag.
    s3 = "s_cov_3"
    events.append(SessionStart(ts=at(8, 9), session=s3))
    peel_task = f"{s3}-review-{s.step_a}"
    s.served(at(8, 9, 1), s3, peel_task, TaskType.review, s.step_a)
    s.attempt(
        at(8, 9, 3), s3, peel_task, s.step_a, 0, TaskType.review, False, "nearly_passable"
    )
    events.append(SessionEnd(ts=at(8, 10), session=s3, xp_earned=0.0, minutes=30.0))

    # ---- phase G: the three `decay_for` bands ------------------------------ #
    # Each miss reads a different overdue ratio against the topic's own interval:
    #   day 14 on `step_a`  -- between 1 and 3, so the factor is interpolated
    #   day 20 on `deep`    -- below 1, so the factor is 1.0
    #   day 40 on `top`     -- far past 3, so the factor caps at 3.0 and `repNum`
    #                          floors at 0.0
    s.review(
        at(14, 9),
        None,
        s.step_a,
        False,
        "poor",
        task_xp(TaskType.review, WorkQuality.poor, cfg),
        assisted=True,
    )
    s.review(
        at(20, 9),
        None,
        s.deep,
        False,
        "nearly_passable",
        task_xp(TaskType.review, WorkQuality.nearly_passable, cfg),
    )
    s.review(
        at(40, 9),
        None,
        s.top,
        False,
        "blowoff",
        task_xp(TaskType.review, WorkQuality.blowoff, cfg),
    )
