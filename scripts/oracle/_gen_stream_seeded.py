"""Stream #2 onward of `gen_stream_1_0.py`: the seeded coverage stream.

One stream per seed that reaches every event type, every reachable FIRe branch,
both lesson-close XP paths, the diagnostic initial and refresh paths,
`profile_reset`, the rounding edges, the quiz threshold, the streak edges in a
non-UTC time zone, and several corrections (spec section 9). The seed varies
the prerequisite chain, the tiers, the gaps, the balances, and the corrected
tasks; the phases are the same for every seed.

Import this module after the 1.0 loader is pointed at its trees: it imports the
1.0 models at load time.
"""

from __future__ import annotations

import math
import random
from datetime import datetime, timedelta

from _gen_stream_base import CHAIN_LENGTH, COURSE, COVERAGE_ORIGIN, SEED, problem_text
from _gen_stream_phases_a import phase_a, phase_b, phase_c, phase_d_e, phase_f_g
from _gen_stream_phases_b import phase_h_i_j, phase_k, phase_l, phase_m, phase_n, phase_o
from cadus.model import (
    Attempt,
    AttemptProblem,
    LessonResult,
    QuizResult,
    Regraded,
    ReviewResult,
    ServedProblem,
    TaskServed,
    WorkQuality,
)
from cadus.projector import problem_text_hash

#: The phases of the stream, in order.
PHASES = (
    phase_a,
    phase_b,
    phase_c,
    phase_d_e,
    phase_f_g,
    phase_h_i_j,
    phase_k,
    phase_l,
    phase_m,
    phase_n,
    phase_o,
)

#: The event types that carry XP into the grand total.
PRICED_TYPES = (LessonResult, ReviewResult, QuizResult)

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


class _SeededStream:
    """The builder of one seeded stream.

    The phases run in order and draw from one rng, so the stream is a pure
    function of the seed and of the constants of this module.
    """

    def __init__(self, seed: int, cfg, graph) -> None:
        self.seed = seed
        self.cfg = cfg
        self.graph = graph
        self.rng = random.Random(1_000_003 * seed + 7)
        self.events: list[object] = []

        course_topics = sorted(graph.topics_in_course(COURSE))
        top, step_a, step_b, step_c, deep = self.rng.choice(
            prerequisite_chains(graph, COURSE)
        )
        self.chain = [top, step_a, step_b, step_c, deep]
        self.top, self.step_a, self.step_b = top, step_a, step_b
        self.step_c, self.deep = step_c, deep
        self.spares = self.rng.sample(
            [tid for tid in course_topics if tid not in set(self.chain)], 8
        )
        self.other_courses = sorted(
            course.id for course in graph.catalog.courses if course.id != COURSE
        )
        self.tuner_index = 0

    # ---- the event builders ------------------------------------------------ #

    def at(self, day: int, hour: int, minute: int = 0) -> datetime:
        """The instant `day` days and `hour:minute` after the origin."""
        return COVERAGE_ORIGIN + timedelta(days=day, hours=hour, minutes=minute)

    def kps(self, tid: str) -> list[str]:
        """The knowledge point ids of a topic."""
        return [kp.id for kp in self.graph.topics[tid].knowledge_points]

    def served(self, ts, session, task_id, task_type, topic, kp=None, count=2) -> None:
        """Append a `task_served` event with `count` problems."""
        self.events.append(
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
                        expected_time_secs=self.graph.topics[topic].expected_time_secs,
                    )
                    for n in range(count)
                ],
                seed=SEED + self.seed * 100 + len(self.events),
            )
        )

    def attempt(
        self, ts, session, task_id, topic, index, task_type, correct, tier, kp=None, assisted=False
    ) -> None:
        """Append an `attempt` event with a seeded duration and error tag."""
        self.events.append(
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
                secs=self.rng.randrange(12, 240),
                error_tags=[] if correct else [self.rng.choice(["sign-error", "notation"])],
                work_quality=WorkQuality(tier),
                grader_note=None,
                assisted=assisted,
            )
        )

    def review(self, ts, session, topic, passed, tier, xp, assisted=False, task_id=None) -> None:
        """Append a `review_result` event with a seeded weighted score."""
        self.events.append(
            ReviewResult(
                ts=ts,
                session=session,
                topic=topic,
                passed=passed,
                weighted_score=self.rng.choice([0.0, 7 / 15, 0.65, 11 / 15, 1.0]),
                xp=xp,
                quality_tier=WorkQuality(tier),
                assisted=assisted,
                task_id=task_id,
            )
        )

    def lesson_result(self, ts, session, topic, passed, failed_at_kp, xp, tier) -> None:
        """Append a `lesson_result` event."""
        self.events.append(
            LessonResult(
                ts=ts,
                session=session,
                topic=topic,
                passed=passed,
                failed_at_kp=failed_at_kp,
                xp=xp,
                quality_tier=WorkQuality(tier),
                assisted=False,
            )
        )

    def regraded(self, ts, session, task_id, topic, attempts, tier, xp, reason) -> None:
        """Append a `regraded` event."""
        self.events.append(
            Regraded(
                ts=ts,
                session=session,
                task_id=task_id,
                topic=topic,
                attempts=attempts,
                quality_tier=WorkQuality(tier),
                xp=xp,
                reason=reason,
            )
        )

    def priced_total(self) -> float:
        """The XP sum over every priced event."""
        return sum(event.xp for event in self.events if isinstance(event, PRICED_TYPES))

    def price_tuner(self) -> None:
        """Fill the tuner slot, so the grand XP total lands on an even-half tie."""
        priced = self.priced_total()
        target = _even_half(priced)
        delta = target - priced
        self.events[self.tuner_index] = LessonResult(
            ts=self.at(420, 9),
            session=None,
            topic=self.spares[4],
            passed=False,
            failed_at_kp=self.kps(self.spares[4])[0],
            xp=delta,
            quality_tier=WorkQuality.blowoff,
            assisted=False,
        )
        total = self.priced_total()
        seed = self.seed
        assert total == target, f"seed {seed}: the XP total {total!r} is not {target!r}"
        assert math.floor(total) % 2 == 0, f"seed {seed}: {total!r} does not round half-even down"

    def build(self) -> list[object]:
        """Run every phase in order and return the events, sorted by timestamp."""
        for phase in PHASES:
            phase(self)
        self.price_tuner()
        self.events.sort(key=lambda event: event.ts)
        return self.events


def build_seeded_stream(seed: int, cfg, graph) -> list[object]:
    """The spec-section-9 coverage stream for ``seed`` (``seed >= 2``).

    The phases are the coverage skeleton; the seed varies the chain, the spare
    topics, the tiers, the balance order, and the corrected tasks."""
    return _SeededStream(seed, cfg, graph).build()
