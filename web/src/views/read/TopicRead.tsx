/**
 * The read page: the approved lesson text of one topic, opened outside a task (D-TR1).
 *
 * The page reads `GET /api/topics/{id}/teach`. The service writes no event for it, so the
 * page records nothing. It has no answer field and no check button. The links come from the
 * course order and the prerequisite edges: the previous and next topic, "Learn first", and
 * "Opens next". The learner reads a neighbor without a return to a list.
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import { ApiError } from '@/api';
import { MathBlock } from '@/components/MathBlock';
import { Chip, LoadingBlock } from '@/components/primitives';
import { ReadingFold } from '@/components/ReadingFold';
import { SecondExampleFold, WorkedExample } from '@/views/session/Teach';
import { LtTermLine, PictureFold, TermList, WatchOutFold } from '@/views/session/TeachExtras';
import '../../styles/fix-read.css';
import type { ApiClient, TopicLink, TopicTeachResponse } from '@/api/types';

export interface TopicReadProps {
  api: ApiClient;
  /** Demo mode. A 401 then keeps the learner on the screen. */
  demo: boolean;
  /** The session-expired path. */
  onUnauthorized: () => void;
  /** The topic to read. */
  topic: string;
  /** Open the read page of a neighbor topic. */
  onOpen: (id: string) => void;
  /** Leave the page for the screen it was opened from. */
  onExit: () => void;
  /** The text of the Back button: where it goes. */
  backLabel?: string;
}

/** The line under a topic id the curriculum does not hold. */
export const READ_NOT_FOUND = 'This topic does not exist. Go back and pick another.';

/** The line under a topic with no approved lesson text. */
export const READ_NO_TEXT = 'This topic has no lesson text yet.';

/** The line under a point with no approved page. */
export const READ_NO_PAGE = 'This step has no lesson text yet.';

/** One read of one topic. The `key` names the request, so a stale reply never shows. */
type State =
  | { kind: 'loading'; key: string }
  | { kind: 'ready'; key: string; page: TopicTeachResponse }
  | { kind: 'failed'; key: string; message: string };

/** The line for a failed read. */
function failureMessage(err: Error): string {
  if (err instanceof ApiError) {
    if (err.status === 404) return READ_NOT_FOUND;
    if (err.status === 409 && err.code === 'no_instruction') return READ_NO_TEXT;
    return err.message;
  }
  return 'The lesson text did not load. Try again.';
}

export function TopicRead({ api, demo, onUnauthorized, topic, onOpen, onExit, backLabel = 'Back' }: TopicReadProps) {
  const [reply, setReply] = useState<State>({ kind: 'loading', key: '' });
  const [attempt, setAttempt] = useState(0);
  const key = `${topic}:${String(attempt)}`;
  // A reply for another topic or another attempt reads as loading.
  const state: State = reply.key === key ? reply : { kind: 'loading', key };
  const unauthorized = useRef(onUnauthorized);
  useEffect(() => { unauthorized.current = onUnauthorized; }, [onUnauthorized]);

  useEffect(() => {
    let live = true;
    window.scrollTo(0, 0);
    api.topicTeach(topic).then(
      (page) => { if (live) setReply({ kind: 'ready', key, page }); },
      (err: Error) => {
        if (!live) return;
        if (err instanceof ApiError && err.sessionExpired && !demo) { unauthorized.current(); return; }
        setReply({ kind: 'failed', key, message: failureMessage(err) });
      },
    );
    return () => { live = false; };
  }, [api, demo, topic, key]);

  const retry = useCallback(() => { setAttempt((n) => n + 1); }, []);

  return (
    <section className="read-page" aria-label="Lesson text">
      {state.kind === 'loading' ? <LoadingBlock label="Loading the lesson text…" /> : null}
      {state.kind === 'failed' ? (
        <div className="card read-error" role="alert">
          <p>{state.message}</p>
          {state.message === READ_NOT_FOUND || state.message === READ_NO_TEXT ? null : (
            <button type="button" className="btn btn-ghost" onClick={retry}>Try again</button>
          )}
        </div>
      ) : null}
      {state.kind === 'ready' ? <ReadBody page={state.page} onOpen={onOpen} /> : null}
      <div className="actions">
        <button type="button" className="btn btn-primary" onClick={onExit}>{backLabel}</button>
      </div>
    </section>
  );
}

function ReadBody({ page, onOpen }: { page: TopicTeachResponse; onOpen: (id: string) => void }) {
  return (
    <>
      <div className="task-header">
        <div className="task-meta">
          <Chip className="chip-lesson">lesson text</Chip>
          <h1 className="h-screen topic-name">{page.name || page.topic}</h1>
          {page.module ? <span className="topic-module">{page.module}</span> : null}
        </div>
      </div>

      <LtTermLine term={page.lt_term} />
      <p className="read-position">
        {`Topic ${String(page.position)} of ${String(page.count)} in ${page.course_name ?? page.course}`}
      </p>
      {page.previous || page.next ? (
        <div className="read-steps">
          {page.previous ? (
            <button type="button" className="btn btn-ghost" onClick={() => { onOpen(page.previous?.id ?? ''); }}>
              {`← Previous: ${page.previous.name ?? page.previous.id}`}
            </button>
          ) : null}
          {page.next ? (
            <button type="button" className="btn btn-ghost" onClick={() => { onOpen(page.next?.id ?? ''); }}>
              {`Next: ${page.next.name ?? page.next.id} →`}
            </button>
          ) : null}
        </div>
      ) : null}

      <LinkList title="Learn first" links={page.learn_first} onOpen={onOpen} />
      <LinkList title="Opens next" links={page.opens_next} onOpen={onOpen} />

      {page.points.map((point) => (
        <div key={point.kp} className="card teach-card read-point">
          <h2 className="read-point-name">{point.name}</h2>
          {point.page ? (
            <>
              <PictureFold picture={point.page.picture} />
              <div className="teach-concept">
                <MathBlock className="teach-concept-text">{point.page.concept}</MathBlock>
              </div>
              <WorkedExample example={point.page.worked_example} />
              <WatchOutFold watchOut={point.page.watch_out} />
              <SecondExampleFold example={point.page.second_example} />
              <TermList terms={point.page.terms} />
            </>
          ) : (
            <p className="muted">{READ_NO_PAGE}</p>
          )}
        </div>
      ))}
      <ReadingFold reading={page.reading} />
    </>
  );
}

function LinkList({ title, links, onOpen }: { title: string; links: TopicLink[]; onOpen: (id: string) => void }) {
  if (links.length === 0) return null;
  return (
    <div className="read-links">
      <h2 className="read-links-title">{title}</h2>
      <ul>
        {links.map((link) => (
          <li key={link.id}>
            <button type="button" className="map-link" onClick={() => { onOpen(link.id); }}>
              {link.name ?? link.id}
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
