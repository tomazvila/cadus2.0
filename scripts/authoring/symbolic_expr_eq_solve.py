"""New held-out exemplars for the equation-solving topics of
`curriculum/foundations/03-expressions-equations.yaml`.

Every answer is computed by [`symbolic_common`]'s exact `Fraction` solvers
from the chosen integer/decimal parameters, never retyped by hand, so a
transcription slip cannot silently diverge from what the production answer
grammar checks.
"""
from __future__ import annotations

from fractions import Fraction as F

from foundations_curriculum_patch import ExemplarKey
from symbolic_common import frac_answer, label_contract, mk, solve_linear

KP = tuple  # (topic_id, kp_id)


def _solve(problem_tex: str, a, b, c, *, sketch: str) -> object:
    x = solve_linear(a, b, c)
    return mk(problem_tex, frac_answer(x), sketch)


NEW: dict[KP, list] = {
    ("addition-subtraction-equations", "kp1"): [
        _solve(
            "Solve $x + 4 = 15$.", 1, 4, 15,
            sketch="Subtract $4$: $x = 11$.",
        ),
        _solve(
            "Solve $m + 12 = -3$.", 1, 12, -3,
            sketch="Subtract $12$: $m = -15$.",
        ),
    ],
    ("addition-subtraction-equations", "kp2"): [
        _solve(
            "Solve $x - 9 = 2$.", 1, -9, 2,
            sketch="Add $9$: $x = 11$.",
        ),
        _solve(
            "Solve $y - 13 = -6$.", 1, -13, -6,
            sketch="Add $13$: $y = 7$.",
        ),
    ],
    ("one-step-equations", "kp1"): [
        mk("Solve $8x = 56$.", "7", "Divide by $8$: $x = 7$."),
        mk("Solve $4t = -28$.", "-7", "Divide by $4$: $t = -7$."),
    ],
    ("one-step-equations", "kp2"): [
        mk("Solve $\\frac{x}{4} = 7$.", "28", "Multiply by $4$: $x = 28$."),
        mk("Solve $\\frac{x}{-6} = 5$.", "-30", "Multiply by $-6$: $x = -30$."),
    ],
    ("one-step-equations", "kp3"): [
        mk("Solve $-9x = 45$.", "-5", "Divide by $-9$: $x = -5$."),
        mk("Solve $x + 13 = 4$.", "-9", "Subtract $13$: $x = -9$."),
    ],
    ("two-step-equations", "kp1"): [
        _solve("Solve $5x + 7 = 32$.", 5, 7, 32, sketch="$5x = 25$; $x = 5$."),
        _solve("Solve $4x - 6 = 18$.", 4, -6, 18, sketch="$4x = 24$; $x = 6$."),
    ],
    ("two-step-equations", "kp2"): [
        _solve(
            "Solve $\\frac{x}{4} + 5 = 9$.", F(1, 4), 5, 9,
            sketch="$\\frac{x}{4} = 4$; $x = 16$.",
        ),
        _solve("Solve $-5x + 9 = 34$.", -5, 9, 34, sketch="$-5x = 25$; $x = -5$."),
    ],
}


def _both_sides(problem_tex: str, a, b, c, d, *, sketch: str):
    # a*x + b = c*x + d  ->  (a-c) x = d - b
    x = solve_linear(F(a) - F(c), 0, F(d) - F(b))
    return mk(problem_tex, frac_answer(x), sketch)


NEW[("variables-both-sides", "kp1")] = [
    _both_sides(
        "Solve $7x + 4 = 3x + 24$.", 7, 4, 3, 24,
        sketch="Subtract $3x$: $4x + 4 = 24$; $4x = 20$; $x = 5$.",
    ),
    _both_sides(
        "Solve $5x - 9 = 2x + 3$.", 5, -9, 2, 3,
        sketch="Subtract $2x$: $3x - 9 = 3$; $3x = 12$; $x = 4$.",
    ),
]
NEW[("variables-both-sides", "kp2")] = [
    _both_sides(
        "Solve $4x + 15 = 9x - 5$.", 4, 15, 9, -5,
        sketch="Subtract $4x$: $15 = 5x - 5$; $20 = 5x$; $x = 4$.",
    ),
    _both_sides(
        "Solve $3 - 4x = 2x + 21$.", -4, 3, 2, 21,
        sketch="Add $4x$: $3 = 6x + 21$; $-18 = 6x$; $x = -3$.",
    ),
]


def _distribute(problem_tex: str, k, b, c, *, extra=0, sketch: str):
    # k*(x + b) + extra = c  ->  k*x + (k*b + extra) = c
    x = solve_linear(k, F(k) * F(b) + F(extra), c)
    return mk(problem_tex, frac_answer(x), sketch)


NEW[("distribute-then-solve", "kp1")] = [
    _distribute(
        "Solve $5(x + 1) = 30$.", 5, 1, 30,
        sketch="$5x + 5 = 30$; $5x = 25$; $x = 5$.",
    ),
    _distribute(
        "Solve $2(3x - 2) = 14$.", 6, F(-2, 3), 14,
        sketch="$6x - 4 = 14$; $6x = 18$; $x = 3$.",
    ),
]
NEW[("distribute-then-solve", "kp2")] = [
    _distribute(
        "Solve $-3(x - 5) = 21$.", -3, -5, 21,
        sketch="$-3x + 15 = 21$; $-3x = 6$; $x = -2$.",
    ),
    _distribute(
        "Solve $-2(x + 3) + 5 = 9$.", -2, 3, 9, extra=5,
        sketch="$-2x - 6 + 5 = 9$; $-2x - 1 = 9$; $-2x = 10$; $x = -5$.",
    ),
]

NEW[("multi-step-equations", "kp1")] = [
    _solve(
        "Solve $5x + 3x + 6 = 38$.", 8, 6, 38,
        sketch="$8x + 6 = 38$; $8x = 32$; $x = 4$.",
    ),
    _solve(
        "Solve $9x - 4x - 3 = 22$.", 5, -3, 22,
        sketch="$5x - 3 = 22$; $5x = 25$; $x = 5$.",
    ),
]
def _pipeline_a(problem_tex: str, left_coeff, left_const, right_coeff, right_const, *, sketch: str):
    x = solve_linear(F(left_coeff) - F(right_coeff), left_const, right_const)
    return mk(problem_tex, frac_answer(x), sketch)


NEW[("multi-step-equations", "kp2")] = [
    _pipeline_a(
        "Solve $4(2x - 3) = 5x + 3$.", 8, -12, 5, 3,
        sketch="$8x - 12 = 5x + 3$; $3x = 15$; $x = 5$.",
    ),
    _pipeline_a(
        "Solve $3(x - 4) = 2(x + 1)$.", 3, -12, 2, 2,
        sketch="$3x - 12 = 2x + 2$; $x = 14$.",
    ),
]
NEW[("multi-step-equations", "kp3")] = [
    _pipeline_a(
        "Solve $6x + 4 - 2x = 3(x + 2)$.", 4, 4, 3, 6,
        sketch="$4x + 4 = 3x + 6$; $x = 2$.",
    ),
    _pipeline_a(
        "Solve $5 - 3(x - 4) = 2x - 3$.", -3, 17, 2, -3,
        sketch="$17 - 3x = 2x - 3$; $20 = 5x$; $x = 4$.",
    ),
]

NEW[("equations-with-decimals", "kp1")] = [
    _solve(
        "Solve $0.4x + 0.6 = 2.6$.", F("0.4"), F("0.6"), F("2.6"),
        sketch="$0.4x = 2.0$; $x = 5$.",
    ),
    _solve(
        "Solve $1.5x - 0.5 = 4$.", F("1.5"), F("-0.5"), 4,
        sketch="$1.5x = 4.5$; $x = 3$.",
    ),
]
NEW[("equations-with-decimals", "kp2")] = [
    _solve(
        "Solve $0.2x - 1.1 = 0.7$.", F("0.2"), F("-1.1"), F("0.7"),
        sketch="Multiply by $10$: $2x - 11 = 7$; $2x = 18$; $x = 9$.",
    ),
    _solve(
        "Solve $0.75x + 0.25 = 3.25$.", F("0.75"), F("0.25"), F("3.25"),
        sketch="Multiply by $100$: $75x + 25 = 325$; $75x = 300$; $x = 4$.",
    ),
]

NEW[("equations-with-fractions", "kp1")] = [
    _solve(
        "Solve $\\frac{x}{4} + 5 = 9$.", F(1, 4), 5, 9,
        sketch="$\\frac{x}{4} = 4$; $x = 16$.",
    ),
    _solve(
        "Solve $\\frac{3}{5}x = 12$.", F(3, 5), 0, 12,
        sketch="Multiply by $\\frac{5}{3}$: $x = 20$.",
    ),
]
NEW[("equations-with-fractions", "kp2")] = [
    _solve(
        "Solve $\\frac{x}{4} + \\frac{x}{2} = 9$.", F(3, 4), 0, 9,
        sketch="Multiply by $4$: $x + 2x = 36$; $3x = 36$; $x = 12$.",
    ),
    mk(
        "Solve $\\frac{x + 2}{3} = \\frac{x - 2}{5}$.", "-8",
        "$5(x + 2) = 3(x - 2)$; $5x + 10 = 3x - 6$; $x = -8$.",
    ),
]
NEW[("equations-with-fractions", "kp3")] = [
    mk(
        "Solve $\\frac{3}{4}(x - 4) = 9$.", "16",
        "$x - 4 = 12$; $x = 16$.",
    ),
    _solve(
        "Solve $\\frac{1}{3}x + \\frac{1}{6}x = 9$.", F(1, 2), 0, 9,
        sketch="LCD $6$: $2x + x = 54$; $3x = 54$; $x = 18$.",
    ),
]

NO_SOLUTION = label_contract(["no solution"])
ALL_REALS = label_contract(["all real numbers"])

NEW[("equations-special-cases", "kp1")] = [
    mk(
        "Solve $4x + 7 = 4x - 1$.", "no solution",
        "Subtracting $4x$ leaves $7 = -1$, which is false.",
        contract=NO_SOLUTION,
    ),
    mk(
        "Solve $3(x + 2) = 3x + 5$.", "no solution",
        "$3x + 6 = 3x + 5$ gives $6 = 5$, false.",
        contract=NO_SOLUTION,
    ),
]
NEW[("equations-special-cases", "kp2")] = [
    mk(
        "Solve $5(x + 2) = 5x + 10$.", "all real numbers",
        "Both sides expand to $5x + 10$; every value works.",
        contract=ALL_REALS,
    ),
    mk(
        "Solve $3x + 4 - x = 2x + 4$.", "all real numbers",
        "The left side simplifies to $2x + 4$, identical to the right.",
        contract=ALL_REALS,
    ),
]
NEW[("equations-special-cases", "kp3")] = [
    mk(
        "Solve $2x + 7 = 2x - 1$.", "no solution",
        "Subtracting $2x$ leaves $7 = -1$, which is false.",
        contract=NO_SOLUTION,
    ),
    _solve(
        "Solve $4x - 3 = x + 6$.", 3, -3, 6,
        sketch="Subtract $x$ and add $3$: $3x = 9$; $x = 3$ — one solution.",
    ),
]

CONTRACTS: dict[ExemplarKey, str] = {
    ExemplarKey("equations-special-cases", "kp1", 0): NO_SOLUTION,
    ExemplarKey("equations-special-cases", "kp1", 1): NO_SOLUTION,
    ExemplarKey("equations-special-cases", "kp2", 0): ALL_REALS,
    ExemplarKey("equations-special-cases", "kp2", 1): ALL_REALS,
    ExemplarKey("equations-special-cases", "kp3", 0): ALL_REALS,
}
