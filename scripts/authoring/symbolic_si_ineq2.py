"""Reviewed inequality and interval exemplars.

Interval notation and unions use the production inequality_union contract,
which compares mathematical sets and catches endpoint direction and openness.
"""
from __future__ import annotations

from foundations_curriculum_patch import ExemplarKey
from symbolic_common import label_contract, mk

KP = tuple
UNION = '{"kind":"inequality_union"}'
YES_NO = label_contract("yes", "no")

NEW: dict[KP, list] = {
    ("and-or-inequalities", "kp1"): [
        mk("Is $x = 2$ in the solution set of $-1 < x < 6$?", "yes",
           "$2$ lies between $-1$ and $6$.", contract=YES_NO),
        mk("Is $x = -3$ a solution of $x < -5$ or $x > 2$?", "no",
           "$-3$ satisfies neither piece.", contract=YES_NO),
    ],
    ("and-or-inequalities", "kp2"): [
        mk("Solve $-2 < x + 4 < 9$.", "-6 < x < 5",
           "Subtract $4$ from all three parts."),
        mk("Solve $4 \\le 2x \\le 16$.", "2 <= x <= 8",
           "Divide all three parts by $2$."),
    ],
    ("and-or-inequalities", "kp3"): [
        mk("Solve $x - 2 < -3$ or $x - 2 > 4$.", "x < -1 or x > 6",
           "$x < -1$ or $x > 6$.", contract=UNION),
        mk("Solve $3x \\le -9$ or $x + 2 > 6$.", "x <= -3 or x > 4",
           "$x \\le -3$ or $x > 4$.", contract=UNION),
    ],
}

CONTRACTS: dict[ExemplarKey, str] = {
    ExemplarKey("and-or-inequalities", "kp3", 0): UNION,
    ExemplarKey("and-or-inequalities", "kp3", 1): UNION,
    ExemplarKey("inequality-word-problems", "kp1", 0): label_contract("at most 5", "at least 5", "exactly 5"),
    ExemplarKey("inequality-word-problems", "kp1", 1): label_contract("at most 6 km", "at least 6 km", "exactly 6 km"),
    ExemplarKey("inequality-word-problems", "kp2", 0): label_contract("at least 9", "at most 9", "exactly 9"),
    ExemplarKey("inequality-word-problems", "kp2", 1): label_contract(
        "more than 7 weeks (so at least 8 full weeks)", "at most 7 weeks", "exactly 7 weeks"
    ),
}

NEW[("compound-inequalities", "kp1")] = [
    mk("Solve $-3 \\le 2x + 1 < 9$.", "-2 <= x < 4",
       "Subtract $1$: $-4 \\le 2x < 8$; divide by $2$."),
    mk("Solve $5 < 3x - 1 \\le 14$.", "2 < x <= 5",
       "Add $1$: $6 < 3x \\le 15$; divide by $3$."),
]
NEW[("compound-inequalities", "kp2")] = [
    mk("Solve $-8 < -2x \\le 6$.", "-3 <= x < 4",
       "Divide by $-2$ and reverse both signs."),
    mk("Solve $2 \\le 6 - x < 9$.", "-3 < x <= 4",
       "Subtract $6$: $-4 \\le -x < 3$; multiply by $-1$."),
]
NEW[("compound-inequalities", "kp3")] = [
    mk("Solve $2x + 3 < -7$ or $3x - 1 > 8$.", "x < -5 or x > 3",
       "$2x < -10$ gives $x < -5$; $3x > 9$ gives $x > 3$.", contract=UNION),
    mk("Solve $5 - x \\ge 8$ or $2x \\ge 10$.", "x <= -3 or x >= 5",
       "$-x \\ge 3$ flips to $x \\le -3$; $x \\ge 5$.", contract=UNION),
]
CONTRACTS[ExemplarKey("compound-inequalities", "kp3", 0)] = UNION
CONTRACTS[ExemplarKey("compound-inequalities", "kp3", 1)] = UNION

NEW[("interval-notation", "kp1")] = [
    mk("Write $-3 \\le x < 4$ in interval notation.", "[-3, 4)",
       "Inclusive at $-3$ gives a square bracket; strict at $4$ gives a round bracket.",
       contract=UNION),
]
for index in (0, 1):
    CONTRACTS[ExemplarKey("interval-notation", "kp1", index)] = UNION

NEW[("interval-notation", "kp2")] = [
    mk("Write $(-\\infty, 3]$ as an inequality.", "x <= 3",
       "The ray extends left and includes $3$.", contract=UNION),
]
for index in range(3):
    CONTRACTS[ExemplarKey("interval-notation", "kp2", index)] = UNION

NEW[("interval-notation", "kp3")] = [
    mk("Solve $x + 1 \\le -4$ or $2x - 3 > 5$ and give the answer in interval notation.",
       "(-∞, -5] ∪ (4, ∞)",
       "$x \\le -5$ or $x > 4$, so $(-\\infty,-5] \\cup (4,\\infty)$.",
       contract=UNION),
]
for index in range(3):
    CONTRACTS[ExemplarKey("interval-notation", "kp3", index)] = UNION


AT_MOST_8 = label_contract(["at most 8"])
AT_MOST_9KM = label_contract(["at most 9 km"])
AT_LEAST_10 = label_contract(["at least 10"])
MORE_THAN_5W = label_contract(["more than 5 weeks (so at least 6 full weeks)"])

NEW[("inequality-word-problems", "kp1")] = [
    mk(
        "Souvenirs cost €6 each plus a flat €14 shipping fee. You can spend at most "
        "€62. How many souvenirs can you buy?",
        "at most 8", "$6x + 14 \\le 62$; $x \\le 8$.", contract=AT_MOST_8,
    ),
    mk(
        "A courier charges €4 plus €3 per kilometre. What distances keep the fare at "
        "most €31?",
        "at most 9 km", "$3k + 4 \\le 31$; $k \\le 9$.", contract=AT_MOST_9KM,
    ),
]
NEW[("inequality-word-problems", "kp2")] = [
    mk(
        "Your first three marks are $7$, $8$, and $9$ on the $0$-$10$ scale. What must "
        "you score on the fourth to average at least $8.5$?",
        "at least 10", "$\\frac{24 + x}{4} \\ge 8.5$; $24 + x \\ge 34$; $x \\ge 10$.",
        contract=AT_LEAST_10,
    ),
    mk(
        "You have €80 and save €15 per week. After how many weeks will you have more "
        "than €155?",
        "more than 5 weeks (so at least 6 full weeks)", "$80 + 15w > 155$; $w > 5$.",
        contract=MORE_THAN_5W,
    ),
]
NEW[("inequality-word-problems", "kp3")] = [
    mk(
        "Rides cost €3 each plus €5 entry, and you have €23. How many rides can you go "
        "on?",
        "6", "$3r + 5 \\le 23$; $r \\le 6$.",
    ),
    mk(
        "A bus seats at most $50$ people and $32$ seats are taken. How many more "
        "people can board?",
        "18", "$32 + n \\le 50$; $n \\le 18$.",
    ),
]

NEW[("basic-absolute-value-inequalities", "kp1")] = [
    mk(
        "Solve $|x| < 8$.", "-8 < x < 8",
        "One bounded interval within $8$ units of $0$: $-8 < x < 8$.",
    ),
    mk("Solve $|x| \\le 6$.", "-6 <= x <= 6", "Inclusive at both boundaries."),
]
NEW[("basic-absolute-value-inequalities", "kp2")] = [
    mk(
        "Solve $|x| > 5$.", "x < -5 or x > 5", "Points more than $5$ from zero.", contract=UNION,
    ),
    mk(
        "Solve $|x| \\ge 3$.", "x <= -3 or x >= 3", "Points at least $3$ from zero.", contract=UNION,
    ),
]
CONTRACTS[ExemplarKey("basic-absolute-value-inequalities", "kp2", 0)] = UNION
CONTRACTS[ExemplarKey("basic-absolute-value-inequalities", "kp2", 1)] = UNION

NO_SOLUTION = label_contract(["no solution"])
ALL_REALS = label_contract(["all real numbers"])
NEW[("basic-absolute-value-inequalities", "kp3")] = [
    mk(
        "Solve $|x| \\le -4$.", "no solution", "An absolute value is never negative.",
        contract=NO_SOLUTION,
    ),
    mk(
        "Solve $|x| > -6$.", "all real numbers", "$|x| \\ge 0 > -6$ always.",
        contract=ALL_REALS,
    ),
]
CONTRACTS[ExemplarKey("basic-absolute-value-inequalities", "kp3", 0)] = NO_SOLUTION
CONTRACTS[ExemplarKey("basic-absolute-value-inequalities", "kp3", 1)] = ALL_REALS

NEW[("absolute-value-inequalities", "kp1")] = [
    mk(
        "Solve $|x - 5| < 4$.", "1 < x < 9", "$-4 < x - 5 < 4$; add $5$.",
    ),
    mk(
        "Solve $|3x + 1| \\le 10$.", "-11/3 <= x <= 3",
        "$-10 \\le 3x + 1 \\le 10$; subtract $1$, divide by $3$.",
    ),
]
NEW[("absolute-value-inequalities", "kp2")] = [
    mk(
        "Solve $|3x - 2| \\ge 7$.", "x >= 3 or x <= -5/3",
        "$3x - 2 \\ge 7$ or $3x - 2 \\le -7$.", contract=UNION,
    ),
    mk(
        "Solve $|x + 6| > 3$.", "x > -3 or x < -9", "$x + 6 > 3$ or $x + 6 < -3$.", contract=UNION,
    ),
]
CONTRACTS[ExemplarKey("absolute-value-inequalities", "kp2", 0)] = UNION
CONTRACTS[ExemplarKey("absolute-value-inequalities", "kp2", 1)] = UNION

NEW[("absolute-value-inequalities", "kp3")] = [
    mk(
        "Solve $3|x - 2| + 1 \\le 13$.", "-2 <= x <= 6",
        "$|x - 2| \\le 4$; $-4 \\le x - 2 \\le 4$.",
    ),
    mk(
        "Solve $2|x| - 3 > 5$.", "x < -4 or x > 4", "$|x| > 4$.", contract=UNION,
    ),
]
CONTRACTS[ExemplarKey("absolute-value-inequalities", "kp3", 1)] = UNION

DASHED_SOLID = label_contract("dashed", "solid")
NEW[("graphing-linear-inequalities", "kp1")] = [
    mk(
        "For $y < 3x - 2$, is the boundary line solid or dashed?", "dashed",
        "Strict inequality ($<$) $\\to$ dashed.", contract=DASHED_SOLID,
    ),
    mk(
        "For $y \\ge -2x + 1$, is the boundary line solid or dashed?", "solid",
        "Inclusive inequality ($\\ge$) $\\to$ solid.",
        contract=DASHED_SOLID,
    ),
]
CONTRACTS[ExemplarKey("graphing-linear-inequalities", "kp1", 0)] = DASHED_SOLID
CONTRACTS[ExemplarKey("graphing-linear-inequalities", "kp1", 1)] = DASHED_SOLID

NEW[("graphing-linear-inequalities", "kp2")] = [
    mk(
        "Is $(1, 1)$ a solution of $y < 2x + 3$?", "yes", "$1 < 2 + 3 = 5$ is true.",
        contract=YES_NO,
    ),
    mk(
        "Is $(0, -1)$ a solution of $y \\ge x - 4$?", "yes", "$-1 \\ge -4$ is true.",
        contract=YES_NO,
    ),
]
NEW[("graphing-linear-inequalities", "kp3")] = [
    mk(
        "For $3x + 2y \\le 12$, does the shaded region include the origin?", "yes",
        "$0 \\le 12$ is true.", contract=YES_NO,
    ),
    mk(
        "For $y > x + 2$, does the shaded region include the origin?", "no",
        "$0 > 2$ is false.", contract=YES_NO,
    ),
]
