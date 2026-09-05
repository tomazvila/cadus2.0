#!/usr/bin/env python
"""M3 parity oracle: the 1.0 selector answers AT its constant boundaries.

Read-only against the 1.0 code base. It builds the smallest graph that reads one
selector constant, calls the 1.0 selector on each side of that constant, and
prints the answer. `crates/core/tests/selector.rs` carries every printed value as
a literal (M3 review round 1, findings #8, #9, #10):

* `DRILL_MASTERY_ABILITY` (0.95) -- abilities 0.949, 0.95, 0.951,
* `DRILL_INTERVAL_DAYS` (3.5) -- drill gaps of 3.49, 3.5, and 3.51 days,
* `QUIZ_RECENT_DAYS` (14) -- topics learned 13, 14, and 15 days ago,
* trap T1 at `selector.py:602` -- a knockout mass of ten 0.1 weights, whose
  CPython `sum()` is exactly 1.0 and whose naive total is 0.9999999999999999.

`t` is the `T = datetime(2026, 7, 14, 12, 0, 0)` of the 1.0 selector tests, so
no wall clock enters the answers.

Usage:
  dump_selector_boundaries_1_0.py [--curriculum DIR] [--config FILE]
"""

from __future__ import annotations

import argparse
from datetime import UTC, datetime, timedelta

from _common import add_code_base_arguments, point_at_code_base

#: `T` of the 1.0 selector tests, as an aware UTC instant (trap T8).
T = datetime(2026, 7, 14, 12, 0, 0, tzinfo=UTC)

#: The seeds the quiz probe samples with. The strata must not move with the seed.
QUIZ_SEEDS = [7, 42]

#: The knockout weights: ten tenths, which CPython sums to exactly 1.0.
KNOCKOUT_WEIGHT = 0.1
KNOCKOUT_TARGETS = 10


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    add_code_base_arguments(ap)
    args = ap.parse_args()
    point_at_code_base(args)

    from cadus.events import seeded_rng
    from cadus.graph import Graph
    from cadus.model import (
        AnswerKind,
        Config,
        Course,
        CourseCatalog,
        Exemplar,
        KnowledgePoint,
        PrereqEdge,
        Topic,
        TopicState,
        TopicStatus,
        Unit,
    )
    from cadus.selector import (
        DRILL_INTERVAL_DAYS,
        DRILL_MASTERY_ABILITY,
        QUIZ_RECENT_DAYS,
        importance,
        quiz_composer,
        schedule_drills,
    )

    cfg = Config()

    def topic(tid, prereqs=None, *, drill=False, core=True):
        """The 1.0 `_topic` builder of `tests/test_selector.py`."""
        return Topic(
            id=tid,
            name=tid,
            core=core,
            difficulty=0.3,
            drill=drill,
            answer_kind=AnswerKind.numeric,
            expected_time_secs=30,
            prerequisites=[PrereqEdge(id=p, weight=w, key=False) for (p, w) in (prereqs or [])],
            knowledge_points=[
                KnowledgePoint(id="kp1", name="kp1", exemplars=[Exemplar(problem="p", answer="a")])
            ],
        )

    def graph_of(topics):
        """The 1.0 `_graph` builder: one course, one module."""
        unit = Unit(unit="u", course="c", module="M", topics=topics)
        catalog = CourseCatalog(courses=[Course(id="c", name="c", order=0)])
        return Graph(catalog, [unit])

    def learned(memory, ability):
        """The 1.0 `_learned(memory)` state, read at `T`."""
        return TopicState(
            status=TopicStatus.learning,
            repNum=3.0,
            memoryBase=memory,
            t0=T,
            interval_days=10.0,
            ability=ability,
            speed=1.0,
        )

    print(f"DRILL_MASTERY_ABILITY = {DRILL_MASTERY_ABILITY!r}")
    print(f"DRILL_INTERVAL_DAYS = {DRILL_INTERVAL_DAYS!r}")
    print(f"QUIZ_RECENT_DAYS = {QUIZ_RECENT_DAYS!r}")

    drill_graph = graph_of([topic("d", drill=True)])
    for ability in (0.949, 0.95, 0.951):
        states = {"d": learned(0.9, ability)}
        out = schedule_drills(states, drill_graph, cfg, T, None)
        print(f"schedule_drills ability {ability!r} -> {out}")

    states = {"d": learned(0.9, 0.8)}
    for gap in (3.49, 3.5, 3.51):
        last = {"d": T - timedelta(days=gap)}
        out = schedule_drills(states, drill_graph, cfg, T, last)
        print(f"schedule_drills gap {gap!r} days -> {out}")

    quiz_ids = ["q13", "q14", "q15"]
    quiz_graph = graph_of([topic(tid) for tid in quiz_ids])
    quiz_states = {tid: learned(0.9, 0.8) for tid in quiz_ids}
    learned_at = {
        "q13": T - timedelta(days=13),
        "q14": T - timedelta(days=14),
        "q15": T - timedelta(days=15),
    }
    for seed in QUIZ_SEEDS:
        plan = quiz_composer(
            quiz_states, quiz_graph, cfg, T, seeded_rng(seed), learned_at=learned_at
        )
        rows = sorted((q.topic, q.stratum, q.time_budget_secs) for q in plan.questions)
        print(f"quiz_composer seed {seed} -> {rows}")

    leaves = [f"leaf-{index}" for index in range(KNOCKOUT_TARGETS)]
    mass_topics = [topic(leaf) for leaf in leaves]
    mass_topics.append(
        topic("lesson-topic", [(leaf, KNOCKOUT_WEIGHT) for leaf in leaves], core=False)
    )
    mass_graph = graph_of(mass_topics)
    value = importance("lesson-topic", mass_graph, set(leaves), set(mass_graph.topics))
    naive = 0.0
    for _ in range(KNOCKOUT_TARGETS):
        naive += KNOCKOUT_WEIGHT
    print(f"importance of ten {KNOCKOUT_WEIGHT!r} weights -> {value!r} (naive {naive!r})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
