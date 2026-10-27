/**
 * The optional parts of a teach body, as a reviewer reads them (D-TM1, D-TD1).
 *
 * The reviewer approves the whole page, so each part the body holds is listed in plain text
 * above the raw body. The reviewer reads every part without opening the JSON. A body from
 * before the fields holds none, and the component renders nothing.
 */
import { MathBlock } from '@/components/MathBlock';
import type { JsonValue } from '@/api/types';
import { TermList } from '../session/TeachExtras';

type Row = { term: string; meaning: string };

function asRows(value: JsonValue | undefined): Row[] {
  if (!Array.isArray(value)) return [];
  const rows: Row[] = [];
  for (const row of value) {
    if (typeof row !== 'object' || row === null || Array.isArray(row)) continue;
    const { term, meaning } = row;
    if (typeof term === 'string' && typeof meaning === 'string') rows.push({ term, meaning });
  }
  return rows;
}

function asText(value: JsonValue | undefined): string | null {
  return typeof value === 'string' && value.trim() !== '' ? value : null;
}

function asSecond(value: JsonValue | undefined): { problem: string; steps: string[] } | null {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return null;
  const { problem, steps } = value;
  if (typeof problem !== 'string' || !Array.isArray(steps)) return null;
  return { problem, steps: steps.filter((step): step is string => typeof step === 'string') };
}

export function ReviewTeachParts({ body }: { body: JsonValue }) {
  if (typeof body !== 'object' || body === null || Array.isArray(body)) return null;
  const fields = body;
  const terms = asRows(fields.terms);
  const picture = asText(fields.picture);
  const watchOut = asText(fields.watch_out);
  const second = asSecond(fields.second_example);
  return (
    <>
      {picture ? (
        <section aria-labelledby="review-picture-h">
          <h3 id="review-picture-h">Picture of the idea</h3>
          <MathBlock>{picture}</MathBlock>
        </section>
      ) : null}
      {watchOut ? (
        <section aria-labelledby="review-watch-h">
          <h3 id="review-watch-h">Where the method fails</h3>
          <MathBlock>{watchOut}</MathBlock>
        </section>
      ) : null}
      {second ? (
        <section aria-labelledby="review-second-h">
          <h3 id="review-second-h">Second example</h3>
          <MathBlock>{second.problem}</MathBlock>
          <ol>
            {second.steps.map((step, index) => (
              <li key={index}>
                <MathBlock as="span">{step}</MathBlock>
              </li>
            ))}
          </ol>
        </section>
      ) : null}
      {terms.length > 0 ? (
        <section aria-labelledby="review-terms-h">
          <h3 id="review-terms-h">Words of the page ({terms.length})</h3>
          <TermList terms={terms} />
        </section>
      ) : null}
    </>
  );
}
