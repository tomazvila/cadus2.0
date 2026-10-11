import type { FrontierWait } from '@/api/types';

/** The local calendar day of a date, as a number that compares by day. */
function dayOf(d: Date): number {
  return Date.UTC(d.getFullYear(), d.getMonth(), d.getDate());
}

/** "today at 18:04", "tomorrow at 18:04", or "on 14 Oct at 18:04", in the viewer's local time. Empty for a value that does not parse. */
export function reopensText(iso: string, now: Date = new Date()): string {
  const at = new Date(iso);
  if (Number.isNaN(at.getTime())) return '';
  const time = at.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
  const days = Math.round((dayOf(at) - dayOf(now)) / 86_400_000);
  if (days === 0) return `today at ${time}`;
  if (days === 1) return `tomorrow at ${time}`;
  const date = at.toLocaleDateString(undefined, { day: 'numeric', month: 'short' });
  return `on ${date} at ${time}`;
}

/** "<Topic name> opens again tomorrow at 18:04. A lesson with too many misses waits one day." */
export function frontierWaitText(wait: FrontierWait, now?: Date): string {
  const name = wait.topic.name ?? wait.topic.id;
  const when = reopensText(wait.until, now);
  const opens = when ? `${name} opens again ${when}.` : `${name} opens again soon.`;
  return `${opens} A lesson with too many misses waits one day.`;
}
