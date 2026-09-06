"""New held-out exemplars and contract retrofits for the core
systems-of-equations topics of
`curriculum/foundations/05-systems-inequalities.yaml`.
"""
from __future__ import annotations

from foundations_curriculum_patch import ExemplarKey
from symbolic_common import EXACT, frac_answer, label_contract, mk, solve_2x2

KP = tuple

YES_NO = label_contract("yes", "no")


def _sys(problem: str, a1, b1, c1, a2, b2, c2, *, sketch: str):
    x, y = solve_2x2(a1, b1, c1, a2, b2, c2)
    return mk(problem, f"({frac_answer(x)}, {frac_answer(y)})", sketch, contract=EXACT)


NEW: dict[KP, list] = {
    ("checking-systems-solutions", "kp1"): [
        mk(
            "Is $(2, 7)$ the solution of the system $y = 2x + 3$ and $y = x + 5$?",
            "yes", "$2(2) + 3 = 7$ and $2 + 5 = 7$.", contract=YES_NO,
        ),
        mk(
            "Is $(3, 4)$ the solution of $y = x + 1$ and $y = 2x - 2$?", "yes",
            "$3 + 1 = 4$ and $2(3) - 2 = 4$.", contract=YES_NO,
        ),
    ],
    ("checking-systems-solutions", "kp2"): [
        mk(
            "Is $(4, 1)$ a solution of the system $x + y = 5$ and $x - y = 2$?", "no",
            "$4 + 1 = 5$ holds, but $4 - 1 = 3 \\ne 2$.", contract=YES_NO,
        ),
        mk(
            "Is $(3, 2)$ a solution of $2x + y = 8$ and $x - y = 2$?", "no",
            "$8$ holds, but $3 - 2 = 1 \\ne 2$.", contract=YES_NO,
        ),
    ],
    ("checking-systems-solutions", "kp3"): [
        mk(
            "Which of $(2, 5)$ or $(3, 7)$ solves the system $y = 2x + 1$ and "
            "$y = x + 4$?",
            "(3, 7)", "$(3, 7)$ satisfies both; $(2, 5)$ fails $y = x + 4$.",
        ),
        mk(
            "Which of $(1, 4)$ or $(5, 8)$ solves $y = x + 3$ and $x + y = 13$?",
            "(5, 8)", "$8 = 5 + 3$ and $5 + 8 = 13$.",
        ),
    ],
}

NEW[("graphing-systems", "kp1")] = [
    mk(
        "Where do the lines $y = x + 2$ and $y = -x + 8$ intersect?", "(3, 5)",
        "$x + 2 = -x + 8$; $2x = 6$; $x = 3$; $y = 5$.",
    ),
    mk(
        "Where do the lines $y = 3x$ and $y = x + 6$ intersect?", "(3, 9)",
        "$3x = x + 6$; $x = 3$; $y = 9$.",
    ),
]
NEW[("graphing-systems", "kp2")] = [
    mk(
        "Graph $x + y = 7$ and $y = x + 1$. Where do they intersect?", "(3, 4)",
        "$x + (x + 1) = 7$; $x = 3$; $y = 4$.",
    ),
    mk(
        "Where do the lines $y = -4$ and $x = 3$ intersect?", "(3, -4)",
        "A horizontal and a vertical line meet where their fixed values pair up.",
    ),
]
YES_BOTH = label_contract(["yes, it checks in both"])
NEW[("graphing-systems", "kp3")] = [
    mk(
        "The graphs suggest $(3, 4)$ solves $y = 3x - 5$ and $y = x + 1$. Verify.",
        "yes, it checks in both", "$3(3) - 5 = 4$ and $3 + 1 = 4$.",
        contract=YES_BOTH,
    ),
    mk(
        "The graphs suggest $(2, 5)$ solves $x + y = 7$ and $x - y = -2$. Verify.",
        "no; it fails x - y = -2",
        "$2 + 5 = 7$ holds, but $2 - 5 = -3 \\ne -2$.",
        contract=label_contract(["no; it fails x - y = -2"]),
    ),
]

NEW[("substitution-with-isolated-variable", "kp1")] = [
    _sys(
        "Solve the system $y = 3x$, $x + y = 12$.", -3, 1, 0, 1, 1, 12,
        sketch="$x + 3x = 12$; $x = 3$; $y = 9$.",
    ),
    _sys(
        "Solve the system $y = x + 2$, $3x + y = 10$.", -1, 1, 2, 3, 1, 10,
        sketch="$3x + x + 2 = 10$; $4x = 8$; $x = 2$; $y = 4$.",
    ),
]
NEW[("substitution-with-isolated-variable", "kp2")] = [
    _sys(
        "Solve the system $y = 2x + 1$, $y = x + 6$.", -2, 1, 1, -1, 1, 6,
        sketch="$2x + 1 = x + 6$; $x = 5$; $y = 11$.",
    ),
    _sys(
        "Solve the system $y = 4x$, $y = x + 9$.", -4, 1, 0, -1, 1, 9,
        sketch="$4x = x + 9$; $3x = 9$; $x = 3$; $y = 12$.",
    ),
]
NEW[("substitution-with-isolated-variable", "kp3")] = [
    _sys(
        "Solve the system $y = x - 3$, $x + 2y = 9$ and check your answer.",
        -1, 1, -3, 1, 2, 9,
        sketch="$x + 2(x - 3) = 9$; $3x = 15$; $x = 5$; $y = 2$; check $5 + 4 = 9$.",
    ),
    _sys(
        "Solve the system $y = 3x$, $x + y = 20$ and check your answer.",
        -3, 1, 0, 1, 1, 20,
        sketch="$4x = 20$; $x = 5$; $y = 15$; check $x + y = 20$: $5 + 15 = 20$.",
    ),
]

NEW[("systems-substitution", "kp1")] = [
    _sys(
        "Solve the system $3x + y = 11$, $2x - y = 4$.", 3, 1, 11, 2, -1, 4,
        sketch="$y = 11 - 3x$; $2x - (11 - 3x) = 4$; $5x = 15$; $x = 3$; $y = 2$.",
    ),
    _sys(
        "Solve the system $x + 3y = 14$, $x = y + 2$.", 1, -1, 2, 1, 3, 14,
        sketch="$(y + 2) + 3y = 14$; $4y = 12$; $y = 3$; $x = 5$.",
    ),
]
NEW[("systems-substitution", "kp2")] = [
    _sys(
        "Solve the system $x = 3y - 1$, $2x - 5y = 3$.", 1, -3, -1, 2, -5, 3,
        sketch="$2(3y - 1) - 5y = 3$; $y - 2 = 3$; $y = 5$; $x = 14$.",
    ),
    _sys(
        "Solve the system $y = 3x - 4$, $5x - 2y = 7$.", -3, 1, -4, 5, -2, 7,
        sketch="$5x - 2(3x - 4) = 7$; $-x + 8 = 7$; $x = 1$; $y = -1$.",
    ),
]
NEW[("systems-substitution", "kp3")] = [
    _sys(
        "Solve the system $x + y = 9$, $x - y = 3$.", 1, 1, 9, 1, -1, 3,
        sketch="$x = 9 - y$; $(9 - y) - y = 3$; $y = 3$; $x = 6$.",
    ),
    _sys(
        "Solve the system $x - 3y = 2$, $2x + y = 11$.", 1, -3, 2, 2, 1, 11,
        sketch="$x = 2 + 3y$; $2(2 + 3y) + y = 11$; $7y = 7$; $y = 1$; $x = 5$.",
    ),
]

NEW[("elimination-with-addition", "kp1")] = [
    _sys(
        "Solve the system $x + y = 9$, $x - y = 5$.", 1, 1, 9, 1, -1, 5,
        sketch="Add: $2x = 14$; $x = 7$; $y = 2$.",
    ),
    _sys(
        "Solve the system $3x + y = 13$, $2x - y = 7$.", 3, 1, 13, 2, -1, 7,
        sketch="Add: $5x = 20$; $x = 4$; $y = 1$.",
    ),
]
NEW[("elimination-with-addition", "kp2")] = [
    _sys(
        "Solve the system $x + 3y = 10$, $x + y = 6$.", 1, 3, 10, 1, 1, 6,
        sketch="Subtract: $2y = 4$; $y = 2$; $x = 4$.",
    ),
    _sys(
        "Solve the system $4x + y = 15$, $x + y = 6$.", 4, 1, 15, 1, 1, 6,
        sketch="Subtract: $3x = 9$; $x = 3$; $y = 3$.",
    ),
]
NEW[("elimination-with-addition", "kp3")] = [
    _sys(
        "Solve the system $3x + y = 14$, $x - y = 2$ and check.", 3, 1, 14, 1, -1, 2,
        sketch="Add: $4x = 16$; $x = 4$; $y = 2$; check $12 + 2 = 14$ and $4 - 2 = 2$.",
    ),
    _sys(
        "Solve the system $x + y = 5$, $x - y = 9$ and check.", 1, 1, 5, 1, -1, 9,
        sketch="Add: $2x = 14$; $x = 7$; $y = -2$; check $7 + (-2) = 5$ and "
        "$7 - (-2) = 9$.",
    ),
]

NEW[("systems-elimination", "kp1")] = [
    _sys(
        "Solve the system $3x + 2y = 16$, $x + y = 6$.", 3, 2, 16, 1, 1, 6,
        sketch="Multiply 2nd by $-2$: $-2x - 2y = -12$; add: $x = 4$; $y = 2$.",
    ),
    _sys(
        "Solve the system $x + 3y = 13$, $2x - y = 5$.", 1, 3, 13, 2, -1, 5,
        sketch="Multiply 1st by $-2$: $-2x - 6y = -26$; add 2nd: $-7y = -21$; $y = 3$; "
        "$x = 4$.",
    ),
]
NEW[("systems-elimination", "kp2")] = [
    _sys(
        "Solve the system $3x + 2y = 16$, $2x + 3y = 19$.", 3, 2, 16, 2, 3, 19,
        sketch="$\\times 3$ and $\\times 2$: $9x + 6y = 48$, $4x + 6y = 38$; subtract: "
        "$5x = 10$; $x = 2$; $y = 5$.",
    ),
    _sys(
        "Solve the system $2x + 3y = 16$, $3x + 2y = 19$.", 2, 3, 16, 3, 2, 19,
        sketch="$\\times 3$ and $\\times 2$: $6x + 9y = 48$, $6x + 4y = 38$; subtract: "
        "$5y = 10$; $y = 2$; $x = 5$.",
    ),
]
NEW[("systems-elimination", "kp3")] = [
    _sys(
        "Solve the system $2x + 3y = 18$, $4x + 7y = 40$.", 2, 3, 18, 4, 7, 40,
        sketch="Multiply 1st by $-2$: $-4x - 6y = -36$; add: $y = 4$; $x = 3$.",
    ),
    _sys(
        "Solve the system $3x + 2y = 14$, $9x + 5y = 41$.", 3, 2, 14, 9, 5, 41,
        sketch="Multiply 1st by $-3$: $-9x - 6y = -42$; add: $-y = -1$; $y = 1$; $x = 4$.",
    ),
]

NO_SOLUTION = label_contract(["no solution"])
INFINITE = label_contract(["infinitely many solutions"])
NONE_OPT = label_contract(["none"])
INFINITE2 = label_contract(["infinitely many"])
EXACTLY_ONE = label_contract(["exactly one"])

NEW[("systems-special-cases", "kp1")] = [
    mk(
        "Solve the system $x + y = 5$, $x + y = 9$.", "no solution",
        "Subtracting gives $0 = 4$, a contradiction.", contract=NO_SOLUTION,
    ),
    mk(
        "Solve the system $3x + 3y = 12$, $x + y = 4$.", "infinitely many solutions",
        "The first is three times the second; elimination gives $0 = 0$.",
        contract=INFINITE,
    ),
]
NEW[("systems-special-cases", "kp2")] = [
    mk(
        "How many solutions does the system $y = 4x + 2$, $y = 4x - 1$ have?", "none",
        "Equal slopes, different intercepts.", contract=NONE_OPT,
    ),
    mk(
        "How many solutions does the system $y = 3x - 1$, $2y = 6x - 2$ have?",
        "infinitely many", "Same line.", contract=INFINITE2,
    ),
]
NEW[("systems-special-cases", "kp3")] = [
    mk(
        "How many solutions does the system $y = 2x + 5$, $y = -x + 5$ have?",
        "exactly one", "Different slopes, so the lines cross once (at $(0, 5)$).",
        contract=EXACTLY_ONE,
    ),
    mk(
        "How many solutions does the system $y = 5x - 2$, $y = 5x + 3$ have?",
        "no solution",
        "Equal slopes, different intercepts: the lines are parallel and never meet.",
        contract=NO_SOLUTION,
    ),
]

CONTRACTS: dict[ExemplarKey, str] = {
    ExemplarKey("graphing-systems", "kp3", 0): label_contract(
        ["yes, it checks in both"]
    ),
    ExemplarKey("graphing-systems", "kp3", 1): label_contract(
        ["no; it fails x - y = 1"]
    ),
    ExemplarKey("systems-special-cases", "kp1", 0): label_contract(["no solution"]),
    ExemplarKey("systems-special-cases", "kp1", 1): label_contract(
        ["infinitely many solutions"]
    ),
    ExemplarKey("systems-special-cases", "kp2", 0): label_contract(["none"]),
    ExemplarKey("systems-special-cases", "kp2", 1): label_contract(
        ["infinitely many"]
    ),
    ExemplarKey("systems-special-cases", "kp3", 0): label_contract(["exactly one"]),
    ExemplarKey("systems-special-cases", "kp3", 1): label_contract(
        ["infinitely many"]
    ),
}
