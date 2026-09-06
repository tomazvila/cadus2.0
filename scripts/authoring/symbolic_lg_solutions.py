"""Backfill `solution_sketch` for existing `04-linear-graphs.yaml`
exemplars that lost their held-out exemption once this lane's fourth
exemplar moved the held-out index. See `symbolic_expr_eq_solutions.py` for
the same rationale.
"""
from __future__ import annotations

from foundations_curriculum_patch import ExemplarKey

SKETCHES: dict[ExemplarKey, str] = {
    ExemplarKey("plotting-points", "kp1", 0):
        "Right is positive $x$, up is positive $y$: $(3, 5)$.",
    ExemplarKey("plotting-points", "kp1", 1):
        "The first number of an ordered pair is the x-coordinate.",
    ExemplarKey("plotting-points", "kp2", 0):
        "Left makes $x$ negative; up makes $y$ positive.",
    ExemplarKey("plotting-points", "kp2", 1):
        "Right stays positive $x$; the second move is $2$ units down.",
    ExemplarKey("plotting-points", "kp3", 0): "A point on the x-axis has $y = 0$.",
    ExemplarKey("plotting-points", "kp3", 1): "A point on the y-axis has $x = 0$.",
    ExemplarKey("coordinate-plane", "kp1", 0):
        "Negative $x$, positive $y$ is quadrant II.",
    ExemplarKey("coordinate-plane", "kp1", 1):
        "An x-coordinate of $0$ places the point on the y-axis.",
    ExemplarKey("coordinate-plane", "kp2", 0):
        "Negative $x$, negative $y$ is quadrant III.",
    ExemplarKey("constant-of-proportionality", "kp2", 1): "$\\frac{12}{3} = 4$.",
    ExemplarKey("graphing-proportional-relationships", "kp1", 0):
        "$y = 3(0) = 0$ and $y = 3(2) = 6$.",
    ExemplarKey("graphing-proportional-relationships", "kp1", 1):
        "At $x = 0$, $y = k(0) = 0$ for any $k$.",
    ExemplarKey("graphing-proportional-relationships", "kp2", 1):
        "$k = \\frac{3}{4}$.",
    ExemplarKey("slope-from-a-graph", "kp1", 1): "$\\frac{3}{6} = \\frac{1}{2}$.",
    ExemplarKey("slope-from-a-graph", "kp2", 0):
        "Falling left to right is a negative slope.",
    ExemplarKey("slope-from-two-points", "kp1", 1):
        "$\\frac{10 - 0}{5 - 0} = 2$.",
    ExemplarKey("horizontal-vertical-slopes", "kp1", 1):
        "A horizontal line $y = c$ has slope $0$.",
    ExemplarKey("horizontal-vertical-slopes", "kp2", 1):
        "A vertical line $x = c$ has undefined slope.",
    ExemplarKey("slope-as-rate-of-change", "kp2", 1):
        "A zero slope means the temperature did not change.",
    ExemplarKey("solutions-of-two-variable-equations", "kp2", 0):
        "$2(4) + 3 = 11$.",
    ExemplarKey("solutions-of-two-variable-equations", "kp3", 1):
        "$x + 6 = 12$; $x = 6$.",
    ExemplarKey("graphing-from-a-table", "kp1", 0):
        "$2(0) + 1 = 1$; $2(1) + 1 = 3$; $2(2) + 1 = 5$.",
    ExemplarKey("graphing-from-a-table", "kp1", 1):
        "$-(0) + 4 = 4$; $-(2) + 4 = 2$; $-(4) + 4 = 0$.",
    ExemplarKey("graphing-from-a-table", "kp2", 1):
        "The rows have constant first differences $\\Delta x = 1$, $\\Delta y = 1$; the row $(3, 0)$ lies on the x-axis.",
    ExemplarKey("graphing-from-a-table", "kp3", 1):
        "Each step of $x$ adds $4$ to $y$.",
    ExemplarKey("x-y-intercepts", "kp1", 0): "Set $x = 0$: $y = -6$.",
    ExemplarKey("x-y-intercepts", "kp1", 1): "Set $x = 0$: $y = 8$.",
    ExemplarKey("x-y-intercepts", "kp2", 1): "$2x + 0 = 8$; $x = 4$.",
    ExemplarKey("reading-slope-intercept-equations", "kp1", 0):
        "Read the coefficients directly from $y = mx + b$.",
    ExemplarKey("reading-slope-intercept-equations", "kp1", 1):
        "The constant term is $b = 4$.",
    ExemplarKey("reading-slope-intercept-equations", "kp2", 1):
        "$y = x$ is $y = 1x + 0$.",
    ExemplarKey("reading-slope-intercept-equations", "kp3", 0):
        "A constant line has slope $0$.",
    ExemplarKey("reading-slope-intercept-equations", "kp3", 1):
        "The slope $\\frac{2}{3}$ is rise over run.",
    ExemplarKey("slope-intercept-form", "kp1", 0):
        "Slope $4$ is the coefficient of $x$; y-intercept $-1$ is the constant.",
    ExemplarKey("graphing-linear-equations", "kp1", 1):
        "Start at $(0, 4)$; rise $-1$, run $1$ to $(1, 3)$.",
    ExemplarKey("graphing-linear-equations", "kp2", 0):
        "$y = 0 \\to x = 3$; $x = 0 \\to y = 2$.",
    ExemplarKey("graphing-linear-equations", "kp2", 1):
        "$y = 0 \\to x = 4$; $x = 0 \\to y = -4$.",
    ExemplarKey("graphing-linear-equations", "kp3", 0):
        "$x = c$ is always a vertical line, crossing the x-axis at $c$.",
    ExemplarKey("graphing-linear-equations", "kp3", 1):
        "$y = c$ is always a horizontal line, crossing the y-axis at $c$.",
    ExemplarKey("point-slope-form", "kp1", 0):
        "$y - y_1 = m(x - x_1)$ with $m = 3$, $(x_1, y_1) = (2, 5)$.",
    ExemplarKey("point-slope-form", "kp1", 1):
        "$m = -1$, $(x_1, y_1) = (-4, 2)$, so $x - (-4)$ is $x + 4$.",
    ExemplarKey("point-slope-form", "kp2", 0):
        "$y + 3 = 5(x - 1)$ matches $m = 5$, $(x_1, y_1) = (1, -3)$.",
    ExemplarKey("point-slope-form", "kp2", 1):
        "$y - 2$ means $y_1 = 2$; $x + 6$ means $x_1 = -6$.",
    ExemplarKey("slopes-of-parallel-perpendicular-lines", "kp1", 1):
        "Parallel lines share the same slope.",
    ExemplarKey("interpreting-linear-models", "kp3", 1): "$8(5) + 100 = 140$.",
    ExemplarKey("linear-word-problems", "kp1", 0):
        "The rate €2 per km is the slope; the €3 fee is the y-intercept.",
    ExemplarKey("interpreting-graphs-qualitatively", "kp1", 0):
        "A flat segment has zero slope, so the distance is not changing over "
        "that interval.",
    ExemplarKey("interpreting-graphs-qualitatively", "kp1", 1):
        "The graph falls left to right, so the temperature is decreasing.",
    ExemplarKey("interpreting-graphs-qualitatively", "kp2", 0):
        "The point $(4, 10)$ pairs input $4$ with output $10$: $4$ items cost "
        "€$10$.",
    ExemplarKey("interpreting-graphs-qualitatively", "kp2", 1):
        "The highest point of a height graph marks the maximum height.",
    ExemplarKey("interpreting-graphs-qualitatively", "kp3", 0):
        "A steeper line covers more distance in the same time, so runner A "
        "is faster.",
    ExemplarKey("interpreting-graphs-qualitatively", "kp3", 1):
        "The steep part rises more per unit of time, so growth was faster at "
        "the start.",
    ExemplarKey("interpreting-linear-models", "kp1", 0):
        "In $y = mx + b$, $m$ is the slope: the cost added per one more hour.",
    ExemplarKey("interpreting-linear-models", "kp1", 1):
        "The slope $-5$ is the rate of change: the tank loses $5$ L each "
        "minute.",
    ExemplarKey("interpreting-linear-models", "kp2", 0):
        "The intercept is the value at $x = 0$: the tank starts with $50$ L.",
    ExemplarKey("interpreting-linear-models", "kp2", 1):
        "The intercept is the cost at $h = 0$: a fixed €$40$ charged no "
        "matter what.",
}
