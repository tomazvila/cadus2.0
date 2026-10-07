/**
 * "Your proofs" (D-PR1): every revision chain, each with its versions, verdicts and feedback.
 *
 * A chain is one problem's drafts. The screen lists the open chains first. A lesson chain is
 * revised inside its lesson, which the plan serves first; a review chain is revised here.
 * The reference solution shows only when the service sends it: after a pass, once the chain
 * closed, or in the one reveal at the revision cap.
 *
 * The same card renders the chains of one topic in the map's topic panel.
 */
import { useEffect, useState } from 'react';
import { MathBlock } from '@/components/MathBlock';
import { LoadingBlock } from '@/components/primitives';
import { useCall, type Call } from '@/hooks/useCall';
import type { ApiClient, ProofChain, ProofPhase, ProofVersion } from '@/api/types';
import { Checks, Dispute, FirstUnmet, Obligations, Solution } from '@/views/session/LessonProof';

/** The label of each phase. */
export const PHASE_LABEL: Record<ProofPhase, string> = {
  draft: 'Not started',
  grading: 'Being checked',
  passed: 'Passed',
  revise: 'Needs revision',
  reveal: 'Revisions used, reference solution ready',
  rewrite: 'Write again without the reference solution',
  unavailable: 'Not graded, send again',
  closed: 'Closed',
};

/** The label of each verdict status of a version. */
const STATUS_LABEL: Record<ProofVersion['status'], string> = {
  pending: 'being checked',
  pass: 'passed',
  needs_revision: 'needs revision',
  failed: 'not graded',
  capped: 'not graded (daily limit)',
};

/** Whether a chain still asks something of the learner. */
export function isOpen(chain: ProofChain): boolean {
  return chain.phase !== 'closed' && !(chain.phase === 'passed' && chain.context !== 'lesson');
}

/** The name of one version inside its chain. */
function versionName(version: ProofVersion, at: number): string {
  if (version.rewrite) return 'Rewrite';
  return at === 0 ? 'First draft' : `Revision ${String(at)}`;
}

export interface ProofsScreenProps {
  api: ApiClient;
  demo: boolean;
  onUnauthorized: () => void;
  onExit: () => void;
}

/** The "Your proofs" screen. */
export function ProofsScreen({ api, demo, onUnauthorized, onExit }: ProofsScreenProps) {
  const call = useCall({ demo, onUnauthorized });
  const chains = useChains(api, call);
  return (
    <section className="view-proofs">
      <div className="map-head">
        <h1>Your proofs</h1>
        <button type="button" className="btn btn-ghost" onClick={onExit}>Done</button>
      </div>
      <ChainList api={api} call={call} chains={chains.list} reload={chains.reload}
        empty="No written proofs yet. They appear here when a lesson or a review asks for one." />
    </section>
  );
}

/** The chains of one topic, for the map's topic panel. */
export function TopicProofs({ api, topic }: { api: ApiClient; topic: string }) {
  const call = useCall({ demo: !!api.demo, onUnauthorized: () => undefined });
  const chains = useChains(api, call, topic);
  if (!chains.list || chains.list.length === 0) return null;
  return (
    <div className="topic-proofs">
      <h3>Your proofs on this topic</h3>
      <ChainList api={api} call={call} chains={chains.list} reload={chains.reload} empty="" />
    </div>
  );
}

/** Load the chains, with a reload the cards call after a write. */
function useChains(api: ApiClient, call: Call, topic?: string) {
  const [list, setList] = useState<ProofChain[] | null>(null);
  const [tick, setTick] = useState(0);
  useEffect(() => {
    let live = true;
    void call(() => api.listProofs(topic), (reply) => { if (live) setList(reply.chains); });
    return () => { live = false; };
  }, [api, call, topic, tick]);
  return { list, reload: () => { setTick((n) => n + 1); } };
}

function ChainList({ api, call, chains, reload, empty }: {
  api: ApiClient;
  call: Call;
  chains: ProofChain[] | null;
  reload: () => void;
  empty: string;
}) {
  if (chains === null) return <LoadingBlock label="Loading your proofs…" />;
  if (chains.length === 0) return empty ? <p className="muted">{empty}</p> : null;
  const open = chains.filter(isOpen).reverse();
  const done = chains.filter((chain) => !isOpen(chain)).reverse();
  return (
    <div className="proof-chains">
      {open.length ? <h2 className="proof-group">Open</h2> : null}
      {open.map((chain) => <ChainCard key={chain.root_id} api={api} call={call} chain={chain} onChanged={reload} />)}
      {done.length ? <h2 className="proof-group">Finished</h2> : null}
      {done.map((chain) => <ChainCard key={chain.root_id} api={api} call={call} chain={chain} onChanged={reload} />)}
    </div>
  );
}

/** One chain: the problem, the versions, the solution when it is open, and what to do next. */
export function ChainCard({ api, call, chain, onChanged }: {
  api: ApiClient;
  call: Call;
  chain: ProofChain;
  onChanged: () => void;
}) {
  const [revealed, setRevealed] = useState<string | null>(null);
  const head = chain.versions[chain.versions.length - 1];
  // A settled head on screen counts as seen; the reveal is never triggered from here.
  const unseen = head !== undefined && !head.seen && head.status !== 'pending' && chain.phase !== 'reveal';
  useEffect(() => {
    if (unseen) api.proofSeen(chain.head_id).catch(() => undefined);
  }, [api, chain.head_id, unseen]);
  return (
    <article className="card proof-chain" data-phase={chain.phase}>
      <div className="proof-chain-head">
        <strong>{chain.topic_name ?? chain.topic ?? 'Proof'}</strong>
        {chain.kp ? <span className="muted">{` · ${chain.kp}`}</span> : null}
        <span className="chip">{PHASE_LABEL[chain.phase]}</span>
        {unseen ? <span className="chip chip-accent">new result</span> : null}
      </div>
      {chain.problem ? <MathBlock>{chain.problem}</MathBlock> : null}
      <ol className="proof-versions">
        {chain.versions.map((version, at) => (
          <li key={version.id}>
            <div>
              <strong>{versionName(version, at)}</strong>
              <span className="muted">{`: ${version.human_verdict ? `checked by a person: ${version.human_verdict.replace('_', ' ')}` : STATUS_LABEL[version.status]}`}</span>
            </div>
            {version.feedback ? <MathBlock className="proof-feedback">{version.feedback}</MathBlock> : null}
            {version.status === 'needs_revision' ? <FirstUnmet unmet={version.first_unmet ?? null} /> : null}
            <details>
              <summary>Your text and the checks</summary>
              <MathBlock className="proof-answer">{version.answer}</MathBlock>
              <Checks checks={version.checks ?? []} />
            </details>
            {version.status !== 'pending' && !version.human_verdict ? (
              <Dispute api={api} call={call} jobId={version.id} disputed={version.disputed} />
            ) : null}
          </li>
        ))}
      </ol>
      {chain.solution ? <Solution text={chain.solution} /> : null}
      {revealed ? <Solution text={revealed} /> : null}
      <NextStep api={api} call={call} chain={chain} revealed={revealed}
        onReveal={setRevealed} onChanged={onChanged} />
    </article>
  );
}

/** What the learner may do next with one chain. */
function NextStep({ api, call, chain, revealed, onReveal, onChanged }: {
  api: ApiClient;
  call: Call;
  chain: ProofChain;
  revealed: string | null;
  onReveal: (solution: string | null) => void;
  onChanged: () => void;
}) {
  if (chain.context === 'lesson') {
    return isOpen(chain)
      ? <p className="muted">This proof belongs to a lesson: you return to it first next time.</p>
      : null;
  }
  if (chain.context !== 'review') return null;
  switch (chain.phase) {
    case 'grading':
      return <p className="muted" role="status">Being checked. Reload this page in a minute.</p>;
    case 'reveal':
      if (revealed) {
        return (
          <button type="button" className="btn btn-primary" onClick={() => { onReveal(null); onChanged(); }}>
            Write my proof again without it
          </button>
        );
      }
      return (
        <button type="button" className="btn" onClick={() => {
          void call(() => api.proofSeen(chain.head_id), (seen) => { onReveal(seen.chain.solution ?? null); });
        }}>
          Show the reference solution
        </button>
      );
    case 'revise':
    case 'unavailable':
    case 'rewrite':
      return <Revise api={api} call={call} chain={chain} onChanged={onChanged} />;
    default:
      return null;
  }
}

/** The textarea of a review chain's next draft. */
function Revise({ api, call, chain, onChanged }: { api: ApiClient; call: Call; chain: ProofChain; onChanged: () => void }) {
  const rewrite = chain.phase === 'rewrite';
  const [text, setText] = useState(rewrite ? '' : chain.draft);
  const [busy, setBusy] = useState(false);
  return (
    <div className="proof-write">
      {rewrite ? <p role="status">Write the proof again from memory, without the reference solution.</p> : <Obligations />}
      <textarea className="work-input proof-input" rows={8} aria-label="Your proof" value={text}
        disabled={busy} onChange={(e) => { setText(e.target.value); }} />
      <div className="actions">
        <button type="button" className="btn btn-primary" disabled={busy || !text.trim()} onClick={() => {
          setBusy(true);
          void call(() => api.reviseProof(chain.head_id, text), () => { setBusy(false); onChanged(); },
            { onFail: () => { setBusy(false); } });
        }}>
          {rewrite ? 'Send my new proof' : 'Revise and send again'}
        </button>
      </div>
    </div>
  );
}
