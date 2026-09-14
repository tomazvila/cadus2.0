import { afterEach, expect, it, vi } from 'vitest';
import { api } from '@/api/endpoints';

afterEach(() => vi.unstubAllGlobals());

it('sends only server identity fields for a pre-answer integrated report', async () => {
  const fetch = vi.fn().mockResolvedValue(new Response(JSON.stringify({ report_id: 'r', status: 'queued' }), { status: 200 }));
  vi.stubGlobal('fetch', fetch);
  const context = { problem_id: 'item', report_kind: 'served' as const, item_digest: 'digest', field_id: 'step-1',
    request_id: 'request', problem_text: 'Untrusted text', expected: 'Untrusted answer' };
  await api.taskReport('task/id', context);
  const [url, options] = fetch.mock.calls[0];
  expect(url).toBe('/api/task/task%2Fid/report');
  expect(JSON.parse(options.body)).toEqual({ problem_id: 'item', report_kind: 'served', item_digest: 'digest', field_id: 'step-1', request_id: 'request' });
  expect(options.credentials).toBe('same-origin');
});
