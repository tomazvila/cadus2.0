# Owner decisions

Decisions that REQUIREMENTS.md §8 left open. Each line records the answer and the date.

| ID | Decision | Date | Effect |
|---|---|---|---|
| O1 | Start learners fresh. Do not migrate 1.0 `events`. | 2026-08-26 | No migrator. The `events` row shape stays as in 1.0 because it is the proven shape, not for migration. M3 parity fixtures come from recorded 2.0 streams and from the curriculum answer corpus. |
| O2 | Diagnosis model: DeepSeek V4 now — OpenRouter slug `deepseek/deepseek-v4-pro`, `OPENAI_BASE_URL=https://openrouter.ai/api/v1` (the 1.0 setup; provider order pinned per T5). Later pivot: a local Qwen 3.6 behind an OpenAI-compatible endpoint (`http://10.8.0.3:8080/v1` on the VPN peer; not running on 2026-08-27, port 8080 refused). Caps: NONE — the sole user is the owner; not a public or production deployment. | 2026-08-27 | M5: the model id and base URL are configuration; the T4 per-session and per-call caps keep their knobs with default `0` = unlimited; T5 (prompt caching, reasoning-token cap, provider order) stays as a default because it bounds latency, not spend. |
| O3 | Rewrite the frontend as a React + TypeScript SPA. Do not keep the 1.0 SPA. | 2026-08-26 | M6 builds the new SPA against the HTTP API. The 1.0 `static/` tree is a design reference only (Tokyo Night). |
| — | Review loop: stop after four rounds; fix the accepted-not-fixed items; continue with M1. | 2026-08-26 | Recorded in PROGRESS.md. |
| — | Continue with M3 when M2 is closed. | 2026-08-27 | The M3 survey of the 1.0 scheduler/projector starts in parallel with the M2 verification round; M3 units start after the M2 close commit. |
| — | Continue with M4 when M3 is closed. | 2026-08-27 | The M4 survey (1.0 templates, exemplar rotation, anti-repeat) starts in parallel with the M3 pipeline; M4 units start after the M3 close commit. |
| — | Go for M5 (2026-08-27). D-M5-2 stays on the plan default: a decided miss records `nearly_passable`, a blank records `poor`; the async diagnosis never moves the tier. | 2026-08-27 | M5 pipeline U1–U12 starts from `docs/plans/M5.md`. |
