/**
 * The daily goal control (D-XP1): five presets and one number field.
 *
 * A press sends ONE request, `POST /api/goal/xp`. The new goal applies to every day of the
 * streak, so the toast says the streak counts the days that reach it. No button here is a
 * primary action (W-C2): the dashboard keeps exactly one `.btn-primary`.
 */
import { useState } from 'react';
import { toast } from '@/app/toast';
import type { Call } from '@/hooks/useCall';
import type { ApiClient, XpState } from '@/api/types';

/** The goals the service accepts. */
export const MIN_GOAL = 10;
export const MAX_GOAL = 100;

const PRESETS = [10, 20, 40, 60, 100];

export interface GoalControlProps {
  api: ApiClient;
  /** The one request wrapper of the screen (F-36-1). */
  call: Call;
  /** The goal the status reports now. */
  goal: number;
  /** Called with the XP totals under the saved goal, after the service answers. */
  onSaved: (xp: XpState) => void;
}

export function GoalControl({ api, call, goal, onSaved }: GoalControlProps) {
  const [typed, setTyped] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  const save = async (value: number): Promise<void> => {
    setSaving(true);
    try {
      const res = await call(() => api.setXpGoal(value));
      if (!res) return;
      onSaved(res.xp);
      setTyped('');
      toast(
        `Goal set to ${res.goal} points a day. Your streak counts the days that reach it.`,
        { kind: 'success' },
      );
    } finally {
      setSaving(false);
    }
  };

  const saveTyped = (): void => {
    const text = typed.trim();
    const value = Number(text);
    if (!/^\d+$/.test(text) || value < MIN_GOAL || value > MAX_GOAL) {
      setError('Type a whole number from 10 to 100.');
      return;
    }
    setError(null);
    void save(value);
  };

  return (
    <div className="goal-control">
      <p>{`Daily goal: ${goal} points.`}</p>
      <div className="more-actions">
        {PRESETS.map((value) => (
          <button
            key={value}
            type="button"
            className="btn"
            aria-label={`${value} points a day`}
            aria-pressed={value === goal}
            disabled={saving}
            onClick={() => { setError(null); void save(value); }}
          >
            {value}
          </button>
        ))}
      </div>
      <div className="goal-custom">
        <label htmlFor="goal-typed" className="auth-label">Or type a goal from 10 to 100</label>
        <div className="more-actions">
          <input
            id="goal-typed"
            className="auth-input goal-input"
            type="number"
            min={MIN_GOAL}
            max={MAX_GOAL}
            inputMode="numeric"
            value={typed}
            aria-invalid={error ? true : undefined}
            onChange={(e) => { setTyped(e.target.value); setError(null); }}
          />
          <button type="button" className="btn" disabled={saving} onClick={saveTyped}>
            Set goal
          </button>
        </div>
        {error ? <p className="field-error" role="alert">{error}</p> : null}
      </div>
    </div>
  );
}
