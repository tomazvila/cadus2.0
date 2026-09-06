"""Rejected probes remain outside the pending template inventory."""
from itertools import product
import json
from build import OUT


def arguments(statement,expression,expected,contract,params=None,sketch=None):
    params=params or {'a':list(range(11,23))}
    samples=[dict(params=dict(zip(params,values)),expected=expected(**dict(zip(params,values))))
             for values in product(*params.values())]
    return dict(statement=statement,answer_expr=expression,answer_contract=contract,
        params={k:dict(kind='choice',values=v) for k,v in params.items()},constraints=[],
        samples=samples,distractors=[],solution_sketch=sketch or
        'Solve each comparison while preserving endpoint inclusion, then combine the requested solution sets.',
        hints=['Check both the comparison direction and whether the endpoint is included.'])


def generate():
    label=lambda values:dict(kind='label',options=[[v] for v in values])
    union={'kind':'inequality_union'}
    rows=[]
    def add(key,reason,args,verdict):
        rows.append(dict(kp_id=key,kind='probe',status='blocked',reason=reason,
                         expected_gate=verdict,arguments=args))
    add('solutions-of-inequalities/kp3',
        'Changing the boundary value leaves the inclusive boundary decision constantly yes; the candidate cancels its only varying numeric input.',
        arguments('Is $x={a}$ a solution of $x+2<={a}+2$?',
            'equalitylabel(a+2,a+2)',lambda a:'yes',label(['yes','no'])), 'accepted')
    add('graphing-inequalities-number-line/kp2',
        'Changing only the endpoint leaves the requested ray direction constantly left. A direct direction-valued parameter does not compute the direction.',
        arguments('For $x<{a}$, does the ray point left or right?', 'g',lambda a,g:g,
            label(['left','right']),{'a':list(range(11,23)),'g':['left']},
            'Values smaller than {a} lie toward {g}, so the ray points {g}.'),'accepted')
    add('writing-inequalities-from-statements/kp2',
        'The translation must retain its operations without solving. The current template expression grammar cannot represent this polynomial relation.',
        arguments('Write an inequality: {a} more than twice x is at most 40. Do not solve.',
            '2*x+a<=40',lambda a:f'2x+{a} <= 40',{'kind':'polynomial_relation'}),'grammar')
    add('and-or-inequalities/kp3',
        'Two separated rays require a union; raw inequality disjunctions cannot be evaluated by the current template grammar.',
        arguments('Solve $x-{a}<-4$ or $x-{a}>5$.','x < a-4 or x > a+5',
            lambda a:f'x < {a-4} or x > {a+5}',union),'grammar')
    add('compound-inequalities/kp3',
        'Each branch has a material two-step bound, but the expression grammar rejects the required two-ray union.',
        arguments('Solve $2x+{a}<-5$ or $3x-{a}>7$.','x < (-5-a)/2 or x > (a+7)/3',
            lambda a:f'x < {-5-a}/2 or x > {a+7}/3',union),'grammar')
    add('basic-absolute-value-inequalities/kp2',
        'A positive radius gives two separated rays. excludepoint would remove just one point and would change the mathematics.',
        arguments('Solve $|x|>{a}$.','x < -a or x > a',
            lambda a:f'x < {-a} or x > {a}',union),'grammar')
    add('absolute-value-inequalities/kp2',
        'A shifted and scaled positive-radius inequality still requires two separated rays, unsupported by the template disjunction writer.',
        arguments('Solve $|2x-{a}|>7$.','x < (a-7)/2 or x > (a+7)/2',
            lambda a:f'x < {a-7}/2 or x > {a+7}/2',union),'grammar')
    add('basic-absolute-value-inequalities/kp3',
        'The KP restricts the right side to negative values. All candidates in this less-than family have the same empty solution set and zero answer entropy.',
        arguments('Solve $|x|<-{a}$.','q',lambda a,q:q,label(['no solution','all real numbers']),
            {'a':list(range(11,23)),'q':['no solution']},
            'An absolute value is nonnegative, whereas -{a} is negative. Therefore there is {q}.'),'accepted')
    return rows


if __name__=='__main__':
    (OUT/'blockers.json').write_text(json.dumps(generate(),indent=2)+'\n')
