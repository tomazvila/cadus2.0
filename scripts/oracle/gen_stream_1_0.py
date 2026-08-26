#!/usr/bin/env python
"""M3 parity fixture generator: build ONE synthetic 1.0 event stream.

Read-only against the 1.0 code base. Constructs events directly through the 1.0
pydantic models (no store, no database, no wall clock), so the stream is a pure
function of the constants in this file.

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

Usage:
  gen_stream_1_0.py [--out-dir DIR] [--curriculum DIR] [--config FILE]
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
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


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
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

    from cadus.fire import grade_review
    from cadus.loader import load_config, load_graph
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

    cfg = load_config()
    graph = load_graph()
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

    # ---- write the stream -------------------------------------------------- #
    out_dir = args.out_dir
    os.makedirs(out_dir, exist_ok=True)
    stream_path = os.path.join(out_dir, "m3_stream_1.jsonl")
    lines: list[str] = []
    for event in events:
        payload = json.loads(event.model_dump_json())
        lines.append(
            json.dumps(payload, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
        )
    blob = "\n".join(lines) + "\n"
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
