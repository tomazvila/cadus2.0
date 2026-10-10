/** Completed quiz evidence and untimed, fresh independent practice. */
import { useRef, useState } from 'react';
import type { AnswerFieldHandle } from '@/components/AnswerField';
import { AnswerInput, TypedSubmit } from '@/components/AnswerInput';
import { MathBlock } from '@/components/MathBlock';
import { useCall } from '@/hooks/useCall';
import { usePhase } from '@/hooks/usePhase';
import { ProblemReport, QuestionReport } from './session/ProblemReport';
import { useProblemReport } from './session/useProblemReport';
import { applyReportCorrection } from './session/applyReportCorrection';
import { QuizProofGrading } from './session/ProofGrading';
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
    <h3>Your quiz result</h3>
    {result.inconclusive ? <p>Some answers are not marked yet. No progress or points are added until they are.</p> : <p>{Math.round(result.score * 100)}% of marked answers right · {result.xp} points</p>}
    {result.answers.map((answer) => <article key={answer.problem_id} className="card">
      <MathBlock>{answer.text}</MathBlock>
      <p>Your answer: {answer.given_answer || '(blank)'}</p>
      {answer.proof_grading ? <QuizProofGrading api={api} field={answer.proof_grading} />
        : <p>{answer.outcome === 'ungraded' ? `Not marked yet: ${answer.reason ?? 'the check did not finish. Your answer is saved.'}` : answer.correct ? 'Correct' : 'Not quite'}</p>}
      {answer.outcome !== 'ungraded' && answer.solution_sketch ? <MathBlock>{answer.solution_sketch}</MathBlock> : null}
      <QuestionReport api={api} onApplied={() => load(false)} context={{ task_id: taskId, problem_id: answer.problem_id,
        report_kind: 'attempt', problem_text: answer.text, answer: answer.given_answer, work: '' }} />
    </article>)}
    {result.practice_available || result.practice_pending ? <button type="button" className="btn btn-primary" disabled={phase === 'loading'} onClick={() => load(true)}>Practice the skills you missed</button> : null}
  </div>;
}

function QuizPractice({ api, taskId, onUnauthorized }: Props) {
  const [problem, setProblem] = useState<ServedProblem | null>(null);
  const [feedback, setFeedback] = useState<AnswerResponse | null>(null);
  const report = useProblemReport(api, undefined, (receipt, context) => {
    setFeedback((previous) => applyReportCorrection(previous, receipt, context));
  });
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
        report.remember({ task_id: taskId, problem_id: problem.problem_id, attempt_id: reply.attempt_id,
          problem_text: problem.text, answer, work: '' });
        setFeedback(reply);
        if (reply.correct && !reply.next && !reply.next_unavailable) setFinished(true);
      }
      setProblem(null);
      gate.enter('ready');
    }, { onFail: () => gate.enter('ready') });
  };
  if (finished) return <><p>Practice done. Your quiz result stays the same.</p><ProblemReport report={report} /></>;
  return <div className="quiz-practice">
    <h3>Practice on your own</h3>
    <p>No timer. Solve a fresh problem for each skill you missed. No solution is shown first.</p>
    {problem ? <div key={problem.problem_id}>
      <MathBlock>{problem.text}</MathBlock>
      <AnswerInput ref={answerRef} choices={problem.choices} contract={problem.answer_contract} disabled={phase === 'loading'} draftKey={problem.problem_id} onSubmit={submit} />
      <QuestionReport api={api} hideResult context={{ task_id: taskId, problem_id: problem.problem_id,
        report_kind: 'served', problem_text: problem.text, answer: '', work: '' }} />
      <TypedSubmit choices={problem.choices} busy={false} disabled={phase === 'loading'} onClick={submit}>Submit</TypedSubmit>
    </div> : <>
      {feedback ? <div><p>{feedback.correct ? 'Correct' : feedback.outcome === 'ungraded' ? 'This answer is not marked yet.' : 'Read the solution, then try a fresh problem.'}</p>{feedback.solution ? <MathBlock>{feedback.solution}</MathBlock> : null}</div> : null}
      <button type="button" className="btn btn-primary" disabled={phase === 'loading'} onClick={serve}>{feedback ? 'Next fresh problem' : 'Start fresh practice'}</button>
    </>}
    <ProblemReport report={report} />
  </div>;
}
