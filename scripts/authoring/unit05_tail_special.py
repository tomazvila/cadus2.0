"""Narrow special-case classification using genuine yes/no propositions."""
from unit05_tail_common import YESNO, exemplar, multipart, recipe, write_batch

TWO = multipart(infinite=YESNO, none=YESNO)
THREE = multipart(one=YESNO, infinite=YESNO)
REQUEST = (' Classify the solution set: report one and infinite as yes/no '
           'fields. Here one means exactly one solution. Both no means no solutions; both yes is invalid.')
PARALLEL_REQUEST = (' Classify the solution set: report infinite and none as yes/no '
                    'fields, with exactly one yes.')


def flags(kind, three=True):
    names = ('one','infinite') if three else ('infinite','none')
    return '; '.join(f'{name} = '+('yes' if name == kind else 'no') for name in names)


def classify(a,b,c,target):
    return flags('one' if a != b else 'infinite' if c == target else 'none')


def sample(eqs,kind,sketch,three=True,prefix='Classify algebraically'):
    return exemplar(f'{prefix} ${eqs[0]}$ and ${eqs[1]}$.'+
                    (REQUEST if three else PARALLEL_REQUEST),flags(kind,three),sketch)


def build_rows():
    rows = [recipe('systems-special-cases/kp1',TWO,
        'Use elimination on $x+({a}+2({b}))y={a}$ and $2x+2({a}+2({b}))y=2({b})$.'+PARALLEL_REQUEST,
        'multipart(equalitylabel(a,b),equalitylabel(signcase(a-b,[1,0,1]),1))',
        dict(a=[2,5,8],b=[2,5,8,11]),
        lambda a,b:flags('infinite' if a == b else 'none',False),
        'Twice the first left side equals the second left side. Subtracting twice the '
        'first equation leaves a right-side difference of twice the second parameter minus twice the first. '
        'A nonzero difference is inconsistent; a vanishing difference leaves the same nondegenerate line.')]
    for kp,target,aa,bb,cc,statement in [
        ('kp2',13,[-3,1,4],[-3,1,4,6],[13,19],
         'Compare slopes and intercepts of $y=({a})x+13+2({a})+({b})$ and $2y=2({b})x+2({c}+2({a})+({b}))$.'),
        ('kp3',17,[2,5,7],[2,5,7,9],[17,23],
         'Classify $({a})x+y=17+2({a})+({b})$ and $({b})x+y={c}+2({a})+({b})$ using elimination or the coefficient determinant.')]:
        delta = f'({target}-c)'
        expr = ('multipart(equalitylabel(signcase(a-b,[1,0,1]),1),'
                f'equalitylabel((a-b)^2+{delta}^2,0))')
        rows.append(recipe('systems-special-cases/'+kp,THREE,statement+REQUEST,expr,
            dict(a=aa,b=bb,c=cc),lambda a,b,c,t=target:classify(a,b,c,t),
            'Normalize both equations. Different slopes yield one intersection. With equal '
            'slopes, matching intercepts describe the same line and infinitely many points; '
            'different intercepts describe parallel distinct lines with no common point.'))
    return rows


def authored():
    return {'systems-special-cases/kp1':[
        sample(('3x+y=5','6x+2y=14'),'none',
               'Twice the first equation has right side ten. The second has the identical left side but right side fourteen; subtracting gives an impossible equality.',False),
        sample(('x=2y+3','2x-4y=6'),'infinite',
               'Move the vertical term to the left in the first equation, then double both sides. This produces the second equation, so every point of that line solves both.',False),
        sample(('y=-3x+4','3x+y=1'),'none',
               'Rearranging the first gives the same left side as the second with right side four. The required right sides differ by three, so no point can satisfy both.',False),
        sample(('2x-3y=7','-6x+9y=-21'),'infinite',
               'Multiplying every term of the first equation by negative three produces the second equation. A whole nondegenerate line of points therefore satisfies both.',False)],
        'systems-special-cases/kp2':[
        sample(('y=2x+5','y=2x-1'),'none',
               'Both slopes are two, while the vertical intercepts are five and negative one. The lines are parallel and six vertical units apart, so they never intersect.',prefix='Read the graph geometry of'),
        sample(('y=-x+6','2y=-2x+12'),'infinite',
               'Divide the second equation by two. Both graphs then have slope negative one and vertical intercept six; they coincide, with every point shared.',prefix='Compare slopes and intercepts of'),
        sample(('x=5','y=3x-2'),'one',
               'The first graph is vertical. Substituting its fixed horizontal coordinate in the second gives height thirteen, so there is exactly one crossing at (5,13).',prefix='Compare the vertical and sloping graphs'),
        sample(('2x+y=9','y=4'),'one',
               'The first line has slope negative two; the second is horizontal. Their directions differ, and substituting height four gives one horizontal coordinate, five halves.',prefix='Interpret the graphs'),],
        'systems-special-cases/kp3':[
        sample(('2x+y=3','x-y=6'),'one',
               'Adding eliminates the vertical coordinate and gives three times the horizontal coordinate equal to nine. This fixes one pair, (3,-3), so the solution is unique.'),
        sample(('y=4x-7','2y=8x-10'),'none',
               'Halve the second equation. Its slope is four with intercept negative five; the first intercept is negative seven. Equal slopes and unequal intercepts give no intersection.'),
        sample(('x/2+y=3','x+2y=6'),'infinite',
               'Double every term of the first equation to obtain the second. The two equations impose the same line condition, leaving infinitely many possible points.'),
        sample(('x=2y-5','3x+y=13'),'one',
               'Substitute the first expression for the horizontal coordinate in the second. Seven times the vertical coordinate equals twenty-eight, giving the unique pair (3,4).')]}


if __name__ == '__main__':
    write_batch('special',build_rows(),authored())
