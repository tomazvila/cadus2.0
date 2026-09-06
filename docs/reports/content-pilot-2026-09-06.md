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
The entire arithmetic unit contains 137 knowledge points; it exceeds this pilot.

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
