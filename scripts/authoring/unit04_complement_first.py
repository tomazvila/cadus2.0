"""Five live Unit04 recipes and four worked exemplars per KP."""
from unit04_complement_common import Q, example as ex, fields, multipart, publish, recipe



def proportional():
    key = 'proportional-relationships/kp2'
    contract = multipart(('ratios','pair'),('proportional',True))
    cases = [
        ('A table has positive inputs', '(2,8)', '(5,20)', 4,4),
        ('A straight graph includes the origin and the two indicated points', '(2,5)', '(6,15)',Q(5,2),Q(5,2)),
        ('A table has a nonzero starting offset; inspect its two rows', '(1,5)', '(4,11)',5,Q(11,4)),
        ('A table crosses from negative to positive inputs', '(-2,3)', '(6,-8)',Q(-3,2),Q(-4,3))]
    rows = []
    for intro,p,q,r,s in cases:
        rows.append(ex(f'{intro}: ${p}$ and ${q}$. Give ratios=(first,second) for the output/input ratios, then proportional=yes or no.',
                       fields(ratios=(r,s),proportional=r==s),
                       f'Dividing output by its paired input gives r={r} and s={s}. A proportional relationship requires these ratios to agree for every nonzero input.', contract))
    r = recipe(key, 'A table contains $(3,{a})$ and $(5,{b})$. Give ratios=(first,second) in row order, then proportional=yes or no.',
               'multipart((a/3,b/5),equalitylabel(5*a,3*b))',
               'Compute ${a}/3$ and ${b}/5$. Compare cross-products $5*{a}$ and $3*{b}$; equality means one constant maps both inputs to their outputs.',
               {'a':[3,6,9], 'b':[5,10,15,20]},
               lambda a,b: fields(ratios=(Q(a,3),Q(b,5)),proportional=5*a==3*b), contract)
    return key, rows, r


def compare_rates():
    key = 'graphing-proportional-relationships/kp3'
    contract = multipart(('rates','pair'),('faster',True))
    cases = [('Compare two proportional graphs', '(2,11)', '(3,12)',Q(11,2),4),
             ('A proportional graph and a table are represented by their nonzero points', '(4,7)', '(5,15)',Q(7,4),3),
             ('The proportional lines coincide; check their unit rates', '(3,9)', '(7,21)',3,3),
             ('Both proportional lines decrease as x increases', '(2,-3)', '(4,-10)',Q(-3,2),Q(-5,2))]
    rows = []
    for intro,p,q,a,b in cases:
        rows.append(ex(f'{intro}. A passes through ${p}$; B passes through ${q}$. Both include the origin. Give rates=(A,B), and faster=yes if A has the greater signed rate, otherwise no.',
                       fields(rates=(a,b),faster=a>b),
                       f'The output/input ratios give a={a} and b={b}. Compare these signed rates; equal rates produce no greater rate, and negative rates retain their ordering.',contract))
    r = recipe(key, 'Proportional graph A passes through $(2,{u})$ and the origin. Model B is $y=({v}/3)x$. Give unit rates as rates=(A,B); faster=yes if A has the greater signed rate, otherwise no.',
               'multipart((u/2,v/3),equalitylabel(signcase(u/2-v/3,[0,0,1]),1))',
               'For A divide ${u}$ by its input two. For B read the coefficient ${v}/3$. Compare the two signed ratios to decide which rate is greater.',
               {'u':[3,7,11], 'v':[3,9,18,24]},
               lambda u,v: fields(rates=(Q(u,2),Q(v,3)),faster=Q(u,2)>Q(v,3)),contract)
    return key, rows, r


def axis_constants():
    key = 'horizontal-vertical-slopes/kp3'
    contract = multipart(('h',False),('v',False))
    cases = [('Find the equations through these points', '(3,-5)','(2,7)',-5,2),
             ('The two axis-parallel lines meet in quadrant II', '(-4,6)','(-4,6)',6,-4),
             ('One line is the x-axis and the other is left of the origin', '(7,0)','(-3,4)',0,-3),
             ('Use fractional coordinates to locate the lines', '(1,3/2)','(-5/2,2)',Q(3,2),Q(-5,2))]
    rows=[]
    for intro,p,q,h,v in cases:
        rows.append(ex(f'{intro}: horizontal through ${p}$, vertical through ${q}$. Give h in $y=h$ and v in $x=v$.', fields(h=h,v=v),
                       f'The horizontal line keeps the first point y-coordinate {h}. The vertical line keeps the second point x-coordinate {v}. These fixed coordinates determine the two equations.',contract))
    r=recipe(key,'Find h in $y=h$ for the horizontal line through $(4,{a})$, and v in $x=v$ for the vertical line through $({b},-3)$.',
             'multipart(a,b)', 'Horizontal motion leaves the y-coordinate ${a}$ unchanged. Vertical motion leaves the x-coordinate ${b}$ unchanged. Use those coordinates as the two equation constants.',
             {'a':[-7,-2,5], 'b':[-6,-1,3,8]},lambda a,b:fields(h=a,v=b),contract)
    return key,rows,r



def signed_rate():
    key='slope-as-rate-of-change/kp2'
    contract=multipart(('rate',False),('increasing',True))
    cases=[('A water tank drains steadily','(1,42)','(4,21)',-7),
           ('A temperature sensor records cooling','(2,-3)','(7,-14)',Q(-11,5)),
           ('A savings balance grows steadily','(0,15)','(6,45)',5),
           ('A temperature rises while remaining below zero','(3,-8)','(7,-1)',Q(7,4))]
    rows=[]
    for intro,p,q,m in cases:
        rows.append(ex(f'{intro}. The (time,amount) readings are ${p}$ and ${q}$. Give the signed rate per time unit, then increasing=yes/no.',
                       fields(rate=m,increasing=m>0),
                       f'Divide the change in amount by the positive elapsed time to get rate={m}. Positive rate means increase, negative rate means decrease, and zero means neither direction.',contract))
    r=recipe(key,'A sensor changes steadily from reading $(1,{a})$ to $(5,{b})$, with time first. Give the signed rate per time unit, then increasing=yes/no.',
             'multipart((b-a)/4,equalitylabel(signcase(b-a,[0,0,1]),1))',
             'Subtract the starting amount ${a}$ from the final amount ${b}$ and divide by the elapsed time $5-1$. Its sign determines the direction; zero change has neither direction.',
             {'a':[11,23,37],'b':[5,11,29,47]},lambda a,b:fields(rate=Q(b-a,4),increasing=b>a),contract)
    return key,rows,r



def graph_axis():
    key='graphing-linear-equations/kp3'
    contract=multipart(('x',False),('y',False))
    cases=[('Graph the vertical line $x=4$ and horizontal line $y=-2$; give their intersection.',4,-2),
           ('The lines $y=6$ and $x=-3$ cross. Give the coordinates x and y of their meeting point.',-3,6),
           ('Sketch the coordinate axes $x=0$ and $y=0$. Give their intersection.',0,0),
           ('Locate where the vertical line $x=-3/2$ meets the horizontal line $y=5/2$.',Q(-3,2),Q(5,2))]
    rows=[]
    for problem,x,y in cases:
        rows.append(ex(problem+' Answer x=...; y=....',fields(x=x,y=y),f'The vertical line fixes x at {x}, while the horizontal line fixes y at {y}. Their intersection satisfies both fixed-coordinate conditions, so plot that ordered pair.',contract))
    r=recipe(key,'Graph the vertical line $x={a}$ and horizontal line $y={b}$. Give their intersection as x=...; y=....',
             'multipart(a,b)', 'Every point of the vertical line has x-coordinate ${a}$. Every point of the horizontal line has y-coordinate ${b}$. Combining these conditions locates their unique intersection.',
             {'a':[-8,-4,2],'b':[-7,-1,3,9]},lambda a,b:fields(x=a,y=b),contract)
    return key,rows,r


if __name__=='__main__':
    content=[f() for f in (proportional,compare_rates,axis_constants,signed_rate,graph_axis)]
    publish({k:rows for k,rows,_ in content},[r for _,_,r in content])
