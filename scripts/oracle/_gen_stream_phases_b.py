"""Phases H to O of the seeded stream of `gen_stream_1_0.py`.

H, I, and J: the quiz threshold, the profile reset, and the interval ladder.
K: the refresh diagnostic. L: the no-op types. M: the corrections. N: the open
remediation target and the XP tuner slot. O: the streak block. Every function
takes the `_SeededStream` builder `s` and appends to `s.events`. Import this
module after the 1.0 loader is pointed at its trees.
"""

from __future__ import annotations

from _gen_stream_base import COURSE, OFF_CURRICULUM, STREAK_BLOCK, problem_text
from cadus.model import (
    AnkiCardCreated,
    ConfigChanged,
    CurriculumChanged,
    DiagnosticAnswer,
    DiagnosticPlaced,
    ProfileReset,
    QuizResult,
    QuizTopicResult,
    RegradedAttempt,
    RemediationTriggered,
    SessionEnd,
    SessionStart,
    TaskServed,
    TaskType,
    WorkQuality,
)
from cadus.projector import problem_text_hash
from cadus.xp import task_xp


def phase_h_i_j(s) -> None:
    """Phases H, I, and J: the quiz threshold, the profile reset, the interval ladder."""
    at, events = s.at, s.events
    # ---- phase H: the quiz threshold --------------------------------------- #
    # The first quiz is above `retake_below` (0.8) and the second is below it, so
    # `quiz.retake_pending` is False and then True. Both carry an off-curriculum row
    # and a missed row.
    for index, (day, score) in enumerate([(15, 0.875), (16, 0.75)]):
        events.append(
            QuizResult(
                ts=at(day, 9),
                session=None,
                quiz_id=f"q{index + 1}-seed{s.seed}",
                score=score,
                per_topic=[
                    QuizTopicResult(topic=s.deep, correct=True, secs=20 + index),
                    QuizTopicResult(topic=OFF_CURRICULUM, correct=True, secs=15),
                    QuizTopicResult(topic=s.step_c, correct=False, secs=95 + index),
                ],
                xp=15.0,
            )
        )

    # ---- phase I: the profile reset ---------------------------------------- #
    # `step_b` carries accumulated state by now, so the reset is observable.
    events.append(
        ProfileReset(ts=at(18, 9), session=None, topics=[s.step_b, OFF_CURRICULUM])
    )

    # ---- phase J: the interval ladder -------------------------------------- #
    # Twelve monthly passes on `deep`. The first few read a memory near zero, so
    # `early` clamps at 1.0 and `repNum` climbs by a full `speed` step; the later
    # ones read a saturated memory and climb by the `early` floor. `repNum` ends
    # past the last index of `interval_table`, which is that table's flat tail.
    # Each carries ZERO XP, so the ladder cannot disturb the streak block below.
    for day in range(22, 22 + 30 * 12, 30):
        s.review(at(day, 9), None, s.deep, True, "perfect", 0.0)

def phase_k(s) -> None:
    """Phase K: the refresh diagnostic."""
    at, rng, events = s.at, s.rng, s.events
    # `top` is answered, so its ability blends; `step_c` is not, so it keeps its own.
    # `step_b` was reset to untouched but carries a POSITIVE balance, so the refresh
    # promotes it. `spares[5]` is untouched with a non-positive balance, which is the
    # H2 promote-guard: it is skipped. `spares[6]` carries the guard's BOUNDARY, a
    # balance of exactly 0.0 -- also non-positive, so it is skipped too. Without that
    # row, a `balance < 0.0` guard folds identically to the `balance <= 0.0` guard 1.0
    # writes, and the boundary goes untested.
    for index, weight in enumerate([1.0, 0.8]):
        events.append(
            DiagnosticAnswer(
                ts=at(400, 9, index),
                session=None,
                topic=s.top,
                correct=False,
                secs=140 + index,
                weight=weight,
            )
        )
    refresh = [
        (s.top, -1.0),
        (s.step_b, 1.0),
        (s.spares[5], -1.0),
        (OFF_CURRICULUM, 2.0),
        (s.step_c, 3.0),
    ]
    rng.shuffle(refresh)
    # Appended AFTER the shuffle on purpose: it draws no randomness, so every later
    # phase keeps the draws it had before this row existed.
    refresh.append((s.spares[6], 0.0))
    events.append(
        DiagnosticPlaced(
            ts=at(400, 10),
            session=None,
            balances=dict(refresh),
            conditional=[s.step_c],
            refresh=True,
        )
    )

def phase_l(s) -> None:
    """Phase L: the no-op types."""
    at, events, seed = s.at, s.events, s.seed
    events.append(
        AnkiCardCreated(
            ts=at(402, 9),
            session=None,
            topic=s.top,
            deck=f"Cadus::{COURSE}",
            note_id=1_600_000_000 + seed,
            front_hash=problem_text_hash(problem_text(s.top, 99)),
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
            component_topics=[s.deep, s.step_c, s.step_b],
            seed=None,
        )
    )

def phase_m(s) -> None:
    """Phase M: the corrections."""
    at, cfg, events = s.at, s.cfg, s.events
    s4 = "s_cov_4"
    events.append(SessionStart(ts=at(404, 9), session=s4))
    # 1. A lesson task corrected TWICE. The later correction wins, and its
    #    `lesson_result` binds through the most recent preceding attempt.
    lesson_topic = s.spares[6]
    lesson_task = f"{s4}-lesson-{lesson_topic}"
    s.served(at(404, 9, 1), s4, lesson_task, TaskType.lesson, lesson_topic)
    s.attempt(
        at(404, 9, 3), s4, lesson_task, lesson_topic, 0, TaskType.lesson, True, "nearly_passable"
    )
    s.attempt(
        at(404, 9, 5), s4, lesson_task, lesson_topic, 1, TaskType.lesson, True, "nearly_passable"
    )
    s.lesson_result(
        at(404, 9, 7),
        s4,
        lesson_topic,
        True,
        None,
        task_xp(TaskType.lesson, WorkQuality.nearly_passable, cfg, kp_count=3),
        "nearly_passable",
    )
    # 2. A review task whose `review_result` carries its own `task_id`.
    review_topic = s.spares[7]
    review_task = f"{s4}-review-{review_topic}"
    s.served(at(404, 9, 9), s4, review_task, TaskType.review, review_topic)
    s.attempt(at(404, 9, 11), s4, review_task, review_topic, 0, TaskType.review, False, "poor")
    s.review(
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
    s.lesson_result(
        at(404, 9, 15),
        s4,
        s.spares[2],
        True,
        None,
        task_xp(TaskType.lesson, WorkQuality.poor, cfg, kp_count=3),
        "poor",
    )
    s.regraded(
        at(404, 10),
        s4,
        lesson_task,
        lesson_topic,
        [
            RegradedAttempt(
                attempt_id=f"{lesson_task}-a1",
                work_quality=WorkQuality.passable,
                error_tags=["notation"],
                grader_note="regraded: first pass",
            )
        ],
        "passable",
        task_xp(TaskType.lesson, WorkQuality.passable, cfg, kp_count=3),
        "the first correction, superseded below",
    )
    s.regraded(
        at(404, 10, 5),
        s4,
        lesson_task,
        lesson_topic,
        [
            RegradedAttempt(
                attempt_id=f"{lesson_task}-a1",
                work_quality=WorkQuality.perfect,
                error_tags=[],
                grader_note="regraded: second pass, method sound",
            )
        ],
        "perfect",
        task_xp(TaskType.lesson, WorkQuality.perfect, cfg, kp_count=3),
        "a later correction of the same task wins",
    )
    s.regraded(
        at(404, 10, 10),
        s4,
        review_task,
        review_topic,
        [
            RegradedAttempt(
                attempt_id=f"{review_task}-a0",
                work_quality=WorkQuality.nearly_perfect,
                error_tags=["timing-unreliable"],
                grader_note="regraded",
            )
        ],
        "nearly_perfect",
        task_xp(TaskType.review, WorkQuality.nearly_perfect, cfg),
        "the review carries its own task_id",
    )
    s.regraded(
        at(404, 10, 15),
        s4,
        f"{s4}-lesson-{s.spares[2]}",
        s.spares[2],
        [],
        "perfect",
        99.0,
        "no attempt precedes that lesson_result, so nothing binds",
    )
    events.append(SessionEnd(ts=at(404, 11), session=s4, xp_earned=0.0, minutes=120.0))

def phase_n(s) -> None:
    """Phases N1 and N: an open remediation target, and the XP total tuner slot."""
    at, events = s.at, s.events
    # ---- phase N1: a remediation target that stays open -------------------- #
    # `spares[5]` is never practiced after this instant, so the target survives to
    # `pending_remediation`. The off-curriculum id beside it is dropped.
    events.append(
        RemediationTriggered(
            ts=at(406, 9),
            session=None,
            kind="quiz-miss",
            source_topic=s.step_c,
            targets=[s.spares[5], OFF_CURRICULUM],
        )
    )

    # ---- phase N: the XP total tuner --------------------------------------- #
    # A failing `lesson_result` records XP and fires no FIRe, so it moves the grand
    # total without touching a memory state. Its day is far outside the trailing
    # 28-day window, so it moves neither the streak nor the velocity. Its XP is
    # filled in below, once the rest of the stream is priced.
    s.tuner_index = len(events)
    events.append(None)

def phase_o(s) -> None:
    """Phase O: the streak block, in COVERAGE_TZ."""
    streak_topics = s.chain + s.spares[:5]
    for index, (ts, xp) in enumerate(STREAK_BLOCK):
        s.review(
            ts,
            None,
            streak_topics[index % len(streak_topics)],
            True,
            s.rng.choice(["perfect", "nearly_perfect", "passable"]),
            xp,
        )
