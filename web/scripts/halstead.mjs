// Halstead difficulty per function for TypeScript, from the TypeScript AST.
// D = (n1 / 2) * (N2 / n2), where n1 = distinct operators, n2 = distinct operands,
// N2 = total operands.
import ts from 'typescript';
import { readFileSync } from 'node:fs';
const files = process.argv.slice(2);
const LIMIT = Number(process.env.HALSTEAD_LIMIT ?? 80);
const isFn = (n) => ts.isFunctionDeclaration(n) || ts.isMethodDeclaration(n) || ts.isArrowFunction(n) || ts.isFunctionExpression(n) || ts.isConstructorDeclaration(n) || ts.isGetAccessor(n) || ts.isSetAccessor(n);
const operandKinds = new Set([ts.SyntaxKind.Identifier, ts.SyntaxKind.PrivateIdentifier, ts.SyntaxKind.StringLiteral, ts.SyntaxKind.NumericLiteral, ts.SyntaxKind.BigIntLiteral, ts.SyntaxKind.NoSubstitutionTemplateLiteral, ts.SyntaxKind.TemplateHead, ts.SyntaxKind.TemplateMiddle, ts.SyntaxKind.TemplateTail, ts.SyntaxKind.RegularExpressionLiteral, ts.SyntaxKind.TrueKeyword, ts.SyntaxKind.FalseKeyword, ts.SyntaxKind.NullKeyword, ts.SyntaxKind.ThisKeyword, ts.SyntaxKind.JsxText]);
function measure(fn, sf) {
  const ops = new Map(), opnds = new Map();
  const visit = (n) => {
    if (n !== fn && isFn(n)) return; // nested functions are measured on their own
    const k = n.kind;
    if (operandKinds.has(k)) { const t = n.getText(sf); opnds.set(t, (opnds.get(t) ?? 0) + 1); }
    else if (k >= ts.SyntaxKind.FirstToken && k <= ts.SyntaxKind.LastToken && k !== ts.SyntaxKind.EndOfFileToken) { const t = ts.tokenToString(k) ?? String(k); ops.set(t, (ops.get(t) ?? 0) + 1); }
    else if (ts.isCallExpression(n) || ts.isNewExpression(n) || ts.isPropertyAccessExpression(n) || ts.isElementAccessExpression(n) || ts.isConditionalExpression(n) || ts.isIfStatement(n) || ts.isForStatement(n) || ts.isForOfStatement(n) || ts.isWhileStatement(n) || ts.isReturnStatement(n) || ts.isJsxElement(n) || ts.isJsxSelfClosingElement(n)) { const t = ts.SyntaxKind[k]; ops.set(t, (ops.get(t) ?? 0) + 1); }
    ts.forEachChild(n, visit);
  };
  visit(fn);
  const n1 = ops.size, n2 = opnds.size, N2 = [...opnds.values()].reduce((a, b) => a + b, 0);
  return n2 === 0 ? 0 : (n1 / 2) * (N2 / n2);
}
let bad = 0, total = 0, max = 0;
for (const f of files) {
  const sf = ts.createSourceFile(f, readFileSync(f, 'utf8'), ts.ScriptTarget.Latest, true, f.endsWith('x') ? ts.ScriptKind.TSX : ts.ScriptKind.TS);
  const walk = (n) => {
    if (isFn(n)) { total++; const d = measure(n, sf); if (d > max) max = d; if (d >= LIMIT) { bad++; const { line } = sf.getLineAndCharacterOfPosition(n.getStart(sf)); console.log(`${f}:${line + 1} ${n.name?.getText(sf) ?? '<anon>'} difficulty=${d.toFixed(1)}`); } }
    ts.forEachChild(n, walk);
  };
  walk(sf);
}
console.log(`functions=${total} over_limit=${bad} max=${max.toFixed(1)}`);
process.exit(bad ? 1 : 0);
