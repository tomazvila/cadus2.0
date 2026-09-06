"""New held-out exemplars and contract retrofits for the slope-intercept,
point-slope, and parallel/perpendicular topics of
`curriculum/foundations/04-linear-graphs.yaml`.
"""
from __future__ import annotations

from foundations_curriculum_patch import ExemplarKey
from symbolic_common import label_contract, mk

KP = tuple

NEW: dict[KP, list] = {
    ("reading-slope-intercept-equations", "kp1"): [
        mk(
            "State the slope and y-intercept of $y = 4x - 7$.", "slope 4, y-intercept -7",
            "Read the coefficients directly from $y = mx + b$.",
            contract=label_contract(["slope 4, y-intercept -7"]),
        ),
        mk(
            "State the y-intercept of $y = -5x + 9$.", "9", "The constant term is $b = 9$.",
        ),
    ],
    ("reading-slope-intercept-equations", "kp2"): [
        mk(
            "What is the slope of $y = 8 - 3x$?", "-3", "Rewrite as $y = -3x + 8$.",
        ),
        mk(
            "State the slope and y-intercept of $y = -x$.", "slope -1, y-intercept 0",
            "$y = -x$ is $y = -1x + 0$.",
            contract=label_contract(["slope -1, y-intercept 0"]),
        ),
    ],
    ("reading-slope-intercept-equations", "kp3"): [
        mk(
            "State the slope and y-intercept of $y = -4$.", "slope 0, y-intercept -4",
            "A constant line has slope $0$.",
            contract=label_contract(["slope 0, y-intercept -4"]),
        ),
        mk(
            "For $y = \\frac{3}{4}x + 2$, give the rise and run of the slope.",
            "rise 3, run 4", "The slope $\\frac{3}{4}$ is rise over run.",
            contract=label_contract(["rise 3, run 4"]),
        ),
    ],
}

CONTRACTS: dict[ExemplarKey, str] = {
    ExemplarKey("reading-slope-intercept-equations", "kp1", 0): label_contract(
        ["slope 3, y-intercept -5"]
    ),
    ExemplarKey("reading-slope-intercept-equations", "kp2", 1): label_contract(
        ["slope 1, y-intercept 0"]
    ),
    ExemplarKey("reading-slope-intercept-equations", "kp3", 0): label_contract(
        ["slope 0, y-intercept 5"]
    ),
    ExemplarKey("reading-slope-intercept-equations", "kp3", 1): label_contract(
        ["rise 2, run 3"]
    ),
}

NEW[("slope-intercept-form", "kp1")] = [
    mk(
        "Write the equation of the line with slope $5$ and y-intercept $-2$.",
        "y = 5x - 2", "Slope $5$ is the coefficient of $x$; y-intercept $-2$ is the constant.",
    ),
    mk(
        "A line has slope $3$ and passes through $(2, 4)$. Write its equation.",
        "y = 3x - 2", "$b = 4 - 3(2) = -2$.",
    ),
]
NEW[("slope-intercept-form", "kp2")] = [
    mk(
        "Write the equation of the line through $(2, 5)$ and $(3, 8)$.", "y = 3x - 1",
        "$m = 3$; $b = 5 - 3(2) = -1$.",
    ),
    mk(
        "Write the equation of the line through $(0, -6)$ and $(3, 0)$.", "y = 2x - 6",
        "$m = 2$; $b = -6$.",
    ),
]
NEW[("slope-intercept-form", "kp3")] = [
    mk(
        "Write the equation of the line with x-intercept $(4, 0)$ and y-intercept "
        "$(0, 8)$.",
        "y = -2x + 8", "$m = \\frac{8 - 0}{0 - 4} = -2$; $b = 8$.",
    ),
    mk(
        "A line crosses the y-axis at $5$ and passes through $(3, 11)$. Write its "
        "equation.",
        "y = 2x + 5", "$m = \\frac{11 - 5}{3 - 0} = 2$.",
    ),
]

NEW[("point-slope-form", "kp1")] = [
    mk(
        "Write the point-slope equation of the line with slope $4$ through $(1, 3)$.",
        "y - 3 = 4(x - 1)", "$y - y_1 = m(x - x_1)$ with $m = 4$, $(x_1, y_1) = (1, 3)$.",
        contract=label_contract(["y - 3 = 4(x - 1)"]),
    ),
    mk(
        "Write the point-slope equation of the line with slope $-2$ through $(-3, 5)$.",
        "y - 5 = -2(x + 3)", "$m = -2$, $(x_1, y_1) = (-3, 5)$, so $x - (-3)$ is $x + 3$.",
        contract=label_contract(["y - 5 = -2(x + 3)"]),
    ),
]
NEW[("point-slope-form", "kp2")] = [
    mk(
        "For $y - 4 = 2(x - 3)$, state the slope and the point used.", "slope 2, point (3, 4)",
        "$y - 4 = 2(x - 3)$ matches $y - y_1 = m(x - x_1)$ with $m = 2$, $(x_1, y_1) = (3, 4)$.",
        contract=label_contract(["slope 2, point (3, 4)"]),
    ),
    mk(
        "For $y + 5 = -3(x - 2)$, state the point used.", "(2, -5)",
        "$y + 5$ means $y_1 = -5$; the point is $(2, -5)$.",
    ),
]
NEW[("point-slope-form", "kp3")] = [
    mk(
        "Convert $y - 4 = 3(x - 2)$ to slope-intercept form.", "y = 3x - 2",
        "$y = 3x - 6 + 4 = 3x - 2$.",
    ),
    mk(
        "Convert $y - 2 = -2(x + 3)$ to slope-intercept form.", "y = -2x - 4",
        "$y = -2x - 6 + 2 = -2x - 4$.",
    ),
]

NEW[("point-slope-standard-form", "kp1")] = [
    mk(
        "Write $y = 3x + 2$ in canonical standard form $Ax + By = C$ with integer coefficients and $A > 0$.", "3x - y = -2",
        "Move $3x$ left: $-3x + y = 2$; multiply by $-1$ so $A > 0$: $3x - y = -2$.",
        contract=label_contract(
            ["3x - y = -2"]
        ),
    ),
    mk(
        "Write $y - 3 = \\frac{1}{2}(x - 2)$ in canonical standard form $Ax + By = C$ with integer coefficients and $A > 0$.",
        "x - 2y = -4",
        "$y = \\frac{1}{2}x + 2$; multiply by $2$: $2y = x + 4$, so $x - 2y = -4$.",
        contract=label_contract(["x - 2y = -4"]),
    ),
]
NEW[("point-slope-standard-form", "kp2")] = [
    mk(
        "Write $2x + 5y = 20$ in slope-intercept form.", "y = -2/5 x + 4",
        "$5y = -2x + 20$; $y = -\\frac{2}{5}x + 4$.",
    ),
    mk(
        "Find the slope of $3x - 4y = 8$.", "3/4", "$-4y = -3x + 8$; $y = \\frac{3}{4}x - 2$.",
    ),
]
NEW[("point-slope-standard-form", "kp3")] = [
    mk(
        "Write the line through $(2, 3)$ and $(4, 9)$ in canonical standard form $Ax + By = C$ with integer coefficients and $A > 0$.", "3x - y = 3",
        "$m = 3$; $y = 3x - 3$; move terms: $3x - y = 3$.",
        contract=label_contract(["3x - y = 3"]),
    ),
    mk(
        "Write the line through $(1, 4)$ and $(3, 5)$ in canonical standard form $Ax + By = C$ with integer coefficients and $A > 0$.", "x - 2y = -7",
        "$m = \\frac{1}{2}$; $y = \\frac{1}{2}x + \\frac{7}{2}$; times $2$: $2y = x + 7$.",
        contract=label_contract(["x - 2y = -7"]),
    ),
]

CONTRACTS[ExemplarKey("point-slope-form", "kp1", 0)] = label_contract(
    ["y - 5 = 3(x - 2)"]
)
CONTRACTS[ExemplarKey("point-slope-form", "kp1", 1)] = label_contract(
    ["y - 2 = -1(x + 4)"]
)
CONTRACTS[ExemplarKey("point-slope-form", "kp2", 0)] = label_contract(
    ["slope 5, point (1, -3)"]
)
CONTRACTS[ExemplarKey("point-slope-standard-form", "kp1", 0)] = label_contract(
    ["2x - y = -3"]
)
CONTRACTS[ExemplarKey("point-slope-standard-form", "kp1", 1)] = label_contract(
    ["x - 2y = 2"]
)
CONTRACTS[ExemplarKey("point-slope-standard-form", "kp3", 0)] = label_contract(
    ["3x - y = 1"]
)
CONTRACTS[ExemplarKey("point-slope-standard-form", "kp3", 1)] = label_contract(
    ["x - 2y = -8"]
)

PARALLEL = label_contract("parallel", "perpendicular", "neither")

NEW[("slopes-of-parallel-perpendicular-lines", "kp1")] = [
    mk(
        "Are the lines $y = 5x - 2$ and $y = 5x + 7$ parallel, perpendicular, or "
        "neither?",
        "parallel", "Equal slopes (both $5$).", contract=PARALLEL,
    ),
    mk(
        "What is the slope of any line parallel to $y = 4x - 1$?", "4",
        "Parallel lines share the same slope.",
    ),
]
NEW[("slopes-of-parallel-perpendicular-lines", "kp2")] = [
    mk(
        "What is the slope of any line perpendicular to $y = 4x + 1$?", "-1/4",
        "Negative reciprocal of $4$.",
    ),
    mk(
        "What is the slope of any line perpendicular to $y = -\\frac{3}{5}x + 2$?", "5/3",
        "Negative reciprocal of $-\\frac{3}{5}$.",
    ),
]
NEW[("slopes-of-parallel-perpendicular-lines", "kp3")] = [
    mk(
        "Are $y = \\frac{1}{3}x + 1$ and $y = -3x - 2$ parallel, perpendicular, or "
        "neither?",
        "perpendicular", "$\\frac{1}{3} \\cdot (-3) = -1$.", contract=PARALLEL,
    ),
    mk(
        "Are $y = 4x + 1$ and $y = -4x + 1$ parallel, perpendicular, or neither?",
        "neither", "Slopes $4$ and $-4$: not equal, product $-16 \\ne -1$.",
        contract=PARALLEL,
    ),
]

CONTRACTS[ExemplarKey("slopes-of-parallel-perpendicular-lines", "kp1", 0)] = PARALLEL
CONTRACTS[ExemplarKey("slopes-of-parallel-perpendicular-lines", "kp3", 0)] = PARALLEL
CONTRACTS[ExemplarKey("parallel-perpendicular-lines", "kp3", 1)] = PARALLEL

NEW[("parallel-perpendicular-lines", "kp1")] = [
    mk(
        "Write the line parallel to $y = 3x - 2$ through $(1, 4)$.", "y = 3x + 1",
        "$m = 3$; $b = 4 - 3(1) = 1$.",
    ),
    mk(
        "Write the line parallel to $y = -2x + 5$ through $(2, 3)$.", "y = -2x + 7",
        "$m = -2$; $b = 3 - (-2)(2) = 7$.",
    ),
]
NEW[("parallel-perpendicular-lines", "kp2")] = [
    mk(
        "Write the line perpendicular to $y = 4x - 3$ through $(8, 1)$.", "y = -1/4 x + 3",
        "$m = -\\frac{1}{4}$; $b = 1 - (-\\frac{1}{4})(8) = 3$.",
    ),
    mk(
        "Write the line perpendicular to $y = -2x$ through $(4, 0)$.", "y = 1/2 x - 2",
        "$m = \\frac{1}{2}$; $b = 0 - (\\frac{1}{2})(4) = -2$.",
    ),
]
NEW[("parallel-perpendicular-lines", "kp3")] = [
    mk(
        "Write the line through the origin perpendicular to $3x + 4y = 8$.",
        "y = 4/3 x", "Given slope $-\\frac{3}{4}$; perpendicular slope $\\frac{4}{3}$.",
    ),
    mk(
        "Are $2x + 3y = 6$ and $3x + 2y = 6$ parallel, perpendicular, or neither?",
        "neither", "Slopes $-\\frac{2}{3}$ and $-\\frac{3}{2}$: not equal, product "
        "$\\ne -1$.", contract=PARALLEL,
    ),
]
