"""Backfill `solution_sketch` for existing `03-expressions-equations.yaml`
exemplars that lost their held-out exemption once this lane's fourth
exemplar moved the held-out index (D-F5's `solutions` fact recomputes over
whichever exemplars are NOT held out; adding a fourth exemplar always shifts
that boundary by one). Each sketch names only that exemplar's own already
authored problem and answer.
"""
from __future__ import annotations

from foundations_curriculum_patch import ExemplarKey

SKETCHES: dict[ExemplarKey, str] = {
    ExemplarKey("parts-of-an-expression", "kp1", 2):
        "The constant term has no variable factor: $-8$.",
    ExemplarKey("substituting-values", "kp1", 1): "$2(10) - 7 = 13$.",
    ExemplarKey("substituting-values", "kp1", 2): "$10 - 2(3) = 4$.",
    ExemplarKey("substituting-values", "kp2", 1): "$2 + 3(4) = 14$.",
    ExemplarKey("translating-phrases-to-expressions", "kp1", 0):
        '"$7$ more than" adds: $n + 7$.',
    ExemplarKey("translating-phrases-to-expressions", "kp1", 1):
        "The difference subtracts in the stated order: $x - 4$.",
    ExemplarKey("translating-phrases-to-expressions", "kp2", 0):
        "Product means multiply: $6w$.",
    ExemplarKey("translating-phrases-to-expressions", "kp2", 1):
        "Quotient means divide: $y/5$.",
    ExemplarKey("translating-phrases-to-expressions", "kp3", 1):
        "The product $4t$ comes first, then add $5$.",
    ExemplarKey("writing-expressions-from-patterns", "kp2", 0):
        "Fixed €20 plus €8 per month: $8m + 20$.",
    ExemplarKey("evaluating-formulas", "kp1", 0): "$8 \\times 5 = 40$.",
    ExemplarKey("evaluating-formulas", "kp1", 2): "$\\frac{1}{2}(10)(7) = 35$.",
    ExemplarKey("evaluating-formulas", "kp2", 0): "$55 \\times 3 = 165$.",
    ExemplarKey("combining-like-terms", "kp1", 0): "$3 + 5 = 8$, so $8x$.",
    ExemplarKey("combining-like-terms", "kp1", 1): "$7 + 2 + 1 = 10$, so $10a$.",
    ExemplarKey("combining-like-terms", "kp2", 0): "$8 - 3 = 5$, so $5y$.",
    ExemplarKey("combining-like-terms", "kp3", 1):
        "$(6a - 2a) + (4b + b) = 4a + 5b$.",
    ExemplarKey("distributive-property", "kp1", 0):
        "$3 \\cdot x = 3x$; $3 \\cdot 4 = 12$.",
    ExemplarKey("distributive-property", "kp1", 1):
        "$5 \\cdot 2a = 10a$; $5 \\cdot 1 = 5$.",
    ExemplarKey("factoring-linear-expressions", "kp1", 1):
        "GCF of $10$ and $15$ is $5$: $10a - 15 = 5(2a - 3)$.",
    ExemplarKey("factoring-linear-expressions", "kp2", 1):
        "GCF of $12$ and $18$ is $6$: $12x + 18y = 6(2x + 3y)$.",
    ExemplarKey("rearranging-formulas", "kp1", 0):
        "Subtract $b$ from both sides: $x = y - b$.",
    ExemplarKey("basic-absolute-value-equations", "kp1", 1):
        "Two numbers sit $12$ units from $0$.",
    ExemplarKey("translating-sentences-to-equations", "kp2", 0):
        '"is" marks the equals sign: $3n + 5 = 26$.',
    ExemplarKey("translating-sentences-to-equations", "kp2", 1):
        "The quotient of $y$ and $4$ is $y/4$.",
}
