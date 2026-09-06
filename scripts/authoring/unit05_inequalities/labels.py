"""Varying truth families: no answer-valued parameters or constant decisions."""
CONTRACT={'kind':'label','options':[['yes'],['no']]}
DATA=[
 ('solutions-of-inequalities/kp2',
  ['Is $x=3$ a solution of $2x+4<=11$?', 'Does $x=-2$ satisfy $7-x<8$?',
   'Test $x=4$ in $x/2>=2$. Is it a solution?', 'Is $x=5$ a solution of $3x-2>14$?'],
  'Is $x={a}$ a solution of $2x+{b}<=12$?',
  'equalitylabel(signcase(2*a+b-12,[1,1,0]),1)',[1,5,9],[1,3,5,7],
  'Substitution gives $2*{a}+{b}<=12$. Evaluate the left side and compare it with 12; equality is allowed.'),
 ('two-step-inequalities/kp3',
  ['Solve $3x+2<=14$, then test $x=4$. Is it included?',
   'Solve $x/2-3>1$, then test $x=8$. Is it included?',
   'Solve $4x-5>=11$, then test $x=5$. Is it included?',
   'Solve $2x+7<15$, then test $x=6$. Is it included?'],
  'Solve $3x+{a}<={b}$, then test $x=4$. Is it included?',
  'equalitylabel(signcase(12+a-b,[1,1,0]),1)',[1,5,9],[12,16,20,24],
  'Subtract {a}, then divide by 3: $x <= ({b}-{a})/3$. Substituting 4 in the original comparison gives $12+{a}<={b}$. Equality is included.'),
 ('and-or-inequalities/kp1',
  ['Is $x=3$ a solution of $x>1$ and $x<5$?',
   'Does $x=0$ satisfy $x<-2$ or $x>4$?',
   'Test $x=-3$ in $x<=-3$ or $x>=6$. Is it a solution?',
   'Is $x=7$ a solution of $x>2$ and $x<=7$?'],
  'Is $x=5$ a solution of $x>{a}$ and $x<{b}$?',
  'equalitylabel(signcase(5-a,[0,0,signcase(b-5,[0,0,1])]),1)',[1,4,6],[2,4,7,9],
  'Substitute 5 into both pieces: $5>{a}$ and $5<{b}$. The intersection includes the candidate only when both strict comparisons hold.'),
 ('graphing-linear-inequalities/kp3',
  ['For $2x+3y<=18$, does the shaded region include the origin?',
   'For $y>2x+4$, does the shaded region include the origin?',
   'For $5x-y>=-7$, does the shaded region include the origin?',
   'For $3y+2x<-6$, does the shaded region include the origin?'],
  'For $2x+3y<={a}$, does the shaded region include the origin?',
  'equalitylabel(signcase(a,[0,1,1]),1)',[-11,-9,-7,-5,-3,-1,2,4,6,8,10,12],None,
  'At the origin both coordinates are zero, so the test becomes $0<={a}$. A true comparison shades the side containing the origin; a false comparison shades the opposite side.'),
]


def populate(add):
    for key,authored,statement,expression,aa,bb,sketch in DATA:
        add(key,authored,statement,expression,aa,bb,CONTRACT)
