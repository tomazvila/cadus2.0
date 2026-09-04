/**
 * The worked example of a lesson (L4).
 *
 * PEDAGOGY: a lesson teaches BEFORE it practises. The learner reads this, then presses
 * "I've got it" — which SERVES the first problem. The serve happens on that press and not
 * before it, because `POST /serve` stamps `started_at`: a problem served behind this page
 * bills the reading time as solve time, and this view mount would issue two writes
 * (NO-2BILL).
 */
import { useEffect, useRef } from 'react';
import { MathBlock } from '@/components/MathBlock';
import { Chip } from '@/components/primitives';
import type { PlanTask, TeachResponse } from '@/api/types';

export interface TeachProps {
  task: PlanTask;
  instruction: TeachResponse;
  onContinue: () => void;
}

export function Teach({ task, instruction, onContinue }: TeachProps) {
  const buttonRef = useRef<HTMLButtonElement>(null);
  const topic = task.topic ?? { id: '', name: null, module: '' };
  const example = instruction.worked_example;

  useEffect(() => { buttonRef.current?.focus(); }, []);

  return (
    <>
      <div className="task-header">
        <div className="task-meta">
          <Chip className="chip-lesson">lesson</Chip>
          <span className="topic-name">{topic.name || topic.id || 'Lesson'}</span>
          {topic.module ? <span className="topic-module">{topic.module}</span> : null}
        </div>
        <div className="task-right">
          <span className="teach-badge">Worked example</span>
        </div>
      </div>

      <div className="card teach-card">
        {/* NOT `.problem-text`: the concept is prose, and borrowing the problem class made
            a 1.0 test assert a problem card against a lesson that never reached one. */}
        <div className="teach-concept">
          <MathBlock className="teach-concept-text">{instruction.concept}</MathBlock>
        </div>
        <div className="teach-example">
          <div className="teach-label">Example</div>
          <MathBlock className="teach-problem">{example.problem}</MathBlock>
          <div className="teach-label">Solution</div>
          <MathBlock className="teach-steps">{example.steps}</MathBlock>
        </div>
        <button ref={buttonRef} type="button" className="btn btn-primary" onClick={onContinue}>
          I&apos;ve got it — practice ▸
        </button>
      </div>
    </>
  );
}
