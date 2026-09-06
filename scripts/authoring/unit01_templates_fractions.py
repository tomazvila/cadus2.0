"""KP-specific finite fraction recipes, with exact independent sample calculations."""
from fractions import Fraction as F
from math import gcd
from unit01_template_common import recipe, cmp


def concepts():
    yield recipe('fraction-basics/kp1',
        'A unit strip has ${b}$ equal cells, of which ${a}$ are marked. Give the marked fraction.',
        'a/b', 'Each cell is $1/{b}$ of the strip; ${a}$ marked cells represent ${a}/{b}$.',
        'Identify the marked part and the total number of equal cells.',
        {'a': [1, 2, 3], 'b': [4, 5, 6, 7]}, lambda a, b: a/b)
    yield recipe('fraction-basics/kp2',
        'A set has ${a}$ triangles and ${b}$ circles. What fraction of all shapes are triangles? No reduction is required.',
        'a/(a+b)', 'There are ${a}+{b}$ shapes altogether. The triangle share is ${a}/({a}+{b})$.',
        'Count both kinds to find the whole set.',
        {'a': [1, 2, 3], 'b': [13, 14, 15, 16]}, lambda a, b: a/(a+b))
    yield recipe('fraction-basics/kp3',
        'Two equal wholes are divided into ${a}$ and ${b}$ equal pieces. Give the larger single-piece fraction.',
        '1/min(a,b)', 'A piece is $1/{a}$ or $1/{b}$; the smaller denominator makes the larger piece.',
        'Compare the sizes of pieces from the same whole.',
        {'a': [2, 3, 4], 'b': [6, 7, 8, 9]}, lambda a, b: 1/min(a,b))
    yield recipe('fractions-on-number-line/kp1',
        'A line from zero to one has ${b}$ equal intervals. A point is ${a}$ ticks left of one. Give its fraction coordinate.',
        '(b-a)/b', 'One is ${b}/{b}$. Moving ${a}$ ticks left gives $({b}-{a})/{b}$.',
        'Express one whole using the tick denominator.',
        {'a': [1, 2, 3], 'b': [5, 6, 7, 8]}, lambda a, b: (b-a)/b)
    yield recipe('fractions-on-number-line/kp2',
        'A line is marked in thirds. Locate ${a}/3$ and give the whole-number tick immediately to its left.',
        'floor(a/3)', 'Group ${a}$ thirds into complete sets of three. The quotient $\\lfloor {a}/3\\rfloor$ is the left whole-number tick.',
        'Each complete group of three thirds reaches another whole.',
        {'a': [4,5,7,8,10,11,13,14,16,17,19,20]}, lambda a: a//3)
    yield recipe('fractions-on-number-line/kp3',
        'A point is at whole-number coordinate ${a}$ on a line with step size $1/{b}$. Give the numerator of that coordinate when written over denominator ${b}$.',
        'a*b', 'Each whole contributes ${b}$ ticks, so ${a}$ wholes give $({a}\\times{b})/{b}$.',
        'Count the equal fractional steps inside each whole unit.',
        {'a': [2,3,4], 'b': [4,6,7,8]}, lambda a,b: a*b)


def equivalents_and_comparisons():
    yield recipe('equivalent-fractions/kp1',
        'Scale the fraction ${a}/7$ by ${b}$ in both numerator and denominator. Give the new numerator.',
        'a*b', 'The scaled fraction is $({a}\\times{b})/(7\\times{b})$; its numerator is ${a}\\times{b}$.',
        'The same nonzero factor must multiply both parts.',
        {'a':[1,2,3,4], 'b':[2,3,4]}, lambda a,b:a*b)
    for key,den,nums in [('kp2',30,[2,4,5,6,8,9,10,12,14,15,16,18]),
                         ('kp3',60,[12,15,18,20,21,24,27,30,33,36,39,42])]:
        yield recipe('equivalent-fractions/'+key,
            f'A fraction bar shows ${{a}}/{den}$. Divide both parts by their greatest common factor to give lowest terms.',
            f'a/{den}', f'Let $g=\\gcd({{a}},{den})$. Cancelling $g$ gives $({{a}}/g)/({den}/g)$.',
            'Find a common factor that divides both numerator and denominator.',
            {'a':nums}, lambda a,d=den:a/d)
    yield recipe('comparing-ordering-fractions/kp1',
        'Two points have coordinates ${a}/11$ and ${b}/11$. Give the smaller coordinate.',
        'min(a,b)/11', 'Both points use elevenths. The smaller numerator in ${a}/11$ and ${b}/11$ gives the smaller coordinate.',
        'Equal denominators mean equal-sized pieces.',
        {'a':[1,2,3], 'b':[5,6,7,8]}, lambda a,b:min(a,b)/11)
    yield recipe('comparing-ordering-fractions/kp2',
        'Compare the fill levels ${a}/5$ and ${b}/7$ of identical jars. Give the greater fraction.',
        'max(a/5,b/7)', 'Over denominator $35$, compare $(7\\times{a})/35$ with $(5\\times{b})/35$ and choose the larger numerator.',
        'Rewrite both levels using equal-sized pieces.',
        {'a':[1,2,3], 'b':[2,3,4,5]}, lambda a,b:max(a/5,b/7))
    yield recipe('comparing-ordering-fractions/kp3',
        'A bar is ${a}/{b}$ full. Give the fraction of a whole by which its fill exceeds one half.',
        'a/b-1/2', 'One half is ${b}/(2\\times{b})$; the excess is $(2\\times{a}-{b})/(2\\times{b})$.',
        'Compare the filled fraction with the half-full benchmark.',
        {'a':[7,8,9], 'b':[10,11,12,13]}, lambda a,b:a/b-F(1,2))


def addition():
    yield recipe('adding-subtracting-like-fractions/kp1',
        'A strip has ${a}/11$ coloured red and ${b}/11$ coloured blue, without overlap. What fraction is coloured?',
        '(a+b)/11', 'The disjoint regions use the same elevenths; adding their counts gives $({a}+{b})/11$.',
        'Count the coloured pieces while keeping their size unchanged.',
        {'a':[1,2,3], 'b':[4,5,6,7]}, lambda a,b:(a+b)/11)
    yield recipe('adding-subtracting-like-fractions/kp2',
        'A jug contains ${a}/12$ litre. Remove ${b}/12$ litre. Give the remaining litres in lowest terms.',
        '(a-b)/12', 'Subtract twelfths to obtain $({a}-{b})/12$ litre, then cancel the GCF of ${a}-{b}$ and $12$.',
        'The piece size stays constant when twelfths are removed.',
        {'a':[8,9,10], 'b':[1,2,3,4]}, lambda a,b:(a-b)/12)
    yield recipe('adding-subtracting-like-fractions/kp3',
        'Combine two measures of ${a}/6$ litre and ${b}/6$ litre. Give total litres as a reduced fraction.',
        '(a+b)/6', 'There are ${a}+{b}$ sixths; the total is $({a}+{b})/6$, reduced by its common factor.',
        'Keep counting sixths even after passing a full litre.',
        {'a':[4,5,6], 'b':[3,4,5,6]}, lambda a,b:(a+b)/6)
    yield recipe('adding-subtracting-fractions/kp1',
        'Add ribbon lengths ${a}/4$ metre and ${b}/12$ metre. Give the total metres as a reduced fraction.',
        'a/4+b/12', 'Each quarter is three twelfths, so the total is $(3\\times{a}+{b})/12$ metres.',
        'Use the larger denominator as the common piece size.',
        {'a':[1,2,3], 'b':[1,2,4,5]}, lambda a,b:a/4+b/12)
    yield recipe('adding-subtracting-fractions/kp2',
        'Two consecutive path sections are ${a}/5$ km and ${b}/6$ km. Give their total length as a reduced fraction.',
        'a/5+b/6', 'The LCM is $30$. Add $(6\\times{a})/30+(5\\times{b})/30$ and reduce.',
        'Find a denominator divisible by both piece counts.',
        {'a':[1,2,3], 'b':[1,2,3,4]}, lambda a,b:a/5+b/6)
    yield recipe('adding-subtracting-fractions/kp3',
        'Three connected lengths measure ${a}/4$, ${b}/6$, and $5/8$ metre. Give their combined length as a reduced fraction.',
        'a/4+b/6+5/8', 'In twenty-fourths the lengths are $(6\\times{a})/24$, $(4\\times{b})/24$, and $15/24$. Add the numerators and reduce.',
        'Choose one common denominator for all three lengths.',
        {'a':[1,2,3], 'b':[2,3,4,5]}, lambda a,b:a/4+b/6+F(5,8))


def fraction_of_and_multiplication():
    yield recipe('fraction-of-a-number/kp1',
        'A strip is ${a}$ cm long. Find the length in cm of one half of the strip.',
        'a/2', 'One half is one of two equal lengths, so divide the whole: ${a}\\div2={a}/2$ cm.',
        'Split the whole length into the number of equal parts named by the denominator.',
        {'a':list(range(14,38,2))}, lambda a:a/2)
    yield recipe('fraction-of-a-number/kp2',
        'A set contains ${a}$ counters. Find the number in a three-quarter share.',
        'a*3/4', 'One quarter contains ${a}\\div4$ counters. Three quarters contain $3\\times({a}\\div4)$.',
        'Find one equal share before counting several shares.',
        {'a':list(range(24,72,4))}, lambda a:a/4*3)
    yield recipe('fraction-of-a-number/kp3',
        'A workshop has ${a}$ seats. One half are occupied. How many seats are occupied?',
        'a/2', 'Divide the ${a}$ seats into two equal groups; the occupied group has ${a}/2$ seats.',
        'Identify the whole count and the denominator of the selected share.',
        {'a':list(range(26,50,2))}, lambda a:a/2)
    yield recipe('multiplying-fractions/kp1',
        'A rectangle has sides ${a}/7$ metre and ${b}/9$ metre. Give its area as a fraction of a square metre.',
        'a*b/63', 'Multiply the side lengths: $({a}\\times{b})/(7\\times9)=({a}\\times{b})/63$.',
        'An area model multiplies the numerator counts and the denominator counts.',
        {'a':[1,2,4,5], 'b':[1,2,4]}, lambda a,b:a*b/63)
    yield recipe('multiplying-fractions/kp2',
        'Find the area of a rectangle with sides ${a}/6$ and ${b}/8$ metres. Give square metres in lowest terms.',
        'a*b/48', 'The product is $({a}\\times{b})/48$. Cancel the common factor between that numerator and $48$.',
        'Multiply the lengths, then inspect common factors in the product.',
        {'a':[2,3,4], 'b':[2,4,6,8]}, lambda a,b:a*b/48)
    yield recipe('multiplying-fractions/kp3',
        'Find the total litres in ${a}$ bottles each holding ${b}/6$ litre. Cross-cancel before multiplying.',
        'a*b/6', 'Write ${a}$ as ${a}/1$. Cancel $\\gcd({a},6)$ before multiplying the remaining numerator by ${b}$.',
        'A whole-number factor can cancel against a fractional denominator.',
        {'a':[2,4,6], 'b':[1,3,5,7]}, lambda a,b:a*b/6)


def division_and_mixed():
    yield recipe('dividing-fractions/kp1',
        'Give the multiplicative inverse of the nonzero fraction ${a}/{b}$.',
        'b/a', 'Swapping numerator and denominator gives ${b}/{a}$, whose product with ${a}/{b}$ is $1$.',
        'A reciprocal must multiply the original to one.',
        {'a':[2,3,4], 'b':[5,6,7,8]}, lambda a,b:b/a)
    yield recipe('dividing-fractions/kp2',
        'How many ${b}/5$-litre measures fit in ${a}/3$ litre, allowing a fractional measure?',
        '(a/3)/(b/5)', 'Divide by the measure size: $({a}/3)\\times(5/{b})=(5\\times{a})/(3\\times{b})$.',
        'Invert the measure size when converting division to multiplication.',
        {'a':[1,2,4], 'b':[1,2,3,4]}, lambda a,b:(a/3)/(b/5))
    yield recipe('dividing-fractions/kp3',
        'Share ${a}/7$ litre equally among ${b}$ cups. Give the litres per cup as a fraction.',
        'a/(7*b)', 'Dividing by ${b}$ means multiplying by $1/{b}$, so each cup holds ${a}/(7\\times{b})$ litre.',
        'Equal sharing by a whole number uses its reciprocal.',
        {'a':[1,2,3], 'b':[2,3,4,5]}, lambda a,b:a/(7*b))
    yield recipe('improper-fractions-mixed-numbers/kp1',
        'Convert ${a}/5$ to a mixed number. Give its whole-number part.',
        'floor(a/5)', 'Divide ${a}$ by $5$: the integer quotient counts wholes, and the remainder gives fifths of the next whole.',
        'Find how many full groups of denominator-sized pieces fit.',
        {'a':[6,7,8,9,11,12,13,14,16,17,18,19]}, lambda a:a//5)
    yield recipe('improper-fractions-mixed-numbers/kp2',
        'Count all fifths in ${a}\\frac{{{b}}}{{5}}$. Give the amount as an improper fraction.',
        '(5*a+b)/5', 'The ${a}$ wholes contain $5\\times{a}$ fifths. Adding ${b}$ fifths gives $(5\\times{a}+{b})/5$.',
        'Convert the whole part to pieces of the same size as the fractional part.',
        {'a':[2,3,4], 'b':[1,2,3,4]}, lambda a,b:a+b/5)
    yield recipe('improper-fractions-mixed-numbers/kp3',
        'Locate ${a}\\frac{{{b}}}{{5}}$ on a number line. Give its distance from the next whole number to the right.',
        'ceiling(a+b/5)-(a+b/5)', 'The next whole is ${a}+1$. The remaining gap is $({a}+1)-({a}+{b}/5)=1-{b}/5$.',
        'Compare the fractional part with one whole.',
        {'a':[2,3,4], 'b':[1,2,3,4]}, lambda a,b:1-b/5)
    yield recipe('mixed-numbers/kp1',
        'Combine lengths ${a}\\frac{{1}}{{3}}$ m and $1\\frac{{{b}}}{{5}}$ m. Give the exact total metres.',
        'a+1/3+1+b/5', 'Whole parts total ${a}+1$; fractional parts give $(5+3\\times{b})/15$. Regroup any whole in that fraction.',
        'Add whole parts and use a common denominator for fractional parts.',
        {'a':[2,3,4], 'b':[1,2,3,4]}, lambda a,b:a+F(1,3)+1+b/5)
    yield recipe('mixed-numbers/kp2',
        'A length of ${a}\\frac{{1}}{{6}}$ m is shortened by $1\\frac{{{b}}}{{6}}$ m. Give the exact remaining metres.',
        'a+1/6-1-b/6', 'Regroup the first length as $({a}-1)+7/6$. Subtraction leaves $({a}-2)+(7-{b})/6$.',
        'Trade a whole for sixths before subtracting a larger fractional part.',
        {'a':[3,4,5], 'b':[2,3,4,5]}, lambda a,b:a+F(1,6)-1-b/6)
    yield recipe('mixed-numbers/kp3',
        'A rectangle is ${a}\\frac{{1}}{{2}}$ m by ${b}/3$ m. Give its area in square metres.',
        '(a+1/2)*b/3', 'The mixed length is $(2\\times{a}+1)/2$. Multiply by ${b}/3$ to obtain $((2\\times{a}+1)\\times{b})/6$.',
        'Convert the mixed length to an improper fraction first.',
        {'a':[2,3,4], 'b':[1,2,4,5]}, lambda a,b:(a+F(1,2))*b/3)


def contextual_division():
    yield recipe('dividing-mixed-numbers/kp1',
        'How many one-half-litre measures are in ${a}\\frac{{1}}{{2}}$ litres?',
        '(a+1/2)/(1/2)', 'Convert to $(2\\times{a}+1)/2$ litres. Dividing by $1/2$ doubles this, giving $2\\times{a}+1$ measures.',
        'Both quantities must use the same fractional unit before division.',
        {'a':list(range(2,14))}, lambda a:2*a+1)
    yield recipe('dividing-mixed-numbers/kp2',
        'A ${a}\\frac{{1}}{{2}}$-metre length is how many times a $1\\frac{{{b}}}{{5}}$-metre length?',
        '(a+1/2)/(1+b/5)', 'Convert the lengths to $(2\\times{a}+1)/2$ and $(5+{b})/5$. Multiply the first by $5/(5+{b})$ and reduce.',
        'Convert both mixed numbers before taking the quotient.',
        {'a':[2,3,4], 'b':[1,2,3,4]}, lambda a,b:(a+F(1,2))/(1+b/5))
    yield recipe('dividing-mixed-numbers/kp3',
        'A pot holds ${a}\\frac{{1}}{{2}}$ litres. How many half-litre jars fill exactly from it?',
        '(a+1/2)/(1/2)', 'The pot holds $(2\\times{a}+1)$ half-litres, so it fills $2\\times{a}+1$ jars.',
        'Count how many serving-sized units make the total volume.',
        {'a':list(range(3,15))}, lambda a:2*a+1)
    yield recipe('fraction-word-problems/kp1',
        'A trail has sections ${a}/5$ km and ${b}/8$ km. What is the total distance as a reduced fraction of a kilometre?',
        'a/5+b/8', 'In fortieths, the distances are $(8\\times{a})/40$ and $(5\\times{b})/40$. Add and reduce.',
        'Match the units before combining the distances.',
        {'a':[1,2,3], 'b':[1,2,3,4]}, lambda a,b:a/5+b/8)
    yield recipe('fraction-word-problems/kp2',
        'A box has ${a}$ beads. One quarter are used. How many beads remain?',
        'a-a/4', 'Used beads number ${a}/4$. Subtract from the whole to get ${a}-{a}/4$ remaining beads.',
        'Work out the selected part, then relate it to the whole.',
        {'a':list(range(28,76,4))}, lambda a:a-a/4)
    yield recipe('fraction-word-problems/kp3',
        'A workshop has ${a}$ metres of wire. Each frame needs half a metre. How many complete frames can be made?',
        'a/(1/2)', 'Divide available wire by wire per frame: ${a}\\div(1/2)={a}\\times2$ frames.',
        'Divide the total quantity by the quantity needed for one item.',
        {'a':list(range(7,19))}, lambda a:a*2)


def recipes():
    for group in (concepts, equivalents_and_comparisons, addition,
                  fraction_of_and_multiplication, division_and_mixed, contextual_division):
        yield from group()
