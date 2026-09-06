"""Eight material solving objectives, with structurally varied exemplars."""
# key, four authored inequalities, recipe, answer_expr, domains
DATA = [
 ('one-step-inequalities/kp1',
  ['x+8<=21','x-7>4','13<x+2','x+5>=-3'],
  'x+{a}<={b}', 'x <= b-a', [2,5,8], [31,41,51,61]),
 ('one-step-inequalities/kp2',
  ['4x<=28','x/3>5','21>=7x','x/5>=-2'],
  'x/{a}>={b}', 'x >= a*b', [2,3,5], [13,17,19,23]),
 ('two-step-inequalities/kp1',
  ['3x+4<=28','2x-9>5','17>=4x+1','5x-3>=-18'],
  '3x+{a}<={b}', 'x <= (b-a)/3', [1,4,7], [34,64,94,124]),
 ('two-step-inequalities/kp2',
  ['x/2+5<=12','x/3-4>2','7>=x/4+1','x/5-2>=-6'],
  'x/{a}-7>={b}', 'x >= a*(b+7)', [2,3,5], [24,30,34,36]),
 ('linear-inequalities/kp1',
  ['-3x<=18','-x+4>11','12>=-4x','5-x>=-3'],
  '-3x+{a}<={b}', 'x >= (a-b)/3', [2,5,8], [47,77,107,137]),
 ('linear-inequalities/kp2',
  ['3(x-2)<=x+8','5-2(x+3)>x+2','4(x+1)>=2x-8','2(3-x)<5x+11'],
  '4(x-{a})<=x+{b}', 'x <= (4*a+b)/3', [2,5,8], [37,67,97,127]),
 ('linear-inequalities/kp3',
  ['2x+9<=5x-3','7-3x>2x-8','4x-1>=x+11','5x+2<8x-7'],
  '2x+{a}>=5x-{b}', 'x <= (a+b)/3', [2,5,8], [43,73,103,133]),
 ('inequality-word-problems/kp1',
  ['8x+10<=58','5x+7<=46','12x+4<=100','3x+9<=32'],
  '7x+{a}<={b}', 'floor((b-a)/7)', [2,9,16], [142,212,282,352]),
]
CONTEXTS = [
 'Shirts cost 8 euros each with 10 euros shipping. The budget is 58 euros.',
 'Each notebook costs 5 euros plus a 7 euro order fee. The budget is 46 euros.',
 'Tickets cost 12 euros each and booking costs 4 euros. The budget is 100 euros.',
 'Tokens cost 3 euros each; entry costs 9 euros. The budget is 32 euros.',
]


def problem(key, eq, index=None):
    if key=='inequality-word-problems/kp1':
        context = (CONTEXTS[index] if index is not None else
                   'Each pack costs 7 euros plus a {a} euro delivery fee. The budget is {b} euros.')
        return f'{context} With $x$ the number bought, solve ${eq}$ and give the maximum whole count.'
    return f'Solve ${eq}$.'


def populate(add):
    for key, examples, equation, expression, aa, bb in DATA:
        authored = [problem(key,eq,i) for i,eq in enumerate(examples)]
        add(key, authored, problem(key,equation), expression, aa, bb,
            {'kind':'exact'})
