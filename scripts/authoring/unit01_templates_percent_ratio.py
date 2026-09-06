"""Percent and rate recipes with explicit deterministic answer policies."""
from fractions import Fraction as F
from math import gcd
from unit01_template_common import recipe


def percent_conversion_and_part():
    yield recipe('percent-conversions/kp1',
        'Convert ${a}\\%$ to a decimal. Give its hundredths digit.',
        'a-10*floor(a/10)', 'Percent counts hundredths: ${a}\\%={a}/100$; read the second digit after the decimal point.',
        'Percent uses one hundred as its reference whole.',
        {'a':list(range(31,43))},lambda a:a-10*(a//10))
    yield recipe('percent-conversions/kp2',
        'A shaded region covers ${a}\\%$ of a diagram. Give its fraction in lowest terms.',
        'a/100', 'Start with ${a}/100$ and cancel $\\gcd({a},100)$ from both parts.',
        'Translate the percent into a fraction before reducing it.',
        {'a':[5,10,15,20,25,30,40,50,60,75,80,90]},lambda a:a/100)
    yield recipe('percent-conversions/kp3',
        'A scale factor is ${a}$ times one whole. Convert it to a percent and give just the percent number.',
        '100*a', 'Each whole contributes one hundred percent. Multiply ${a}$ by $100$ to find the percent number.',
        'A percent above one hundred represents more than one whole.',
        {'a':['0.1','0.2','0.25','0.3','0.4','0.5','0.6','0.75','1.2','1.25','1.5','2']},lambda a:100*a)
    yield recipe('percent-of-a-number/kp1',
        'A shipment has ${a}$ items. Thirty percent pass inspection. Use a decimal multiplier to find the count that pass.',
        'a*0.3', 'Thirty percent is $0.30$ of the whole. Multiply ${a}\\times0.30$ for the passing count.',
        'Convert the percent to a decimal before taking the part.',
        {'a':list(range(70,190,10))},lambda a:a*F(3,10))
    yield recipe('percent-of-a-number/kp2',
        'Mentally find the ten-percent service charge on a €{a} bill. Give the charge in euros.',
        'a/10', 'Ten percent is one tenth: ${a}\\div10={a}/10$ euros.',
        'Use the place-value shift for one tenth.',
        {'a':list(range(130,250,10))},lambda a:a/10)
    yield recipe('percent-of-a-number/kp3',
        'A copier scales a ${a}$ cm segment to one hundred fifty percent of its original length. Give the copy length in cm.',
        'a*3/2', 'One hundred fifty percent is one whole plus one half. Add ${a}+{a}/2$ cm.',
        'Break the percent into a whole and a familiar fractional benchmark.',
        {'a':list(range(22,46,2))},lambda a:a*F(3,2))


def percentages_and_wholes():
    yield recipe('percentages/kp1',
        'A progress bar has ${a}$ completed units out of fifty. Give the percent number, without the percent sign.',
        '2*a', 'The completed share is ${a}/50=(2\\times{a})/100$, so the percentage number is $2\\times{a}$.',
        'Compare the completed part with the whole before converting to hundredths.',
        {'a':list(range(10,22))},lambda a:2*a)
    yield recipe('percentages/kp2',
        'A €{a} purchase earns a ten-percent rebate. Find just the rebate in euros.',
        'a/10', 'The rebate is the selected part: ${a}\\times0.10={a}/10$ euros.',
        'Identify whether the question asks for a part or a whole.',
        {'a':list(range(110,230,10))},lambda a:a/10)
    yield recipe('percentages/kp3',
        'A class has ${a}$ students; half attend a workshop. Find the attending count.',
        'a/2', 'The whole is ${a}$ and the selected share is one half, so the count is ${a}/2$.',
        'Identify the given whole and the requested part.',
        {'a':list(range(22,46,2))},lambda a:a/2,
        answer_contract={'kind':'exact'})
    yield recipe('percent-finding-the-whole/kp1',
        'A payment of €{a} covers twenty-five percent of a price. Give the full price in euros.',
        '4*a', 'Twenty-five percent is one quarter; four such payments make the whole price, $4\\times{a}$ euros.',
        'Determine how many benchmark shares complete one whole.',
        {'a':list(range(11,23))},lambda a:4*a)
    yield recipe('percent-finding-the-whole/kp2',
        'A fee of €{a} is thirty-five percent of a price. Find the full price in euros.',
        'a/0.35', 'Thirty-five percent is $7/20$. The whole price is ${a}\\div(7/20)={a}\\times20/7$ euros.',
        'Divide the known part by its decimal share of the whole.',
        {'a':list(range(14,98,7))},lambda a:a*F(20,7))
    yield recipe('percent-finding-the-whole/kp3',
        'A bicycle deposit is €{a}, exactly twenty percent of its full price. Give the full price in euros.',
        '5*a', 'The deposit is one fifth of the whole, so the price is $5\\times{a}$ euros.',
        'Relate the known payment to the whole purchase price.',
        {'a':list(range(21,33))},lambda a:5*a)


def ratios_and_rates():
    yield recipe('understanding-ratios/kp1',
        'A set has ${a}$ red counters and seven blue counters. Give the red-to-blue ratio in simplest colon form.',
        'a/7', 'The counts give ${a}:7$; their GCF is one, so both ratio parts remain unchanged.',
        'Keep the requested order of the two categories.',
        {'a':[1,2,3,4,5,6,8,9,10,11,12,13]},
        lambda a:f'{int(a)}:7')
    yield recipe('understanding-ratios/kp2',
        'Simplify the ratio ${a}:40$ and use colon notation.',
        'a/40', 'Divide both ${a}$ and $40$ by $\\gcd({a},40)$ to obtain the two reduced parts.',
        'Divide both parts by their greatest common factor.',
        {'a':[2,4,5,6,8,10,12,14,15,16,18,20]},
        lambda a:f'{int(a)//gcd(int(a),40)}:{40//gcd(int(a),40)}')
    yield recipe('understanding-ratios/kp3',
        'Scale the ratio ${a}:7$ by ${b}$. Give its new first part.',
        'a*b', 'Multiplying both parts by ${b}$ gives $({a}\\times{b}):(7\\times{b})$.',
        'Use the same multiplier on both parts.',
        {'a':[2,3,4], 'b':[2,3,4,5]},lambda a,b:a*b,
        answer_contract={'kind':'exact'})
    yield recipe('ratio-tables-equivalent-ratios/kp1',
        'A ratio table pairs two input units with ${a}$ output units. Complete the output for ${b}$ input units.',
        'a*b/2', 'The input multiplier is ${b}/2$. Apply it to the output: ${a}\\times({b}/2)$.',
        'Scale both entries of the table row by the same factor.',
        {'a':[3,5,7], 'b':[4,6,8,10]},lambda a,b:a*b/2)
    yield recipe('ratio-tables-equivalent-ratios/kp2',
        'A drawing uses one cm for ${a}$ metres. How many metres correspond to ${b}$ cm?',
        'a*b', 'Scale the ratio $1:{a}$ by ${b}$ to obtain ${b}:({a}\\times{b})$.',
        'Find the scale factor from the first corresponding quantity.',
        {'a':[3,5,7], 'b':[2,3,4,5]},lambda a,b:a*b)
    yield recipe('ratio-tables-equivalent-ratios/kp3',
        'Mix A uses ${a}$ scoops per two cups. Mix B uses ${b}$ scoops per three cups. Give the larger scoop count after both are scaled to six cups.',
        'max(3*a,2*b)', 'Scale A by three and B by two. Compare $3\\times{a}$ and $2\\times{b}$ scoops over the same six cups.',
        'Use a common second part before comparing ratios.',
        {'a':[2,3,4], 'b':[2,4,5,7]},lambda a,b:max(3*a,2*b),
        answer_contract={'kind':'exact'})
    yield recipe('unit-rates/kp1',
        'A pump moves ${a}$ litres in two minutes. Give the litres per minute.',
        'a/2', 'Divide the total litres by elapsed minutes: ${a}\\div2={a}/2$ litres per minute.',
        'A unit rate describes the quantity for one unit of the denominator.',
        {'a':list(range(42,66,2))},lambda a:a/2)
    yield recipe('unit-rates/kp2',
        'Four notebooks cost €{a}. Give the price per notebook in cents.',
        '25*a', 'The cost is $100\\times{a}$ cents for four notebooks. Each costs $(100\\times{a})/4$ cents.',
        'Use one currency unit, then divide by the item count.',
        {'a':list(range(6,18))},lambda a:25*a,
        answer_contract={'kind':'exact'})
    yield recipe('unit-rates/kp3',
        'A scanner handles ${a}$ pages per minute. How many pages can it handle in seven minutes?',
        '7*a', 'Seven equal minute-long groups contain $7\\times{a}$ pages.',
        'Multiply the per-unit rate by the number of time units.',
        {'a':list(range(21,33))},lambda a:7*a)


def proportions_and_applications():
    yield recipe('ratios-proportions/kp1',
        'Solve the proportion ${a}/7=x/21$ for the missing numerator.',
        '3*a', 'The denominator triples from seven to twenty-one. Triple ${a}$ too: $x=3\\times{a}$.',
        'Compare corresponding known parts to determine the scale factor.',
        {'a':list(range(8,20))},lambda a:3*a)
    yield recipe('ratios-proportions/kp2',
        'Five equal boxes contain ${a}$ parts altogether. At that same rate, how many parts are in three boxes?',
        'a/5*3', 'One box contains ${a}/5$ parts. Three boxes contain $3\\times({a}/5)$ parts.',
        'Find the whole-number rate per box before scaling.',
        {'a':list(range(35,95,5))},lambda a:a/5*3)
    yield recipe('ratios-proportions/kp3',
        'A scale model uses one cm for thirty cm. The model measures ${a}$ cm. Give the real length in cm.',
        '30*a', 'The scale ratio multiplies model lengths by thirty: $30\\times{a}$ cm.',
        'Use corresponding lengths in the same measurement unit.',
        {'a':list(range(3,15))},lambda a:30*a)
    yield recipe('percent-applications/kp1',
        'A €{a} item receives a twenty-percent discount. Give the reduced price in euros.',
        'a*4/5', 'After the discount, eighty percent remains. Multiply ${a}\\times0.8$ for the new price.',
        'Find the fraction of the original price that remains.',
        {'a':list(range(35,95,5))},lambda a:a*F(4,5))
    yield recipe('percent-applications/kp2',
        'After a twenty-percent increase, an item costs €{a}. Find its earlier price in euros.',
        'a*5/6', 'The final price is $1.2$ times the original. Divide: ${a}/1.2={a}\\times5/6$ euros.',
        'Use the multiplier linking the original price to the final price.',
        {'a':list(range(42,114,6))},lambda a:a*F(5,6))
    yield recipe('percent-applications/kp3',
        'A €{a} order is discounted ten percent, then charged twenty percent tax on the discounted price. Give the final euros.',
        'a*9/10*6/5', 'The discount leaves $0.9\\times{a}$ euros. Tax then multiplies that amount by $1.2$, giving $1.08\\times{a}$ euros.',
        'Apply the second percent to the amount left after the first change.',
        {'a':list(range(25,85,5))},lambda a:a*F(27,25))


def interest():
    yield recipe('simple-interest/kp1',
        'A €{a} balance earns five percent simple interest for exactly one year. Give just the interest in euros.',
        'a/20', 'One-year interest is principal times the annual rate: ${a}\\times0.05={a}/20$ euros.',
        'Use the annual rate on the original principal.',
        {'a':list(range(420,660,20))},lambda a:a/20)
    yield recipe('simple-interest/kp2',
        'A €{a} loan accrues five percent simple interest each year for three years. Give total interest in euros.',
        'a*3/20', 'Each year earns ${a}/20$ euros. Three equal yearly amounts give $3\\times{a}/20$ euros.',
        'Simple interest uses the unchanged principal in every year.',
        {'a':list(range(420,660,20))},lambda a:a*F(3,20))
    yield recipe('simple-interest/kp3',
        'A €{a} deposit earns five percent simple interest per year for two years. Give the final balance in euros.',
        'a*11/10', 'Two years add $2\\times0.05\\times{a}=0.10\\times{a}$ euros. Add principal to obtain $1.10\\times{a}$.',
        'Find the simple interest and then add it to the starting principal.',
        {'a':list(range(410,530,10))},lambda a:a*F(11,10))


def recipes():
    for group in (percent_conversion_and_part, percentages_and_wholes, ratios_and_rates,
                  proportions_and_applications, interest):
        yield from group()
