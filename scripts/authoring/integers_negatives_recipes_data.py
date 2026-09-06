"""The per-KP recipe table for `integers-negatives`: 46 knowledge points,
each raised to at least 4 decidable exemplars by 1-2 explicit calls into
`integers_negatives_helpers`, one per KP's own authored constraint.
"""
from __future__ import annotations

from fractions import Fraction

from foundations_curriculum_patch import KpKey, NewExemplar
from integers_negatives_contracts import ASCENDING_CHAIN, LABEL_LT_GT
from integers_negatives_recipe_wording import apply_wording
from integers_negatives_recipes_more import RECIPES as MORE_RECIPES

from integers_negatives_helpers import (
    _abs_combo,
    _abs_value,
    _add,
    _add_three,
    _chain_answer,
    _compare,
    _compare_rationals,
    _decimal_add,
    _decimal_muldiv,
    _distance,
    _exact,
    _frac_add,
    _frac_answer,
    _frac_muldiv,
    _frac_str,
    _gap,
    _left_of_zero,
    _missing_add,
    _missing_sub,
    _mixed_add,
    _mixed_two_term,
    _muldiv_chain,
    _neg_base_power,
    _neg_pow_no_parens,
    _nested_opposite,
    _net_change,
    _opposite,
    _order_least_greatest,
    _order_rationals,
    _product,
    _repeated_change,
    _single_change,
    _sub,
    _three_term_chain,
    _truth,
    _two_moves,
    _which_greater,
)

RECIPES: dict[KpKey, list[NewExemplar]] = {
    # -- plotting-integers ---------------------------------------------------
    # constraint: "real-world contexts (temperature, elevation, money); values from -20 to 20"
    KpKey("plotting-integers", "kp1"): [
        _exact(
            "A scuba diver is $18$ metres below sea level. What integer represents this depth?",
            "-18",
            "Below sea level is negative: a depth of $18$ metres below is $-18$.",
        ),
        # held-out: a money context with a POSITIVE change, distinct from the
        # three prior exemplars, which are all negative-valued contexts.
        _exact(
            "The value of an investment rises by $\\$11$. What integer represents this change?",
            "11",
            "A rise in value is a positive integer: a rise of $\\$11$ is $+11$.",
        ),
    ],
    # constraint: "integers from -10 to 10; moves start at 0"
    KpKey("plotting-integers", "kp2"): [
        _exact(
            "A grasshopper starts at $0$ and hops $9$ units toward the negative direction. What integer is it on now?",
            "-9",
            "Hopping toward the negative direction from $0$ by $9$ units lands on $-9$.",
        ),
        # held-out: a two-step move, a family the first three (single moves)
        # do not cover, landing inside the authored -10..10 range.
        _two_moves(3, 7),
    ],
    # -- opposites-of-integers ------------------------------------------------
    # constraint: "integers from -20 to 20; include 0 occasionally" (already has 3)
    KpKey("opposites-of-integers", "kp1"): [
        # held-out: the boundary of the authored range, not yet exercised.
        _exact(
            "Reflect $-20$ across $0$ on the number line. What integer results?",
            "20",
            "Reflecting across $0$ preserves distance while swapping the side: $-20$ reflects to $20$.",
        ),
    ],
    # constraint: "up to three nested opposites; integers from -12 to 12"
    KpKey("opposites-of-integers", "kp2"): [
        _exact(
            "What is the opposite of the opposite of $8$?",
            "8",
            "An even number of sign flips (two) returns the original number, $8$.",
        ),
        # held-out: the zero case at a single nesting depth, a sub-case the
        # double- and triple-nest exemplars do not touch.
        _exact(
            "Reflecting $0$ across zero, then reflecting the result across zero again, gives what integer?",
            "0",
            "Reflecting $0$ across zero always lands back on $0$, however many times it repeats.",
        ),
    ],
    # -- number-line-integers -------------------------------------------------
    # constraint: "integers from -10 to 10"
    KpKey("number-line-integers", "kp1"): [
        _exact(
            "Which integer is smaller, $-8$ or $-6$?",
            "-8",
            "$-8$ is to the left of $-6$ on the number line, so $-8$ is the smaller value.",
        ),
        # held-out: a five-value chain (existing exemplar orders four), still
        # inside -10..10.
        _order_least_greatest([2, -4, 0, -1, 5]),
    ],
    KpKey("number-line-integers", "kp2"): [
        _exact(
            "Points $A$ and $B$ sit at $-7$ and $4$. How far apart are they?",
            "11",
            "From $-7$ to $0$ is $7$, from $0$ to $4$ is $4$; total $11$.",
        ),
        # held-out: both endpoints negative, a sub-case neither existing
        # exemplar (one straddles 0, one is entirely negative-to-negative
        # already) — pick one with the larger endpoint negative too.
        _exact(
            "By how many units do you move to get from $-9$ to $-3$ on the number line?",
            "6",
            "Count from $-9$ up to $-3$: $6$ units.",
        ),
    ],
    # -- absolute-value ---------------------------------------------------------
    # constraint: "integers from -20 to 20"
    KpKey("absolute-value", "kp1"): [
        _exact(
            "A temperature reading of $-13$ degrees has what absolute value?",
            "13",
            "A reading of $-13$ sits $13$ units from $0$, so its absolute value is $13$.",
        ),
        # held-out: the zero case, the one value the sign rule collapses.
        _exact(
            "What absolute value does $0$ have?",
            "0",
            "The distance of $0$ from $0$ is $0$.",
        ),
    ],
    # constraint: "sum or difference of two absolute values"
    KpKey("absolute-value", "kp2"): [
        _exact(
            "Add the absolute values of $-8$ and $-2$.",
            "10",
            "$|-8| = 8$ and $|-2| = 2$; the sum of these two distances is $10$.",
        ),
        # held-out: a difference whose result is negative (the first
        # difference exemplar's own result stays positive).
        _exact(
            "Subtract $|-9|$ from $|7|$.",
            "-2",
            "$|7| = 7$ and $|-9| = 9$; $7$ minus $9$ leaves $-2$.",
        ),
    ],
    # -- adding-integers ----------------------------------------------------
    # constraint: "integers from -12 to 12; both addends negative or both positive"
    KpKey("adding-integers", "kp1"): [
        _exact(
            "Add $-9$ and $-6$.",
            "-15",
            "Same signs: $9 + 6 = 15$, keep the sign: $-15$.",
        ),
        # held-out: both addends POSITIVE, the other half of "same sign" that
        # the two existing (both-negative) exemplars never exercise.
        _add(8, 5),
    ],
    # constraint: "integers from -12 to 12; one positive, one negative addend"
    KpKey("adding-integers", "kp2"): [
        _exact(
            "What is $-10$ plus $3$?",
            "-7",
            "Different signs: $10 - 3 = 7$, keep the sign of the larger absolute value: $-7$.",
        ),
        # held-out: the positive addend has the larger absolute value (the
        # existing two both keep the negative on top).
        _exact(
            "Combine $6$ and $-2$ using addition.",
            "4",
            "Different signs: $6 - 2 = 4$, keep the sign of the larger absolute value: $4$.",
        ),
    ],
    # constraint: "sums that cancel to 0; up to three addends from -12 to 12"
    KpKey("adding-integers", "kp3"): [
        _exact(
            "What is the sum of $-9$ and its opposite, $9$?",
            "0",
            "A number and its opposite always sum to $0$.",
        ),
        # held-out: three addends canceling to 0 with a different partition
        # than the existing three-addend exemplar (-8,3,5).
        _exact(
            "Three numbers, $-7$, $2$, and $5$, are added together. What is the total?",
            "0",
            "Adding left to right, $-7$ and $2$ give $-5$, and $-5$ combined with $5$ leaves $0$.",
        ),
    ],
    # -- subtracting-integers -------------------------------------------------
    # constraint: "integers from -12 to 12; subtrahend positive"
    KpKey("subtracting-integers", "kp1"): [
        _exact(
            "Subtract $9$ from $-5$.",
            "-14",
            "Adding the opposite of $9$ to $-5$ gives $-14$.",
        ),
        # held-out: subtracting a positive from a positive minuend smaller
        # than the subtrahend (the existing two keep the minuend's sign
        # matching the result; this crosses zero from the positive side).
        _exact(
            "Find the difference when $11$ is taken from $4$.",
            "-7",
            "Adding the opposite of $11$ to $4$ gives $-7$.",
        ),
    ],
    # constraint: "integers from -12 to 12; subtrahend negative"
    KpKey("subtracting-integers", "kp2"): [
        _exact(
            "Subtract $-6$ from $8$.",
            "14",
            "Subtracting $-6$ from $8$ is the same as adding $6$ to $8$.",
        ),
        # held-out: a negative minuend minus a negative subtrahend of larger
        # magnitude (the existing two keep the final sign positive).
        _exact(
            "Find $-10$ minus $-3$.",
            "-7",
            "Subtracting $-3$ from $-10$ is the same as adding $3$ to $-10$.",
        ),
    ],
    # -- integer-addition-subtraction -----------------------------------------
    # constraint: "integers from -12 to 12; mix all four sign cases"
    KpKey("integer-addition-subtraction", "kp1"): [
        _mixed_two_term("+", -8, -3),
        # held-out: the fourth sign case (positive minus negative), which
        # neither existing exemplar (a plain subtraction, a subtract-negative)
        # nor the new same-sign addition covers.
        _exact(
            "Take $-5$ away from $6$.",
            "11",
            "Taking away $-5$ is the same as adding $5$ to $6$.",
        ),
    ],
    # constraint: "three or four terms; integers from -15 to 15"
    KpKey("integer-addition-subtraction", "kp2"): [
        _three_term_chain(9, [("-", 4), ("+", 7)]),
        # held-out: a four-term chain (the existing two are three- and
        # three-term), reaching the constraint's stated upper bound.
        _three_term_chain(-6, [("+", 11), ("-", 8), ("+", 3)]),
    ],
    # constraint: "one missing term; integers from -12 to 12"
    KpKey("integer-addition-subtraction", "kp3"): [
        _missing_add(6, -2),
        # held-out: the missing term sits on the LEFT of a subtraction (the
        # existing two are "square + k" and "a - square"; this is the second
        # shape already, so instead vary it to a negative target crossing
        # sign, a sub-case neither existing exemplar reaches).
        _exact(
            "Find the value of $\\square$ that satisfies $-3 - \\square = -10$.",
            "7",
            "Isolating the unknown gives $\\square$ equal to $-3$ minus $-10$, which is $7$.",
        ),
    ],
    # -- multiplying-integers -------------------------------------------------
    # constraint: "factors from -12 to 12; exactly one negative factor"
    KpKey("multiplying-integers", "kp1"): [
        _exact(
            "Find the product of $-9$ and $4$.",
            "-36",
            "1 negative factor (odd): the product is negative; $9 \\times 4 = 36$.",
        ),
        # held-out: the negative factor written second (both existing
        # exemplars write it first or as the sole parenthesized factor in a
        # position already covered) — here the first factor is positive.
        _exact(
            "Multiply $8$ by $-3$.",
            "-24",
            "1 negative factor (odd): the product is negative; $8 \\times 3 = 24$.",
        ),
    ],
    # constraint: "factors from -12 to 12; both factors negative"
    KpKey("multiplying-integers", "kp2"): [
        _exact(
            "Find the product of $-9$ and $-3$.",
            "27",
            "2 negative factors (even): the product is positive; $9 \\times 3 = 27$.",
        ),
        # held-out: a factor pair reaching the constraint's upper bound.
        _exact(
            "Multiply $-12$ by $-11$.",
            "132",
            "2 negative factors (even): the product is positive; $12 \\times 11 = 132$.",
        ),
    ],
    # -- dividing-integers ----------------------------------------------------
    # constraint: "divides evenly; values from -60 to 60; exactly one negative"
    KpKey("dividing-integers", "kp1"): [
        _exact(
            "Divide $-42$ by $7$.",
            "-6",
            "One negative: the quotient is negative; $42 \\div 7 = 6$, so $-6$.",
        ),
        # held-out: the negative sits in the divisor instead of the dividend.
        _exact(
            "What is $56$ divided by $-8$?",
            "-7",
            "One negative: the quotient is negative; $56 \\div 8 = 7$, so $-7$.",
        ),
    ],
    # constraint: "divides evenly; values from -60 to 60; both negative"
    KpKey("dividing-integers", "kp2"): [
        _exact(
            "What is $-45$ divided by $-9$?",
            "5",
            "Two negatives: the quotient is positive; $45 \\div 9 = 5$.",
        ),
        # held-out: reaching toward the constraint's upper magnitude bound.
        _exact(
            "Find the quotient of $-60$ and $-5$.",
            "12",
            "Two negatives: the quotient is positive; $60 \\div 5 = 12$.",
        ),
    ],
    # -- integer-multiplication-division --------------------------------------
    # constraint: "three factors; values from -12 to 12"
    KpKey("integer-multiplication-division", "kp1"): [
        _product([-3, -2, 5]),
        # held-out: all three factors negative (the existing two use two and
        # three negatives already covering odd/even parity with mixed
        # magnitudes; this is the "all three identical sign" boundary).
        _product([-2, -2, -2]),
    ],
    # constraint: "left-to-right chains of two operations; exact quotients"
    KpKey("integer-multiplication-division", "kp2"): [
        _muldiv_chain(-40, [("/", 5), ("*", -3)]),
        # held-out: the chain starts with a multiplication, then a division
        # (the existing two both start with a division).
        _muldiv_chain(-6, [("*", 7), ("/", -2)]),
    ],
    # constraint: "three to five factors; reason from parity of negatives"
    KpKey("integer-multiplication-division", "kp3"): [
        _product([-2, -2, -2, -2, -2]),
        # held-out: an even count of negatives among five factors is
        # impossible with all-negative, so vary composition: four negative,
        # one positive factor (even negatives, mixed magnitudes) — a family
        # the "reasoning-only" and "four identical negative" exemplars above
        # do not reach.
        _product([-3, -1, 4, -2, -1]),
    ],
    # -- integer-order-of-operations -------------------------------------------
    # constraint: "one multiplication or division plus one addition or subtraction; values from -12 to 12"
    KpKey("integer-order-of-operations", "kp1"): [
        _exact(
            "Compute $-8 - 3 \\times (-2)$.",
            str(-8 - 3 * -2),
            f"Multiply first: $3 \\times (-2) = -6$; $-8 - (-6) = {-8 - 3 * -2}$.",
        ),
        # held-out: division comes first, and the leading term is positive
        # (the existing two both start negative).
        _exact(
            "Compute $9 + 20 \\div (-4)$.",
            str(9 + 20 // -4),
            f"Divide first: $20 \\div (-4) = -5$; $9 + (-5) = {9 + 20 // -4}$.",
        ),
    ],
    # constraint: "one set of parentheses; values from -12 to 12"
    KpKey("integer-order-of-operations", "kp2"): [
        _exact(
            "Compute $(-6 - 4) \\times (-3)$.",
            str((-6 - 4) * -3),
            f"Inside first: $-6 - 4 = -10$; $-10 \\times (-3) = {(-6 - 4) * -3}$.",
        ),
        # held-out: the parenthesized group is the SECOND factor, and the
        # outer multiplier is positive (the existing two put the group
        # first or multiply by a negative).
        _exact(
            "Compute $3(4 - 10)$.",
            str(3 * (4 - 10)),
            f"Inside first: $4 - 10 = -6$; $3 \\times (-6) = {3 * (4 - 10)}$.",
        ),
    ],
    # constraint: "absolute value evaluated before outside operations"
    KpKey("integer-order-of-operations", "kp3"): [
        _exact(
            "Compute $2|-9| - 15$.",
            str(2 * 9 - 15),
            f"$|-9| = 9$; $2 \\times 9 - 15 = {2 * 9 - 15}$.",
        ),
        # held-out: the outside operation is addition and the bars enclose a
        # NEGATIVE result of an inner subtraction whose absolute value adds
        # to a negative outside term, a combination neither existing
        # exemplar (subtraction outside; addition of a positive |..|) reaches.
        _exact(
            "Compute $|4 - 11| + (-20)$.",
            str(abs(4 - 11) + -20),
            f"$|4 - 11| = |-7| = 7$; $7 + (-20) = {abs(4 - 11) + -20}$.",
        ),
    ],
}

RECIPES.update(MORE_RECIPES)
apply_wording(RECIPES)
