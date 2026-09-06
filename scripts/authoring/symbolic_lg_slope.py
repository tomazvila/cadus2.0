"""New held-out exemplars and contract retrofits for the slope topics of
`curriculum/foundations/04-linear-graphs.yaml`.
"""
from __future__ import annotations

from foundations_curriculum_patch import ExemplarKey
from symbolic_common import frac_answer, label_contract, mk, slope

KP = tuple

NEW: dict[KP, list] = {
    ("slope-from-a-graph", "kp1"): [
        mk(
            "Between two points on a line, the graph rises $8$ units over a run of $4$ "
            "units to the right. What is the slope?",
            "2", "$\\frac{8}{4} = 2$.",
        ),
        mk(
            "A line rises $2$ units over a run of $8$ units. What is the slope?", "1/4",
            "$\\frac{2}{8} = \\frac{1}{4}$.",
        ),
    ],
    ("slope-from-a-graph", "kp2"): [
        mk(
            "A line drops $6$ units for every $3$ units it runs to the right. What is the "
            "slope?",
            "-2", "Rise is $-6$ over run $3$: $-2$.",
        ),
        mk(
            "A line rises from left to right. Is its slope positive or negative?",
            "positive", "Rising left to right is a positive slope.",
            contract=label_contract("positive", "negative"),
        ),
    ],
    ("slope-from-a-graph", "kp3"): [
        mk(
            "A graphed line passes through $(0, 2)$ and $(3, 8)$. Count the rise and run "
            "to find the slope.",
            "2", "Rise $6$, run $3$: slope $2$.",
        ),
        mk(
            "A graphed line passes through $(0, 5)$ and $(2, 1)$. What is the slope?", "-2",
            "Rise $-4$, run $2$.",
        ),
    ],
}


def _slope_kp(problem: str, x1, y1, x2, y2, *, sketch: str):
    m = slope(x1, y1, x2, y2)
    return mk(problem, frac_answer(m), sketch)


NEW[("slope-from-two-points", "kp1")] = [
    _slope_kp(
        "Find the slope of the line through $(2, 3)$ and $(6, 11)$.", 2, 3, 6, 11,
        sketch="$\\frac{11 - 3}{6 - 2} = \\frac{8}{4} = 2$.",
    ),
    _slope_kp(
        "Find the slope of the line through $(0, 0)$ and $(4, 12)$.", 0, 0, 4, 12,
        sketch="$\\frac{12 - 0}{4 - 0} = 3$.",
    ),
]
NEW[("slope-from-two-points", "kp2")] = [
    _slope_kp(
        "Find the slope of the line through $(-3, -1)$ and $(2, 9)$.", -3, -1, 2, 9,
        sketch="$\\frac{9 - (-1)}{2 - (-3)} = \\frac{10}{5} = 2$.",
    ),
    _slope_kp(
        "Find the slope of the line through $(-2, 5)$ and $(3, -10)$.", -2, 5, 3, -10,
        sketch="$\\frac{-10 - 5}{3 - (-2)} = \\frac{-15}{5} = -3$.",
    ),
]
NEW[("slope-from-two-points", "kp3")] = [
    mk(
        "Compute the slope through $(4, 6)$ and $(7, 12)$ subtracting in both orders. "
        "What do you get?",
        "2 both ways", "$\\frac{12 - 6}{7 - 4} = 2$ and $\\frac{6 - 12}{4 - 7} = 2$.",
        contract=label_contract(["2 both ways"]),
    ),
    _slope_kp(
        "Find the slope of the line through $(8, 2)$ and $(3, 12)$.", 8, 2, 3, 12,
        sketch="$\\frac{12 - 2}{3 - 8} = \\frac{10}{-5} = -2$.",
    ),
]

UNDEFINED = label_contract(["undefined"])

NEW[("horizontal-vertical-slopes", "kp1")] = [
    mk(
        "Find the slope of the line through $(3, 5)$ and $(9, 5)$.", "0",
        "Rise is $0$, so slope is $0$ (horizontal).",
    ),
    mk("What is the slope of the line $y = -7$?", "0", "A horizontal line has slope $0$."),
]
NEW[("horizontal-vertical-slopes", "kp2")] = [
    mk(
        "Find the slope of the line through $(4, 2)$ and $(4, 9)$.", "undefined",
        "Run is $0$, so slope is undefined (vertical).", contract=UNDEFINED,
    ),
    mk(
        "What is the slope of the line $x = 5$?", "undefined",
        "A vertical line has undefined slope.", contract=UNDEFINED,
    ),
]
NEW[("horizontal-vertical-slopes", "kp3")] = [
    mk(
        "The horizontal line through $(3, -8)$ has equation $y = c$. Find $c$.", "-8",
        "A horizontal line fixes $y$ at the given $y$-value.",
    ),
    mk(
        "The vertical line through $(9, 4)$ has equation $x = c$. Find $c$.", "9",
        "A vertical line fixes $x$ at the given $x$-value.",
    ),
]

NEW[("slope", "kp1")] = [
    _slope_kp(
        "Find the slope of the line through $(3, 8)$ and $(7, 2)$.", 3, 8, 7, 2,
        sketch="$\\frac{2 - 8}{7 - 3} = -\\frac{6}{4} = -\\frac{3}{2}$.",
    ),
    _slope_kp(
        "Find the slope of the line through $(-2, 1)$ and $(4, 4)$.", -2, 1, 4, 4,
        sketch="$\\frac{4 - 1}{4 - (-2)} = \\frac{3}{6} = \\frac{1}{2}$.",
    ),
]
NEW[("slope", "kp2")] = [
    mk(
        "The line through $(1, 4)$ and $(6, y)$ has slope $3$. Find $y$.", "19",
        "$\\frac{y - 4}{5} = 3$, so $y - 4 = 15$ and $y = 19$.",
    ),
    mk(
        "The line through $(x, 2)$ and $(5, 8)$ has slope $2$. Find $x$.", "2",
        "$\\frac{6}{5 - x} = 2$, so $5 - x = 3$ and $x = 2$.",
    ),
]
NEW[("slope", "kp3")] = [
    mk(
        "Are $(0, 2)$, $(3, 8)$, and $(5, 12)$ collinear?", "yes",
        "Slopes $\\frac{6}{3} = 2$ and $\\frac{4}{2} = 2$ match.",
        contract=label_contract("yes", "no"),
    ),
    mk(
        "Which line is steeper: through $(0, 0)$ and $(3, 8)$, or through $(0, 0)$ and "
        "$(4, 9)$? Give the steeper slope.",
        "8/3", "Slopes $\\frac{8}{3} \\approx 2.67$ vs $\\frac{9}{4} = 2.25$.",
    ),
]

NEW[("slope-as-rate-of-change", "kp1")] = [
    mk(
        "At $2$ pm a tank holds $60$ L; at $5$ pm it holds $30$ L. Find the rate of "
        "change in litres per hour, as a signed number.",
        "-10", "$\\frac{30 - 60}{5 - 2} = -10$ L per hour.",
    ),
    mk(
        "A plant grows from $4$ cm on day $1$ to $16$ cm on day $5$. Find the growth "
        "rate, in cm per day.",
        "3", "$\\frac{16 - 4}{5 - 1} = 3$ cm per day.",
    ),
]
CONSTANT = label_contract(["the balance stayed constant"])
NEW[("slope-as-rate-of-change", "kp2")] = [
    mk(
        "A bank balance changes at $-20$ euros per week. Is the balance increasing or "
        "decreasing?",
        "decreasing", "A negative rate means the balance falls.",
        contract=label_contract("increasing", "decreasing"),
    ),
    mk(
        "The slope of a bank-balance-vs-time graph is $0$ this month. What does that "
        "mean?",
        "the balance stayed constant", "A zero rate means no change.",
        contract=CONSTANT,
    ),
]
NEW[("slope-as-rate-of-change", "kp3")] = [
    mk(
        "A table shows $(3 \\text{ h}, 120 \\text{ km})$ and $(6 \\text{ h}, 270 "
        "\\text{ km})$. Find the speed, in km per hour.",
        "50", "$\\frac{270 - 120}{6 - 3} = 50$ km/h.",
    ),
    mk(
        "A cost table shows $2$ shirts cost €22 and $5$ shirts cost €55. Find the cost "
        "per shirt, in euros.",
        "11", "$\\frac{55 - 22}{5 - 2} = 11$ euros per shirt.",
    ),
]

CONTRACTS: dict[ExemplarKey, str] = {
    ExemplarKey("horizontal-vertical-slopes", "kp2", 0): UNDEFINED,
    ExemplarKey("horizontal-vertical-slopes", "kp2", 1): UNDEFINED,
    ExemplarKey("slope-from-two-points", "kp3", 0): label_contract(["2 both ways"]),
    ExemplarKey("slope-from-a-graph", "kp2", 0): label_contract("negative", "positive"),
    ExemplarKey("slope-as-rate-of-change", "kp2", 0): label_contract(
        "decreasing", "increasing"
    ),
    ExemplarKey("slope-as-rate-of-change", "kp2", 1): label_contract(
        ["the temperature stayed constant"]
    ),
}
