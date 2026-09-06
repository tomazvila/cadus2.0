/* Render the real content with the repository's production KaTeX artifact. */
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');
const root = path.resolve(__dirname, '../../..');
const context = { module: { exports: {} }, exports: {} };
vm.runInNewContext(fs.readFileSync(path.join(root, 'web/public/vendor/katex/katex.min.js'), 'utf8'), context);
const katex = context.module.exports;
const evidence = JSON.parse(fs.readFileSync(path.join(root, 'docs/reports/unit07-template-evidence.json'), 'utf8'));
const facts = JSON.parse(fs.readFileSync(path.join(root, 'target/unit07/facts.json'), 'utf8'));
const owned = new Set(JSON.parse(fs.readFileSync(path.join(root, 'target/unit07/candidates.json'), 'utf8')).map(row => row.kp_id));
const fields = evidence.flatMap(row => row.instances.flatMap(item => [item.problem, item.solution_sketch, ...item.hints]));
fields.push(...facts.kps.filter(row => owned.has(row.kp_key)).flatMap(row => row.exemplars.flatMap(item => [item.problem, item.solution_sketch])));
let formulas = 0;
for (const text of fields) {
  const pieces = text.split('$');
  if (pieces.length % 2 === 0) throw new Error(`Unpaired math delimiter: ${text}`);
  for (let i = 1; i < pieces.length; i += 2) {
    if (!pieces[i].trim()) throw new Error(`Empty math: ${text}`);
    try { katex.renderToString(pieces[i], { throwOnError: true, strict: 'error' }); }
    catch (error) { throw new Error(`${text}\n${error.message}`); }
    formulas += 1;
  }
}
const result = { katex_version: katex.version, text_fields: fields.length, formulas, errors: 0 };
fs.writeFileSync(path.join(root, 'docs/reports/unit07-render-evidence.json'), JSON.stringify(result, null, 2) + '\n');
console.log(JSON.stringify(result));
