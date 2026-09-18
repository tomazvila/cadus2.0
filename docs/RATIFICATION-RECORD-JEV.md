# Jev Amendment — Ratification Record (evidence artifacts)

## B6 verification call (recorded before any benchmark run)

Date: 2026-09-18 (session logs). Endpoint: `POST https://openrouter.ai/api/alpha/decisions`,
key: existing OpenRouter key. Probe model: `~typesafe/jev-latest`.

| Fact | Result |
|---|---|
| `usage` fields returned | `{"input_tokens": 280, "output_tokens": 20, "cost": 0.00001176}` — input tokens, output tokens, USD cost. No cached-token or reasoning-token fields exist. |
| Resolved model id | `typesafe/jev-1.13-20260917` (versioned, dated) — returned in every response `model` field. |
| Unknown parameter handling | **Silently ignored** at both request level (`provider_order`, `reasoning`) and question level (`prompt_cache_key`): normal answer returned, no error body, no 400. |

Consequence recorded in J1: the Jev client must send no caching, reasoning,
or provider parameters — a silent ignore would mask a false belief that
caching is active. The applicability check repeats when the route leaves
alpha or the resolved id changes.

## Benchmark ground truth (pending)

- Positive set: 200 exemplars, model-drafted formal specifications (J11
  format), person-audited one by one. Audit date and auditor to be recorded
  here before the benchmark run.
- Negative class A (≥2,000, code-made) and class B (≥300 first run): created
  after the deterministic stage and code rephraser exist.
- Split: tune on half, report on the other. Rates reported with 95% upper
  bounds; vote correlation reported alongside.
