# Classification Parity Fixtures

Cross-runtime contract test corpus for the classification substrate.
Both the TypeScript classification modules (`src/core/classification/`)
and the Rust crate (`rust/crates/classification/`) consume these
fixtures to prove byte-equal output for every pure classification
function.

## Layout

```
classification-parity-fixtures/
├── README.md
└── <fn>__<scenario>/
    ├── input.json
    └── expected.json
```

Each fixture directory is named `<fn>__<scenario>` where `fn` is the
dispatched function name and `scenario` is a short descriptor.

## input.json shape

```json
{
  "fn": "classify_unresolved_edge",
  "edge": { ... },
  "category": "...",
  "snapshotSignals": { ... },
  "fileSignals": { ... }
}
```

The `fn` field selects which function to call. The remaining
fields are function-specific arguments.

Supported `fn` values:
- `classify_unresolved_edge`
- `derive_blast_radius`
- `compute_matcher_key`
- `match_boundary_facts`
- `detect_framework_boundary`
- `detect_lambda_entrypoints`

## expected.json shape

The expected return value of the function call, serialized as JSON.
For functions returning `Option<T>`, `null` represents `None`.

## No normalization

Classification is pure policy with no generated UIDs, no timestamps,
no randomness. For the same input, both runtimes produce byte-equal
JSON. No normalization pass is needed.

## Coverage rule

Scenario-driven, not compression-driven. Every distinct TS-pinned
behavior class must be represented. Minimum floors per function:

- `classify_unresolved_edge`: one per distinct precedence path
- `derive_blast_radius`: one per receiver-origin + scope combination
- `compute_matcher_key`: one per segment-normalization rule
- `match_boundary_facts`: one per match/nomatch class
- `detect_framework_boundary`: one per Express pattern + negatives
- `detect_lambda_entrypoints`: one per handler pattern + negatives

Signals predicates (`matches_any_alias`, `has_package_dependency`,
etc.) are `pub(crate)` and NOT directly callable from the parity
harness. Their behavior is verified transitively through the
classifier's fixtures, which exercise all four predicates through
the classification rules that call them.
