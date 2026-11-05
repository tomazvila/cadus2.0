/**
 * A topic name and a knowledge-point name render `$…$` math through the problem renderer.
 * One test per view family: a name with `$e^{rt}$` shows a math element and no raw `$`;
 * a plain name shows as text.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, render, screen } from '@testing-library/react';
import katex from 'katex';
import renderMathInElement from 'katex/contrib/auto-render';
import { MathName, plainName } from '@/components/MathName';
import { TodayCard } from '@/views/dashboard/TodayCard';
import { PrimaryAction } from '@/views/dashboard/PrimaryAction';
import { StudyHeader, EmptyPlan } from '@/views/session/SessionScreens';
import { TermList } from '@/views/session/TeachExtras';
import { TopicRead } from '@/views/read/TopicRead';
import { status } from './helpers/dashboard';
import { renderInView } from './helpers/render';
import { stubApi } from './helpers/session';
import type { TopicTeachResponse } from '@/api/types';

const MATH = 'From n compoundings per year to $A = Pe^{rt}$';
const PLAIN = 'Continuous Growth and Compounding';

beforeEach(() => {
  vi.stubGlobal('renderMathInElement', renderMathInElement);
  vi.stubGlobal('katex', katex);
});
afterEach(() => { vi.unstubAllGlobals(); });

/** The page shows a math element and no raw `$`. */
function mathShown(root: HTMLElement): boolean {
  return root.querySelector('.katex') !== null && !(root.textContent ?? '').includes('$');
}

describe('MathName', () => {
  it('renders a name with math as a math element', () => {
    const { container } = render(<MathName>{MATH}</MathName>);
    expect(mathShown(container)).toBe(true);
  });

  it('renders a plain name as text and no math element', () => {
    const { container } = render(<MathName>{PLAIN}</MathName>);
    expect(container.querySelector('.katex')).toBeNull();
    expect(container.textContent).toBe(PLAIN);
  });

  it('plainName drops the marks and keeps the source', () => {
    expect(plainName(MATH)).toBe('From n compoundings per year to A = Pe^{rt}');
    expect(plainName(null)).toBe('');
  });
});

describe('the lesson and review header', () => {
  it('renders math in the title', () => {
    const { container } = render(<StudyHeader kind="lesson" chipClass="chip-lesson" title={MATH} />);
    expect(mathShown(container.querySelector('h1')!)).toBe(true);
  });

  it('renders a plain title as text', () => {
    render(<StudyHeader kind="lesson" chipClass="chip-lesson" title={PLAIN} />);
    expect(screen.getByRole('heading', { name: PLAIN })).toBeTruthy();
  });
});

describe('the Today card', () => {
  const withName = (name: string) => {
    const base = status();
    const today = base.today!;
    return status({
      today: {
        ...today,
        set: [{ ...today.set[0]!, topic: { id: 'growth', name } }],
        read: { ...today.read!, topic: { id: 'growth', name } },
      },
    });
  };

  it('renders math in a set line and in the read line', () => {
    const { container } = render(<TodayCard status={withName(MATH)} onRead={vi.fn()} />);
    expect(mathShown(container)).toBe(true);
    expect(container.querySelectorAll('.katex').length).toBeGreaterThanOrEqual(3);
  });

  it('keeps a plain name as one text line', () => {
    render(<TodayCard status={withName(PLAIN)} onRead={vi.fn()} />);
    expect(screen.getByText(`Read: ${PLAIN}`)).toBeTruthy();
  });
});

describe('the plan card', () => {
  const noop = vi.fn(async () => undefined);
  const busy = { cls: (_k: string, c: string) => c, is: () => false, run: vi.fn() };
  const props = (name: string) => ({
    status: status({
      session_open: true,
      plan_preview: { first_topic: { id: 'growth', name } } as never,
    }),
    work: true, busy: busy as never, startSession: noop, enroll: noop, onDiagnostic: vi.fn(),
  });

  it('renders math in the unfinished-lesson line', () => {
    const { container } = render(<PrimaryAction {...props(MATH)} />);
    expect(mathShown(container.querySelector('.primary-sub')!)).toBe(true);
  });

  it('renders a plain name as text', () => {
    const { container } = render(<PrimaryAction {...props(PLAIN)} />);
    expect(container.querySelector('.katex')).toBeNull();
    expect(container.textContent).toContain(`Unfinished: ${PLAIN}.`);
  });
});

describe('the empty-plan and word-list views', () => {
  it('renders math in the empty-plan message', () => {
    const { container } = render(<EmptyPlan message={`${MATH} opens again soon.`} onExit={vi.fn()} />);
    expect(mathShown(container.querySelector('p')!)).toBe(true);
  });

  it('renders math in a word of the word list', () => {
    const { container } = render(<TermList terms={[{ term: '$e^{rt}$', meaning: 'Growth.' }]} />);
    expect(mathShown(container.querySelector('dt')!)).toBe(true);
  });

  it('renders a plain word as text', () => {
    render(<TermList terms={[{ term: 'rate', meaning: 'Growth.' }]} />);
    expect(screen.getByText('rate')).toBeTruthy();
  });
});

describe('the topic read page', () => {
  const page = (name: string): TopicTeachResponse => ({
    topic: 'growth', name, module: 'Models', course: 'c1', course_name: 'Foundations',
    position: 2, count: 3,
    previous: { id: 'a', name }, next: { id: 'b', name: PLAIN },
    learn_first: [{ id: 'a', name }], opens_next: [],
    points: [{ kp: 'kp1', name, page: null }],
  });
  const open = async (name: string) => {
    const view = await renderInView(
      <TopicRead api={stubApi({ topicTeach: async () => page(name) })} demo={false} topic="growth"
        backLabel="Back" onOpen={vi.fn()} onExit={vi.fn()} onUnauthorized={vi.fn()} />,
    );
    await act(async () => { await Promise.resolve(); });
    return view;
  };

  it('renders math in the heading, the point names and the links', async () => {
    const view = await open(MATH);
    expect(mathShown(view.container)).toBe(true);
    expect(view.container.querySelector('h1 .katex')).not.toBeNull();
    expect(view.container.querySelector('h2.read-point-name .katex')).not.toBeNull();
    view.unmount();
  });

  it('renders a plain name as text', async () => {
    const view = await open(PLAIN);
    expect(view.container.querySelector('.katex')).toBeNull();
    expect(screen.getByRole('heading', { level: 1, name: PLAIN })).toBeTruthy();
    view.unmount();
  });
});
