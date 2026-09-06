# Foundations content pilot

## Scope

The pilot covers three knowledge points in `arithmetic-core`:

- `single-digit-addition/kp1`: sums within ten.
- `single-digit-addition/kp2`: sums within twenty, with a bridge through ten.
- `subtraction-facts/kp1`: differences within ten.

The empty bank needs 18 documents: nine templates, three teach pages, three hint
ladders and three diagnosis sets. One pass requests 12 documents. Two later
`--kind template` passes fill the remaining six template slots. Every accepted
body stays `pending` and needs human digest approval before use (C6).
The entire arithmetic unit contains 81 knowledge points; it exceeds this pilot.

## Command

Use the isolated candidate binary with an approved database target and verified
model route. Keep credentials in the existing secure process environment.

```sh
cadus-worker author \
  --kp single-digit-addition/kp1 \
  --kp single-digit-addition/kp2 \
  --kp subtraction-facts/kp1 \
  --budget-usd 5 \
  --request-reserve-usd 0.50 \
  --concurrency 4 \
  --dry-run
```

Remove `--dry-run` only after the route-price check and pilot authorization.
The reserve is $0.50 per HTTP request. OpenRouter author requests enforce
`max_price: {prompt: 1.91, completion: 3.83, request: 0}`,
`allow_fallbacks: false` and `sort: throughput`. Token prices are USD per
million tokens; request price is USD per request. See the official
[provider routing reference](https://openrouter.ai/docs/guides/routing/provider-selection).
The cap applies to one process. Multiple invocations need separate allocations
from the owner's total $100 envelope. The root orchestrator owns that allocation.

## Enforced bounds

The paid CLI requires an explicit budget and an explicit request reserve. Before
each HTTP attempt, including transport retries and truncation retries, a shared
atomic ledger reserves one full request amount. Each request has at most 65,536
serialized JSON bytes and at most 16,000 output tokens. A wider configured output
ceiling fails the same pre-request check. Unknown prices, timeouts and cancellations
retain their full reserve. A confirmed lower price returns the unused reservation to the balance. Unknown
prices retain the full reserve. The cap covers known charges plus outstanding
and unknown charges.

When every response omits its cost, at most ten HTTP requests leave the process.
Confirmed lower costs permit more requests within the same shared cap. A first
pass needs at least 12 requests when every pair succeeds immediately. The normal
five author attempts and two transport attempts permit up to 120 requests, but
the reservation cap also limits retries. The CLI plan includes the hidden HTTP
retry ceiling. A $5 run with ten unknown-priced requests leaves this pilot partial.

The route-price reserve check allocates one input token per serialized request
byte plus 4,096 tokens for provider protocol overhead, and up to 16,000 output
tokens. It refuses a request whose configured reserve falls below this bound.
The route has zero per-request fees and no unknown fallback. Enforce a provider credit limit
for an independent actual-charge cap. The local ledger strictly bounds reserved
money; its actual-charge guarantee depends on the verified request-price bound.

Each HTTP reply is reconciled before any retry. A reported request price above
its reserve, an invalid price or a money overflow stops all later requests.
Already active requests drain and retain their ledger rows. Missing prices remain
unknown and consume the full reserve. The summary reports reserved and known
charged amounts separately and exits with an error after a price-bound breach.

## Throughput and review

At most four knowledge points run together; this three-key pilot uses three. Content kinds remain
ordered: all templates finish before teach pages, then hint ladders, then
diagnosis sets. Each wave drains before its database error returns. Duplicate
knowledge-point keys are refused before a request starts.

Inspect all 12 pending bodies for mathematical correctness and teaching clarity.
Run readiness against the pilot keys after approval. A successful generation pass
alone proves neither content approval nor knowledge-point readiness.

## Status

Prepared command and safeguards; no real model call, live content write or
content approval performed by this lane. Extra API cost remains $0.

## Zero-cost instruction import

Six explicit operator drafts live in
`docs/content-pilot/arithmetic-instruction-drafts.json`. They pass the production
instruction gates against the real curriculum and dense small-number answers.
The worker still checks them against the actual pending templates at import.

After the three-template pilot, set `DATABASE_URL` to the isolated pilot database:

```sh
python3 scripts/authoring/import_pilot_drafts.py \
  --worker /home/deploy/.cache/cadus2_content_target/debug/cadus-worker
```

The helper serves only those six drafts through a loopback endpoint, overrides
its own child credentials with a local placeholder, and runs the normal worker.
Every reported cost is zero and the model id is `operator-draft-v1`. The worker
applies every gate and writes only `pending` content. The helper stops its server
after the worker exits, including on an error. It never contacts a model API.

The concurrency limit supports explicit values up to 64; the pilot remains four.
Provider costs round up to one millionth of a dollar for the reservation ledger.

## Complete zero-cost pilot fallback

Three explicit template drafts live in
`docs/content-pilot/arithmetic-template-drafts.json`. Their exhaustive parameter
spaces contain 62 addition-within-ten problems, 34 bridge-ten problems and 65
subtraction problems. Constraints exclude the corresponding worked-example
operand pairs; addition also excludes the reversed pair. This keeps all three
worked examples outside the practice template spaces.

```sh
python3 scripts/authoring/import_pilot_drafts.py \
  --worker /home/deploy/.cache/cadus2_content_target/debug/cadus-worker \
  --include-templates
```

Against an empty isolated database, the real CLI test stores nine pending rows:
three templates, three teach pages and three hint ladders. The ledger holds nine
local-draft calls and exactly zero API cost. All 161 satisfying template tuples
pass deterministic validation; instruction gates also pass against each complete
space. This supplies one template per knowledge point. Two further template
families and the diagnosis bank remain separate work.

Existing paid template drafts also participate in the instruction gate. If they
contain a worked-example problem, the import refuses that teaching page and
reports a partial pass. The helper preserves every existing content verdict.

Permanent HTTP 400, 401, 403, 404, 405, 410 and 422 responses stop the shared author
job after the first observed rejection. Concurrent requests already in flight
finish their ledger writes. The CLI returns exit 2 for a permanent endpoint
failure, any reservation denial or an all-declined paid pass, and prints the
stored and declined counts. Content-gate repair retries remain available.

## Provider-portable schema and decline artifacts

For providers that emit empty dynamic dictionaries, add these options:

```sh
--portable-schema --decline-dir /home/deploy/.cache/cadus2_orchestration/declines
```

The wire tool then has one required string field, `document_json`. Its string
contains the complete original document JSON, with the original logical schema
supplied as text. The worker decodes it and applies the same production gates.
The logical authoring schema retains its prompt digest; this option changes the
provider transport envelope. Draft artifacts record the decoded arguments, key,
kind, attempt and gate refusal. They contain no client configuration or API key.

Portable template instructions explicitly require named parameter domains and
complete sample bindings. They explain the zero-boundary distractor collision
and permit an empty distractor list. Portable teach instructions include the
actual served problem identities and require distinct worked-example operands.
