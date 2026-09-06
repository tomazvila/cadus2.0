"""Mixture setup, concentration, and price-blend pending authoring."""
from fractions import Fraction as Q
from unit05_tail_common import COORD, exemplar as ex, recipe, write_batch

PREFIX = 'systems-mixture-problems/'
SETUP = (' Let x and y be the amounts of the first and second ingredients. '
         'Complete $x+y=T$ and $px+qy=M$ by giving $(T,p,q,M)$; '
         'p and q are pure-substance fractions, and M is the pure amount.')
SOLVE = ' Give the amounts (first, second) in liters; assume volumes add.'
PRICE = ' Give the masses (first, second) in kilograms; assume no loss.'


def build():
    rows = [recipe(PREFIX+'kp1', COORD(4),
        'Blend 20% and 60% solutions into {a} liters at {b}%.'+SETUP,
        '(a,1/5,3/5,a*b/100)', dict(a=[18,24,30], b=[25,35,45,55]),
        lambda a,b: f'({a},1/5,3/5,{Q(a*b,100)})',
        'Total liquid gives $x+y={a}$. Convert each percent to a fraction of one; '
        'pure substance gives $(1/5)x+(3/5)y=({b}/100)({a})$.'),
        recipe(PREFIX+'kp2', COORD(2),
        'Blend 15% and 65% saline into {a} liters at {b}%.'+SOLVE,
        '(a*(65-b)/50,a*(b-15)/50)', dict(a=[30,40,50], b=[25,35,45,55]),
        lambda a,b: f'({Q(a*(65-b),50)},{Q(a*(b-15),50)})',
        'Let the first amount be $x$ and the second $y$. Use $x+y={a}$ and '
        '$15x+65y={b}({a})$. Subtract fifteen times the amount equation to find '
        '$50y=({b}-15)({a})$, then subtract $y$ from the total.'),
        recipe(PREFIX+'kp3', COORD(2),
        'Blend coffee priced at 5 and 15 euros/kg into {a} kilograms worth {b} euros/kg.'+PRICE,
        '(a*(15-b)/10,a*(b-5)/10)', dict(a=[11,17,23], b=[7,9,11,13]),
        lambda a,b: f'({Q(a*(15-b),10)},{Q(a*(b-5),10)})',
        'Mass balance gives $x+y={a}$; value balance gives $5x+15y={b}({a})$. '
        'Subtract five times the mass equation, solve for the second mass, and recover the first.')]
    authored = {PREFIX+'kp1': [
        ex('Blend 10% and 50% acid into 16 liters at 25%.'+SETUP,
           '(16,1/10,1/2,4)',
           'The liquid amounts sum to 16. Pure acid is one tenth of the first and one half of the second; the target holds $16(1/4)=4$ liters of acid.'),
        ex('Dilute pure acid (100%) with water (0%) into 9 liters at 40%.'+SETUP,
           '(9,1,0,18/5)',
           'Water contributes liquid but no acid. The pure ingredient contributes its full amount; the target acid amount is $9(2/5)=18/5$ liters.'),
        ex('Combine solutions with pure fractions 1/4 and 3/4 into 14 liters with pure fraction 3/7.'+SETUP,
           '(14,1/4,3/4,6)',
           'Use 14 as the total volume. The ingredient fractions multiply their volumes; the target pure volume is $14(3/7)=6$ liters.'),
        ex('Blend 30% and 80% solutions into 20 liters containing 11 liters of pure substance.'+SETUP,
           '(20,3/10,4/5,11)',
           'The total-volume equation uses 20. Convert 30% and 80% to fractional multipliers; the pure-volume equation uses the stated 11 liters.')],
        PREFIX+'kp2': [
        ex('Blend 10% and 40% acid into 18 liters at 20%.'+SOLVE, '(12,6)',
           'Use $x+y=18$ and $x+4y=36$ after scaling the acid balance. Subtraction gives $3y=18$. Check: $12+6=18$ and $12+4(6)=36$.'),
        ex('Dilute pure acid (100%) with water (0%) into 7 liters at 30%.'+SOLVE, '(21/10,49/10)',
           'Only the first ingredient supplies acid, so its volume is $7(3/10)=21/10$. Check: $21/10+49/10=7$ and the pure share is three tenths.'),
        ex('Combine solutions with pure fractions 1/5 and 4/5 into 15 liters with pure fraction 1/2.'+SOLVE, '(15/2,15/2)',
           'Use $x+y=15$ and $2x+8y=75$. Subtraction gives $6y=45$. Check: $15/2+15/2=15$ and $2(15/2)+8(15/2)=75$.'),
        ex('Blend 25% and 75% solutions into 24 liters containing 15 liters of pure substance.'+SOLVE, '(6,18)',
           'Use $x+y=24$ and $x+3y=60$. Subtraction gives $2y=36$. Check: $6+18=24$ and $6/4+3(18)/4=15$.')],
        PREFIX+'kp3': [
        ex('Blend tea priced at 6 and 14 euros/kg into 12 kilograms worth 9 euros/kg.'+PRICE, '(15/2,9/2)',
           'Use $x+y=12$ and $6x+14y=108$. Subtraction gives $8y=36$. Check: $15/2+9/2=12$ and $6(15/2)+14(9/2)=108$.'),
        ex('Combine nuts priced at 8 and 20 euros/kg into 10 kilograms with total value 128 euros.'+PRICE, '(6,4)',
           'The total value is already supplied: $8x+20y=128$ with $x+y=10$. Check: $6+4=10$ and $8(6)+20(4)=128$.'),
        ex('Mix coffee priced at 1600 and 1000 cents/kg into 9 kilograms worth 12 euros/kg.'+PRICE, '(3,6)',
           'Convert the prices to 16 and 10 euros/kg. Use $x+y=9$ and $16x+10y=108$. Check: $3+6=9$ and $16(3)+10(6)=108$.'),
        ex('Blend grain priced at 4 and 10 euros/kg into 8000 grams worth 7 euros/kg.'+PRICE, '(4,4)',
           'Convert the total mass to 8 kilograms. Use $x+y=8$ and $4x+10y=56$. Check: $4+4=8$ and $4(4)+10(4)=56$.')]}
    return rows, authored


if __name__ == '__main__':
    write_batch('mixtures', *build())
