#!/usr/bin/env python
"""The U3 coverage stream: the handlers `stream_1.jsonl` never reaches.

It is hand written and deterministic: no clock, no random number generator, no store.
Run it, then fold the result with `dump_projector_1_0.py` to renew the two committed
1.0 models.

    python3 scripts/oracle/gen_stream_u3_coverage.py \
        crates/core/tests/fixtures/events/stream_u3_coverage.jsonl

Branches it covers that `stream_1.jsonl` does not: the mastery-floor stamp of a
`gap-fill` enrollment; a two-answer diagnostic seed; an inferred placement seed; a
conditional flag and its peel-back; a placement balance above the repNum cap; a
non-positive placement balance; an off-curriculum id in every list that carries one; a
lesson that fails at its second knowledge point; an assisted review pass; a remediation
trigger that one lesson closes in part; a repeated trigger that the dedup drops; a quiz
below the retake threshold with a missed row and an off-curriculum row; a profile reset;
a refresh diagnostic with an answered demote, an unanswered demote, and the H2 guard on
never-learned material; the three remaining no-op types; a regrade that reaches a
`review_result` carrying no `task_id`; and an XP record whose LOCAL day moves with the
time zone.
"""

import json
import sys

S1 = "s_2026-03-02a"
S2 = "s_2026-03-05a"
S3 = "s_2026-03-08a"

E = []


def add(**row):
    row.setdefault("v", 1)
    E.append(row)


def attempt(ts, session, aid, task_id, topic, kp, task_type, correct, tier, tags=None):
    add(
        type="attempt",
        ts=ts,
        session=session,
        attempt_id=aid,
        task_id=task_id,
        topic=topic,
        kp=kp,
        task_type=task_type,
        problem={"text": f"[{topic}] coverage problem {aid}", "expected": "42"},
        given_answer="42" if correct else "41",
        work=None,
        answer_kind=None,
        correct=correct,
        secs=25,
        error_tags=tags or [],
        work_quality=tier,
        grader_note=None,
        assisted=False,
    )


# 1. enrolled -> mastery floor stamp.
add(type="enrolled", ts="2026-03-02T09:00:00Z", session=None, course="foundations",
    reason="gap-fill", return_to=None)
add(type="session_start", ts="2026-03-02T09:01:00Z", session=S1)

# 2. diagnostic answers: two on one topic, one on another.
for ts, topic, correct, weight in [
    ("2026-03-02T09:02:00Z", "absolute-value", True, 1.0),
    ("2026-03-02T09:03:00Z", "absolute-value", False, 1.0),
    ("2026-03-02T09:04:00Z", "number-line-integers", True, 0.4),
]:
    add(type="diagnostic_answer", ts=ts, session=S1, topic=topic, correct=correct,
        secs=30, weight=weight)

# 3. initial placement: answered seed, inferred seed, conditional flag, an off-graph id,
#    a non-positive balance, and a balance above the repNum cap.
add(type="diagnostic_placed", ts="2026-03-02T09:05:00Z", session=S1,
    balances={"number-line-integers": 6.5, "absolute-value": 0.5,
              "one-step-equations": 2.0, "not-a-real-topic": 5.0,
              "opposites-of-integers": -1.0},
    conditional=["absolute-value"], refresh=False)

# 4. a lesson that fails at kp2, then passes.
add(type="task_served", ts="2026-03-02T09:06:00Z", session=S1,
    task_id=f"{S1}-lesson-absolute-value", task_type="lesson", topic="absolute-value",
    kp="kp1", problems=[], component_topics=[], seed=None)
attempt("2026-03-02T09:07:00Z", S1, f"{S1}-lesson-absolute-value-a0",
        f"{S1}-lesson-absolute-value", "absolute-value", "kp2", "lesson", False, "poor")
add(type="lesson_result", ts="2026-03-02T09:08:00Z", session=S1, topic="absolute-value",
    passed=False, failed_at_kp="kp2", xp=0.0, quality_tier="poor", assisted=False)
attempt("2026-03-02T09:20:00Z", S1, f"{S1}-lesson-absolute-value-a1",
        f"{S1}-lesson-absolute-value", "absolute-value", "kp2", "lesson", True, "passable")
# 3.5 x 2 KP x 0.85 = 5.95.
add(type="lesson_result", ts="2026-03-02T09:21:00Z", session=S1, topic="absolute-value",
    passed=True, failed_at_kp=None, xp=5.95, quality_tier="passable", assisted=False)
add(type="session_end", ts="2026-03-02T09:30:00Z", session=S1, xp_earned=5.95, minutes=29.0)

# 5. an assisted review pass, then a remediation trigger closed by one lesson only.
add(type="session_start", ts="2026-03-05T09:00:00Z", session=S2)
add(type="task_served", ts="2026-03-05T09:01:00Z", session=S2,
    task_id=f"{S2}-review-absolute-value", task_type="review", topic="absolute-value",
    kp=None, problems=[], component_topics=[], seed=None)
attempt("2026-03-05T09:02:00Z", S2, f"{S2}-review-absolute-value-a0",
        f"{S2}-review-absolute-value", "absolute-value", None, "review", True, "perfect")
add(type="review_result", ts="2026-03-05T09:03:00Z", session=S2, topic="absolute-value",
    passed=True, weighted_score=1.0, xp=6.5, quality_tier="perfect", assisted=True,
    task_id=f"{S2}-review-absolute-value")
add(type="remediation_triggered", ts="2026-03-05T09:10:00Z", session=S2, kind="quiz-miss",
    source_topic="absolute-value",
    targets=["adding-integers", "two-step-equations", "not-a-real-topic"])
# 3.5 x 3 KP x 1.3 = 13.65.
add(type="lesson_result", ts="2026-03-05T09:20:00Z", session=S2, topic="adding-integers",
    passed=True, failed_at_kp=None, xp=13.65, quality_tier="perfect", assisted=False)

# 6. a quiz below the retake threshold, with an off-graph row and a missed row.
add(type="quiz_result", ts="2026-03-06T09:00:00Z", session=S2, quiz_id="q1", score=0.75,
    per_topic=[{"topic": "absolute-value", "correct": True, "secs": 20},
               {"topic": "not-a-real-topic", "correct": True, "secs": 10},
               {"topic": "two-step-equations", "correct": False, "secs": 30}],
    xp=15.0)
# 7. the same trigger again: the dedup keeps the first occurrence only.
add(type="remediation_triggered", ts="2026-03-06T09:05:00Z", session=S2, kind="quiz-miss",
    source_topic="absolute-value",
    targets=["adding-integers", "two-step-equations", "not-a-real-topic"])
# 8. a profile reset on a topic with state, plus an off-graph id.
add(type="profile_reset", ts="2026-03-06T09:10:00Z", session=S2,
    topics=["number-line-integers", "not-a-real-topic"])
add(type="session_end", ts="2026-03-06T09:30:00Z", session=S2, xp_earned=15.0, minutes=30.0)

# 9. a refresh: an answered demote, an unanswered demote, and the H2 promote guard.
add(type="diagnostic_answer", ts="2026-03-07T09:00:00Z", session=None,
    topic="one-step-equations", correct=False, secs=90, weight=1.0)
add(type="diagnostic_placed", ts="2026-03-07T09:01:00Z", session=None,
    balances={"one-step-equations": 1.0, "absolute-value": 2.0,
              "two-step-equations": -1.0, "fraction-word-problems": -1.0,
              "not-a-real-topic": 2.0},
    conditional=[], refresh=True)

# 10. the three remaining no-op types.
add(type="anki_card_created", ts="2026-03-07T10:00:00Z", session=None,
    topic="absolute-value", deck="Cadus", note_id=1234, front_hash="abc123")
add(type="config_changed", ts="2026-03-07T10:01:00Z", session=None, summary="tuned",
    git_ref=None)
add(type="curriculum_changed", ts="2026-03-07T10:02:00Z", session=None, summary="edited",
    git_ref="deadbeef")

# 11. a regrade that reaches a review_result through its preceding attempt.
add(type="session_start", ts="2026-03-08T09:00:00Z", session=S3)
attempt("2026-03-08T09:01:00Z", S3, f"{S3}-review-adding-integers-a0",
        f"{S3}-review-adding-integers", "adding-integers", None, "review", True, "perfect")
add(type="review_result", ts="2026-03-08T09:02:00Z", session=S3, topic="adding-integers",
    passed=True, weighted_score=1.0, xp=5.0, quality_tier="perfect", assisted=False,
    task_id=None)
# 12. a review whose LOCAL day depends on the time zone: 02:30 UTC is the day before
#     in America/New_York, so "today" and the streak move with the zone (trap T9).
add(type="review_result", ts="2026-03-09T02:30:00Z", session=None, topic="place-value",
    passed=True, weighted_score=1.0, xp=45.0, quality_tier="perfect", assisted=False,
    task_id=None)
add(type="regraded", ts="2026-03-09T09:00:00Z", session=None,
    task_id=f"{S3}-review-adding-integers", topic="adding-integers",
    attempts=[{"attempt_id": f"{S3}-review-adding-integers-a0", "work_quality": "poor",
               "error_tags": ["sign-error"], "grader_note": "regraded"}],
    quality_tier="poor", xp=0.0, reason="grader drift")
add(type="session_end", ts="2026-03-09T09:30:00Z", session=S3, xp_earned=0.0, minutes=1.0)

out = sys.argv[1] if len(sys.argv) > 1 else "stream_u3_coverage.jsonl"
with open(out, "w", encoding="utf-8") as handle:
    for row in E:
        handle.write(json.dumps(row, sort_keys=True, separators=(",", ":"),
                                ensure_ascii=False) + "\n")
print(len(E))
