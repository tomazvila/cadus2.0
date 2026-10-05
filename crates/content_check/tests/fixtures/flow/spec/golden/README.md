# Golden pairs (frozen with the freeze pack)

Four KPs as packet + row pairs: `calc-chain-rule`, `linalg-inverse`, `proof-induction`, `stats-ci`.
- Packets: built from the real tree at `d2ca1421` (`dump_curriculum`): topic header, KP block, `existing` with the real base exemplars, their hashes (`hash.md`) and their status. The `teach_page` of `calc-chain-rule` and `linalg-inverse` was written by the lead to rule T1; it was not read from the database. The other two packets have `teach_page: null`.
- Rows: base exemplars with a verdict are `{"keep": hash}`; the two base exemplars of the proof KP have no verdict and are named by `replaces`; each new item is a text of `02-implementation-review.md` section 3.4 with no change. Review items that repeat a kept base exemplar are not in the row.
- Verified by the lead: each row and packet obeys its schema; CK2 holds; each status equals the result of `Exemplar::verdict_policy` (stub of the frozen signature on the real code); each new key grades correct against itself under its contract with `check_contract` (the `function` keys were checked with `canonical_form`, because the variant does not exist at the base); the relaxed CK9 rules (content-spec X11, X12) pass on each new item.
- Check: `FLOW/bin/pyenv FLOW/spec/golden/validate_spec.py` → `RESULT PASS`.
