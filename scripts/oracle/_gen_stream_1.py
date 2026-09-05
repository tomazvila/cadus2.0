"""Stream #1 of `gen_stream_1_0.py`: the frozen hand-designed stream (seed 1).

Its bytes are frozen: `docs/plans/M3.md` pins the digest of its fold. Import this
module after the 1.0 loader is pointed at its trees: it imports the 1.0 models
at load time.
"""

from __future__ import annotations

from datetime import UTC, datetime, timedelta

from _gen_stream_base import COURSE, SEED, problem_text
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


class _Cursor:
    """The clock and the attempt slot of one session, advanced in place."""

    def __init__(self, start: datetime) -> None:
        self.t = start
        self.slot = 0

    def tick(self, gap: timedelta = ATTEMPT_GAP) -> None:
        """Move the clock forward by `gap`."""
        self.t += gap


def _lesson_attempts(events, cursor, session, task_id, tid, kp_ids):
    """The two attempts of one lesson task. Return the tier of the last one."""
    last_tier = None
    for n in (0, 1):
        tier_name, correct = LESSON_ATTEMPTS[cursor.slot]
        last_tier = WorkQuality(tier_name)
        events.append(
            Attempt(
                ts=cursor.t,
                session=session,
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
                secs=20 + 7 * cursor.slot,
                error_tags=[] if correct else ["arithmetic-slip"],
                work_quality=last_tier,
                grader_note=None,
                assisted=False,
            )
        )
        cursor.tick()
        cursor.slot += 1
    return last_tier


def _lesson_task(events, cfg, graph, cursor, session, i, tid) -> None:
    """One served lesson, its attempts, its result, and its remediation on a fail."""
    topic = graph.topics[tid]
    kp_ids = [kp.id for kp in topic.knowledge_points]
    task_id = f"{session}-lesson-{tid}"

    events.append(
        TaskServed(
            ts=cursor.t,
            session=session,
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
    cursor.tick()

    last_tier = _lesson_attempts(events, cursor, session, task_id, tid, kp_ids)

    passed = i != FAILING_LESSON
    # `service._advance_lesson` prices the lesson from the LAST attempt's
    # tier and writes the RAW (unrounded) float onto the event.
    xp = task_xp(TaskType.lesson, last_tier, cfg, kp_count=len(kp_ids))
    events.append(
        LessonResult(
            ts=cursor.t,
            session=session,
            topic=tid,
            passed=passed,
            failed_at_kp=None if passed else (kp_ids[-1] if kp_ids else None),
            xp=xp,
            quality_tier=last_tier,
            assisted=False,
        )
    )
    cursor.tick()

    if not passed:
        events.append(
            RemediationTriggered(
                ts=cursor.t,
                session=session,
                kind="lesson_fail",
                source_topic=tid,
                targets=[],
            )
        )
        cursor.tick()

    cursor.tick(TASK_GAP)


def _lesson_session(events, cfg, graph, cursor) -> None:
    """Session 1: the lesson block over the five topics."""
    s1 = "s_2026-03-02a"
    events.append(SessionStart(ts=cursor.t, session=s1))
    cursor.tick()

    for i, tid in enumerate(TOPICS):
        _lesson_task(events, cfg, graph, cursor, s1, i, tid)

    events.append(SessionEnd(ts=cursor.t, session=s1, xp_earned=0.0, minutes=48.5))


def _review_attempts(events, cursor, session, task_id, tid, assisted):
    """The two attempts of one review task. Return (correct sequence, last tier)."""
    seq: list[bool] = []
    last_tier = None
    for n in (0, 1):
        tier_name, correct = REVIEW_ATTEMPTS[cursor.slot]
        last_tier = WorkQuality(tier_name)
        seq.append(correct)
        events.append(
            Attempt(
                ts=cursor.t,
                session=session,
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
                secs=30 + 11 * cursor.slot,
                error_tags=[] if correct else ["sign-error"],
                work_quality=last_tier,
                grader_note=None,
                assisted=assisted and n == 1,
            )
        )
        cursor.tick()
        cursor.slot += 1
    return seq, last_tier


def _review_task(events, cfg, graph, cursor, session, i, tid) -> str:
    """One served review, its attempts, and its result. Return the task id."""
    task_id = f"{session}-review-{tid}"
    assisted = i == ASSISTED_REVIEW

    events.append(
        TaskServed(
            ts=cursor.t,
            session=session,
            task_id=task_id,
            task_type=TaskType.review,
            topic=tid,
            kp=None,
            problems=[],
            seed=SEED + 100 + i,
        )
    )
    cursor.tick()

    seq, last_tier = _review_attempts(events, cursor, session, task_id, tid, assisted)

    # `service.review_result`: order-sensitive grade, tier from the LAST
    # attempt, xp ROUNDED to 2dp on the event.
    passed, score = grade_review(seq, cfg)
    xp = round(task_xp(TaskType.review, last_tier, cfg), 2)
    events.append(
        ReviewResult(
            ts=cursor.t,
            session=session,
            topic=tid,
            passed=passed,
            weighted_score=score,
            xp=xp,
            quality_tier=last_tier,
            assisted=assisted,
            task_id=None,
        )
    )
    cursor.tick()
    cursor.tick(TASK_GAP)
    return task_id


def _correction(events, cursor, session, review_task_ids) -> None:
    """The one `regraded` event of the stream.

    It supersedes ONE attempt's tier and the review_result that closes that task.
    The review_result carries task_id=None, so `apply_regrades` matches it via
    the most recent preceding attempt on the same topic -- pinning that rule.
    """
    regraded_topic = TOPICS[REGRADED_REVIEW]
    regraded_task = review_task_ids[regraded_topic]
    events.append(
        Regraded(
            ts=cursor.t,
            session=session,
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
    cursor.tick()


def _review_session(events, cfg, graph) -> None:
    """Session 2: the review block, 8 days later, and the correction."""
    cursor = _Cursor(T_START + REVIEW_DAY_GAP)
    s2 = "s_2026-03-10a"
    events.append(SessionStart(ts=cursor.t, session=s2))
    cursor.tick()

    review_task_ids: dict[str, str] = {}
    for i, tid in enumerate(TOPICS):
        review_task_ids[tid] = _review_task(events, cfg, graph, cursor, s2, i, tid)

    _correction(events, cursor, s2, review_task_ids)

    events.append(SessionEnd(ts=cursor.t, session=s2, xp_earned=0.0, minutes=36.25))


def build_stream_1(cfg, graph) -> list[object]:
    """The original stream #1. Its bytes are frozen: `docs/plans/M3.md` pins the
    digest of its fold."""
    for tid in TOPICS:
        assert tid in graph.topics, f"topic {tid!r} left the curriculum"

    events: list[object] = []
    cursor = _Cursor(T_START)

    # ---- enrollment (out of session: `session` stays None) ---------------- #
    events.append(Enrolled(ts=cursor.t, session=None, course=COURSE))
    cursor.tick()

    _lesson_session(events, cfg, graph, cursor)
    _review_session(events, cfg, graph)
    return events
