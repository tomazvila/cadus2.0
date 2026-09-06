"""Backfill `solution_sketch` for existing `05-systems-inequalities.yaml`
exemplars that lost their held-out exemption once this lane's fourth
exemplar moved the held-out index. See `symbolic_expr_eq_solutions.py` for
the same rationale.
"""
from __future__ import annotations

from foundations_curriculum_patch import ExemplarKey

SKETCHES: dict[ExemplarKey, str] = {
    ExemplarKey("solutions-of-inequalities", "kp1", 1): "$4 \\ge 4$ holds with equality.",
    ExemplarKey("graphing-inequalities-number-line", "kp1", 0):
        "$\\ge$ is inclusive, so the circle is closed.",
    ExemplarKey("graphing-inequalities-number-line", "kp1", 1):
        "$<$ is strict, so the circle is open.",
    ExemplarKey("graphing-inequalities-number-line", "kp2", 0): "$>$ points right.",
    ExemplarKey("graphing-inequalities-number-line", "kp2", 1):
        "$\\le$ is inclusive (closed) and points left (less than).",
    ExemplarKey("graphing-inequalities-number-line", "kp3", 0):
        "A closed circle is inclusive; left means less than or equal.",
    ExemplarKey("graphing-inequalities-number-line", "kp3", 1):
        "An open circle is strict; right means greater than.",
    ExemplarKey("one-step-inequalities", "kp1", 0): "Subtract $5$: $x > 4$.",
    ExemplarKey("one-step-inequalities", "kp1", 1): "Add $3$: $x \\le 5$.",
    ExemplarKey("one-step-inequalities", "kp2", 0): "Divide by $4$: $x < 5$.",
    ExemplarKey("one-step-inequalities", "kp2", 1): "Multiply by $3$: $x \\ge 6$.",
    ExemplarKey("two-step-inequalities", "kp1", 1): "$3x > 15$; $x > 5$.",
    ExemplarKey("two-step-inequalities", "kp2", 1):
        "$\\frac{x}{4} \\ge 2$; $x \\ge 8$.",
    ExemplarKey("writing-inequalities-from-statements", "kp1", 0):
        "\"At least\" means $\\ge$.",
    ExemplarKey("writing-inequalities-from-statements", "kp1", 1):
        "\"Fewer than\" is strict: $n < 8$.",
    ExemplarKey("writing-inequalities-from-statements", "kp2", 0):
        "\"At most\" is $\\le$; the product $2x$ comes first.",
    ExemplarKey("writing-inequalities-from-statements", "kp2", 1):
        "\"Exceeds\" is a strict $>$.",
    ExemplarKey("writing-inequalities-from-statements", "kp3", 0):
        "The fewest value meeting $n \\ge 30$ is $30$ itself.",
    ExemplarKey("writing-inequalities-from-statements", "kp3", 1):
        "\"At most\" means $\\le$.",
    ExemplarKey("and-or-inequalities", "kp1", 0): "$3$ lies between $1$ and $5$.",
    ExemplarKey("and-or-inequalities", "kp2", 1): "Divide all parts by $2$.",
    ExemplarKey("and-or-inequalities", "kp3", 0): "$x - 1 < -2$ gives $x < -1$; "
        "$x - 1 > 3$ gives $x > 4$.",
    ExemplarKey("and-or-inequalities", "kp3", 1): "$2x \\le -6$ gives $x \\le -3$; "
        "$x + 1 > 5$ gives $x > 4$.",
    ExemplarKey("compound-inequalities", "kp1", 1):
        "Subtract $1$: $2 < 2x \\le 10$; divide by $2$.",
    ExemplarKey("basic-absolute-value-inequalities", "kp1", 0):
        "One bounded interval within $5$ units of $0$: $-5 < x < 5$.",
    ExemplarKey("basic-absolute-value-inequalities", "kp1", 1):
        "Inclusive at both boundaries.",
    ExemplarKey("basic-absolute-value-inequalities", "kp2", 0):
        "Points more than $4$ from zero.",
    ExemplarKey("basic-absolute-value-inequalities", "kp2", 1):
        "Points at least $2$ from zero.",
    ExemplarKey("absolute-value-inequalities", "kp2", 1):
        "$x + 4 > 2$ or $x + 4 < -2$.",
    ExemplarKey("graphing-linear-inequalities", "kp1", 1):
        "Inclusive inequality ($\\le$) $\\to$ solid.",
    ExemplarKey("graphing-systems", "kp1", 1): "$2x = x + 3$; $x = 3$; $y = 6$.",
    ExemplarKey("graphing-systems", "kp2", 1):
        "A horizontal and a vertical line meet where their fixed values pair up.",
    ExemplarKey("systems-of-linear-inequalities", "kp2", 0):
        "A strict inequality ($>$ or $<$) draws a dashed line.",
    ExemplarKey("systems-of-linear-inequalities", "kp2", 1):
        "A point solves the system only where both shaded regions overlap.",
    ExemplarKey("interval-notation", "kp2", 1):
        "Every value at or below $-1$; the round bracket at $-\\infty$ never "
        "closes, and the square bracket at $-1$ includes it.",
    ExemplarKey("interval-notation", "kp2", 2):
        "A square bracket at $4$ becomes $\\ge$; the ray extends right without "
        "bound.",
    ExemplarKey("interval-notation", "kp3", 2):
        "The interval $(-\\infty, 1)$ is $x < 1$; $[5, \\infty)$ is $x \\ge 5$.",
}
