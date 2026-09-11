"""Point checks and a bounded complete record for two nonvertical half-planes."""
from unit05_tail_common import COORD, YESNO, exemplar, multipart, recipe, write_batch
from unit05_tail_checking import CHECK, INSTRUCTION, verdict

REGION = multipart(solid=COORD(3),dashed=COORD(3))
RECORD = (' Describe the overlap by reporting solid and dashed as (m,c,d) '
          'for boundary lines in y=mx+c form. Direction d is 1 for shading above '
          'and -1 for shading below. Include the solid boundary and exclude the dashed boundary. '
          'The solution is the intersection of those two half-planes.')
CHECK_INSTRUCTION = INSTRUCTION.replace('equation','inequality')


def checked(point,ineqs,answer,sketch):
    return exemplar(f'Check $({point[0]},{point[1]})$ against ${ineqs[0]}$ and ${ineqs[1]}$.'+
                    CHECK_INSTRUCTION,answer,sketch)


def region(ineqs,solid,dashed,up_solid,up_dashed,sketch):
    solid = solid[:-1]+','+('1' if up_solid == 'yes' else '-1')+')'
    dashed = dashed[:-1]+','+('1' if up_dashed == 'yes' else '-1')+')'
    ans = f'solid = {solid}; dashed = {dashed}'
    return exemplar(f'For ${ineqs[0]}$ and ${ineqs[1]}$,'+RECORD,ans,sketch)


def selected(points,ineqs,answer,sketch):
    pts = ' or '.join(f'$({x},{y})$' for x,y in points)
    return exemplar(f'Which of {pts} belongs to the overlap of ${ineqs[0]}$ and ${ineqs[1]}$? Give the ordered pair.',
                    answer,sketch)


def build_rows():
    return [recipe('systems-of-linear-inequalities/kp1',CHECK,
        'Check $({a},{b})$ against $y>x+1$ and $y<=-x+9$.'+CHECK_INSTRUCTION,
        'multipart((b-a-1,b+a-9),equalitylabel(signcase(b-a-1,[0,0,1])+signcase(b+a-9,[1,1,0]),2))',
        dict(a=[1,2,3],b=[1,3,5,7]),
        lambda a,b:f'residuals = ({b-a-1},{b+a-9}); solution = '+('yes' if b>a+1 and b<=-a+9 else 'no'),
        'Substitute the point into each inequality. The first left-minus-right difference must '
        'be strictly positive; the second must be nonpositive. Both conditions are required.'),
        recipe('systems-of-linear-inequalities/kp2',REGION,
        'For $({a})y<=2x+({b})$ and $y>-x+4$,'+RECORD,
        'multipart((2/a,b/a,signcase(a,[1,0,-1])),(-1,4,1))',
        dict(a=[-3,-1,2,4],b=[-5,1,6]),
        region_expected,
        'Divide the first inequality by its vertical coefficient. Negative division reverses '
        'the direction, while its boundary remains solid. The second inequality is strict, '
        'so its boundary is dashed and the shaded side is above it. Intersect the two shaded sides.'),
        recipe('systems-of-linear-inequalities/kp3',COORD(2),
        'Which of $({a},2({a})+1)$ or $({a},2({a})+5)$ belongs to the overlap of '
        '$y>2x+({b})$ and $y<2x+({b})+6$? Give the ordered pair.',
        '(a,2*a+signcase(b,[1,3,5]))',dict(a=[1,2,3,4,5,6],b=[-2,2]),
        lambda a,b:f'({a},{2*a+(1 if b<0 else 5)})',
        'Subtract twice the horizontal coordinate from the candidate height. The resulting '
        'offset must lie strictly between the lower offset and that offset plus six. '
        'Test each candidate against both bounds; neither candidate lies on a boundary.')]


def region_expected(a,b):
    from fractions import Fraction as Q
    return (f'solid = ({Q(2,a)},{Q(b,a)},{1 if a<0 else -1}); dashed = (-1,4,1)')


def authored():
    return {'systems-of-linear-inequalities/kp1':[
        checked((1,2),('y>x','y<=-x+4'),'residuals = (1,-1); solution = yes',
                'The first difference is positive one, so the strict lower condition holds. The second difference is negative one, so the inclusive upper condition holds too.'),
        checked((2,3),('2x+y<7','y>=x-2'),'residuals = (0,3); solution = no',
                'The first left side reaches the boundary value seven. A strict inequality excludes that boundary, even though the second difference is positive and satisfies its condition.'),
        checked((-1,4),('-2y<=x-3','y<x+7'),'residuals = (-4,-2); solution = yes',
                'The first difference is negative four, which meets its nonpositive condition. The second difference is negative two, which meets the strict negative condition.'),
        checked((3,-1),('x-2y>=6','y<=2x+1'),'residuals = (-1,-8); solution = no',
                'The first left side is five, short of the required six. Its negative difference fails the nonnegative condition, so the overlap excludes this point despite the second passing.')],
        'systems-of-linear-inequalities/kp2':[
        region(('y<=2x+3','y>-x+1'),'(2,3)','(-1,1)','no','yes',
               'The inclusive first inequality has a solid boundary with slope two and intercept three, shading below. The strict second has slope negative one and intercept one, shading above.'),
        region(('y<3x-2','y>=x+4'),'(1,4)','(3,-2)','yes','no',
               'The second inequality supplies the solid boundary, slope one and intercept four, with shading above. The first supplies the dashed boundary, slope three and intercept negative two, shading below.'),
        region(('-2y<=4x-6','3y>3x+12'),'(-2,3)','(1,4)','yes','yes',
               'Divide the first inequality by negative two and reverse its direction: shade above its solid line with slope negative two and intercept three. Divide the second by three; shade above its dashed line.'),
        region(('x+2y>=8','-y>x-5'),'(-1/2,4)','(-1,5)','yes','no',
               'Isolate height in the first: its solid line has slope negative one half and intercept four, with shading above. Negative division in the second reverses direction, shading below its dashed line.')],
        'systems-of-linear-inequalities/kp3':[
        selected([(1,3),(2,1)],('y>x+1','y<x+5'),'(1,3)',
                 'At the first candidate the height exceeds the lower line by one and is three below the upper line. The second candidate lies below its lower bound, so only the first is in the strip.'),
        selected([(0,7),(2,3)],('x+y<6','y>=x'),'(2,3)',
                 'The first candidate has coordinate sum seven, so it fails the strict upper condition. The second has sum five and height three above horizontal coordinate two, so it meets both conditions.'),
        selected([(-1,3),(1,-2)],('-2y<x','y<=x+6'),'(-1,3)',
                 'For the first point, negative six is less than negative one and three is at most five. For the second, four is not less than one, so it fails the strict condition.'),
        selected([(3,6),(2,4)],('y>2x-1','y<-x+8'),'(2,4)',
                 'The first point fails the upper bound because six exceeds five. The second height is above three and below six, so it satisfies both strict inequalities.')],}


if __name__ == '__main__':
    write_batch('regions',build_rows(),authored())
