/**
 * D-PR1: "Your proofs" — every revision chain with its versions, verdicts and feedback. A
 * review chain is revised here; a lesson chain points back at its lesson. The solution
 * shows only when the service sends it.
 */
import { describe, expect, it, vi } from 'vitest';
import { act, fireEvent, screen, waitFor } from '@testing-library/react';
import { createDemoApi } from '@/api';
import { ProofsScreen, TopicProofs } from '@/views/proofs/Proofs';
import { renderInView } from './helpers/render';
import type { ApiClient, ProofChain, ProofVersion } from '@/api/types';

const SOLUTION = 'Let n = 2k. Then n^2 = 2(2k^2).';

const version = (over: Partial<ProofVersion> = {}): ProofVersion => ({
  id: 'job-1', attempt_id: 'a-1', revision: 0, rewrite: false, status: 'needs_revision',
  answer: 'n is even so n squared is even.', created_at: '2026-10-01T09:00:00Z', seen: true,
  disputed: false, human_verdict: null, feedback: 'Justify the last step.',
  first_unmet: { id: 'G3', text: 'Each step is justified.', evidence: 'so n squared is even' },
  checks: [], ...over,
});

const chain = (over: Partial<ProofChain> = {}): ProofChain => ({
  root_id: 'job-1', head_id: 'job-1', context: 'review', task_id: 't-review', topic: 'parity',
  topic_name: 'Parity', kp: 'kp2', problem: 'Prove that the square of an even integer is even.',
  phase: 'revise', revision: 0, cap: 2, revisions_left: 2, passed: false,
  draft: 'n is even so n squared is even.', versions: [version()], ...over,
});

function api(over: Partial<ApiClient>): ApiClient {
  return { ...createDemoApi(), proofSeen: vi.fn(async () => { throw new Error('unused'); }), ...over };
}

async function show(client: ApiClient) {
  return renderInView(<ProofsScreen api={client} demo={false} onUnauthorized={vi.fn()} onExit={vi.fn()} />);
}

describe('your proofs', () => {
  it('lists a review chain with its feedback and revises it with the last draft prefilled', async () => {
    const reviseProof = vi.fn<ApiClient['reviseProof']>(async () => ({
      proof_grading: { id: 'job-2', status: 'pending' }, chain: chain({ phase: 'grading' }),
    }));
    const listProofs = vi.fn<ApiClient['listProofs']>()
      .mockResolvedValueOnce({ chains: [chain()] })
      .mockResolvedValue({ chains: [chain({ phase: 'grading', head_id: 'job-2',
        versions: [version(), version({ id: 'job-2', status: 'pending', revision: 1 })] })] });
    await show(api({ listProofs, reviseProof }));
    await waitFor(() => expect(screen.getByText('Parity')).toBeTruthy());
    expect(screen.getByText('Needs revision')).toBeTruthy();
    expect(screen.getByText('Justify the last step.')).toBeTruthy();
    expect(screen.queryByText(SOLUTION)).toBeNull();
    const area = screen.getByLabelText('Your proof') as HTMLTextAreaElement;
    expect(area.value).toBe('n is even so n squared is even.');
    await act(async () => { fireEvent.change(area, { target: { value: 'Let n = 2k.' } }); });
    await act(async () => { fireEvent.click(screen.getByRole('button', { name: /Revise and send again/ })); });
    expect(reviseProof).toHaveBeenCalledWith('job-1', 'Let n = 2k.');
    await waitFor(() => expect(screen.getByText('Revision 1')).toBeTruthy());
  });

  it('shows the solution of a passed chain', async () => {
    const listProofs = vi.fn<ApiClient['listProofs']>(async () => ({
      chains: [chain({ phase: 'passed', passed: true, solution: SOLUTION, versions: [version({ status: 'pass' })] })],
    }));
    await show(api({ listProofs }));
    await waitFor(() => expect(screen.getByText(SOLUTION)).toBeTruthy());
    expect(screen.getByText('Finished')).toBeTruthy();
  });

  it('at the cap reveals the solution once on request', async () => {
    const proofSeen = vi.fn<ApiClient['proofSeen']>(async () => ({
      job: { id: 'job-3', attempt_id: 'a-3', status: 'needs_revision' },
      chain: chain({ phase: 'rewrite', solution: SOLUTION }),
    }));
    const listProofs = vi.fn<ApiClient['listProofs']>(async () => ({
      chains: [chain({ phase: 'reveal', head_id: 'job-3', revision: 2, revisions_left: 0 })],
    }));
    await show(api({ listProofs, proofSeen }));
    await waitFor(() => expect(screen.getByText('Show the reference solution')).toBeTruthy());
    expect(proofSeen).not.toHaveBeenCalled();
    await act(async () => { fireEvent.click(screen.getByText('Show the reference solution')); });
    expect(proofSeen).toHaveBeenCalledWith('job-3');
    expect(screen.getByText(SOLUTION)).toBeTruthy();
  });

  it('points a lesson chain back at its lesson', async () => {
    const listProofs = vi.fn<ApiClient['listProofs']>(async () => ({ chains: [chain({ context: 'lesson' })] }));
    await show(api({ listProofs }));
    await waitFor(() => expect(screen.getByText(/you return to it first next time/)).toBeTruthy());
    expect(screen.queryByLabelText('Your proof')).toBeNull();
  });

  it('the topic panel lists the chains of one topic', async () => {
    const listProofs = vi.fn<ApiClient['listProofs']>(async () => ({ chains: [chain()] }));
    await renderInView(<TopicProofs api={api({ listProofs })} topic="parity" />);
    await waitFor(() => expect(screen.getByText('Your proofs on this topic')).toBeTruthy());
    expect(listProofs).toHaveBeenCalledWith('parity');
  });

  it('the topic panel shows nothing for a topic with no proof', async () => {
    const listProofs = vi.fn<ApiClient['listProofs']>(async () => ({ chains: [] }));
    await renderInView(<TopicProofs api={api({ listProofs })} topic="algebra" />);
    await waitFor(() => expect(listProofs).toHaveBeenCalledWith('algebra'));
    expect(screen.queryByText('Your proofs on this topic')).toBeNull();
  });
});
