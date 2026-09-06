"""Bounded-choice interpretation exemplars for linear graphs and models.

Every learner-facing prompt names a finite, mutually exclusive choice set.
The matching label contract contains the same choices, so grading cannot
silently turn an open-ended explanation into an exact-string password.
"""
from __future__ import annotations

from foundations_curriculum_patch import ExemplarKey
from symbolic_common import label_contract, mk

KP = tuple

CHANGE = label_contract("increasing", "decreasing", "constant")
MOTION = label_contract(
    ["standing still", "standing still (distance is not changing)"],
    ["moving at constant nonzero speed", "traveling at a constant speed"],
    "speeding up",
)
CAR_MOTION = label_contract(
    ["stopped", "standing still"],
    ["traveling at a constant speed", "traveling at a constant speed (60 km/h, not changing)"],
    "speeding up",
)
COST_POINT = label_contract(
    ["4 items cost €10", "4 items cost €10 in total"],
    "10 items cost €4",
    "each item costs €4",
)
HIKER_POINT = label_contract(
    ["after 3 hours, the hiker has walked 12 km"],
    "after 12 hours, the hiker has walked 3 km",
    "the hiker walks 4 km per hour",
)
BALL_FEATURE = label_contract(
    ["maximum height", "the maximum height the ball reaches"],
    "starting height",
    "landing time",
)
PROFIT_FEATURE = label_contract(
    ["break-even point", "the break-even point (zero profit)"],
    "maximum profit",
    "starting cost",
)
RUNNER = label_contract("runner A", "runner B", "equal speed")
ACCOUNT = label_contract("account X", "account Y", "equal growth")
WHEN = label_contract(
    ["at the start", "at the start (the steep part)"],
    "in the middle",
    "at the end",
)
HOUR = label_contract(
    ["the first hour", "first hour"],
    ["the second hour", "the second hour (the steep part)"],
    "both hours equally",
)

PHONE_RATE = label_contract(
    ["the cost per minute", "the cost per minute of use (€0.08 per minute)"],
    "the fixed monthly fee",
    "the total cost after one minute",
)
CANDLE_RATE = label_contract(
    ["height decreases 0.5 cm per minute", "the candle burns down 0.5 cm per minute"],
    "starting height is 0.5 cm",
    "height increases 0.5 cm per minute",
)
PHONE_START = label_contract(
    ["the fixed monthly fee", "the fixed monthly fee regardless of minutes used (€15)"],
    "the cost per minute",
    "the cost after 15 minutes",
)
CANDLE_START = label_contract(
    ["starting height", "the candle's starting height (20 cm)"],
    "burn rate",
    "height after 20 minutes",
)
REPAIR_RATE = label_contract(
    ["cost per hour", "the cost per hour of work (€12 per hour)"],
    "fixed fee",
    "total cost after 12 hours",
)
TANK_RATE = label_contract(
    ["drains 5 L per minute", "the tank drains 5 L per minute"],
    "starts with 5 L",
    "fills 5 L per minute",
)
TANK_START = label_contract(
    ["starting amount", "the starting amount of water (50 L)"],
    "drain rate",
    "amount after 50 minutes",
)
REPAIR_START = label_contract(
    ["fixed fee", "the fixed fee charged regardless of hours (€40)"],
    "hourly rate",
    "cost after 40 hours",
)

NEW: dict[KP, list] = {
    ("interpreting-graphs-qualitatively", "kp1"): [
        mk(
            "A savings-account graph rises steadily from left to right. Is the amount "
            "saved increasing, decreasing, or constant?",
            "increasing",
            "The graph rises left to right, so the amount saved is increasing.",
            contract=CHANGE,
        ),
        mk(
            "A car speed-time graph is flat at $60$ km/h from $t = 1$ to $t = 3$. "
            "Is the car stopped, traveling at a constant speed, or speeding up?",
            "traveling at a constant speed (60 km/h, not changing)",
            "A flat segment at $60$ means a constant nonzero speed.",
            contract=CAR_MOTION,
        ),
    ],
    ("interpreting-graphs-qualitatively", "kp2"): [
        mk(
            "On a distance-versus-time graph, $(3, 12)$ is shown. Does it mean "
            "after 3 hours the hiker has walked 12 km, after 12 hours the hiker has "
            "walked 3 km, or the hiker walks 4 km per hour?",
            "after 3 hours, the hiker has walked 12 km",
            "The ordered pair gives time $3$ and distance $12$.",
            contract=HIKER_POINT,
        ),
        mk(
            "A profit graph crosses the horizontal axis. Does that crossing mark "
            "break-even, maximum profit, or starting cost?",
            "the break-even point (zero profit)",
            "On the horizontal axis the profit coordinate is zero.",
            contract=PROFIT_FEATURE,
        ),
    ],
    ("interpreting-graphs-qualitatively", "kp3"): [
        mk(
            "Account X and account Y start at €0, and X has the steeper line. Which "
            "has faster growth: account X, account Y, or equal growth?",
            "account X",
            "The steeper line has the greater rate.",
            contract=ACCOUNT,
        ),
        mk(
            "A hiking graph is gentle in the first hour and steeper in the second. "
            "Was the hiker faster in the first hour, second hour, or both equally?",
            "the second hour (the steep part)",
            "The steeper segment has the greater distance per hour.",
            contract=HOUR,
        ),
    ],
    ("interpreting-linear-models", "kp1"): [
        mk(
            "For $y = 0.08m + 15$, does $0.08$ mean cost per minute, fixed monthly "
            "fee, or total cost after one minute?",
            "the cost per minute of use (€0.08 per minute)",
            "The coefficient of $m$ is the rate per minute.",
            contract=PHONE_RATE,
        ),
        mk(
            "For candle height $y = -0.5t + 20$, does $-0.5$ mean height decreases "
            "0.5 cm per minute, starting height 0.5 cm, or height increases 0.5 cm per minute?",
            "the candle burns down 0.5 cm per minute",
            "The negative slope is a decrease of $0.5$ cm per minute.",
            contract=CANDLE_RATE,
        ),
    ],
    ("interpreting-linear-models", "kp2"): [
        mk(
            "For $y = 0.08m + 15$, does $15$ mean fixed monthly fee, cost per "
            "minute, or cost after 15 minutes?",
            "the fixed monthly fee regardless of minutes used (€15)",
            "The intercept is the cost at $m=0$.",
            contract=PHONE_START,
        ),
        mk(
            "For candle height $y = -0.5t + 20$, does $20$ mean starting height, "
            "burn rate, or height after 20 minutes?",
            "the candle's starting height (20 cm)",
            "The intercept is the height at $t=0$.",
            contract=CANDLE_START,
        ),
    ],
}

CONTRACTS: dict[ExemplarKey, str] = {
    ExemplarKey("interpreting-graphs-qualitatively", "kp1", 0): MOTION,
    ExemplarKey("interpreting-graphs-qualitatively", "kp1", 1): CHANGE,
    ExemplarKey("interpreting-graphs-qualitatively", "kp2", 0): COST_POINT,
    ExemplarKey("interpreting-graphs-qualitatively", "kp2", 1): BALL_FEATURE,
    ExemplarKey("interpreting-graphs-qualitatively", "kp3", 0): RUNNER,
    ExemplarKey("interpreting-graphs-qualitatively", "kp3", 1): WHEN,
    ExemplarKey("interpreting-linear-models", "kp1", 0): REPAIR_RATE,
    ExemplarKey("interpreting-linear-models", "kp1", 1): TANK_RATE,
    ExemplarKey("interpreting-linear-models", "kp2", 0): TANK_START,
    ExemplarKey("interpreting-linear-models", "kp2", 1): REPAIR_START,
}
