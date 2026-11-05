/**
 * The lesson text after two misses in a row (D-L5).
 *
 * The service names the topic and the point in `teach_first`. This screen reads the approved
 * page of that point through `GET /api/topics/{topic}/teach` (D-TR1), which writes nothing.
 * The learner reads the explanation and the worked example, then presses one button to go on.
 * A page that does not load never blocks the learner: the same button goes on.
 */
import { MathName } from '@/components/MathName';
import { useEffect, useRef, useState } from 'react';
import { MathBlock } from '@/components/MathBlock';
import { Chip, LoadingBlock } from '@/components/primitives';
import type { AnswerResponse, ApiClient, TopicTeachResponse } from '@/api/types';
import { SecondExampleFold, WorkedExample } from './Teach';
import { PictureFold, TermList, WatchOutFold } from './TeachExtras';

export interface ReteachProps {
  api: ApiClient;
  /** The reply that names the page in `teach_first`. A reply without it shows nothing. */
  result: AnswerResponse;
  onContinue: () => void;
}

/** The line under a topic with no approved lesson text. */
export const RETEACH_NO_TEXT = 'There is no lesson text for this skill yet.';

type State =
  | { kind: 'loading' }
  | { kind: 'ready'; page: TopicTeachResponse }
  | { kind: 'failed' };

/** What the next screen is, said before the learner presses the button. */
export function nextLineOf(result: AnswerResponse): string {
  const prerequisite = result.task_status === 'continue' && (result.remediation?.length ?? 0) > 0;
  return prerequisite
    ? 'Next: one question on a skill that this one builds on.'
    : 'Next: a new problem on this skill.';
}

export function Reteach({ api, result, onContinue }: ReteachProps) {
  const topic = result.teach_first?.topic ?? '';
  const kp = result.teach_first?.kp ?? null;
  const nextLine = nextLineOf(result);
  const [state, setState] = useState<State>({ kind: 'loading' });
  const button = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    let live = true;
    window.scrollTo(0, 0);
    api.topicTeach(topic).then(
      (page) => { if (live) setState({ kind: 'ready', page }); },
      () => { if (live) setState({ kind: 'failed' }); },
    );
    return () => { live = false; };
  }, [api, topic]);

  useEffect(() => {
    if (state.kind !== 'loading') button.current?.focus({ preventScroll: true });
  }, [state.kind]);

  const points = state.kind === 'ready'
    ? state.page.points.filter((point) => point.page && (kp === null || point.kp === kp))
    : [];
  const shown = state.kind === 'ready' && points.length === 0
    ? state.page.points.filter((point) => point.page)
    : points;

  return (
    <section className="view-session" aria-busy={state.kind === 'loading'}>
      <div className="task-header">
        <div className="task-meta">
          <Chip className="chip-lesson">read again</Chip>
          <h1 className="h-screen topic-name">
            <MathName>{state.kind === 'ready' ? (state.page.name ?? state.page.topic) : 'Read this first'}</MathName>
          </h1>
        </div>
      </div>
      <p className="muted">Two answers in a row were not right. Read this page, then go on.</p>
      {state.kind === 'loading' ? <LoadingBlock label="Loading the lesson text…" /> : null}
      {state.kind === 'failed' || (state.kind === 'ready' && shown.length === 0) ? (
        <div className="card" role="status"><p>{RETEACH_NO_TEXT}</p></div>
      ) : null}
      {shown.map((point) => point.page ? (
        <div key={point.kp} className="card teach-card read-point">
          <h2 className="read-point-name"><MathName>{point.name}</MathName></h2>
          <PictureFold picture={point.page.picture} />
          <div className="teach-concept">
            <MathBlock className="teach-concept-text">{point.page.concept}</MathBlock>
          </div>
          <WorkedExample example={point.page.worked_example} />
          <WatchOutFold watchOut={point.page.watch_out} />
          <SecondExampleFold example={point.page.second_example} />
          <TermList terms={point.page.terms} />
        </div>
      ) : null)}
      <p className="muted">{nextLine}</p>
      <div className="actions">
        <button ref={button} type="button" className="btn btn-primary" disabled={state.kind === 'loading'} onClick={onContinue}>
          Continue
        </button>
      </div>
    </section>
  );
}
