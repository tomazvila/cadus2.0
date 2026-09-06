"""New held-out exemplars for the expression-manipulation topics of
`curriculum/foundations/03-expressions-equations.yaml`.

Every numeric answer below is a literal Python expression evaluated at
import time (e.g. `3 * 4 + 5`), never a hand-typed value beside a different
problem string, so the two cannot silently drift apart.
"""
from __future__ import annotations

from symbolic_common import label_contract, mk

KP = tuple

YES_NO = label_contract("yes", "no")

NEW: dict[KP, list] = {
    ("parts-of-an-expression", "kp1"): [
        mk(
            "How many terms does $5x - 3y + 8$ have?", "3",
            "Terms are separated by $+$ and $-$: $5x$, $-3y$, $8$.",
        ),
    ],
    ("parts-of-an-expression", "kp2"): [
        mk(
            "In $6x + 4y - 3x + 9$, one other term is a like term of $6x$. Which term is it?",
            "-3x", "Like terms have the same variable part; $6x$ and $-3x$ both have $x$.",
        ),
        mk(
            "Are $5b^2$ and $5b$ like terms? Answer yes or no.", "no",
            "The variable parts $b^2$ and $b$ differ.", contract=YES_NO,
        ),
    ],
    ("substituting-values", "kp1"): [
        mk("Evaluate $4x - 5$ when $x = 6$.", str(4 * 6 - 5), "$4(6) - 5 = 19$."),
    ],
    ("substituting-values", "kp2"): [
        mk(
            "Evaluate $3ab$ when $a = 4$ and $b = 2$.", str(3 * 4 * 2),
            "$3 \\times 4 \\times 2 = 24$.",
        ),
        mk(
            "Evaluate $2x + 5y$ when $x = 3$ and $y = 2$.", str(2 * 3 + 5 * 2),
            "$6 + 10 = 16$.",
        ),
    ],
    ("evaluating-expressions", "kp1"): [
        mk(
            "Evaluate $x^2 + 2y$ when $x = 2$ and $y = 5$.", str(2**2 + 2 * 5),
            "$4 + 10 = 14$.",
        ),
        mk("Evaluate $3a^2$ when $a = 2$.", str(3 * 2**2), "$3 \\times 4 = 12$."),
    ],
    ("evaluating-expressions", "kp2"): [
        mk(
            "Evaluate $x^2 - 4x$ when $x = -3$.", str((-3) ** 2 - 4 * (-3)),
            "$9 + 12 = 21$.",
        ),
        mk(
            "Evaluate $2x^2 - 5x$ when $x = -3$.", str(2 * (-3) ** 2 - 5 * (-3)),
            "$2(-3)^2 - 5(-3) = 18 + 15 = 33$.",
        ),
    ],
    ("evaluating-expressions", "kp3"): [
        mk(
            "Evaluate $-x^2 + 4x - 1$ when $x = -2$.",
            str(-((-2) ** 2) + 4 * (-2) - 1),
            "$-(-2)^2 + 4(-2) - 1 = -4 - 8 - 1 = -13$.",
        ),
        mk(
            "Evaluate $3x^2 - x + 2$ when $x = -2$.", str(3 * (-2) ** 2 - (-2) + 2),
            "$12 + 2 + 2 = 16$.",
        ),
    ],
    ("translating-phrases-to-expressions", "kp1"): [
        mk("Write an expression: $9$ more than a number $m$.", "m + 9", "\"$9$ more than\" adds: $m + 9$."),
        mk("Write an expression: the difference of a number $y$ and $6$.", "y - 6", "The difference subtracts in the stated order: $y - 6$."),
    ],
    ("translating-phrases-to-expressions", "kp2"): [
        mk("Write an expression: the product of $8$ and $v$.", "8v", "Product means multiply: $8v$."),
        mk("Write an expression: the quotient of $z$ and $9$.", "z/9", "Quotient means divide: $z/9$."),
    ],
    ("translating-phrases-to-expressions", "kp3"): [
        mk(
            "Write an expression: $5$ less than three times a number $m$.", "3m - 5",
            '"$5$ less than" reverses the order: start with $3m$, subtract $5$.',
        ),
        mk(
            "Write an expression: $7$ more than the product of $6$ and $w$.", "6w + 7", "The product $6w$ comes first, then add $7$.",
        ),
    ],
    ("writing-expressions-from-patterns", "kp1"): [
        mk(
            "A table pairs $n = 1, 2, 3$ with $4, 7, 10$. Write an expression for the value "
            "at position $n$.",
            "3n + 1",
            "Values climb by $3$ each step, so $3n + c$; at $n = 1$, $3 + c = 4$ gives $c = 1$.",
        ),
        mk(
            "A table pairs $n = 1, 2, 3$ with $9, 13, 17$. Write an expression for the value "
            "at position $n$.",
            "4n + 5",
            "Common step $4$; $4(1) + 5 = 9$ checks.",
        ),
    ],
    ("writing-expressions-from-patterns", "kp2"): [
        mk(
            "A parking garage charges €5 to enter plus €3 per hour. Write an expression for "
            "the total cost after $h$ hours.",
            "3h + 5", "The rate $3$ per hour is the coefficient; the flat $5$ is added once.",
        ),
        mk(
            "A fence post pattern uses $4$ posts for $1$ section, $7$ for $2$, $10$ for $3$, "
            "adding $3$ per section. Write an expression for the posts in $n$ sections.",
            "3n + 1",
            "Step $3$ per section; $3(1) + 1 = 4$ checks.",
        ),
    ],
    ("evaluating-formulas", "kp1"): [
        mk(
            "Use $A = \\frac{1}{2}bh$ to find the area when $b = 12$ and $h = 5$.",
            str(12 * 5 // 2), "$\\frac{1}{2}(12)(5) = 30$.",
        ),
    ],
    ("evaluating-formulas", "kp2"): [
        mk("Use $d = rt$ to find the distance when $r = 60$ and $t = 4$.", str(60 * 4), "$60 \\times 4 = 240$."),
        mk(
            "Use $E = \\frac{1}{2}mv^2$ to find the kinetic energy in joules when $m = 2$ "
            "and $v = 5$.",
            str(2 * 5**2 // 2),
            "$\\frac{1}{2}(2) = 1$; $1 \\times 5^2 = 25$.",
        ),
    ],
    ("combining-like-terms", "kp1"): [
        mk("Simplify $4x + 6x$.", "10x", "$4 + 6 = 10$, so $10x$."),
        mk("Simplify $2a + 5a + a$.", "8a", "$2 + 5 + 1 = 8$, so $8a$."),
    ],
    ("combining-like-terms", "kp2"): [
        mk("Simplify $9y - 2y$.", "7y", "$9 - 2 = 7$, so $7y$."),
        mk("Simplify $3x - 8x$.", "-5x", "$3 - 8 = -5$, so $-5x$."),
    ],
    ("combining-like-terms", "kp3"): [
        mk("Simplify $4x + 5 + 3x - 9$.", "7x - 4", "$(4x + 3x) + (5 - 9) = 7x - 4$."),
        mk("Simplify $5a + 3b - 2a + 2b$.", "3a + 5b", "$(5a - 2a) + (3b + 2b) = 3a + 5b$."),
    ],
    ("distributive-property", "kp1"): [
        mk("Expand $4(x + 5)$.", "4x + 20", "$4 \\cdot x = 4x$; $4 \\cdot 5 = 20$."),
        mk("Expand $3(2a + 3)$.", "6a + 9", "$3 \\cdot 2a = 6a$; $3 \\cdot 3 = 9$."),
    ],
    ("distributive-property", "kp2"): [
        mk("Expand $-3(x - 4)$.", "-3x + 12", "$-3 \\cdot x = -3x$; $-3 \\cdot (-4) = +12$."),
        mk("Expand $-(2y + 5)$.", "-2y - 5", "Distributing $-1$: $-2y - 5$."),
    ],
    ("distributive-property", "kp3"): [
        mk(
            "Expand and simplify $3(x + 2) + 2x$.", "5x + 6", "$3x + 6 + 2x = 5x + 6$.",
        ),
        mk(
            "Expand and simplify $4(a - 1) - (a + 2)$.", "3a - 6", "$4a - 4 - a - 2 = 3a - 6$.",
        ),
    ],
    ("factoring-linear-expressions", "kp1"): [
        mk("Factor $8x + 12$.", "4(2x + 3)", "GCF of $8$ and $12$ is $4$: $8x + 12 = 4(2x + 3)$."),
        mk("Factor $15a - 20$.", "5(3a - 4)", "GCF of $15$ and $20$ is $5$: $15a - 20 = 5(3a - 4)$."),
    ],
    ("factoring-linear-expressions", "kp2"): [
        mk(
            "Factor $-6x - 9$ by pulling out $-3$.", "-3(2x + 3)",
            "$-3 \\cdot 2x = -6x$ and $-3 \\cdot 3 = -9$.",
        ),
        mk("Factor $14x + 21y$.", "7(2x + 3y)", "GCF of $14$ and $21$ is $7$: $14x + 21y = 7(2x + 3y)$."),
    ],
    ("equivalent-expressions", "kp1"): [
        mk(
            "Are $3(x + 4)$ and $3x + 12$ equivalent? Answer yes or no.", "yes",
            "Expanding $3(x + 4)$ gives exactly $3x + 12$.", contract=YES_NO,
        ),
        mk(
            "Are $2x + 5x$ and $10x^2$ equivalent? Answer yes or no.", "no",
            "$2x + 5x = 7x$, not $10x^2$.", contract=YES_NO,
        ),
    ],
    ("equivalent-expressions", "kp2"): [
        mk(
            "Write $3(2x - 1) + 5$ in the form $ax + b$.", "6x + 2", "$6x - 3 + 5 = 6x + 2$.",
        ),
        mk(
            "Write $4(x + 3) - 2(x - 1)$ in the form $ax + b$.", "2x + 14",
            "$4x + 12 - 2x + 2 = 2x + 14$.",
        ),
    ],
    ("checking-a-solution", "kp1"): [
        mk(
            "Is $t = -3$ a solution of $5t + 2 = -13$? Answer yes or no.", "yes",
            "$5(-3) + 2 = -13$.", contract=YES_NO,
        ),
    ],
    ("checking-a-solution", "kp2"): [
        mk(
            "Which of $x = 1$, $x = 2$, $x = 3$ solves $4x - 3 = 5$?", "2", "$4(2) - 3 = 5$.",
        ),
        mk(
            "Which of $y = -2$, $y = 0$, $y = 2$ solves $3y + 7 = 1$?", "-2", "$3(-2) + 7 = 1$.",
        ),
    ],
}
