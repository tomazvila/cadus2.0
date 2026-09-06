"""New held-out exemplars and contract retrofits for the coordinate/point
topics of `curriculum/foundations/04-linear-graphs.yaml`.
"""
from __future__ import annotations

from foundations_curriculum_patch import ExemplarKey
from symbolic_common import label_contract, mk

KP = tuple

NEW: dict[KP, list] = {
    ("plotting-points", "kp1"): [
        mk(
            "Starting at the origin, you move $5$ right and $2$ up. Give the coordinates "
            "of where you land.",
            "(5, 2)", "Right is positive $x$, up is positive $y$: $(5, 2)$.",
        ),
        mk(
            "In the ordered pair $(6, 1)$, which number is the y-coordinate?", "1",
            "The second number of an ordered pair is the y-coordinate.",
        ),
    ],
    ("plotting-points", "kp2"): [
        mk(
            "Give the coordinates of the point $3$ units left and $6$ units up from the "
            "origin.",
            "(-3, 6)", "Left makes $x$ negative; up makes $y$ positive.",
        ),
        mk(
            "Give the coordinates of the point $2$ units right and $7$ units down from "
            "the origin.",
            "(2, -7)", "Right makes $x$ positive; down makes $y$ negative.",
        ),
    ],
    ("plotting-points", "kp3"): [
        mk(
            "Give the coordinates of the point on the x-axis that is $4$ units right of "
            "the origin.",
            "(4, 0)", "A point on the x-axis has $y = 0$.",
        ),
        mk(
            "A point lies on the y-axis $5$ units above the origin. Give its coordinates.",
            "(0, 5)", "A point on the y-axis has $x = 0$.",
        ),
    ],
}

QUADRANT_II = label_contract(["II"])
Y_AXIS = label_contract(["the y-axis"])
QUADRANT_III = label_contract(["III"])

NEW[("coordinate-plane", "kp1")] = [
    mk(
        "In which quadrant does the point $(-6, 2)$ lie?", "II",
        "Negative $x$, positive $y$ is quadrant II.", contract=QUADRANT_II,
    ),
    mk(
        "The point $(4, 0)$ lies on which axis?", "the x-axis",
        "A $y$-coordinate of $0$ places the point on the x-axis.",
        contract=label_contract(["the x-axis"]),
    ),
]
NEW[("coordinate-plane", "kp2")] = [
    mk(
        "A point has a positive x-coordinate and a negative y-coordinate. Which quadrant "
        "is it in?",
        "IV", "Positive $x$, negative $y$ is quadrant IV.", contract=label_contract(["IV"]),
    ),
    mk(
        "Reflect $(4, -3)$ across the y-axis. Give the new coordinates.", "(-4, -3)",
        "Reflecting across the y-axis flips the sign of the x-coordinate.",
    ),
]
NEW[("coordinate-plane", "kp3")] = [
    mk(
        "How far apart are $(3, 4)$ and $(3, -2)$?", "6",
        "Same x, so distance is $|4 - (-2)| = 6$.",
    ),
    mk(
        "How far apart are $(-5, 2)$ and $(3, 2)$?", "8",
        "Same y, so distance is $|-5 - 3| = 8$.",
    ),
]

NEW[("graphing-proportional-relationships", "kp1")] = [
    mk(
        "To graph $y = 5x$, give the points at $x = 0$ and $x = 3$.",
        "(0, 0) and (3, 15)", "$y = 5(0) = 0$ and $y = 5(3) = 15$.",
        contract=label_contract(["(0, 0) and (3, 15)"]),
    ),
    mk(
        "To graph $y = -2x$, give the points at $x = 0$ and $x = 4$.",
        "(0, 0) and (4, -8)", "$y = -2(0) = 0$ and $y = -2(4) = -8$.",
        contract=label_contract(["(0, 0) and (4, -8)"]),
    ),
]
NEW[("graphing-proportional-relationships", "kp2")] = [
    mk(
        "A proportional graph passes through $(6, 24)$. What is $k$?", "4",
        "$k = \\frac{24}{6} = 4$.",
    ),
    mk(
        "A proportional graph passes through $(5, 2)$. What is $k$?", "2/5",
        "$k = \\frac{2}{5}$.",
    ),
]
NEW[("graphing-proportional-relationships", "kp3")] = [
    mk(
        "Line A passes through $(1, 6)$; line B passes through $(3, 12)$. Which has the "
        "larger unit rate?",
        "line A", "$k_A = 6$, $k_B = 4$.", contract=label_contract(["line A"]),
    ),
    mk(
        "A proportional graph passes through $(2, 11)$. Does it grow faster than $y = 5x$?",
        "yes", "Its $k = 5.5 > 5$.", contract=label_contract("yes", "no"),
    ),
]

NEW[("graphing-from-a-table", "kp1")] = [
    mk(
        "Make a table for $y = 3x - 1$ at $x = 0$, $1$, $2$.",
        "(0, -1), (1, 2), (2, 5)", "$3(0) - 1 = -1$; $3(1) - 1 = 2$; $3(2) - 1 = 5$.",
    ),
    mk(
        "Make a table for $y = -2x + 6$ at $x = 0$, $1$, $2$.",
        "(0, 6), (1, 4), (2, 2)", "$-2(0) + 6 = 6$; $-2(1) + 6 = 4$; $-2(2) + 6 = 2$.",
    ),
]
NEW[("graphing-from-a-table", "kp2")] = [
    mk(
        "The table gives $(0, 1)$, $(1, 4)$, $(2, 7)$. Do the plotted points lie on one "
        "straight line?",
        "yes", "$y$ rises by a constant $3$ for each unit of $x$.",
        contract=label_contract("yes", "no"),
    ),
    mk(
        "The table gives $(0, 6)$, $(2, 0)$, $(3, -3)$. Which point lies on the "
        "x-axis?",
        "(2, 0)", "A point on the x-axis has $y = 0$, which is the row $(2, 0)$.",
    ),
]
NEW[("graphing-from-a-table", "kp3")] = [
    mk(
        "A line table shows $(0, 4)$, $(1, 7)$, $(2, 10)$. Find $y$ at $x = 6$.", "22",
        "Step $+3$ per unit: $4 + 3(6) = 22$.",
    ),
    mk(
        "The table $(1, 3)$, $(2, 8)$, $(3, 13)$ is linear. By how much does $y$ increase "
        "per unit of $x$?",
        "5", "Each step of $x$ adds $5$ to $y$.",
    ),
]

NEW[("x-y-intercepts", "kp1")] = [
    mk("Find the y-intercept of $y = 5x - 10$.", "(0, -10)", "Set $x = 0$: $y = -10$."),
    mk("Find the y-intercept of $3x + y = 9$.", "(0, 9)", "Set $x = 0$: $y = 9$."),
]
NEW[("x-y-intercepts", "kp2")] = [
    mk(
        "Find the x-intercept of $y = 5x - 10$.", "(2, 0)", "$0 = 5x - 10$; $x = 2$.",
    ),
    mk(
        "Find the x-intercept of $3x + y = 9$.", "(3, 0)", "$3x + 0 = 9$; $x = 3$.",
    ),
]
NEW[("x-y-intercepts", "kp3")] = [
    mk(
        "Find the x-intercept and the y-intercept of $2x + 5y = 20$. Give them as two "
        "points, x-intercept first.",
        "(10, 0), (0, 4)", "$y = 0 \\to x = 10$; $x = 0 \\to y = 4$.",
    ),
    mk(
        "Find the x-intercept and the y-intercept of $4x - 3y = 12$. Give them as two "
        "points, x-intercept first.",
        "(3, 0), (0, -4)", "$y = 0 \\to 4x = 12 \\to x = 3$; $x = 0 \\to -3y = 12 \\to y = -4$.",
    ),
]

NEW[("graphing-linear-equations", "kp1")] = [
    mk(
        "For $y = 3x - 2$, give the y-intercept and one more point found with the slope.",
        "(0, -2) and (1, 1)", "Start at $(0, -2)$; rise $3$, run $1$ to $(1, 1)$.",
        contract=label_contract(["(0, -2) and (1, 1)"]),
    ),
    mk(
        "For $y = -2x + 5$, give the y-intercept and the next point using the slope.",
        "(0, 5) and (1, 3)", "Start at $(0, 5)$; rise $-2$, run $1$ to $(1, 3)$.",
        contract=label_contract(["(0, 5) and (1, 3)"]),
    ),
]
NEW[("graphing-linear-equations", "kp2")] = [
    mk(
        "To graph $3x + 4y = 12$ by intercepts, which two points do you plot?",
        "(4, 0) and (0, 3)", "$y = 0 \\to x = 4$; $x = 0 \\to y = 3$.",
        contract=label_contract(["(4, 0) and (0, 3)"]),
    ),
    mk(
        "To graph $x - y = 5$ by intercepts, which two points do you plot?",
        "(5, 0) and (0, -5)", "$y = 0 \\to x = 5$; $x = 0 \\to y = -5$.",
        contract=label_contract(["(5, 0) and (0, -5)"]),
    ),
]
NEW[("graphing-linear-equations", "kp3")] = [
    mk(
        "Describe the graph of $x = 6$.", "a vertical line through (6, 0)",
        "$x = c$ is always a vertical line, crossing the x-axis at $c$.",
        contract=label_contract(["a vertical line through (6, 0)"]),
    ),
    mk(
        "Describe the graph of $y = -3$.", "a horizontal line through (0, -3)",
        "$y = c$ is always a horizontal line, crossing the y-axis at $c$.",
        contract=label_contract(["a horizontal line through (0, -3)"]),
    ),
]

CONTRACTS: dict[ExemplarKey, str] = {
    ExemplarKey("plotting-points", "kp2", 1): label_contract(["2 down"]),
    ExemplarKey("coordinate-plane", "kp1", 0): QUADRANT_II,
    ExemplarKey("coordinate-plane", "kp1", 1): Y_AXIS,
    ExemplarKey("coordinate-plane", "kp2", 0): QUADRANT_III,
    ExemplarKey("graphing-proportional-relationships", "kp1", 0): label_contract(
        ["(0, 0) and (2, 6)"]
    ),
    ExemplarKey("graphing-proportional-relationships", "kp1", 1): label_contract(
        ["the origin (0, 0)"]
    ),
    ExemplarKey("graphing-proportional-relationships", "kp3", 0): label_contract(
        ["line A"]
    ),
    ExemplarKey("graphing-linear-equations", "kp1", 0): label_contract(
        ["(0, -3) and (1, -1)"]
    ),
    ExemplarKey("graphing-linear-equations", "kp1", 1): label_contract(
        ["(0, 4) and (1, 3)"]
    ),
    ExemplarKey("graphing-linear-equations", "kp2", 0): label_contract(
        ["(3, 0) and (0, 2)"]
    ),
    ExemplarKey("graphing-linear-equations", "kp2", 1): label_contract(
        ["(4, 0) and (0, -4)"]
    ),
    ExemplarKey("graphing-linear-equations", "kp3", 0): label_contract(
        ["a vertical line through (4, 0)"]
    ),
    ExemplarKey("graphing-linear-equations", "kp3", 1): label_contract(
        ["a horizontal line through (0, -2)"]
    ),
}
