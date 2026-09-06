"""New held-out exemplars for the proportionality, two-variable-solution,
and linear-model-application topics of
`curriculum/foundations/04-linear-graphs.yaml`.
"""
from __future__ import annotations

from symbolic_common import label_contract, mk, unit_contract

KP = tuple

NEW: dict[KP, list] = {
    ("constant-of-proportionality", "kp1"): [
        mk(
            "$y$ is proportional to $x$, and $y = 24$ when $x = 8$. Find the constant "
            "$k$.",
            "3", "$k = \\frac{24}{8} = 3$.",
        ),
        mk(
            "$y$ is proportional to $x$, and $y = 15$ when $x = 4$. Find $k$.", "3.75",
            "$k = \\frac{15}{4} = 3.75$.",
        ),
    ],
    ("constant-of-proportionality", "kp2"): [
        mk(
            "The table $(3, 6)$, $(6, 12)$, $(9, 18)$ is proportional. What is $k$?", "2",
            "Any row gives $k$: $\\frac{6}{3} = 2$.",
        ),
        mk(
            "The table $(2, 9)$, $(4, 18)$ is proportional. What is $k$?", "4.5",
            "$\\frac{9}{2} = 4.5$.",
        ),
    ],
    ("constant-of-proportionality", "kp3"): [
        mk(
            "$4$ kilograms of pears cost €9. What is the price per kilogram, in euros?",
            "2.25", "$9 \\div 4 = 2.25$.",
        ),
        mk(
            "A car travels $150$ kilometres in $2$ hours at constant speed. What is its "
            "unit rate, in km per hour?",
            "75", "$150 \\div 2 = 75$.",
        ),
    ],
    ("proportional-relationships", "kp1"): [
        mk(
            "$y$ is proportional to $x$ with $k = 3.5$. Find $y$ when $x = 4$.", "14",
            "$y = 3.5(4) = 14$.",
        ),
        mk(
            "A proportional relationship passes through $(4, 32)$. Its equation is "
            "$y = kx$. Find $k$.",
            "8", "$k = \\frac{32}{4} = 8$.",
        ),
    ],
    ("proportional-relationships", "kp2"): [
        mk(
            "A proportional graph passes through the origin and $(2, 5)$. Find $y$ when "
            "$x = 8$.",
            "20", "$k = 2.5$, so $y = 2.5(8) = 20$.",
        ),
        mk(
            "Is the table $(2, 6)$, $(4, 12)$, $(5, 16)$ proportional?", "no",
            "$\\frac{6}{2} = \\frac{12}{4} = 3$ but $\\frac{16}{5} \\ne 3$.",
            contract=label_contract("yes", "no"),
        ),
    ],
    ("proportional-relationships", "kp3"): [
        mk(
            "$y$ is proportional to $x$, and $y = 24$ when $x = 6$. Find $x$ when "
            "$y = 60$.",
            "15", "$k = 4$; $60 = 4x$ gives $x = 15$.",
        ),
        mk(
            "Earnings follow $y = 9x$ euros for $x$ hours. How many hours to earn €108?",
            "12", "$108 = 9x$; $x = 12$.",
        ),
    ],
    ("solutions-of-two-variable-equations", "kp1"): [
        mk(
            "Is $(3, 7)$ a solution of $y = 2x + 1$?", "yes", "$2(3) + 1 = 7$.",
            contract=label_contract("yes", "no"),
        ),
        mk(
            "Is $(2, 5)$ a solution of $3x + y = 12$?", "no", "$3(2) + 5 = 11 \\ne 12$.",
            contract=label_contract("yes", "no"),
        ),
    ],
    ("solutions-of-two-variable-equations", "kp2"): [
        mk("For $y = 3x + 2$, find $y$ when $x = 5$.", "17", "$3(5) + 2 = 17$."),
        mk(
            "For $2x + y = 9$, find $y$ when $x = 3$.", "3", "$6 + y = 9$; $y = 3$.",
        ),
    ],
    ("solutions-of-two-variable-equations", "kp3"): [
        mk(
            "For $y = 3x - 4$, find $x$ when $y = 11$.", "5", "$11 = 3x - 4$; $x = 5$.",
        ),
        mk(
            "For $x + 3y = 18$, find $x$ when $y = 4$.", "6", "$x + 12 = 18$; $x = 6$.",
        ),
    ],
}

NEW[("interpreting-linear-models", "kp3")] = [
    mk(
        "A tank holds $60$ L and drains $4$ L per minute. How much water remains after "
        "$5$ minutes?",
        "40 L", "$y = -4(5) + 60 = 40$.", contract=unit_contract("volume", "L"),
    ),
    mk(
        "A population follows $P = 6t + 200$ (people after $t$ years). Find $P$ at "
        "$t = 8$.",
        "248", "$6(8) + 200 = 248$.",
    ),
]

NEW[("linear-word-problems", "kp1")] = [
    mk(
        "A delivery service charges €5 plus €3 per package. Write the cost equation "
        "for $p$ packages.",
        "y = 3p + 5", "The rate €3 is the slope; the flat €5 is the y-intercept.",
    ),
    mk(
        "Using the model $y = 3p + 5$, what is the cost of delivering $9$ packages?",
        "€32",
        "$3(9) + 5 = 32$.",
    ),
]
NEW[("linear-word-problems", "kp2")] = [
    mk(
        "A burning candle is $18$ cm tall after $1$ hour and $10$ cm tall after $5$ "
        "hours. Write its height model $h = mt + b$.",
        "h = -2t + 20", "$m = \\frac{10 - 18}{5 - 1} = -2$; $b = 18 - (-2)(1) = 20$.",
    ),
    mk(
        "A gym membership costs €120 total after $2$ months and €200 after $6$ "
        "months. Write the cost model.",
        "y = 20m + 80", "$m = \\frac{200 - 120}{6 - 2} = 20$; $b = 120 - 20(2) = 80$.",
    ),
]
NEW[("linear-word-problems", "kp3")] = [
    mk(
        "You start with €15 and save €9 per week. After how many weeks will you have "
        "€96?",
        "9", "$9x + 15 = 96$; $9x = 81$; $x = 9$.",
    ),
    mk(
        "A plant is $5$ cm tall and grows $4$ cm per week. After how many weeks is it "
        "$33$ cm?",
        "7", "$4x + 5 = 33$; $x = 7$.",
    ),
]
