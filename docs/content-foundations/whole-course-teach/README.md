# Whole-course Teach drafts
These 735 Foundations `teach` rows are pending human review. The checked-in canonical coverage, selected-template snapshot, drafts, and independent reviews let the unchanged production gate replay without network or database access.

Run the focused verification:
```sh
cargo test -p cadus-worker --test whole_course_teach
```

The committed drafts stay pending; this directory performs no import or approval.

`canonical_source_template_digest` binds the selected current template. `author_source_reference` preserves the earlier authoring-source reference where one existed.

The arithmetic Teach rows identified in `manifest.json` are exact mirrors of the integrated `arithmetic-core` repairs. When one of those source rows changes, update its shard, review digest, shard hashes, and canonical array hashes together.
