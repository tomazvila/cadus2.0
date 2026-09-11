"""Bounded intervals, including negative coefficients and absolute values."""
DATA = [
 ('and-or-inequalities/kp2', ['-7<x+3<9','2<=3x<=17','-4<x-5<=6','-3<=x/2<8'],
  '-{a}<x+4<{b}', '(-a-4,b-4)', [2,5,8], [13,17,23,29]),
 ('compound-inequalities/kp2', ['-8<-2x<=6','1<=5-x<9','-7<3-2x<13','-4<=-x/3<=6'],
  '-{a}<-3x+2<={b}', '[(2-b)/3,(a+2)/3)', [4,7,10], [17,23,29,35]),
 ('interval-notation/kp1', ['-8<=x<=-3','-1<x<=6','2<=x<9','-7<x<4'],
  '-{a}<x<={b}', '(-a,b]', [3,6,9], [14,18,22,26]),
 ('basic-absolute-value-inequalities/kp1', ['|x|<7','|x|<=4','9>|x|','6>=|x|'],
  '|x|<{a}', '(-a,a)', list(range(11,23)), None),
 ('absolute-value-inequalities/kp1', ['|x-3|<8','|2x+1|<=7','5>|3x-2|','9>=|4-x|'],
  '|2x-{a}|<={b}', '[(a-b)/2,(a+b)/2]', [3,7,11], [16,22,28,34]),
 ('absolute-value-inequalities/kp3', ['2|x-4|+3<=15','3|2x+1|-5<16','19>=4|x+2|+3','5|3-x|-2<18'],
  '2|3x-{a}|+5<={b}', '[(a-(b-5)/2)/3,(a+(b-5)/2)/3]',
  [4,8,12], [17,23,29,35]),
]


def problem(key, equation):
    if key=='interval-notation/kp1':
        return f'Write ${equation}$ in interval notation.'
    return f'Solve ${equation}$. Give the solution in interval notation.'


def populate(add):
    for key,eqs,eq,expr,aa,bb in DATA:
        add(key,[problem(key,e) for e in eqs],problem(key,eq),expr,aa,bb,{'kind':'exact'})
