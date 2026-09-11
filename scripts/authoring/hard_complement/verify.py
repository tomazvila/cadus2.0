"""Reconstruct printed mathematics independently of template answer expressions."""
import collections
from decimal import Decimal, localcontext
from fractions import Fraction
import json
import math
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[3]
KEYS = ['common-natural-logarithms/kp1', 'common-natural-logarithms/kp2']


def reconstruct(key, problem):
    """Read rendered inputs; evaluate logarithms numerically at high precision."""
    function, base = ('log', Decimal(10)) if key.endswith('kp1') else ('ln', None)
    matches = re.findall(r'\\' + function + r'\((10|e)\^\{\(?(-?\d+)\)?\}\)', problem)
    assert len(matches) == 2, problem
    with localcontext() as context:
        context.prec = 60
        values = []
        for printed_base, exponent in matches:
            assert printed_base == ('10' if base else 'e')
            argument = (base or Decimal(1).exp()) ** int(exponent)
            values.append(argument.log10() if base else argument.ln())
        value = sum(values)
        nearest = value.to_integral_value()
        assert abs(value - nearest) < Decimal('1e-50')
        return str(nearest)


def evidence(report):
    result = []
    occupied = set()
    for row in report['rows']:
        assert row['passed'] and row['kp_id'] in KEYS
        instances = row['evidence']['instances']
        answers = collections.Counter()
        influences = {axis: False for axis in instances[0]['params']}
        for item in instances:
            assert item['problem'] not in occupied
            occupied.add(item['problem'])
            actual = Fraction(item['answer'])
            expected = Fraction(reconstruct(row['kp_id'], item['problem']))
            assert actual == expected, item
            assert actual + 1 != expected  # Wrong-answer negative control.
            answers[str(actual)] += 1
            for other in instances:
                differing = [k for k in influences if item['params'][k] != other['params'][k]]
                if len(differing) == 1 and item['answer'] != other['answer']:
                    influences[differing[0]] = True
        assert all(influences.values()), influences
        count = len(instances)
        assert count >= 12 and len(answers) > 1
        entropy = -sum(n/count * math.log2(n/count) for n in answers.values())
        result.append(dict(kp_id=row['kp_id'], instances=count, distinct_answers=len(answers),
                           answer_entropy_bits=entropy, material_axes=influences,
                           independent_reconstructions=count, wrong_answer_controls=count,
                           collisions=0))
    return result


def main():
    report = json.loads(Path(sys.argv[1]).read_text())
    print(json.dumps(evidence(report), indent=2))


if __name__ == '__main__':
    main()
