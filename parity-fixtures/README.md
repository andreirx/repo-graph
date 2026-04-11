# Parity Fixtures

Cross-runtime contract test corpus for the operational dependency seam
detector substrate. Both the TypeScript pipeline (`src/core/seams/detectors/`
+ `src/core/seams/comment-masker.ts`) and the Rust pipeline
(`rust/crates/detectors/`) consume the same shared `detectors.toml` and
the fixtures in this directory to prove byte-identical output against a
curated contract corpus.

## Layout

```
parity-fixtures/
├── README.md
└── <dimension>__<language>__<case>/
    ├── input.<ext>
    └── expected.json
```

Each fixture directory is named `<dimension>__<language>__<case>/` where:

- **dimension** is the contract dimension the fixture justifies
- **language** is the source language (`ts`, `py`, `rs`, `java`, `c`)
- **case** is a short descriptor of the specific case

Dimensions:

- `record` — per-record coverage (one fixture per declared detector record,
  or per distinct call shape that triggers a record)
- `dynamic_literal` — dynamic vs literal path handling for fs records that
  support both
- `single_match` — `single_match_per_line` semantics
- `position_dedup` — `position_dedup_group` overlap handling — same-group
  dedup at the same position, distinct positions both emitting
- `confidence` — hook-supplied confidence overrides (e.g., Java
  `FileOutputStream` dynamic case = 0.80 instead of the static 0.85)
- `hook_merge` — hook output merge semantics — access_kind from line
  context, mode dispatch, fallback to static confidence
- `comment_masking` — comment masker integration (patterns inside comments
  must NOT be detected)

Every dimension has at least one fixture that NAMES it explicitly in the
directory name. Some dimensions are also exercised as a side effect by
`record__*` fixtures, but the explicit `<dimension>__*` fixtures exist so
a parity failure isolates the dimension directly: if `position_dedup__*`
fails, the break is in the dedup subsystem; if `record__*` fails, the
break is in the per-record detector.

## expected.json shape

```json
{
  "env": [
    {
      "varName": "PORT",
      "accessKind": "required",
      "accessPattern": "process_env_dot",
      "filePath": "input.ts",
      "lineNumber": 1,
      "defaultValue": null,
      "confidence": 0.95
    }
  ],
  "fs": []
}
```

Rules (locked during Rust-1 slice planning):

- Both `env` and `fs` keys are always present
- Arrays are always present, even when empty
- `null` fields remain explicit (no absent fields)
- `filePath` is always the fixture-local filename (e.g., `input.ts`), not
  an absolute path
- Array ordering is part of the contract — runtimes must produce records
  in the same order
- Field order inside each record is locked (see `types.rs` /
  `env-dependency.ts` declaration order)

## Curation discipline

Fixtures are curated to justify one contract dimension each. They are NOT
verbatim extractions from either runtime's unit tests. Adding a new fixture
requires naming which dimension it covers and why the existing corpus does
not already cover the case.
