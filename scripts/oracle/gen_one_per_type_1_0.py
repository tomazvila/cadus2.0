#!/usr/bin/env python
"""M3 U1 schema fixture: ONE instance of each of the 16 event types.

Read-only against the 1.0 code base. Builds each instance through the 1.0 pydantic
models, so the shapes and the JSON spellings come from the oracle, not from a hand
transcription. `docs/reference/event-schemas-1.0.json` carries no `examples`, so this
script supplies the per-type instance the M3 U1 acceptance check needs.

Each instance fills EVERY field of its type, defaults included, so a round-trip test
exercises the whole shape. Nothing here reads a clock, a store, or a database: the
output is a pure function of the constants below.

Run it from the 1.0 tree:

    cd /home/deploy/dev/cadus
    /home/deploy/dev/cadus/.venv/bin/python \\
        <this file> > crates/core/tests/fixtures/events/one_per_type.jsonl

Output: 16 lines of canonical JSON, one per type, in the declaration order of
`cadus_core::event::Event::TYPE_NAMES`. Canonical means sorted keys, compact
separators, non-ASCII text unescaped.
"""

from __future__ import annotations

import json
import sys
from datetime import UTC, datetime

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
    Regraded,
    RegradedAttempt,
    RemediationTriggered,
    ReviewResult,
    ServedProblem,
    SessionEnd,
    SessionStart,
    TaskServed,
)

#: One fixed instant with a zero microsecond part: it writes as `...Z`.
T = datetime(2026, 5, 4, 13, 45, 6, tzinfo=UTC)

#: One fixed instant with a non-zero microsecond part: it writes six digits.
T_MICROS = datetime(2026, 5, 4, 13, 45, 6, 123456, tzinfo=UTC)

#: The session every instance carries.
SESSION = "s_2026-05-04a"

EVENTS = [
    SessionStart(ts=T, session=SESSION),
    SessionEnd(ts=T_MICROS, session=SESSION, xp_earned=42.5, minutes=31.25),
    Enrolled(ts=T, session=SESSION, course="foundations", reason="gap-fill", return_to="algebra-1"),
    TaskServed(
        ts=T,
        session=SESSION,
        task_id="t-1",
        task_type="multi-step",
        topic="absolute-value",
        kp="kp2",
        problems=[
            ServedProblem(id="p-0", text_hash="0123456789ab", expected_time_secs=35),
            ServedProblem(id="p-1", text_hash="ba9876543210", expected_time_secs=1),
        ],
        component_topics=["adding-integers", "absolute-value"],
        seed=20260504,
    ),
    Attempt(
        ts=T,
        session=SESSION,
        attempt_id="t-1-1",
        task_id="t-1",
        topic="absolute-value",
        kp="kp2",
        task_type="lesson",
        problem=AttemptProblem(text=" |-3| = ? — non-ASCII kept", expected="3"),
        given_answer="3",
        work="|-3| = 3",
        answer_kind="numeric",
        correct=True,
        secs=17,
        error_tags=["notation", "units"],
        work_quality="nearly_perfect",
        grader_note="clean method",
        assisted=True,
    ),
    LessonResult(
        ts=T,
        session=SESSION,
        topic="absolute-value",
        passed=False,
        failed_at_kp="kp3",
        xp=8.924999999999999,
        quality_tier="passable",
        assisted=True,
    ),
    ReviewResult(
        ts=T,
        session=SESSION,
        topic="absolute-value",
        passed=True,
        weighted_score=0.7333333333333333,
        xp=5.0,
        quality_tier="perfect",
        assisted=False,
        task_id="t-1",
    ),
    QuizResult(
        ts=T,
        session=SESSION,
        quiz_id="q-1",
        score=0.75,
        per_topic=[
            QuizTopicResult(topic="absolute-value", correct=True, secs=12),
            QuizTopicResult(topic="not-a-topic", correct=False, secs=0),
        ],
        xp=15.0,
    ),
    RemediationTriggered(
        ts=T,
        session=SESSION,
        kind="lesson-fail",
        source_topic="absolute-value",
        targets=["adding-integers", "absolute-value"],
    ),
    DiagnosticAnswer(ts=T, session=SESSION, topic="absolute-value", correct=False, secs=0, weight=1.0),
    DiagnosticPlaced(
        ts=T,
        session=SESSION,
        balances={"zebra-topic": 2.5, "alpha-topic": -1.0},
        conditional=["alpha-topic"],
        refresh=True,
    ),
    ProfileReset(ts=T, session=SESSION, topics=["absolute-value", "adding-integers"]),
    Regraded(
        ts=T,
        session=SESSION,
        task_id="t-1",
        topic="absolute-value",
        attempts=[
            RegradedAttempt(
                attempt_id="t-1-1",
                work_quality="poor",
                error_tags=["timing-unreliable"],
                grader_note="regraded: method unsound",
            )
        ],
        quality_tier="nearly_passable",
        xp=-3.5,
        reason="grader mis-tiered a wrong method as a pass",
    ),
    AnkiCardCreated(ts=T, session=SESSION, topic="absolute-value", deck="Cadus::Foundations", note_id=170000000001, front_hash="abcdef012345"),
    ConfigChanged(ts=T, session=SESSION, summary="raised the daily goal", git_ref="deadbeef"),
    CurriculumChanged(ts=T, session=SESSION, summary="added a topic", git_ref=None),
]


def main() -> int:
    for event in EVENTS:
        payload = json.loads(event.model_dump_json())
        line = json.dumps(payload, sort_keys=True, separators=(",", ":"), ensure_ascii=False, allow_nan=False)
        print(line)
    return 0


if __name__ == "__main__":
    sys.exit(main())
