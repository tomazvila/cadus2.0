import { describe, expect, it } from 'vitest';
import { reportStatusText, taskKindText, taskReasonText } from '@/lib/stageCopy';

describe('stage copy', () => {
  it('names a remediation task in learner words and never shows the kind', () => {
    const why = 'remediation (confirm_failed); peel-back lesson';
    expect(taskKindText('lesson', why)).toBe('Extra practice on a skill you missed');
    const text = taskReasonText(why)!;
    expect(text).toBe('You missed this skill earlier, so the lesson comes again.');
    expect(text).not.toMatch(/confirm_failed|peel-back|remediation/);
  });

  it('names a confirmation task', () => {
    const why = 'confirmation; the course inferred this topic and never tested it';
    expect(taskKindText('review', why)).toBe('Checking a skill the starting questions assumed');
    expect(taskReasonText(why)).toMatch(/starting questions/);
  });

  it('maps plain task kinds and selector reasons', () => {
    expect(taskKindText('lesson', 'frontier lesson, 0 in-course dependents, core')).toBe('Lesson');
    expect(taskKindText('drill', null)).toBe('Speed practice');
    expect(taskReasonText('due review; knocks out 2 other due topic(s) via encompassing')).toMatch(/review/);
    expect(taskReasonText('quiz due; 3 questions (1 recent, all-history)')).toBe('This is a quiz on skills you learned.');
    expect(taskReasonText('something new')).toBeNull();
    expect(taskReasonText(null)).toBeNull();
  });

  it('writes the report status without stage ids', () => {
    expect(reportStatusText('running', 'verification', 1, 3)).toBe('Checking the math.');
    expect(reportStatusText('failed', 'worker_interrupted', 2, 3)).not.toMatch(/worker_interrupted/);
    expect(reportStatusText('completed', 'completed', 1, 3)).toMatch(/finished/);
  });
});
