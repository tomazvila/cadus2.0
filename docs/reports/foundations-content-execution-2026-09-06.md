# Full Foundations content execution

## Completion target

Foundations contains 809 knowledge points. An empty bank requires 4,854 documents:
2,427 templates, 809 teach pages, 809 hint ladders and 809 diagnosis documents.
Solutions are fields of templates and exemplars; there is no separate solution
content kind. The original nine-document manual pilot filled three templates and six
instruction documents, leaving 4,845 bank slots at that checkpoint. A read-only
plan against the evolving isolated manual database at 15:22:43 UTC found 4,805
remaining staged slots and 3,230 one-pass documents. Re-run the plan before execution.
Every generated document stays pending until human digest approval (C6).
Readiness, grade contracts and diverse item coverage remain separate checks.

| Unit | Knowledge points |
|---|---:|
| Arithmetic | 81 |
| Fractions and decimals | 90 |
| Integers and negatives | 46 |
| Expressions and equations | 67 |
| Linear graphs | 69 |
| Systems and inequalities | 75 |
| Exponents and radicals | 78 |
| Polynomials and quadratics | 102 |
| Functions and exponentials | 92 |
| Rational expressions and trigonometry | 103 |
| Measurement and units | 6 |
| Total | 809 |

## One shared allocation

The root orchestrator has $88 unallocated after its pilot allocations. Use one
worker invocation for every content stage so this cap includes all three template
rounds, all other kinds, transport retries and content-repair retries. A second
invocation requires the root to allocate only the verified remaining amount.

The selected provider environment stays in the existing secure process setup.
First print the exact database-dependent plan:

```sh
cadus-worker author --course foundations --template-passes 3 \
  --portable-schema --budget-usd 88 --request-reserve-usd 0.50 \
  --concurrency 64 \
  --decline-dir /home/deploy/.cache/cadus2_orchestration/foundations-declines \
  --dry-run
```

After the portable pilot demonstrates accepted content, run the same command
without `--dry-run`. It completes template rounds one, two and three before
teach pages, then hint ladders, then diagnosis. The printed staged plan includes
occupied bank slots and a worst-case ceiling of ten HTTP attempts per document.
Unsupported answer kinds decline before a model call. Such declines remain
content/contract work; the numeric bank target does not certify authorability.

The request bounds stay at 65,536 serialized bytes, 16,000 output tokens, and
OpenRouter maximum prices of $1.91/M prompt tokens, $3.83/M completion tokens and
zero request fees. Unknown-price calls retain their full $0.50 reserve. Confirmed
lower prices return the unused reserve. Permanent endpoint errors stop the job.

## Useful parallelism

The hard concurrency limit is 64 active knowledge points. A wave drains before
the next wave, which preserves paid-call ledger writes and template/instruction
barriers. Near the budget boundary, the next wave shrinks to the available fresh
request slots. This avoids an initial wave that cannot fund its own requests.

Measure accepted documents per minute, HTTP requests per accepted document,
known charges plus unknown reserves per accepted document, and timeout/429 rates.
Increase concurrency only while accepted throughput improves. Provider limits,
long-tail requests, deterministic gate CPU work and unknown-price reserves bound
the useful rate. Sixty-four tasks are a capacity limit, not a throughput promise.

For 4,845 remaining slots, $88 permits an average of about $0.01816 reserved cost
per accepted document. This includes every rejected attempt and every unknown
bill. The earlier 15-call Gemini pilot accepted zero documents, so it provides
no measured cost or speed per accepted document. Its low price alone is not
completion evidence. At most 176 wholly unknown-priced requests fit in $88;
unknown-price failures must remain rare for a full course to fit this allocation.

## Review and final evidence

After each stage, inspect the decline artifacts and the pending-content inventory.
Reject a duplicate template family as a content-variety gap even when its JSON
body has a new digest. The three-round target counts distinct stored bodies;
it does not by itself prove pedagogically distinct problem families.

After the approved content exists, run the authoritative read-only audit:

```sh
cadus-worker readiness --course foundations \
  --json /home/deploy/.cache/cadus2_orchestration/readiness-foundations-final.json \
  --md /home/deploy/.cache/cadus2_orchestration/readiness-foundations-final.md
```

Completion requires 809 ready knowledge points, no unresolved grade contracts,
and the remaining handover acceptance evidence. Pending row counts alone do not
meet that requirement. Deployment and live learner data remain outside these
isolated authoring commands until the root completes integration and review.
