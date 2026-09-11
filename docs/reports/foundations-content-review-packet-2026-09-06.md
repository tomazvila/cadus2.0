# Foundations content approval packet
This workflow closes the mechanical part of P2.3/A2.2 while preserving the human gate. It exports every pending document from the existing admin review API, binds the packet to the exact bodies and rendered check evidence, and applies only decisions that a reviewer explicitly lists by digest.
## Why this is the shortest safe path
The `/review` screen remains the best place to inspect one document interactively. The queue can exceed its 200-row API page, so `scripts/review/content_review_packet.py` adds a complete, immutable export and a batch decision receipt. It calls the same four admin endpoints as the screen. It does not write SQL, introduce a second approval path, decide content, or contact a model API.
Each packet item contains:
- exact digest, serving key, kind, body, status, and a fingerprint over the live review fields;
- creation time, authoring attempts, exact stored cost, and exact source-manifest matches when present;
- template gate output, eight rendered problem/answer instances, and any instance refusal note.
Teach and hint bodies have no independent rendered instances. Their presence as `pending` proves they passed the import gate; approving a template invokes the existing server re-gate against pending teach and hint bodies before the reviewer proceeds.
## Review procedure
1. Save the admin session cookie header value to a mode-600 scratch file outside the repository.
2. Export the complete pending snapshot:
```sh
python3 scripts/review/content_review_packet.py export \
  --base-url https://cadus.example \
  --cookie-file ~/.cache/cadus-review-cookie \
  --output ~/.cache/cadus-foundations-review.json
```
3. Review the packet. Use `/review` for interactive rendering when needed. Add only reviewed decisions to the generated `cadus-foundations-review.decisions.json`:
```json
{
  "decision_version": 1,
  "packet_sha256": "copy unchanged from the generated file",
  "decisions": [
    {"digest": "sha256:0123456789abcdef", "decision": "approve"},
    {"digest": "sha256:fedcba9876543210", "decision": "reject", "reason": "The worked example exceeds the authored operand bounds."}
  ]
}
```
4. Run the fail-closed dry run. It re-reads every selected live document and writes nothing:
```sh
python3 scripts/review/content_review_packet.py apply \
  --base-url https://cadus.example \
  --cookie-file ~/.cache/cadus-review-cookie \
  --packet ~/.cache/cadus-foundations-review.json \
  --decisions ~/.cache/cadus-foundations-review.decisions.json \
  --receipt ~/.cache/cadus-foundations-review.dry-run.json
```
5. After the human reviewer has finalized that exact decision file, repeat with `--commit` and a new receipt path. The command rejects an empty decision list, a duplicate or unknown digest, a missing rejection reason, a changed packet, a non-pending live document, or any live body/gate/instance drift before its first write.
6. Export a new packet after each committed batch. Template approval can cause the existing server re-gate to reject related pending instruction documents. Preserve both the command receipt and the fresh packet with the release evidence.
## Verified boundary
`scripts/review/test_content_review_packet.py` proves that one selected digest is the only digest submitted, dry-run writes nothing, malformed/implicit/out-of-packet decisions fail closed, packet tampering fails closed, and drift in any selected live document aborts the whole preflight before a write. Existing store, route, and SPA tests separately pin one-row digest writes, admin authorization, body-loaded confirmation, and pending content never serving.
