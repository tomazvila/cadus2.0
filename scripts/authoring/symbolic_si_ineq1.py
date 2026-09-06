"""New held-out exemplars and contract retrofits for the basic inequality
topics of `curriculum/foundations/05-systems-inequalities.yaml`.
"""
from __future__ import annotations

from foundations_curriculum_patch import ExemplarKey
from symbolic_common import label_contract, mk

KP = tuple

TRUE_FALSE = label_contract("true", "false")
YES_NO = label_contract("yes", "no")

NEW: dict[KP, list] = {
    ("solutions-of-inequalities", "kp1"): [
        mk(
            "Is the statement $-7 < -2$ true or false?", "true",
            "$-7$ is to the left of $-2$ on the number line.", contract=TRUE_FALSE,
        ),
        mk(
            "True or false: $6 \\ge 9$?", "false", "$6$ is less than $9$.",
            contract=TRUE_FALSE,
        ),
    ],
    ("solutions-of-inequalities", "kp2"): [
        mk(
            "Is $x = 4$ a solution of $3x - 2 > 8$?", "yes", "$3(4) - 2 = 10 > 8$.",
            contract=YES_NO,
        ),
        mk(
            "Is $x = 1$ a solution of $6 - x < 4$?", "no", "$6 - 1 = 5$, and $5 < 4$ is "
            "false.", contract=YES_NO,
        ),
    ],
    ("solutions-of-inequalities", "kp3"): [
        mk(
            "Is $x = 5$ a solution of $x + 1 \\le 6$?", "yes", "$6 \\le 6$ is true.",
            contract=YES_NO,
        ),
        mk(
            "Is $x = 5$ a solution of $x + 1 < 6$?", "no", "$6 < 6$ is false.",
            contract=YES_NO,
        ),
    ],
}

CLOSED_OPEN = label_contract("closed", "open")
RIGHT_LEFT = label_contract("right", "left")

NEW[("graphing-inequalities-number-line", "kp1")] = [
    mk(
        "To graph $x \\le 5$, is the circle at $5$ open or closed?", "closed",
        "$\\le$ is inclusive, so the circle is closed.",
        contract=CLOSED_OPEN,
    ),
    mk(
        "To graph $x > -4$, is the circle at $-4$ open or closed?", "open",
        "$>$ is strict, so the circle is open.",
        contract=CLOSED_OPEN,
    ),
]
NEW[("graphing-inequalities-number-line", "kp2")] = [
    mk(
        "For $x < 3$, does the ray point left or right?", "left",
        "$<$ points left.",
        contract=RIGHT_LEFT,
    ),
    mk(
        "Describe the graph of $x \\ge -2$.", "closed circle at -2, ray to the right",
        "$x \\ge -2$ is inclusive (closed) and points right (greater than).",
        contract=label_contract(["closed circle at -2, ray to the right"]),
    ),
]
NEW[("graphing-inequalities-number-line", "kp3")] = [
    mk(
        "A number line shows a closed circle at $2$ with a ray to the right. Write the "
        "inequality.",
        "x >= 2", "A closed circle is inclusive; right means greater than or equal.",
    ),
    mk(
        "A number line shows an open circle at $-5$ with a ray to the left. Write the "
        "inequality.",
        "x < -5", "An open circle is strict; left means less than.",
    ),
]

CONTRACTS: dict[ExemplarKey, str] = {
    ExemplarKey("graphing-inequalities-number-line", "kp1", 0): CLOSED_OPEN,
    ExemplarKey("graphing-inequalities-number-line", "kp1", 1): CLOSED_OPEN,
    ExemplarKey("graphing-inequalities-number-line", "kp2", 0): RIGHT_LEFT,
    ExemplarKey("graphing-inequalities-number-line", "kp2", 1): label_contract(
        ["closed circle at 0, ray to the left"]
    ),
}

NEW[("one-step-inequalities", "kp1")] = [
    mk("Solve $x + 8 > 3$.", "x > -5", "Subtract $8$: $x > -5$."),
    mk("Solve $x - 6 \\le 1$.", "x <= 7", "Add $6$: $x \\le 7$."),
]
NEW[("one-step-inequalities", "kp2")] = [
    mk("Solve $5x < 30$.", "x < 6", "Divide by $5$: $x < 6$."),
    mk("Solve $\\frac{x}{4} \\ge 3$.", "x >= 12", "Multiply by $4$: $x \\ge 12$."),
]
NEW[("one-step-inequalities", "kp3")] = [
    mk(
        "Solve $3x > 21$. On its number-line graph, is the circle at the boundary open "
        "or closed?",
        "open", "$x > 7$; a strict inequality excludes the boundary.",
        contract=CLOSED_OPEN,
    ),
    mk(
        "Solve $x + 4 \\le 9$. On its number-line graph, does the shaded ray point left "
        "or right?",
        "left", "$x \\le 5$; \"at most\" shades everything below the boundary.",
        contract=RIGHT_LEFT,
    ),
]
CONTRACTS[ExemplarKey("one-step-inequalities", "kp3", 0)] = CLOSED_OPEN
CONTRACTS[ExemplarKey("one-step-inequalities", "kp3", 1)] = RIGHT_LEFT

NEW[("two-step-inequalities", "kp1")] = [
    mk("Solve $3x - 4 \\le 11$.", "x <= 5", "$3x \\le 15$; $x \\le 5$."),
    mk("Solve $2x + 5 > 17$.", "x > 6", "$2x > 12$; $x > 6$."),
]
NEW[("two-step-inequalities", "kp2")] = [
    mk("Solve $\\frac{x}{3} + 1 < 5$.", "x < 12", "$\\frac{x}{3} < 4$; $x < 12$."),
    mk("Solve $\\frac{x}{5} - 2 \\ge 0$.", "x >= 10", "$\\frac{x}{5} \\ge 2$; $x \\ge 10$."),
]
NEW[("two-step-inequalities", "kp3")] = [
    mk(
        "Solve $4x - 1 < 19$. Is $x = 5$ a solution? (yes/no)", "no",
        "$4x < 20$; $x < 5$; the boundary $5$ is excluded.", contract=YES_NO,
    ),
    mk(
        "Solve $3x + 2 \\ge 14$. Is $x = 4$ included? (yes/no)", "yes",
        "$3x \\ge 12$; $x \\ge 4$; the boundary $4$ is included.", contract=YES_NO,
    ),
]

NEW[("linear-inequalities", "kp1")] = [
    mk("Solve $-4x < 20$.", "x > -5", "Divide by $-4$ and reverse: $x > -5$."),
    mk("Solve $-x + 3 \\ge 7$.", "x <= -4", "$-x \\ge 4$; multiply by $-1$ and reverse."),
]
NEW[("linear-inequalities", "kp2")] = [
    mk(
        "Solve $4(x - 1) > x + 8$.", "x > 4", "$4x - 4 > x + 8$; $3x > 12$; $x > 4$.",
    ),
    mk("Solve $7 - 3x \\le 22$.", "x >= -5", "$-3x \\le 15$; $x \\ge -5$."),
]
NEW[("linear-inequalities", "kp3")] = [
    mk("Solve $3x + 5 < 6x - 4$.", "x > 3", "$9 < 3x$; $x > 3$."),
    mk("Solve $6 - 2x \\ge 3x - 9$.", "x <= 3", "$15 \\ge 5x$; $x \\le 3$."),
]

NEW[("writing-inequalities-from-statements", "kp1")] = [
    mk("Write an inequality: \"$x$ is at most $20$\".", "x <= 20", "\"At most\" means $\\le$."),
    mk(
        "Write an inequality for the number of items $n$: \"more than $5$ items\".",
        "n > 5", "\"More than\" is strict: $n > 5$.",
    ),
]
EQ1 = label_contract(["2x + 5 <= 17"])
EQ2 = label_contract(["y/3 > 6"])
NEW[("writing-inequalities-from-statements", "kp2")] = [
    mk(
        "Write an inequality: \"$7$ more than three times a number is at most $28$\".",
        "3x + 7 <= 28", "\"At most\" is $\\le$; the product $3x$ comes first.",
        contract=label_contract(["3x + 7 <= 28"]),
    ),
    mk(
        "Write an inequality: \"the quotient of $z$ and $4$ exceeds $9$\".",
        "z/4 > 9", "\"Exceeds\" is a strict $>$.",
        contract=label_contract(["z/4 > 9"]),
    ),
]
CONTRACTS[ExemplarKey("writing-inequalities-from-statements", "kp2", 0)] = EQ1
CONTRACTS[ExemplarKey("writing-inequalities-from-statements", "kp2", 1)] = EQ2

NEW[("writing-inequalities-from-statements", "kp3")] = [
    mk(
        "You need at least $50$ points to qualify. What is the fewest points that "
        "works?",
        "50", "The fewest value meeting $n \\ge 50$ is $50$ itself.",
    ),
    mk("A crate holds at most $40$ kg. Write the inequality for the weight $w$.", "w <= 40", "\"At most\" means $\\le$."),
]
