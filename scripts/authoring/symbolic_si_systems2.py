"""New held-out exemplars and contract retrofits for the system-inequality
and system-word-problem topics of
`curriculum/foundations/05-systems-inequalities.yaml`.

Every prose answer here is closed and deterministic (the exact text this
lane authors), so a single-option `label` policy independently checks it by
exact string match; every numeric answer is checked by the base grammar.
"""
from __future__ import annotations

from foundations_curriculum_patch import ExemplarKey
from symbolic_common import label_contract, mk, unit_contract

KP = tuple

YES_NO = label_contract("yes", "no")

NEW: dict[KP, list] = {
    ("systems-of-linear-inequalities", "kp1"): [
        mk(
            "Is $(2, 0)$ a solution of the system $y \\le x + 1$ and $y > -x$?", "yes",
            "$0 \\le 3$ and $0 > -2$ both hold.", contract=YES_NO,
        ),
        mk(
            "Is $(0, 5)$ a solution of the system $y < 2x + 1$ and $y \\ge x$?", "no",
            "$5 < 1$ fails.", contract=YES_NO,
        ),
    ],
    ("systems-of-linear-inequalities", "kp2"): [
        mk(
            "For the system $y \\le 3x - 1$, $y > -2x + 5$, which boundary line is "
            "dashed?",
            "y = -2x + 5 (the strict one)", "A strict inequality ($>$ or $<$) draws a dashed line.",
            contract=label_contract(["y = -2x + 5 (the strict one)"]),
        ),
        mk(
            "For the system $y \\ge x - 2$, $y < -x + 6$, which boundary line is "
            "dashed?",
            "y = -x + 6 (the strict one)",
            "A strict inequality ($>$ or $<$) draws a dashed line.",
            contract=label_contract(["y = -x + 6 (the strict one)"]),
        ),
    ],
    ("systems-of-linear-inequalities", "kp3"): [
        mk(
            "Which of $(1, 0)$ or $(0, 1)$ satisfies both $y > x$ and $y < x + 3$?",
            "(0, 1)", "$1 > 0$ and $1 < 3$; the other point fails $y > x$.",
        ),
        mk(
            "Does the origin satisfy the system $y \\ge x - 5$ and $y \\le -x + 6$?",
            "yes", "$0 \\ge -5$ and $0 \\le 6$.", contract=YES_NO,
        ),
    ],
}

ADULT6_CHILD6 = label_contract(["6 adult and 6 child"])
COINS_6_4 = label_contract(["6 five-cent and 4 ten-cent coins"])
PENS_NOTEBOOKS = label_contract(["pens €2, notebooks €3"])
BURGERS_CHIPS = label_contract(["burgers €4, chips €2"])

NEW[("systems-money-problems", "kp1")] = [
    mk(
        "Adult tickets cost €9 and child tickets €4. Twelve tickets sold for €78. How "
        "many of each?",
        "6 adult and 6 child", "$a + c = 12$, $9a + 4c = 78$; $a = 6$, $c = 6$.",
        contract=ADULT6_CHILD6,
    ),
    mk(
        "A jar has $5$-cent and $10$-cent coins: $10$ coins worth $70$ cents. How many "
        "of each?",
        "6 five-cent and 4 ten-cent coins", "$n + d = 10$, $5n + 10d = 70$; $n = 6$, "
        "$d = 4$.", contract=COINS_6_4,
    ),
]
NEW[("systems-money-problems", "kp2")] = [
    mk(
        "$4$ pens and $3$ notebooks cost €17; $2$ pens and $3$ notebooks cost €13. Find "
        "each price.",
        "pens €2, notebooks €3", "Subtract: $2p = 4$; $p = 2$; $n = 3$.",
        contract=PENS_NOTEBOOKS,
    ),
    mk(
        "$3$ burgers and $2$ portions of chips cost €16; $1$ burger and $2$ portions of "
        "chips cost €8. Find each price.",
        "burgers €4, chips €2", "Subtract: $2b = 8$; $b = 4$; $f = 2$.",
        contract=BURGERS_CHIPS,
    ),
]
NEW[("systems-money-problems", "kp3")] = [
    mk(
        "Adult tickets cost €12 and student tickets €7. Twenty tickets sold for €205. "
        "How many adult tickets?",
        "13", "$a + s = 20$, $12a + 7s = 205$; $5a = 65$; $a = 13$, $s = 7$, both "
        "nonnegative; check $13 + 7 = 20$ tickets and $12(13) + 7(7) = 205$.",
    ),
    mk(
        "A purse has $25$-cent and $10$-cent coins: $12$ coins worth €2.10. How many "
        "are $25$-cent coins?",
        "6", "$t + f = 12$, $25t + 10f = 210$; $15t = 90$; $t = 6$, $f = 6$, both "
        "nonnegative; check $6 + 6 = 12$ coins and $25(6) + 10(6) = 210$ cents.",
    ),
]

CONTRACTS: dict[ExemplarKey, str] = {
    ExemplarKey("systems-money-problems", "kp1", 0): label_contract(
        ["6 adult and 4 child"]
    ),
    ExemplarKey("systems-money-problems", "kp1", 1): label_contract(
        ["5 five-cent and 7 ten-cent coins"]
    ),
    ExemplarKey("systems-money-problems", "kp2", 0): label_contract(
        ["pens €2, notebooks €3"]
    ),
    ExemplarKey("systems-money-problems", "kp2", 1): label_contract(
        ["burgers €4, chips €3"]
    ),
    ExemplarKey("systems-of-linear-inequalities", "kp2", 0): label_contract(
        ["y = -x + 4 (the strict one)"]
    ),
    ExemplarKey("systems-of-linear-inequalities", "kp2", 1): label_contract(
        ["the overlap (intersection) of the two shaded half-planes"]
    ),
}

NEW[("systems-mixture-problems", "kp1")] = [
    mk(
        "Mixing $x$ L of $20\\%$ acid with $y$ L of $80\\%$ acid to get $10$ L of "
        "$50\\%$ acid: give both equations.",
        "x + y = 10 and 0.20x + 0.80y = 5",
        "Amounts sum to $10$; pure acid sums to $0.50(10) = 5$.",
        contract=label_contract(["x + y = 10 and 0.20x + 0.80y = 5"]),
    ),
    mk(
        "Mixing $x$ L of $30\\%$ acid with $y$ L of $70\\%$ acid to get $16$ L of "
        "$50\\%$ acid: give both equations.",
        "x + y = 16 and 0.30x + 0.70y = 8",
        "Amounts sum to $16$; pure acid sums to $0.50(16) = 8$.",
        contract=label_contract(["x + y = 16 and 0.30x + 0.70y = 8"]),
    ),
]
NEW[("systems-mixture-problems", "kp2")] = [
    mk(
        "How many liters of $10\\%$ acid and $40\\%$ acid make $18$ L of $20\\%$ acid?",
        "12 L of 10% and 6 L of 40%",
        "$x + y = 18$, $0.1x + 0.4y = 3.6$; $x = 12$, $y = 6$.",
        contract=label_contract(["12 L of 10% and 6 L of 40%"]),
    ),
    mk(
        "How many liters of $15\\%$ and $45\\%$ saline make $10$ L of $30\\%$ saline?",
        "5 L of 15% and 5 L of 45%",
        "$x + y = 10$, $0.15x + 0.45y = 3$; $x = 5$, $y = 5$.",
        contract=label_contract(["5 L of 15% and 5 L of 45%"]),
    ),
]
NEW[("systems-mixture-problems", "kp3")] = [
    mk(
        "Coffee at €6/kg is blended with coffee at €10/kg to make $8$ kg worth €7/kg. "
        "How much of each?",
        "6 kg of €6 coffee and 2 kg of €10 coffee",
        "$x + y = 8$, $6x + 10y = 56$; $4x = 24$; $x = 6$; $y = 2$.",
        contract=label_contract(["6 kg of €6 coffee and 2 kg of €10 coffee"]),
    ),
    mk(
        "Almonds at €8/kg are mixed with raisins at €4/kg to make $6$ kg costing €30. "
        "How much of each?",
        "1.5 kg almonds and 4.5 kg raisins",
        "$n + r = 6$, $8n + 4r = 30$; $4n = 6$; $n = 1.5$; $r = 4.5$.",
        contract=label_contract(["1.5 kg almonds and 4.5 kg raisins"]),
    ),
]

CONTRACTS[ExemplarKey("systems-mixture-problems", "kp1", 1)] = label_contract(
    ["x + y = 20 and 0.25x + 0.60y = 8"]
)
CONTRACTS[ExemplarKey("systems-mixture-problems", "kp2", 0)] = label_contract(
    ["20 L of 20% and 10 L of 50%"]
)
CONTRACTS[ExemplarKey("systems-mixture-problems", "kp2", 1)] = label_contract(
    ["8 L of 10% and 4 L of 40%"]
)
CONTRACTS[ExemplarKey("systems-mixture-problems", "kp3", 0)] = label_contract(
    ["7.5 kg of €8 coffee and 2.5 kg of €12 coffee"]
)
CONTRACTS[ExemplarKey("systems-mixture-problems", "kp3", 1)] = label_contract(
    ["3 kg nuts and 6 kg raisins"]
)

NEW[("systems-rate-problems", "kp1")] = [
    mk(
        "A boat travels $15$ km/h downstream and $9$ km/h upstream. Find the boat "
        "speed and current speed.",
        "boat 12 km/h, current 3 km/h", "$b + c = 15$, $b - c = 9$; $b = 12$, $c = 3$.",
        contract=label_contract(["boat 12 km/h, current 3 km/h"]),
    ),
    mk(
        "A plane flies $540$ km/h with the wind and $460$ km/h against it. Find the "
        "plane speed and wind speed.",
        "plane 500 km/h, wind 40 km/h",
        "$p + w = 540$, $p - w = 460$; $p = 500$, $w = 40$.",
        contract=label_contract(["plane 500 km/h, wind 40 km/h"]),
    ),
]
NEW[("systems-rate-problems", "kp2")] = [
    mk(
        "A kayaker paddles $20$ km downstream in $2$ h and back in $4$ h. Find the "
        "kayak and current speeds.",
        "kayak 7.5 km/h, current 2.5 km/h",
        "Speeds $10$ and $5$; $b + c = 10$, $b - c = 5$.",
        contract=label_contract(["kayak 7.5 km/h, current 2.5 km/h"]),
    ),
    mk(
        "A plane flies $600$ km with the wind in $2$ h and back in $3$ h. Find the "
        "plane and wind speeds.",
        "plane 250 km/h, wind 50 km/h",
        "Speeds $300$ and $200$; $p + w = 300$, $p - w = 200$.",
        contract=label_contract(["plane 250 km/h, wind 50 km/h"]),
    ),
]
NEW[("systems-rate-problems", "kp3")] = [
    mk(
        "Two cars $280$ km apart drive toward each other and meet in $4$ h. One is "
        "$10$ km/h faster. Find both speeds.",
        "40 km/h and 30 km/h",
        "Combined speed $\\frac{280}{4} = 70$; $r_1 + r_2 = 70$, $r_1 - r_2 = 10$.",
        contract=label_contract(["40 km/h and 30 km/h"]),
    ),
    mk(
        "Two trains leave stations $440$ km apart toward each other and meet in $4$ h. "
        "Their speeds differ by $10$ km/h. Find both.",
        "60 km/h and 50 km/h", "Combined speed $110$; sum $110$, difference $10$.",
        contract=label_contract(["60 km/h and 50 km/h"]),
    ),
]

CONTRACTS[ExemplarKey("systems-rate-problems", "kp1", 0)] = label_contract(
    ["boat 10 km/h, current 2 km/h"]
)
CONTRACTS[ExemplarKey("systems-rate-problems", "kp1", 1)] = label_contract(
    ["plane 480 km/h, wind 20 km/h"]
)
CONTRACTS[ExemplarKey("systems-rate-problems", "kp2", 0)] = label_contract(
    ["kayak 5 km/h, current 1 km/h"]
)
CONTRACTS[ExemplarKey("systems-rate-problems", "kp2", 1)] = label_contract(
    ["plane 270 km/h, wind 30 km/h"]
)
CONTRACTS[ExemplarKey("systems-rate-problems", "kp3", 0)] = label_contract(
    ["60 km/h and 40 km/h"]
)
CONTRACTS[ExemplarKey("systems-rate-problems", "kp3", 1)] = label_contract(
    ["50 km/h and 40 km/h"]
)

NEW[("systems-word-problems", "kp1")] = [
    mk(
        "The sum of two numbers is $24$ and their difference is $8$. Find the "
        "numbers.",
        "16 and 8", "$x + y = 24$, $x - y = 8$; add: $x = 16$, $y = 8$.",
        contract=label_contract(["16 and 8"]),
    ),
    mk(
        "Two numbers sum to $21$, and one is twice the other. Find them.", "14 and 7",
        "$x + y = 21$, $x = 2y$; $y = 7$, $x = 14$.",
        contract=label_contract(["14 and 7"]),
    ),
]
NEW[("systems-word-problems", "kp2")] = [
    mk(
        "The length of a rectangle is $4$ more than twice its width, and its "
        "perimeter is $32$. Find the dimensions.",
        "width 4, length 12", "$l = 2w + 4$, $2l + 2w = 32$; $6w + 8 = 32$; $w = 4$, "
        "$l = 12$.", contract=label_contract(["width 4, length 12"]),
    ),
    mk(
        "Two angles are complementary and one is $18^\\circ$ more than the other. "
        "Find both.",
        "54 and 36 degrees", "$x + y = 90$, $x = y + 18$; $y = 36$, $x = 54$.",
        contract=label_contract(["54 and 36 degrees"]),
    ),
]
NEW[("systems-word-problems", "kp3")] = [
    mk(
        "The sum of two numbers is $30$; twice the larger minus the smaller is $24$. "
        "Find the numbers.",
        "18 and 12", "$x + y = 30$, $2x - y = 24$; add: $3x = 54$; $x = 18$, $y = 12$.",
        contract=label_contract(["18 and 12"]),
    ),
    mk(
        "Mia is $4$ times as old as Lee and their ages sum to $25$. How old is each?",
        "Mia 20, Lee 5", "$m = 4l$, $m + l = 25$; $5l = 25$; $l = 5$, $m = 20$.",
        contract=label_contract(["Mia 20, Lee 5"]),
    ),
]

CONTRACTS[ExemplarKey("systems-word-problems", "kp1", 0)] = label_contract(
    ["13 and 7"]
)
CONTRACTS[ExemplarKey("systems-word-problems", "kp1", 1)] = label_contract(
    ["10 and 5"]
)
CONTRACTS[ExemplarKey("systems-word-problems", "kp2", 0)] = label_contract(
    ["width 5, length 13"]
)
CONTRACTS[ExemplarKey("systems-word-problems", "kp2", 1)] = label_contract(
    ["105 and 75 degrees"]
)
CONTRACTS[ExemplarKey("systems-word-problems", "kp3", 0)] = label_contract(
    ["15 and 10"]
)
CONTRACTS[ExemplarKey("systems-word-problems", "kp3", 1)] = label_contract(
    ["Sam 17, Tia 13"]
)
