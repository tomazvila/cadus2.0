"""Exact semantic corrections applied after symbolic recipe generation."""
from __future__ import annotations

from foundations_curriculum_patch import ExemplarKey, ExemplarPatch
from symbolic_lg_interp import (
    BALL_FEATURE, COST_POINT, MOTION, WHEN,
    REPAIR_RATE, REPAIR_START, TANK_RATE, TANK_START,
)

EXPRESSION_PATCHES: dict[ExemplarKey, ExemplarPatch] = {}

_CANONICAL = " Use canonical standard form $Ax + By = C$ with integer coefficients and $A > 0$."

LINEAR_PATCHES: dict[ExemplarKey, ExemplarPatch] = {
    ExemplarKey("graphing-from-a-table", "kp2", 1): ExemplarPatch(
        problem="The table gives $(1, -2)$, $(2, -1)$, $(3, 0)$. Which point lies on the x-axis?",
        solution_sketch="The rows have constant first differences $\\Delta x = 1$, $\\Delta y = 1$; the row $(3, 0)$ lies on the x-axis.",
    ),
    ExemplarKey("linear-word-problems", "kp1", 1): ExemplarPatch(
        problem="Using the model $y = 2k + 3$, what is the cost of a $7$-kilometre ride?"
    ),
    ExemplarKey("interpreting-graphs-qualitatively", "kp1", 0): ExemplarPatch(
        problem="A distance-time graph is horizontal from $t=2$ to $t=5$. Is the walker standing still, moving at a constant nonzero speed, or speeding up?",
        answer_contract=MOTION,
    ),
    ExemplarKey("interpreting-graphs-qualitatively", "kp1", 1): ExemplarPatch(
        problem="A temperature graph goes downward from left to right. Is the temperature increasing, decreasing, or constant?",
    ),
    ExemplarKey("interpreting-graphs-qualitatively", "kp2", 0): ExemplarPatch(
        problem="On a cost-versus-items graph, $(4,10)$ is shown. Does it mean 4 items cost €10, 10 items cost €4, or each item costs €4?",
        answer_contract=COST_POINT,
    ),
    ExemplarKey("interpreting-graphs-qualitatively", "kp2", 1): ExemplarPatch(
        problem="On a height-versus-time graph of a thrown ball, does the highest point show maximum height, starting height, or landing time?",
        answer_contract=BALL_FEATURE,
    ),
    ExemplarKey("interpreting-graphs-qualitatively", "kp3", 0): ExemplarPatch(
        problem="Runner A and runner B start together, and A has the steeper distance-time line. Is runner A faster, runner B faster, or is their speed equal?",
    ),
    ExemplarKey("interpreting-graphs-qualitatively", "kp3", 1): ExemplarPatch(
        problem="A graph rises steeply at first and then gently. Was growth faster at the start, in the middle, or at the end?",
        answer_contract=WHEN,
    ),
    ExemplarKey("interpreting-linear-models", "kp1", 0): ExemplarPatch(
        problem="For repair cost $C=12h+40$, does $12$ mean cost per hour, fixed fee, or total cost after 12 hours?",
        answer_contract=REPAIR_RATE,
    ),
    ExemplarKey("interpreting-linear-models", "kp1", 1): ExemplarPatch(
        problem="For tank volume $y=-5x+50$, does $-5$ mean drains 5 L per minute, starts with 5 L, or fills 5 L per minute?",
        answer_contract=TANK_RATE,
    ),
    ExemplarKey("interpreting-linear-models", "kp2", 0): ExemplarPatch(
        problem="For tank volume $y=-5x+50$, does $50$ mean starting amount, drain rate, or amount after 50 minutes?",
        answer_contract=TANK_START,
    ),
    ExemplarKey("interpreting-linear-models", "kp2", 1): ExemplarPatch(
        problem="For repair cost $C=12h+40$, does $40$ mean fixed fee, hourly rate, or cost after 40 hours?",
        answer_contract=REPAIR_START,
    ),
    ExemplarKey("slope-from-a-graph", "kp2", 0): ExemplarPatch(
        solution_sketch="Falling left to right is a negative slope."
    ),
}

# Every standard-form item that uses a literal canonical answer says so to the learner.
for kp in ("kp1", "kp3"):
    for index in range(4):
        key = ExemplarKey("point-slope-standard-form", kp, index)
        # Existing text is retained below by explicit prompt replacements.
        prompts = {
            ("kp1", 0): "Write $y = 2x + 3$ in standard form.",
            ("kp1", 1): "Write $y - 1 = \\frac{1}{2}(x - 4)$ in standard form.",
            ("kp1", 2): "Write $y = 3x + 2$ in standard form.",
            ("kp1", 3): "Write $y - 3 = \\frac{1}{2}(x - 2)$ in standard form.",
            ("kp3", 0): "Write the line through $(1, 2)$ and $(3, 8)$ in standard form.",
            ("kp3", 1): "Write the line through $(2, 5)$ and $(4, 6)$ in standard form.",
            ("kp3", 2): "Write the line through $(2, 3)$ and $(4, 9)$ in standard form.",
            ("kp3", 3): "Write the line through $(1, 4)$ and $(3, 5)$ in standard form.",
        }
        LINEAR_PATCHES[key] = ExemplarPatch(problem=prompts[(kp, index)] + _CANONICAL)

UNION = '{"kind":"inequality_union"}'
SYSTEMS_PATCHES: dict[ExemplarKey, ExemplarPatch] = {
    ExemplarKey("substitution-with-isolated-variable", "kp3", 1): ExemplarPatch(
        problem="Solve the system $y = 4x$, $x + y = 15$ and check your answer.",
        solution_sketch="$x + 4x = 15$; $5x = 15$; $x = 3$; $y = 12$; check $3 + 12 = 15$.",
    ),
    ExemplarKey("elimination-with-addition", "kp3", 1): ExemplarPatch(
        problem="Solve the system $x + y = 6$, $x - y = 8$ and check.",
        solution_sketch="Add: $2x = 14$; $x = 7$; $y = -1$; check $7 + (-1) = 6$ and $7 - (-1) = 8$.",
    ),
    ExemplarKey("systems-of-linear-inequalities", "kp2", 1): ExemplarPatch(
        problem="For the system $y \\le -x + 5$, $y > 2x - 3$, what is the solution region?",
        solution_sketch="One inclusive boundary ($\\le$), one strict ($>$); a point solves the system only in the overlap.",
    ),
    ExemplarKey("systems-mixture-problems", "kp1", 0): ExemplarPatch(
        problem="Mixing $x$ L of $10\\%$ acid with $y$ L of $50\\%$ acid to get $15$ L of $30\\%$ acid: give both equations.",
        answer="x + y = 15 and 0.10x + 0.50y = 4.5",
        solution_sketch="Amounts sum to $15$; pure acid sums to $0.30(15) = 4.5$.",
        answer_contract='{"kind":"label","options":[["x + y = 15 and 0.10x + 0.50y = 4.5"],["x + y = 15 and 0.10x + 0.50y = 15"],["x + y = 4.5 and 0.10x + 0.50y = 15"]]}',
    ),
    ExemplarKey("systems-mixture-problems", "kp1", 2): ExemplarPatch(
        problem="Mixing $x$ L of $20\\%$ acid with $y$ L of $80\\%$ acid to get $10$ L of $50\\%$ acid: give both equations.",
        answer="x + y = 10 and 0.20x + 0.80y = 5",
        solution_sketch="Amounts sum to $10$; pure acid sums to $0.50(10) = 5$.",
        answer_contract='{"kind":"label","options":[["x + y = 10 and 0.20x + 0.80y = 5"],["x + y = 10 and 0.20x + 0.80y = 10"],["x + y = 5 and 0.20x + 0.80y = 10"]]}',
    ),
    ExemplarKey("systems-word-problems", "kp3", 1): ExemplarPatch(
        problem="Sam is $3$ times as old as Tia and their ages sum to $32$. How old is each?",
        answer="Sam 24, Tia 8",
        solution_sketch="$s = 3t$, $s + t = 32$; $4t = 32$; $t = 8$, $s = 24$.",
        answer_contract='{"kind":"label","options":[["Sam 24, Tia 8"],["Sam 8, Tia 24"],["Sam 21, Tia 11"]]}',
    ),
    ExemplarKey("basic-absolute-value-inequalities", "kp1", 0): ExemplarPatch(
        solution_sketch="One bounded interval within $5$ units of $0$: $-5 < x < 5$."
    ),
    ExemplarKey("interval-notation", "kp1", 2): ExemplarPatch(
        problem="Write $1 < x \\le 6$ in interval notation.",
        answer="(1, 6]",
        solution_sketch="Strict at $1$ is open; inclusive at $6$ is closed.",
        answer_contract=UNION,
    ),
}
for topic_kp, sketches in {
    ("systems-elimination", "kp1"): [
        "Multiply 2nd by $-2$: $-2x - 2y = -10$; add: $y = 2$; $x = 3$.",
        "Multiply 1st by $-3$: $-3x - 6y = -33$; add 2nd: $-7y = -28$; $y = 4$; $x = 3$.",
        "Multiply 2nd by $-2$: $-2x - 2y = -12$; add: $x = 4$; $y = 2$.",
        "Multiply 1st by $-2$: $-2x - 6y = -26$; add 2nd: $-7y = -21$; $y = 3$; $x = 4$.",
    ],
}.items():
    for index, sketch in enumerate(sketches):
        SYSTEMS_PATCHES[ExemplarKey(*topic_kp, index)] = ExemplarPatch(solution_sketch=sketch)

for kp in ("kp1", "kp2", "kp3"):
    for index in range(4):
        key = ExemplarKey("interval-notation", kp, index)
        old = SYSTEMS_PATCHES.get(key, ExemplarPatch())
        SYSTEMS_PATCHES[key] = ExemplarPatch(
            problem=old.problem,
            answer=old.answer,
            solution_sketch=old.solution_sketch,
            answer_contract=UNION,
        )
