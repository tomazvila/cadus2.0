"""Audit-clean symbolic recipe data for unit 04."""
DATA = {'additions': {('interpreting-graphs-qualitatively', 'kp1'): [{'answer': 'increasing',
                                                               'contract': '{"kind":"label","options":[["increasing"],["decreasing"],["constant"]]}',
                                                               'contract_override': None,
                                                               'problem': 'A savings-account graph rises steadily from '
                                                                          'left to right. Is the amount saved '
                                                                          'increasing, decreasing, or constant?',
                                                               'solution_sketch': 'The graph rises left to right, so '
                                                                                  'the amount saved is increasing.',
                                                               'with_contract': False},
                                                              {'answer': 'traveling at a constant speed (60 km/h, not '
                                                                         'changing)',
                                                               'contract': '{"kind":"label","options":[["stopped","standing '
                                                                           'still"],["traveling at a constant '
                                                                           'speed","traveling at a constant speed (60 '
                                                                           'km/h, not changing)"],["speeding up"]]}',
                                                               'contract_override': None,
                                                               'problem': 'A car speed-time graph is flat at $60$ km/h '
                                                                          'from $t = 1$ to $t = 3$. Is the car '
                                                                          'stopped, traveling at a constant speed, or '
                                                                          'speeding up?',
                                                               'solution_sketch': 'A flat segment at $60$ means a '
                                                                                  'constant nonzero speed.',
                                                               'with_contract': False}],
               ('interpreting-graphs-qualitatively', 'kp2'): [{'answer': 'after 3 hours, the hiker has walked 12 km',
                                                               'contract': '{"kind":"label","options":[["after 3 '
                                                                           'hours, the hiker has walked 12 '
                                                                           'km"],["after 12 hours, the hiker has '
                                                                           'walked 3 km"],["the hiker walks 4 km per '
                                                                           'hour"]]}',
                                                               'contract_override': None,
                                                               'problem': 'On a distance-versus-time graph, $(3, 12)$ '
                                                                          'is shown. Does it mean after 3 hours the '
                                                                          'hiker has walked 12 km, after 12 hours the '
                                                                          'hiker has walked 3 km, or the hiker walks 4 '
                                                                          'km per hour?',
                                                               'solution_sketch': 'The ordered pair gives time $3$ and '
                                                                                  'distance $12$.',
                                                               'with_contract': False},
                                                              {'answer': 'the break-even point (zero profit)',
                                                               'contract': '{"kind":"label","options":[["break-even '
                                                                           'point","the break-even point (zero '
                                                                           'profit)"],["maximum profit"],["starting '
                                                                           'cost"]]}',
                                                               'contract_override': None,
                                                               'problem': 'A profit graph crosses the horizontal axis. '
                                                                          'Does that crossing mark break-even, maximum '
                                                                          'profit, or starting cost?',
                                                               'solution_sketch': 'On the horizontal axis the profit '
                                                                                  'coordinate is zero.',
                                                               'with_contract': False}],
               ('interpreting-graphs-qualitatively', 'kp3'): [{'answer': 'account X',
                                                               'contract': '{"kind":"label","options":[["account '
                                                                           'X"],["account Y"],["equal growth"]]}',
                                                               'contract_override': None,
                                                               'problem': 'Account X and account Y start at €0, and X '
                                                                          'has the steeper line. Which has faster '
                                                                          'growth: account X, account Y, or equal '
                                                                          'growth?',
                                                               'solution_sketch': 'The steeper line has the greater '
                                                                                  'rate.',
                                                               'with_contract': False},
                                                              {'answer': 'the second hour (the steep part)',
                                                               'contract': '{"kind":"label","options":[["the first '
                                                                           'hour","first hour"],["the second '
                                                                           'hour","the second hour (the steep '
                                                                           'part)"],["both hours equally"]]}',
                                                               'contract_override': None,
                                                               'problem': 'A hiking graph is gentle in the first hour '
                                                                          'and steeper in the second. Was the hiker '
                                                                          'faster in the first hour, second hour, or '
                                                                          'both equally?',
                                                               'solution_sketch': 'The steeper segment has the greater '
                                                                                  'distance per hour.',
                                                               'with_contract': False}],
               ('interpreting-linear-models', 'kp1'): [{'answer': 'the cost per minute of use (€0.08 per minute)',
                                                        'contract': '{"kind":"label","options":[["the cost per '
                                                                    'minute","the cost per minute of use (\\u20ac0.08 '
                                                                    'per minute)"],["the fixed monthly fee"],["the '
                                                                    'total cost after one minute"]]}',
                                                        'contract_override': None,
                                                        'problem': 'For $y = 0.08m + 15$, does $0.08$ mean cost per '
                                                                   'minute, fixed monthly fee, or total cost after one '
                                                                   'minute?',
                                                        'solution_sketch': 'The coefficient of $m$ is the rate per '
                                                                           'minute.',
                                                        'with_contract': False},
                                                       {'answer': 'the candle burns down 0.5 cm per minute',
                                                        'contract': '{"kind":"label","options":[["height decreases 0.5 '
                                                                    'cm per minute","the candle burns down 0.5 cm per '
                                                                    'minute"],["starting height is 0.5 cm"],["height '
                                                                    'increases 0.5 cm per minute"]]}',
                                                        'contract_override': None,
                                                        'problem': 'For candle height $y = -0.5t + 20$, does $-0.5$ '
                                                                   'mean height decreases 0.5 cm per minute, starting '
                                                                   'height 0.5 cm, or height increases 0.5 cm per '
                                                                   'minute?',
                                                        'solution_sketch': 'The negative slope is a decrease of $0.5$ '
                                                                           'cm per minute.',
                                                        'with_contract': False}],
               ('interpreting-linear-models', 'kp2'): [{'answer': 'the fixed monthly fee regardless of minutes used '
                                                                  '(€15)',
                                                        'contract': '{"kind":"label","options":[["the fixed monthly '
                                                                    'fee","the fixed monthly fee regardless of minutes '
                                                                    'used (\\u20ac15)"],["the cost per minute"],["the '
                                                                    'cost after 15 minutes"]]}',
                                                        'contract_override': None,
                                                        'problem': 'For $y = 0.08m + 15$, does $15$ mean fixed monthly '
                                                                   'fee, cost per minute, or cost after 15 minutes?',
                                                        'solution_sketch': 'The intercept is the cost at $m=0$.',
                                                        'with_contract': False},
                                                       {'answer': "the candle's starting height (20 cm)",
                                                        'contract': '{"kind":"label","options":[["starting '
                                                                    'height","the candle\'s starting height (20 '
                                                                    'cm)"],["burn rate"],["height after 20 minutes"]]}',
                                                        'contract_override': None,
                                                        'problem': 'For candle height $y = -0.5t + 20$, does $20$ mean '
                                                                   'starting height, burn rate, or height after 20 '
                                                                   'minutes?',
                                                        'solution_sketch': 'The intercept is the height at $t=0$.',
                                                        'with_contract': False}],
               ('linear-word-problems', 'kp1'): [{'answer': 'y = 3p + 5',
                                                  'contract': None,
                                                  'contract_override': None,
                                                  'problem': 'A delivery service charges €5 plus €3 per package. Write '
                                                             'the cost equation for $p$ packages.',
                                                  'solution_sketch': 'The rate €3 is the slope; the flat €5 is the '
                                                                     'y-intercept.',
                                                  'with_contract': False},
                                                 {'answer': '€32',
                                                  'contract': None,
                                                  'contract_override': None,
                                                  'problem': 'Using the model $y = 3p + 5$, what is the cost of '
                                                             'delivering $9$ packages?',
                                                  'solution_sketch': '$3(9) + 5 = 32$.',
                                                  'with_contract': False}],
               ('parallel-perpendicular-lines', 'kp1'): [{'answer': 'y = 3x + 1',
                                                          'contract': None,
                                                          'contract_override': None,
                                                          'problem': 'Write the line parallel to $y = 3x - 2$ through '
                                                                     '$(1, 4)$.',
                                                          'solution_sketch': '$m = 3$; $b = 4 - 3(1) = 1$.',
                                                          'with_contract': False},
                                                         {'answer': 'y = -2x + 7',
                                                          'contract': None,
                                                          'contract_override': None,
                                                          'problem': 'Write the line parallel to $y = -2x + 5$ through '
                                                                     '$(2, 3)$.',
                                                          'solution_sketch': '$m = -2$; $b = 3 - (-2)(2) = 7$.',
                                                          'with_contract': False}],
               ('plotting-points', 'kp3'): [{'answer': '(4, 0)',
                                             'contract': None,
                                             'contract_override': None,
                                             'problem': 'Give the coordinates of the point on the x-axis that is $4$ '
                                                        'units right of the origin.',
                                             'solution_sketch': 'A point on the x-axis has $y = 0$.',
                                             'with_contract': False},
                                            {'answer': '(0, 5)',
                                             'contract': None,
                                             'contract_override': None,
                                             'problem': 'A point lies on the y-axis $5$ units above the origin. Give '
                                                        'its coordinates.',
                                             'solution_sketch': 'A point on the y-axis has $x = 0$.',
                                             'with_contract': False}],
               ('slopes-of-parallel-perpendicular-lines', 'kp1'): [{'answer': 'parallel',
                                                                    'contract': '{"kind":"label","options":[["parallel"],["perpendicular"],["neither"]]}',
                                                                    'contract_override': None,
                                                                    'problem': 'Are the lines $y = 5x - 2$ and $y = 5x '
                                                                               '+ 7$ parallel, perpendicular, or '
                                                                               'neither?',
                                                                    'solution_sketch': 'Equal slopes (both $5$).',
                                                                    'with_contract': False},
                                                                   {'answer': '4',
                                                                    'contract': None,
                                                                    'contract_override': None,
                                                                    'problem': 'What is the slope of any line parallel '
                                                                               'to $y = 4x - 1$?',
                                                                    'solution_sketch': 'Parallel lines share the same '
                                                                                       'slope.',
                                                                    'with_contract': False}]},
 'contracts': {('interpreting-graphs-qualitatively', 'kp1', 0): '{"kind":"label","options":[["standing '
                                                                'still","standing still (distance is not '
                                                                'changing)"],["moving at constant nonzero '
                                                                'speed","traveling at a constant speed"],["speeding '
                                                                'up"]]}',
               ('interpreting-graphs-qualitatively', 'kp1', 1): '{"kind":"label","options":[["increasing"],["decreasing"],["constant"]]}',
               ('interpreting-graphs-qualitatively', 'kp2', 0): '{"kind":"label","options":[["4 items cost '
                                                                '\\u20ac10","4 items cost \\u20ac10 in total"],["10 '
                                                                'items cost \\u20ac4"],["each item costs \\u20ac4"]]}',
               ('interpreting-graphs-qualitatively', 'kp2', 1): '{"kind":"label","options":[["maximum height","the '
                                                                'maximum height the ball reaches"],["starting '
                                                                'height"],["landing time"]]}',
               ('interpreting-graphs-qualitatively', 'kp3', 0): '{"kind":"label","options":[["runner A"],["runner '
                                                                'B"],["equal speed"]]}',
               ('interpreting-graphs-qualitatively', 'kp3', 1): '{"kind":"label","options":[["at the start","at the '
                                                                'start (the steep part)"],["in the middle"],["at the '
                                                                'end"]]}',
               ('interpreting-linear-models', 'kp1', 0): '{"kind":"label","options":[["cost per hour","the cost per '
                                                         'hour of work (\\u20ac12 per hour)"],["fixed fee"],["total '
                                                         'cost after 12 hours"]]}',
               ('interpreting-linear-models', 'kp1', 1): '{"kind":"label","options":[["drains 5 L per minute","the '
                                                         'tank drains 5 L per minute"],["starts with 5 L"],["fills 5 L '
                                                         'per minute"]]}',
               ('interpreting-linear-models', 'kp2', 0): '{"kind":"label","options":[["starting amount","the starting '
                                                         'amount of water (50 L)"],["drain rate"],["amount after 50 '
                                                         'minutes"]]}',
               ('interpreting-linear-models', 'kp2', 1): '{"kind":"label","options":[["fixed fee","the fixed fee '
                                                         'charged regardless of hours (\\u20ac40)"],["hourly '
                                                         'rate"],["cost after 40 hours"]]}',
               ('slopes-of-parallel-perpendicular-lines', 'kp1', 0): '{"kind":"label","options":[["parallel"],["perpendicular"],["neither"]]}'},
 'patches': {('interpreting-graphs-qualitatively', 'kp1', 0): {'answer': None,
                                                               'answer_contract': '{"kind":"label","options":[["standing '
                                                                                  'still","standing still (distance is '
                                                                                  'not changing)"],["moving at '
                                                                                  'constant nonzero speed","traveling '
                                                                                  'at a constant speed"],["speeding '
                                                                                  'up"]]}',
                                                               'problem': 'A distance-time graph is horizontal from '
                                                                          '$t=2$ to $t=5$. Is the walker standing '
                                                                          'still, moving at a constant nonzero speed, '
                                                                          'or speeding up?',
                                                               'solution_sketch': None},
             ('interpreting-graphs-qualitatively', 'kp1', 1): {'answer': None,
                                                               'answer_contract': None,
                                                               'problem': 'A temperature graph goes downward from left '
                                                                          'to right. Is the temperature increasing, '
                                                                          'decreasing, or constant?',
                                                               'solution_sketch': None},
             ('interpreting-graphs-qualitatively', 'kp2', 0): {'answer': None,
                                                               'answer_contract': '{"kind":"label","options":[["4 '
                                                                                  'items cost \\u20ac10","4 items cost '
                                                                                  '\\u20ac10 in total"],["10 items '
                                                                                  'cost \\u20ac4"],["each item costs '
                                                                                  '\\u20ac4"]]}',
                                                               'problem': 'On a cost-versus-items graph, $(4,10)$ is '
                                                                          'shown. Does it mean 4 items cost €10, 10 '
                                                                          'items cost €4, or each item costs €4?',
                                                               'solution_sketch': None},
             ('interpreting-graphs-qualitatively', 'kp2', 1): {'answer': None,
                                                               'answer_contract': '{"kind":"label","options":[["maximum '
                                                                                  'height","the maximum height the '
                                                                                  'ball reaches"],["starting '
                                                                                  'height"],["landing time"]]}',
                                                               'problem': 'On a height-versus-time graph of a thrown '
                                                                          'ball, does the highest point show maximum '
                                                                          'height, starting height, or landing time?',
                                                               'solution_sketch': None},
             ('interpreting-graphs-qualitatively', 'kp3', 0): {'answer': None,
                                                               'answer_contract': None,
                                                               'problem': 'Runner A and runner B start together, and A '
                                                                          'has the steeper distance-time line. Is '
                                                                          'runner A faster, runner B faster, or is '
                                                                          'their speed equal?',
                                                               'solution_sketch': None},
             ('interpreting-graphs-qualitatively', 'kp3', 1): {'answer': None,
                                                               'answer_contract': '{"kind":"label","options":[["at the '
                                                                                  'start","at the start (the steep '
                                                                                  'part)"],["in the middle"],["at the '
                                                                                  'end"]]}',
                                                               'problem': 'A graph rises steeply at first and then '
                                                                          'gently. Was growth faster at the start, in '
                                                                          'the middle, or at the end?',
                                                               'solution_sketch': None},
             ('interpreting-linear-models', 'kp1', 0): {'answer': None,
                                                        'answer_contract': '{"kind":"label","options":[["cost per '
                                                                           'hour","the cost per hour of work '
                                                                           '(\\u20ac12 per hour)"],["fixed '
                                                                           'fee"],["total cost after 12 hours"]]}',
                                                        'problem': 'For repair cost $C=12h+40$, does $12$ mean cost '
                                                                   'per hour, fixed fee, or total cost after 12 hours?',
                                                        'solution_sketch': None},
             ('interpreting-linear-models', 'kp1', 1): {'answer': None,
                                                        'answer_contract': '{"kind":"label","options":[["drains 5 L '
                                                                           'per minute","the tank drains 5 L per '
                                                                           'minute"],["starts with 5 L"],["fills 5 L '
                                                                           'per minute"]]}',
                                                        'problem': 'For tank volume $y=-5x+50$, does $-5$ mean drains '
                                                                   '5 L per minute, starts with 5 L, or fills 5 L per '
                                                                   'minute?',
                                                        'solution_sketch': None},
             ('interpreting-linear-models', 'kp2', 0): {'answer': None,
                                                        'answer_contract': '{"kind":"label","options":[["starting '
                                                                           'amount","the starting amount of water (50 '
                                                                           'L)"],["drain rate"],["amount after 50 '
                                                                           'minutes"]]}',
                                                        'problem': 'For tank volume $y=-5x+50$, does $50$ mean '
                                                                   'starting amount, drain rate, or amount after 50 '
                                                                   'minutes?',
                                                        'solution_sketch': None},
             ('interpreting-linear-models', 'kp2', 1): {'answer': None,
                                                        'answer_contract': '{"kind":"label","options":[["fixed '
                                                                           'fee","the fixed fee charged regardless of '
                                                                           'hours (\\u20ac40)"],["hourly rate"],["cost '
                                                                           'after 40 hours"]]}',
                                                        'problem': 'For repair cost $C=12h+40$, does $40$ mean fixed '
                                                                   'fee, hourly rate, or cost after 40 hours?',
                                                        'solution_sketch': None},
             ('linear-word-problems', 'kp1', 1): {'answer': None,
                                                  'answer_contract': None,
                                                  'problem': 'Using the model $y = 2k + 3$, what is the cost of a '
                                                             '$7$-kilometre ride?',
                                                  'solution_sketch': None}},
 'path': 'curriculum/foundations/04-linear-graphs.yaml',
 'sketches': {('interpreting-graphs-qualitatively', 'kp1', 0): 'A flat segment has zero slope, so the distance is not '
                                                               'changing over that interval.',
              ('interpreting-graphs-qualitatively', 'kp1', 1): 'The graph falls left to right, so the temperature is '
                                                               'decreasing.',
              ('interpreting-graphs-qualitatively', 'kp2', 0): 'The point $(4, 10)$ pairs input $4$ with output $10$: '
                                                               '$4$ items cost €$10$.',
              ('interpreting-graphs-qualitatively', 'kp2', 1): 'The highest point of a height graph marks the maximum '
                                                               'height.',
              ('interpreting-graphs-qualitatively', 'kp3', 0): 'A steeper line covers more distance in the same time, '
                                                               'so runner A is faster.',
              ('interpreting-graphs-qualitatively', 'kp3', 1): 'The steep part rises more per unit of time, so growth '
                                                               'was faster at the start.',
              ('interpreting-linear-models', 'kp1', 0): 'In $y = mx + b$, $m$ is the slope: the cost added per one '
                                                        'more hour.',
              ('interpreting-linear-models', 'kp1', 1): 'The slope $-5$ is the rate of change: the tank loses $5$ L '
                                                        'each minute.',
              ('interpreting-linear-models', 'kp2', 0): 'The intercept is the value at $x = 0$: the tank starts with '
                                                        '$50$ L.',
              ('interpreting-linear-models', 'kp2', 1): 'The intercept is the cost at $h = 0$: a fixed €$40$ charged '
                                                        'no matter what.',
              ('linear-word-problems', 'kp1', 0): 'The rate €2 per km is the slope; the €3 fee is the y-intercept.',
              ('plotting-points', 'kp3', 0): 'A point on the x-axis has $y = 0$.',
              ('plotting-points', 'kp3', 1): 'A point on the y-axis has $x = 0$.',
              ('slopes-of-parallel-perpendicular-lines', 'kp1', 1): 'Parallel lines share the same slope.'}}
