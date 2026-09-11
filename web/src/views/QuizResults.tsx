/** Completed quiz evidence and untimed, fresh independent practice. */
import { useRef, useState } from 'react';
import { AnswerField, type AnswerFieldHandle } from '@/components/AnswerField';
import { MathBlock } from '@/components/MathBlock';
import { useCall } from '@/hooks/useCall';
import { usePhase } from '@/hooks/usePhase';
import { isQuizReceipt, isRework } from '@/api/types';
import type { AnswerResponse, ApiClient, QuizResultResponse, ServedProblem } from '@/api/types';

type Props = { api: ApiClient; taskId: string; onUnauthorized: () => void };

export function QuizResults({ api, taskId, onUnauthorized, resumePractice = false }: Props & { resumePractice?: boolean }) {
  const [result, setResult] = useState<QuizResultResponse | null>(null);
  const [practicing, setPracticing] = useState(resumePractice);
  const call = useCall({ demo: api.demo, onUnauthorized });
  const [phase, gate] = usePhase<'ready' | 'loading'>('ready');
  const load = (practice: boolean): void => {
    if (!gate.tryEnter('ready', 'loading')) return;
    void call(() => api.taskQuizResult(taskId, practice), (response) => {
      setResult(response);
      setPracticing(practice && response.practice_pending);
      gate.enter('ready');
    }, { onFail: () => gate.enter('ready') });
  };
  if (practicing) return <QuizPractice api={api} taskId={taskId} onUnauthorized={onUnauthorized} />;
  if (!result) return <button type="button" className="btn" disabled={phase === 'loading'} onClick={() => load(false)}>Review results</button>;
  return <div className="quiz-results">
    <h3>Recorded quiz result</h3>
    {result.inconclusive ? <p>Quiz verdict pending: some answers need review. No progress or XP awarded.</p> : <p>{Math.round(result.score * 100)}% of graded answers correct · {result.xp} XP</p>}
    {result.answers.map((answer) => <article key={answer.problem_id} className="card">
      <MathBlock>{answer.text}</MathBlock>
      <p>Your answer: {answer.given_answer || '(blank)'}</p>
      <p>{answer.outcome === 'ungraded' ? `Needs review: ${answer.reason ?? 'No verdict'}` : answer.correct ? 'Correct' : 'Incorrect'}</p>
      {answer.outcome !== 'ungraded' && answer.solution_sketch ? <MathBlock>{answer.solution_sketch}</MathBlock> : null}
    </article>)}
    {result.practice_available || result.practice_pending ? <button type="button" className="btn btn-primary" disabled={phase === 'loading'} onClick={() => load(true)}>Done studying · Practice missed skills</button> : null}
  </div>;
}

function QuizPractice({ api, taskId, onUnauthorized }: Props) {
  const [problem, setProblem] = useState<ServedProblem | null>(null);
  const [feedback, setFeedback] = useState<AnswerResponse | null>(null);
  const [finished, setFinished] = useState(false);
  const answerRef = useRef<AnswerFieldHandle>(null);
  const call = useCall({ demo: api.demo, onUnauthorized });
  const [phase, gate] = usePhase<'ready' | 'loading'>('ready');
  const serve = (): void => {
    if (!gate.tryEnter('ready', 'loading')) return;
    setFeedback(null);
    void call(() => api.taskServe(taskId), (next) => {
      setProblem(next);
      gate.enter('ready');
    }, { onFail: () => gate.enter('ready') });
  };
  const submit = (): void => {
    const answer = answerRef.current?.value();
    if (!problem || !answer || !gate.tryEnter('ready', 'loading')) return;
    void call(() => api.taskAnswer(taskId, { problem_id: problem.problem_id, answer }), (reply) => {
      if (!isQuizReceipt(reply) && !isRework(reply)) {
        setFeedback(reply);
        if (reply.correct && !reply.next && !reply.next_unavailable) setFinished(true);
      }
      setProblem(null);
      gate.enter('ready');
    }, { onFail: () => gate.enter('ready') });
  };
  if (finished) return <p>Independent practice complete. The recorded quiz result is unchanged.</p>;
  return <div className="quiz-practice">
    <h3>Independent practice</h3>
    <p>Untimed. Solve a fresh problem for each missed skill without the worked solution.</p>
    {problem ? <div key={problem.problem_id}>
      <MathBlock>{problem.text}</MathBlock>
      <AnswerField ref={answerRef} disabled={phase === 'loading'} onSubmit={submit} />
      <button type="button" className="btn btn-primary" disabled={phase === 'loading'} onClick={submit}>Submit practice answer</button>
    </div> : <>
      {feedback ? <div><p>{feedback.correct ? 'Correct' : feedback.outcome === 'ungraded' ? 'This answer needs review.' : 'Study the solution, then try a fresh problem.'}</p>{feedback.solution ? <MathBlock>{feedback.solution}</MathBlock> : null}</div> : null}
      <button type="button" className="btn btn-primary" disabled={phase === 'loading'} onClick={serve}>{feedback ? 'Done studying · Next fresh problem' : 'Start fresh practice'}</button>
    </>}
  </div>;
}
