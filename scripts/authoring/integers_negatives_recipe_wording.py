"""Final diversified wording for generated integers-negatives exemplars."""
from __future__ import annotations

from dataclasses import replace

from foundations_curriculum_patch import KpKey, NewExemplar

_WORDING: dict[tuple[KpKey, int], dict[str, str]] = {
    (KpKey('plotting-integers', 'kp2'), 1): {
        'solution_sketch': 'Three units right of $0$ reaches $3$; from $3$, seven units left lands on $-4$.',
    },
    (KpKey('integer-order-of-operations', 'kp2'), 1): {
        'problem': 'Multiply $3$ by the quantity $4 - 10$.',
        'solution_sketch': 'Inside the quantity first: $4 - 10 = -6$; $3 \\times (-6) = -18$.',
    },
    (KpKey('integer-order-of-operations', 'kp3'), 0): {
        'problem': 'Multiply $2$ by $|-9|$, then subtract $15$.',
    },
    (KpKey('integer-order-of-operations', 'kp3'), 1): {
        'problem': 'Find $|4 - 11|$ and add $-20$ to the result.',
        'solution_sketch': '$|4 - 11| = |-7| = 7$; adding $-20$ to that leaves $-13$.',
    },
    (KpKey('ordering-rational-numbers', 'kp1'), 0): {
        'problem': 'Order $-0.7$ and $-0.4$; which is larger?',
        'solution_sketch': '$-0.4$ is closer to $0$ than $-0.7$ is, so $-0.4$ is the larger value.',
    },
    (KpKey('ordering-rational-numbers', 'kp1'), 1): {
        'problem': 'Between $-\\frac{3}{5}$ and $-\\frac{7}{10}$, which represents the greater value?',
    },
    (KpKey('signed-decimal-operations', 'kp2'), 0): {
        'problem': 'Divide $-7.2$ by $-0.8$.',
    },
    (KpKey('adding-subtracting-negative-fractions', 'kp1'), 0): {
        'problem': 'Subtract $\\frac{1}{4}$ from $-\\frac{3}{8}$.',
    },
    (KpKey('adding-subtracting-negative-fractions', 'kp2'), 0): {
        'problem': 'Find $\\frac{1}{4}$ minus $\\frac{5}{6}$.',
    },
    (KpKey('negative-fractions-decimals', 'kp3'), 0): {
        'problem': 'Subtract $1\\frac{1}{2}$ from $-1\\frac{1}{4}$.',
    },
    (KpKey('exponent-notation', 'kp1'), 0): {
        'problem': 'Evaluate $5^3$.',
    },
    (KpKey('exponent-notation', 'kp2'), 0): {
        'problem': 'Find the value of $2^4$.',
    },
    (KpKey('integer-exponents-intro', 'kp1'), 0): {
        'problem': 'What is $(-4)^2$?',
    },
    (KpKey('integer-exponents-intro', 'kp1'), 1): {
        'problem': 'Find the value of $(-4)^4$.',
    },
    (KpKey('integer-exponents-intro', 'kp2'), 0): {
        'problem': 'Evaluate $-3^3$.',
    },
    (KpKey('integer-exponents-intro', 'kp2'), 1): {
        'problem': 'What is $(-3)^3$?',
    },
}


def apply_wording(recipes: dict[KpKey, list[NewExemplar]]) -> None:
    """Apply reviewed wording without changing computed answers or contracts."""
    for (key, index), changes in _WORDING.items():
        recipes[key][index] = replace(recipes[key][index], **changes)
