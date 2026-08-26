#!/usr/bin/env python
"""M3 parity oracle: record the 1.0 `compose_session` plan of 10 seeded states.

Read-only against the 1.0 code base. For each seed `k` in 1..10 it folds
`stream_k.jsonl` with the 1.0 projector, calls the 1.0 `compose_session` on the
resulting state, and prints the served task list as canonical JSON.

The recorded shape per task is
`[task_type, topic, is_remediation, nearly_due, n_problems, time_budget_secs]`.
`crates/core/tests/parity_events.rs` reads the committed output and asserts the
2.0 `compose_session` gives the same list.

**The cap is 40, not the session cap 8.** With `n = 8` every seeded state fills
the plan with remediation, review, and lesson tasks, so the quiz, multi-step, and
drill sections of `compose_session` end after the compared window and no
assertion reaches them (M3 review round 1, finding #13). A cap of 40 is above the
longest recorded plan, so the whole plan of every state is compared.

The context is exactly what BOTH sides derive from the same committed stream, so
neither side needs a value the other cannot rebuild:

* `states` -- the folded `LearnerModel.topics`,
* `t` -- the last event's timestamp (the fold's own `t_ref`),
* `course_id` -- the `course` of the last `enrolled` event,
* `pending_remediation` -- the folded queue,
* `quiz_state` -- the folded quiz cadence,
* `session_id` -- `"s{k}"`, `n` -- 40, `rng` -- `seeded_rng(k)`.

Every other keyword keeps its 1.0 default, which is the 2.0
`SessionContext::default()` value.

**Trap T11.** The quiz sampler is a documented non-parity: 2.0 does not
reproduce the CPython `random.sample` sequence. A quiz task therefore records
`null` for its topic, its `n_problems`, and its `time_budget_secs` -- each of the
three reads the sampled questions -- and the test compares a quiz row by presence
only.

Usage:
  dump_selector_1_0.py [--fixtures DIR] [--out FILE] [--seeds N]
                       [--curriculum DIR] [--config FILE] [--n N]
"""

from __future__ import annotations

import argparse
import json
import os
from datetime import UTC, datetime

#: The task count `compose_session` is capped at. It is above the longest
#: recorded plan, so the comparison covers the whole plan (finding #13).
DEFAULT_N = 40

#: The number of seeded states: seed `k` folds `stream_k.jsonl`.
DEFAULT_SEEDS = 10

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


def canonical(obj: object) -> str:
    """Canonical JSON: sorted keys, compact separators, UTF-8, no NaN."""
    return json.dumps(
        obj, sort_keys=True, separators=(",", ":"), ensure_ascii=False, allow_nan=False
    )


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--fixtures", default=FIXTURES)
    ap.add_argument("--out", default=None)
    ap.add_argument("--seeds", type=int, default=DEFAULT_SEEDS)
    ap.add_argument("--n", type=int, default=DEFAULT_N)
    ap.add_argument("--curriculum", default="/home/deploy/dev/cadus2.0/curriculum")
    ap.add_argument("--config", default="/home/deploy/dev/cadus/config.yaml")
    ap.add_argument("--now", default="2000-01-01T00:00:00+00:00")
    ap.add_argument("--goal", type=int, default=40)
    args = ap.parse_args()

    os.environ["CADUS_CURRICULUM"] = args.curriculum
    os.environ["CADUS_CONFIG"] = args.config

    from cadus.events import seeded_rng, validate_event
    from cadus.loader import load_config, load_graph
    from cadus.projector import PROJECTOR_VERSION, config_hash, project
    from cadus.selector import compose_session

    cfg = load_config()
    graph = load_graph()
    now = datetime.fromisoformat(args.now)
    if now.tzinfo is None:
        now = now.replace(tzinfo=UTC)

    states = []
    for seed in range(1, args.seeds + 1):
        name = f"stream_{seed}.jsonl"
        events = []
        with open(os.path.join(args.fixtures, name), encoding="utf-8") as handle:
            for line in handle:
                line = line.strip()
                if line:
                    events.append(validate_event(json.loads(line)))

        model = project(events, graph, cfg, now=now, tz=None, goal=args.goal)

        # `t` is the last event's timestamp -- the same `t_ref` the fold used, so
        # both sides read one instant off the committed stream.
        last_ts = max(event.ts for event in events)
        if last_ts.tzinfo is None:
            last_ts = last_ts.replace(tzinfo=UTC)

        course_id = None
        for event in events:
            if event.type == "enrolled":
                course_id = event.course

        plan = compose_session(
            model.topics,
            graph,
            cfg,
            last_ts,
            seeded_rng(seed),
            session_id=f"s{seed}",
            course_id=course_id,
            pending_remediation=model.pending_remediation,
            quiz_state=model.quiz,
            n=args.n,
        )

        def is_quiz(task) -> bool:
            """Whether the task is the sampled quiz (trap T11)."""
            return task.task_type.value == "quiz"

        def topic_id(task) -> str | None:
            """The task's topic id, or `None` for a quiz (trap T11) or a topicless task."""
            if is_quiz(task) or task.topic is None:
                return None
            return task.topic.id

        def payload(task, value) -> int | None:
            """A task size field, or `None` for a quiz (trap T11).

            `n_problems` and `time_budget_secs` both read the sampled quiz
            questions, so a quiz row carries neither.
            """
            if is_quiz(task) or value is None:
                return None
            return int(value)

        tasks = [
            [
                task.task_type.value,
                topic_id(task),
                bool(task.is_remediation),
                bool(task.nearly_due),
                payload(task, task.n_problems),
                payload(task, task.time_budget_secs),
            ]
            for task in plan.tasks
        ]
        states.append(
            {
                "seed": seed,
                "stream": name,
                "session_id": f"s{seed}",
                "course_id": course_id,
                "t": last_ts.isoformat().replace("+00:00", "Z"),
                "tasks": tasks,
            }
        )
        print(f"{name}: seed {seed} -> {len(tasks)} tasks")

    index = {
        "oracle": "scripts/oracle/dump_selector_1_0.py",
        "n": args.n,
        "now": args.now,
        "goal": args.goal,
        "projector_version": PROJECTOR_VERSION,
        "config_hash": config_hash(cfg),
        "task_shape": [
            "task_type",
            "topic",
            "is_remediation",
            "nearly_due",
            "n_problems",
            "time_budget_secs",
        ],
        "quiz_topic_is_null": "trap T11: the quiz sample is a documented non-parity",
        "states": states,
    }
    out = args.out or os.path.join(args.fixtures, "selector_1_0.json")
    with open(out, "w", encoding="utf-8") as handle:
        handle.write(json.dumps(index, indent=2, sort_keys=True) + "\n")
    print(f"wrote {out}")
    print(f"canonical_sha_input_len={len(canonical(index))}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
