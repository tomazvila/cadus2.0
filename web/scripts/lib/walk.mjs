/**
 * Every file under a directory, depth first, in the order `readdirSync` lists them.
 *
 * `keep` filters the FILES: the walk descends every directory, so a test file three levels
 * down is found and a directory named like one is not mistaken for one.
 */
import { readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';

export function walk(dir, keep = () => true) {
  const out = [];
  for (const entry of readdirSync(dir)) {
    const p = join(dir, entry);
    if (statSync(p).isDirectory()) out.push(...walk(p, keep));
    else if (keep(entry)) out.push(p);
  }
  return out;
}
