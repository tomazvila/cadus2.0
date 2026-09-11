"""Checking systems: both residuals and verdict, candidate selection, graph estimates."""
from fractions import Fraction as Q
from unit05_tail_common import COORD, EXACT, YESNO, exemplar, multipart, recipe, write_batch

CHECK = multipart(residuals=COORD(2), solution=YESNO)
INSTRUCTION = (' Report residuals as the ordered pair of left-side-minus-right-side differences for each '
               'equation after substitution; report solution as yes or no.')


def verdict(r,s):
    return f'residuals = ({r},{s}); solution = '+('yes' if r == s == 0 else 'no')


def checked(point, eqs, residuals, context='Check the proposed point'):
    r,s = residuals
    x,y = point
    text = f'{context} $({x},{y})$ in ${eqs[0]}$ and ${eqs[1]}$.'+INSTRUCTION
    sketch = (f'Insert horizontal coordinate {x} and vertical coordinate {y} into each equation. '
              f'The first left-minus-right difference is {r}; the second is {s}. '
              + ('Both differences vanish, so the point lies on both lines.' if r == s == 0
                 else 'A nonzero difference means that equation fails, so the pair cannot solve the system.'))
    return exemplar(text,verdict(r,s),sketch)


def selected(candidates,eqs,answer,sketch):
    points = ' or '.join(f'$({x},{y})$' for x,y in candidates)
    return exemplar(f'Which of {points} satisfies both ${eqs[0]}$ and ${eqs[1]}$? Give the ordered pair.',
                    f'({answer[0]},{answer[1]})',sketch)


def constraints(key):
    lit = lambda n: {'lit':n}
    if key == 'reject':
        r = {'sub':[{'add':[{'mul':[lit(2),'a']},{'mul':[lit(3),'b']}]},lit(11)]}
        s = {'sub':[{'add':['a','b']},lit(13)]}
        return [{'op':'eq','left':{'mul':[r,s]},'right':lit(0)},
                {'op':'ne','left':{'add':[{'mul':[r,r]},{'mul':[s,s]}]},'right':lit(0)}]
    diff = {'sub':['b','a']}
    return [{'op':'eq','left':{'mul':[{'sub':[diff,lit(1)]},{'sub':[diff,lit(3)]}]},'right':lit(0)}]


def build():
    rows = [recipe('checking-systems-solutions/kp1',CHECK,
        'Check the proposed point $(2,3)$ in $x+2y={a}$ and $3x-y={b}$.'+INSTRUCTION,
        'multipart((8-a,3-b),equalitylabel((8-a)^2+(3-b)^2,0))',
        dict(a=[6,8,10],b=[0,3,6,9]),lambda a,b:verdict(8-a,3-b),
        'Substitute the point into each left side, obtaining eight and three. Subtract each '
        'corresponding right side. Both resulting differences must vanish for the point to solve the system.'),
        recipe('checking-systems-solutions/kp2',CHECK,
        'One equation holds at $({a},{b})$ in $2x+3y=11$ and $x+y=13$. Identify the failure.'+INSTRUCTION,
        'multipart((2*a+3*b-11,a+b-13),equalitylabel((2*a+3*b-11)^2+(a+b-13)^2,0))',
        dict(a=list(range(-9,14)),b=list(range(-5,14))),
        lambda a,b:verdict(2*a+3*b-11,a+b-13),
        'Evaluate the two left sides using the proposed coordinates. Subtract eleven and thirteen '
        'respectively. The nonzero residual identifies the equation that rejects the candidate.',
        constraints=constraints('reject'),
        predicate=lambda a,b:(2*a+3*b==11) != (a+b==13)),
        recipe('checking-systems-solutions/kp3',COORD(2),
        'Which of $({a},1)$ or $({a},3)$ satisfies both $x+y={b}$ and $2x+y={a}+{b}$? Give the ordered pair.',
        '(a,b-a)',dict(a=[1,2,3,4,5,6],b=[2,3,4,5,6,7,8,9]),
        lambda a,b:f'({a},{b-a})',
        'Subtract the first equation from the second to isolate the horizontal coordinate. '
        'Subtract that coordinate from the first total to find the vertical coordinate, and check both candidates.',
        constraints=constraints('select'),predicate=lambda a,b:b-a in (1,3)),
        recipe('graphing-systems/kp3',CHECK,
        'A graph-based estimate is $({a},{b})$ for $x+2y=7$ and $3x-y=7$. Check the estimate exactly.'+INSTRUCTION,
        'multipart((a+2*b-7,3*a-b-7),equalitylabel((a+2*b-7)^2+(3*a-b-7)^2,0))',
        dict(a=[2,3,4],b=[0,1,2,3]),lambda a,b:verdict(a+2*b-7,3*a-b-7),
        'An apparent crossing must satisfy both equations exactly. Substitute the estimated '
        'coordinates, compute each left-minus-right difference, and accept only if both vanish.')]
    authored = {'checking-systems-solutions/kp1':[
        checked((2,5),('y=2x+1','x+y=7'),(0,0)),
        checked((-1,4),('3x+y=1','2x-y=-7'),(0,1)),
        checked((4,-2),('x/2+y=0','y-x=-6'),(0,0)),
        checked((3,1),('x=2y+2','x-y=5'),(-1,-3))],
        'checking-systems-solutions/kp2':[
        checked((2,3),('y=x+1','2x+y=8'),(0,-1)),
        checked((-2,5),('3x-y=-9','x+y=3'),(-2,0)),
        checked((6,2),('x/3+y=4','y-x=-5'),(0,1)),
        checked((3,-1),('x=2y+4','x-2y=5'),(1,0))],
        'checking-systems-solutions/kp3':[
        selected([(1,4),(2,6)],('y=2x+2','x+y=5'),(1,4),
                 'The first candidate gives $4=2(1)+2$ and $1+4=5$. The second lies on the first line but its coordinate sum is eight, so it fails the other equation.'),
        selected([(3,2),(-2,1)],('x+3y=1','2x-y=-5'),(-2,1),
                 'The second candidate gives $-2+3(1)=1$ and $2(-2)-1=-5$. The first has coordinate sum with triple height equal to nine, so it fails the first equation.'),
        selected([(4,-1),(2,1)],('x/2+y=1','x-y=5'),(4,-1),
                 'The first candidate gives $4/2+(-1)=1$ and $4-(-1)=5$. The second gives a difference of one, so it cannot satisfy both equations.'),
        selected([(0,3),(5,2)],('x=3y-1','x+2y=9'),(5,2),
                 'The second candidate gives $5=3(2)-1$ and $5+2(2)=9$. The first would require the horizontal coordinate to be eight, so it fails.')],
        'graphing-systems/kp3':[
        checked((1,5),('y=3x+2','x+y=6'),(0,0),'A graph suggests'),
        checked((3,2),('2x+y=8','x-y=2'),(0,-1),'Check this apparent graphical intersection'),
        checked((4,1),('x/2-y=1','y+x=5'),(0,0),'The plotted crossing is estimated at'),
        checked((-1,2),('x=2y-4','3x+y=-2'),(-1,1),'A rough sketch suggests')]}
    return rows,authored


if __name__ == '__main__':
    write_batch('checking',*build())
