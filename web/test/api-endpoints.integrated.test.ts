import { beforeEach, expect, it, vi } from 'vitest';
import { api } from '@/api/endpoints';

const transport = vi.fn<typeof fetch>();
beforeEach(() => {
  transport.mockReset();
  transport.mockImplementation(async () => Response.json({ recorded: true }));
  vi.stubGlobal('fetch', transport);
});

it('resolves integrated serve and hint from the escaped task and whitelists hint fields', async () => {
  await api.taskIntegrated('task/with space');
  const hint = { field: 'step/one', index: 0, item_id: 'caller-controlled' };
  await api.taskIntegratedHint('task/with space', hint);
  expect(transport.mock.calls.map(([path, init]) => ({ path, method: init?.method, body: init?.body }))).toEqual([
    { path: '/api/task/task%2Fwith%20space/integrated', method: 'POST', body: '{}' },
    { path: '/api/task/task%2Fwith%20space/integrated/hint', method: 'POST', body: '{"field":"step/one","index":0}' },
  ]);
});

it('sends only authored answer fields while preserving assistance counts and the learner note', async () => {
  const submission = {
    method: 'proportion', reasoning: 'Both quantities scale together.', item_id: 'injected',
    steps: [{ id: 'scaled', answer: '12', hints_used: 2, correct: true }],
    final_answer: { id: 'final', answer: '24', hints_used: 0, assisted: false },
  };
  await expect(api.taskIntegratedAnswer('quiz/one', submission)).resolves.toEqual({ recorded: true });
  const [path, init] = transport.mock.calls[0];
  expect(path).toBe('/api/task/quiz%2Fone/integrated/answer');
  expect(init?.method).toBe('POST');
  expect(JSON.parse(String(init?.body))).toEqual({
    method: 'proportion',
    reasoning: 'Both quantities scale together.',
    steps: [{ id: 'scaled', answer: '12', hints_used: 2 }],
    final_answer: { id: 'final', answer: '24', hints_used: 0 },
  });
});

it.each([{}, { method: null }])('omits an absent method (%j) and reasoning from the submission', async (optional) => {
  const final_answer = { id: 'result', answer: '0', hints_used: 0 };
  await api.taskIntegratedAnswer('task', { ...optional, steps: [], final_answer });
  expect(JSON.parse(String(transport.mock.calls[0][1]?.body))).toEqual({ steps: [], final_answer });
});

it('preserves explicitly empty method and reasoning strings for server validation', async () => {
  await api.taskIntegratedAnswer('task', {
    method: '', reasoning: '', steps: [], final_answer: { id: 'final', answer: '', hints_used: 1 },
  });
  expect(JSON.parse(String(transport.mock.calls[0][1]?.body))).toMatchObject({ method: '', reasoning: '' });
});

it('reads retention and recovery and sends the default quiz practice flag', async () => {
  await api.getRetentionReport();
  await api.listUngraded();
  await api.regradeUngraded('attempt/2', 'incorrect');
  await api.taskQuizResult('quiz');
  expect(transport.mock.calls.map(([path, init]) => [path, init?.method, init?.body])).toEqual([
    ['/api/report/retention', 'GET', undefined],
    ['/api/admin/ungraded', 'GET', undefined],
    ['/api/admin/ungraded/attempt%2F2/regrade', 'POST', '{"outcome":"incorrect"}'],
    ['/api/task/quiz/quiz-result', 'POST', '{"practice":false}'],
  ]);
});
