/**
 * The `--name=value` reader `run.mjs` and `serve.mjs` share.
 *
 * Both take `--port=` and `--api=`, and the driver hands the server the values it read, so
 * one reader keeps the two in step.
 */

/** The value of `--name=…` on the command line, or the fallback. */
export function arg(name, fallback) {
  const hit = process.argv.find((a) => a.startsWith(`--${name}=`));
  return hit ? hit.slice(name.length + 3) : fallback;
}
