# Whole-course Teach drafts
These 713 Foundations `teach` rows are pending human review. The checked-in canonical coverage, selected-template snapshot, drafts, and independent reviews let the unchanged production gate replay without network or database access.

Run the focused verification:
```sh
cargo test -p cadus-worker --test whole_course_teach
```

The committed drafts stay pending; this directory performs no import or approval.

`canonical_source_template_digest` binds the selected current template. 199 source templates conflict with the rewritten curated exemplars and are retired (156 of them on 2026-10-07, courses rewrite, and one on 2026-10-08): `inputs/templates.json` holds the other 585, and `docs/reports/whole-course-teach-retired-source-templates.json` keeps each retired body, its previous digest, the gate refusal and the superseding commit. The Teach pages of those KPs are gated against the curated exemplars alone (`context_coverage: curated_exemplars_only`, `template_digest: null`). Regenerate the evidence with `cargo run -p cadus-worker --example refresh_whole_course_teach_technical -- <new.json>` and rebind with `python3 scripts/review/refresh_whole_course_teach_v2.py --root . --evidence <new.json> --update`. `author_source_reference` preserves the earlier authoring-source reference where one existed.

The arithmetic Teach rows identified in `manifest.json` are exact mirrors of the integrated `arithmetic-core` repairs. When one of those source rows changes, update its shard, review digest, shard hashes, and canonical array hashes together.

The Foundations merge of 2026-10-12 (D-C1) re-keyed the moved steps in every file here, dropped the rows of 25 twin steps, and left 713 Teach pages, 585 selected source templates, and 199 retired ones. The historical archive rows follow the same keys; their file hashes are re-pinned in `historical-archive/index.json`.
