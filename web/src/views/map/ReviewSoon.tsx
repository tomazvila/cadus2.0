/**
 * "Review this topic soon": the button of the topic panel for a topic the learner practiced.
 *
 * It asks the service to put the topic in the next plan. The service answers 409 "Learn this
 * topic first." for a topic without practice, and `useCall` shows that message.
 */
import { useState } from 'react';
import { toast } from '@/app/toast';
import type { ApiClient } from '@/api/types';
import type { Call } from '@/hooks/useCall';

export interface ReviewSoonProps {
  api: ApiClient;
  call: Call;
  topic: string;
}

export function ReviewSoon({ api, call, topic }: ReviewSoonProps) {
  const [busy, setBusy] = useState(false);

  async function ask(): Promise<void> {
    setBusy(true);
    try {
      const res = await call(() => api.reviewSoon(topic));
      if (res) toast('Added to your next reviews.', { kind: 'success' });
    } finally {
      setBusy(false);
    }
  }

  return (
    <button type="button" className="btn btn-ghost" disabled={busy} onClick={() => { void ask(); }}>
      Review this topic soon
    </button>
  );
}
