"""New held-out exemplars for the remaining topics of
`curriculum/foundations/03-expressions-equations.yaml`: rearranging
formulas, literal equations, absolute value equations, and the
translation/word-problem topics.
"""
from __future__ import annotations

from foundations_curriculum_patch import ExemplarKey
from symbolic_common import EXACT, label_contract, mk

KP = tuple

NEW: dict[KP, list] = {
    ("rearranging-formulas", "kp1"): [
        mk(
            "Solve $p = q - r$ for $q$.", "q = p + r", "Add $r$ to both sides.",
            contract=EXACT,
        ),
        mk(
            "Solve $A = bh$ for $h$.", "h = A/b", "Divide both sides by $b$.",
            contract=EXACT,
        ),
    ],
    ("rearranging-formulas", "kp2"): [
        mk(
            "Solve $V = lwh$ for $w$ (treat $l$ and $h$ as known).", "w = V/(l*h)",
            "Divide both sides by $l$ and by $h$.", contract=EXACT,
        ),
        mk(
            "Solve $y = mx + b$ for $x$ (treat $m$ and $b$ as known).", "x = (y - b)/m",
            "Subtract $b$, then divide by $m$.", contract=EXACT,
        ),
    ],
    ("literal-equations", "kp1"): [
        mk(
            "Solve $px + q = r$ for $x$.", "x = (r - q)/p",
            "Subtract $q$, then divide by $p$.", contract=EXACT,
        ),
        mk(
            "Solve $V = \\frac{1}{3}Bh$ for $B$.", "B = 3V/h",
            "Multiply by $3$, divide by $h$.", contract=EXACT,
        ),
    ],
    ("literal-equations", "kp2"): [
        mk(
            "Solve $mx - nx = k$ for $x$.", "x = k/(m - n)",
            "Factor: $x(m - n) = k$; divide by $(m - n)$.", contract=EXACT,
        ),
        mk(
            "Solve $y = \\frac{x + 5}{3}$ for $x$.", "x = 3y - 5",
            "$3y = x + 5$; $x = 3y - 5$.", contract=EXACT,
        ),
    ],
    ("literal-equations", "kp3"): [
        mk(
            "Solve $P = 2(a + b)$ for $b$.", "b = P/2 - a",
            "Divide by $2$: $\\frac{P}{2} = a + b$; subtract $a$.", contract=EXACT,
        ),
        mk(
            "Solve $T = \\frac{1}{2}k(m + n)$ for $m$.", "m = 2T/k - n",
            "Multiply by $2$: $2T = k(m + n)$; divide by $k$; subtract $n$.",
            contract=EXACT,
        ),
    ],
    ("basic-absolute-value-equations", "kp1"): [
        mk("Solve $|x| = 15$.", "x = 15 or x = -15", "Two numbers sit $15$ units from $0$."),
        mk("Solve $|x| = 20$.", "x = 20 or x = -20", "Two numbers sit $20$ units from $0$."),
    ],
    ("basic-absolute-value-equations", "kp2"): [
        mk(
            "Solve $|x - 6| = 9$.", "x = 15 or x = -3",
            "$x - 6 = 9 \\to x = 15$; $x - 6 = -9 \\to x = -3$.",
        ),
        mk(
            "Solve $|x + 7| = 3$.", "x = -4 or x = -10",
            "$x + 7 = 3 \\to x = -4$; $x + 7 = -3 \\to x = -10$.",
        ),
    ],
    ("absolute-value-equations", "kp1"): [
        mk(
            "Solve $|4x - 4| = 16$.", "x = 5 or x = -3",
            "$4x - 4 = 16 \\to x = 5$; $4x - 4 = -16 \\to x = -3$.",
        ),
        mk(
            "Solve $|2x + 5| = 13$.", "x = 4 or x = -9",
            "$2x + 5 = 13 \\to x = 4$; $2x + 5 = -13 \\to x = -9$.",
        ),
    ],
    ("absolute-value-equations", "kp2"): [
        mk(
            "Solve $|x - 1| + 2 = 9$.", "x = 8 or x = -6",
            "$|x - 1| = 7$; $x - 1 = 7$ or $x - 1 = -7$.",
        ),
        mk(
            "Solve $3|x| = 21$.", "x = 7 or x = -7", "$|x| = 7$; $x = 7$ or $x = -7$.",
        ),
    ],
}

NO_SOLUTION = label_contract(["no solution"])

NEW[("absolute-value-equations", "kp3")] = [
    mk(
        "Solve $|x + 3| = -5$.", "no solution", "An absolute value is never negative.",
        contract=NO_SOLUTION,
    ),
    mk(
        "Solve $|3x| + 4 = 1$.", "no solution",
        "Isolating gives $|3x| = -3$, impossible.", contract=NO_SOLUTION,
    ),
]

NEW[("translating-sentences-to-equations", "kp1")] = [
    mk(
        "A number decreased by $9$ is $14$. Find the number.", "23",
        "$x - 9 = 14$; $x = 23$.",
    ),
    mk(
        "Three times a number plus $8$ is $29$. Find the number.", "7",
        "$3x + 8 = 29$; $3x = 21$; $x = 7$.",
    ),
]

EQ_9N9_41 = label_contract(["4n + 9 = 41"])
EQ_X6_9 = label_contract(["x/6 = 9"])

NEW[("translating-sentences-to-equations", "kp2")] = [
    mk(
        "Write an equation: nine more than four times a number $n$ is $41$.",
        "4n + 9 = 41", "\"is\" marks the equals sign: $4n + 9 = 41$.", contract=EQ_9N9_41,
    ),
    mk(
        "Write an equation: the quotient of a number $x$ and $6$ is $9$.",
        "x/6 = 9", "The quotient of $x$ and $6$ is $x/6$.", contract=EQ_X6_9,
    ),
]

NEW[("consecutive-integer-problems", "kp1")] = [
    mk(
        "Five less than four times a number is $23$. Find the number.", "7",
        "$4x - 5 = 23$; $4x = 28$; $x = 7$.",
    ),
    mk(
        "The sum of a number and three times the number is $32$. Find the number.", "8",
        "$x + 3x = 32$; $4x = 32$; $x = 8$.",
    ),
]

NEW[("consecutive-integer-problems", "kp2")] = [
    mk(
        "Two consecutive integers sum to $35$. Find them.", "17 and 18",
        "$x + (x + 1) = 35$; $2x + 1 = 35$; $x = 17$.",
        contract=label_contract(["17 and 18"]),
    ),
]

NEW[("money-geometry-problems", "kp1")] = [
    mk(
        "Tickets cost €6 each and you spent €42. How many tickets did you buy?", "7",
        "$6x = 42$; $x = 7$.", contract=EXACT,
    ),
    mk(
        "A taxi charges a €4 base fare plus €3 per kilometre. The fare was €22. How many "
        "kilometres was the ride?",
        "6", "$3k + 4 = 22$; $3k = 18$; $k = 6$.", contract=EXACT,
    ),
]
NEW[("money-geometry-problems", "kp2")] = [
    mk(
        "The length of a rectangle is $5$ more than its width and the perimeter is $34$. "
        "Find the width.",
        "6", "$2w + 2(w + 5) = 34$; $4w + 10 = 34$; $w = 6$.", contract=EXACT,
    ),
    mk(
        "A triangle has sides $x$, $x + 3$, and $x + 5$, and its perimeter is $32$. Find "
        "the shortest side.",
        "8", "$3x + 8 = 32$; $3x = 24$; $x = 8$.", contract=EXACT,
    ),
]

NEW[("equation-word-problems", "kp1")] = [
    mk(
        "Two clubs have $60$ members combined; the chess club has three times as many "
        "members as the debate club. How many are in the chess club?",
        "45", "Debate $x$, chess $3x$: $4x = 60$; $x = 15$; chess has $45$.",
        contract=EXACT,
    ),
    mk(
        "A $40$ m rope is cut into two pieces; one piece is $6$ m longer than the other. "
        "How long is the shorter piece?",
        "17", "$x + (x + 6) = 40$; $2x = 34$; $x = 17$.", contract=EXACT,
    ),
]
NEW[("equation-word-problems", "kp2")] = [
    mk(
        "Plan A costs €15 plus €4 per class; Plan B costs €5 plus €6 per class. For how "
        "many classes do the plans cost the same?",
        "5", "$15 + 4c = 5 + 6c$; $10 = 2c$; $c = 5$.", contract=EXACT,
    ),
    mk(
        "Tank A holds $280$ L and drains at $12$ L/min; tank B holds $160$ L and drains "
        "at $4$ L/min. After how many minutes do they hold the same amount?",
        "15", "$280 - 12t = 160 - 4t$; $120 = 8t$; $t = 15$.", contract=EXACT,
    ),
]
NEW[("equation-word-problems", "kp3")] = [
    mk(
        "Nora is $6$ years older than Omar, and the sum of their ages is $28$. How old is "
        "Nora?",
        "17", "Omar $x$, Nora $x + 6$: $2x + 6 = 28$; $x = 11$; Nora is $17$.",
        contract=EXACT,
    ),
    mk(
        "A father is $5$ times as old as his son. In $10$ years he will be $3$ times as "
        "old as his son. How old is the son now?",
        "10", "$5s + 10 = 3(s + 10)$; $5s + 10 = 3s + 30$; $s = 10$.", contract=EXACT,
    ),
]

CONTRACTS: dict[ExemplarKey, str] = {
    ExemplarKey("absolute-value-equations", "kp3", 0): NO_SOLUTION,
    ExemplarKey("absolute-value-equations", "kp3", 1): NO_SOLUTION,
    ExemplarKey("translating-sentences-to-equations", "kp2", 0): label_contract(
        ["3n + 5 = 26"]
    ),
    ExemplarKey("translating-sentences-to-equations", "kp2", 1): label_contract(["y/4 = 12"]),
    ExemplarKey("consecutive-integer-problems", "kp2", 0): label_contract(["13 and 14"]),
    ExemplarKey("consecutive-integer-problems", "kp2", 1): label_contract(
        ["13, 14, and 15"]
    ),
    ExemplarKey("consecutive-integer-problems", "kp2", 2): label_contract(["22 and 24"]),
}
