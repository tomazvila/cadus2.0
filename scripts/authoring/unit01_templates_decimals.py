"""Exact decimal recipes with bounded precision and independent sample arithmetic."""
from fractions import Fraction as F
from unit01_template_common import recipe


def place_and_compare():
    yield recipe('decimal-place-value/kp1',
        'The decimal $2.{a}{b}$ is expanded by place value. What is the value contributed by its tenths digit?',
        'a/10', 'The tenths digit is ${a}$, so its contribution is ${a}\\times0.1={a}/10$.',
        'The first place after the point counts tenths.',
        {'a':[1,2,3], 'b':[5,6,7,8]}, lambda a,b:a/10)
    yield recipe('decimal-place-value/kp2',
        'A grid represents the decimal $0.{a}{b}$ as a fraction over one hundred. Give that fraction\'s numerator.',
        '10*a+b', 'There are ${a}$ tenths and ${b}$ hundredths, totalling $(10\\times{a}+{b})/100$.',
        'Convert each tenth to ten hundredths.',
        {'a':[1,2,3], 'b':[2,4,6,8]}, lambda a,b:10*a+b)
    yield recipe('decimal-place-value/kp3',
        'Expand $6.{a}0{b}$ by place value. What value is contributed by the thousandths digit?',
        'b/1000', 'The third digit after the point is ${b}$, contributing ${b}\\times0.001={b}/1000$.',
        'Count three places to the right of the decimal point.',
        {'a':[2,3,4], 'b':[1,3,5,7]}, lambda a,b:b/1000)
    yield recipe('comparing-ordering-decimals/kp1',
        'Compare the lengths $0.{a}$ m and $0.{b}9$ m. Give the greater length in metres.',
        'max(a/10,(10*b+9)/100)', 'Write the first as $0.{a}0$. Compare its hundredths count $10\\times{a}$ with $10\\times{b}+9$.',
        'Align the place values before comparing digits.',
        {'a':[5,6,7], 'b':[1,2,3,4]}, lambda a,b:max(a/10,(10*b+9)/100))
    yield recipe('comparing-ordering-decimals/kp2',
        'Two displays show $0.{a}{b}$ m and $0.{a}{b}0$ m. Find the numerical difference in their represented lengths.',
        '(10*a+b)/100-(100*a+10*b)/1000',
        'The second value is $(100\\times{a}+10\\times{b})/1000=(10\\times{a}+{b})/100$. Subtracting the equal values gives zero.',
        'A trailing zero contributes no additional length.',
        {'a':[1,2,3], 'b':[4,5,6,7]}, lambda a,b:F(0))
    yield recipe('comparing-ordering-decimals/kp3',
        'A number line has points $0.{a}$, $0.{b}9$, and $0.{a}02$. Give the smallest coordinate.',
        'min(a/10,min((10*b+9)/100,(100*a+2)/1000))',
        'In thousandths compare $100\\times{a}$, $100\\times{b}+90$, and $100\\times{a}+2$. The least count gives the leftmost point.',
        'Pad with trailing zeros so every position uses the same units.',
        {'a':[6,7,8], 'b':[1,2,3,4]}, lambda a,b:min(a/10,(10*b+9)/100,(100*a+2)/1000))


def rounding_and_shifts():
    yield recipe('rounding-decimals/kp1',
        'A measured length is ${a}$ metres. Round it to the nearest whole metre using halves-up rounding.',
        'floor(a+1/2)', 'For ${a}$ metres, adding $0.5$ moves the halfway boundary to a whole number. Take $\\lfloor {a}+0.5\\rfloor$.',
        'Compare the tenths digit with the halfway boundary.',
        {'a':['2.14','2.65','3.24','3.75','4.34','4.85','5.44','5.95','6.04','6.55','7.14','7.65']}, lambda a:(a+F(1,2))//1)
    yield recipe('rounding-decimals/kp2',
        'A length is ${a}$ m. Round it to the nearest tenth of a metre.',
        'floor(10*a+1/2)/10', 'Count tenths by multiplying ${a}$ by ten. Round that count with $\\lfloor10\\times{a}+0.5\\rfloor$, then divide by ten.',
        'Use the hundredths digit to decide whether the tenths digit increases.',
        {'a':['1.23','1.28','2.34','2.37','3.41','3.46','4.52','4.58','5.63','5.66','6.72','6.79']}, lambda a:((10*a+F(1,2))//1)/10)
    yield recipe('rounding-decimals/kp3',
        'A checkout total is €{a}. Round the total to the nearest cent.',
        'floor(100*a+1/2)/100', 'Round the cents count $100\\times{a}$ to a whole number using $\\lfloor100\\times{a}+0.5\\rfloor$. Convert back to euros; any carry moves through the nines.',
        'The thousandths digit decides whether to add one cent.',
        {'a':['1.995','2.996','3.997','4.998','5.999','6.995','7.996','8.997','9.998','10.999','11.995','12.996']}, lambda a:((100*a+F(1,2))//1)/100)
    for key,statement,expression,sketch,vals,solve in [
        ('kp1','Each of one hundred tiles weighs ${a}$ g. Give the total grams.', '100*a',
         'One hundred groups give $100\\times{a}$ g; each digit shifts two places left.',
         ['0.23','0.24','0.25','0.26','0.27','0.28','0.29','0.31','0.32','0.33','0.34','0.35'], lambda a:a*100),
        ('kp2','A ${a}$-litre batch is shared among one hundred containers. Give litres per container.', 'a/100',
         'Each container receives one hundredth: ${a}\\div100={a}/100$ litre.',
         list(range(31,43)),lambda a:a/100),
        ('kp3','Convert ${a}$ kilometres to metres using one thousand metres per kilometre.', 'a*1000',
         'Each kilometre contains a thousand metres, so multiply ${a}$ by $1000$ and move digits three places left.',
         ['0.013','0.014','0.015','0.016','0.017','0.018','0.019','0.021','0.022','0.023','0.024','0.025'],lambda a:a*1000),
    ]:
        yield recipe('decimal-multiplication-powers-of-ten/'+key, statement, expression,
                     sketch, 'Track how each place value changes under the stated scale.', {'a':vals}, solve)


def operations():
    yield recipe('decimal-addition-subtraction/kp1',
        'Two parts of a route measure ${a}$ km and $2.65$ km. Give the combined distance in km.',
        'a+2.65', 'Align hundredths, then add: $({a}\\times100+265)/100$ km.',
        'Use matching decimal places for both distances.',
        {'a':['1.1','1.2','1.3','1.4','1.5','1.6','1.7','1.8','1.9','2.1','2.2','2.3']},lambda a:a+F(265,100))
    yield recipe('decimal-addition-subtraction/kp2',
        'A tank contains ${a}$ litres. Remove $1.75$ litres. Give the volume remaining in litres.',
        'a-1.75', 'Write the starting volume in hundredths. Subtract $175$ hundredths: $({a}\\times100-175)/100$ litres.',
        'Pad missing decimal places before regrouping and subtracting.',
        {'a':['4.1','4.2','4.3','4.4','4.5','4.6','4.7','4.8','4.9','5.1','5.2','5.3']},lambda a:a-F(175,100))
    yield recipe('decimal-addition-subtraction/kp3',
        'A receipt lists a €{a} book and a €2.85 pen. Give the total cost in euros.',
        'a+2.85', 'Add cent amounts: $100\\times{a}+285$ cents. Divide by one hundred for euros.',
        'Express both prices in the same currency unit before adding.',
        {'a':['3.10','3.20','3.30','3.40','3.50','3.60','3.70','3.80','3.90','4.10','4.20','4.30']},lambda a:a+F(285,100))
    yield recipe('decimal-operations/kp1',
        'A rectangle measures ${a}$ m by $0.6$ m. Give its area in square metres.',
        'a*0.6', 'The area is ${a}\\times0.6$. Multiplying tenths by tenths produces hundredths: $(10\\times{a}\\times6)/100$.',
        'Multiply as whole-number tenths, then restore the decimal scale.',
        {'a':['0.2','0.3','0.4','0.5','0.7','0.8','0.9','1.1','1.3','1.4','1.5','1.6']},lambda a:a*F(6,10))
    yield recipe('decimal-operations/kp2',
        'Share ${a}$ litres equally among four bottles. Give the litres in one bottle.',
        'a/4', 'Convert ${a}$ litres to hundredths. Dividing the count by four gives $(100\\times{a}/4)/100$ litre per bottle.',
        'Divide the complete quantity into equal groups.',
        {'a':['1.2','1.4','1.6','1.8','2.2','2.4','2.6','3.2','3.4','3.8','4.2','4.4']},lambda a:a/4)
    yield recipe('decimal-operations/kp3',
        'How many $0.2$-litre scoops fill a ${a}$-litre container exactly?',
        'a/0.2', 'Multiply both volumes by ten: ${a}\\div0.2=(10\\times{a})\\div2$.',
        'Scale both operands equally until the divisor is a whole number.',
        {'a':['1.2','1.4','1.6','1.8','2.2','2.4','2.6','2.8','3.2','3.4','3.6','3.8']},lambda a:a/F(2,10))


def conversions():
    yield recipe('fraction-decimal-conversion/kp1',
        'Convert ${a}/{b}$ to a decimal. Give the digit in the hundredths place.',
        'floor(100*a/b)-10*floor(10*a/b)', 'Scale the fraction to denominator $1000$: $({a}\\times(1000/{b}))/1000$. Read the second digit after the decimal point.',
        'Choose a power of ten divisible by the fraction denominator.',
        {'a':[1,3,7], 'b':[2,4,5,8]},lambda a,b:(100*a/b)//1-10*((10*a/b)//1))
    yield recipe('fraction-decimal-conversion/kp2',
        'A measurement is ${a}$ metre. Express it as a fraction of a metre in lowest terms.',
        'a', 'Write ${a}$ as $(100\\times{a})/100$, then divide numerator and denominator by their GCF.',
        'Use a power of ten as the initial denominator.',
        {'a':['0.12','0.14','0.16','0.18','0.22','0.24','0.26','0.28','0.32','0.34','0.36','0.38']},lambda a:a)
    yield recipe('fraction-decimal-conversion/kp3',
        'Use long division to write ${a}/16$ as a terminating decimal. Give its thousandths digit.',
        'floor(1000*a/16)-10*floor(100*a/16)', 'Since $16\\times625=10000$, multiply the numerator by $625$: $(625\\times{a})/10000$. Read the third digit after the decimal point.',
        'Continue the division until the remainder reaches zero.',
        {'a':[1,3,5,7,9,11,13,15,17,19,21,23]},lambda a:(1000*a/16)//1-10*((100*a/16)//1))


def recipes():
    for group in (place_and_compare, rounding_and_shifts, operations, conversions):
        yield from group()
