# Storage Parity Fixtures

Cross-runtime contract test corpus for the storage substrate. The
TypeScript adapter (`src/adapters/storage/sqlite/`) and the Rust crate
(`rust/crates/storage/`) both consume the fixtures in this directory to
prove byte-structural equivalence of post-state AND return values
against a curated corpus.

This corpus is the cross-runtime parity gate for Rust-2. Rust-1 used
`parity-fixtures/` at the repo root for detector parity; R2-F uses
this separate `storage-parity-fixtures/` for storage parity, because
the two surfaces have fundamentally different output shapes (detectors
produce JSON fact arrays; storage produces database state plus method
return values).

## Layout

```
storage-parity-fixtures/
├── README.md                                   ← this file
└── <fixture-name>/
    ├── operations.json                         ← the operation script
    └── expected.json                           ← expected post-state + results
```

Each fixture directory is named descriptively (no language suffix,
because storage is language-neutral at the substrate level). The two
files together define the contract:

- **`operations.json`** — a sequence of real method calls that both
  runtimes execute in order.
- **`expected.json`** — the post-state database dump plus the captured
  return values from any operations tagged with `as`. Both runtimes
  must produce byte-equivalent JSON matching this file after
  normalization.

## Operation script shape

```json
{
  "name": "fixture-name",
  "description": "What this fixture proves.",
  "operations": [
    { "op": "addRepo", "repo": { ... } },
    { "op": "createSnapshot", "as": "snapA", "input": { ... } },
    { "op": "upsertFiles", "files": [ ... ] },
    { "op": "getLatestSnapshot", "as": "latest", "repoUid": "r1" }
  ]
}
```

Each operation is a JSON object with:

- `op` — the method name (required). Uses the camelCase TS port name,
  NOT the Rust snake_case variant. The harness dispatches to the
  corresponding method in each runtime.
- `as` — optional binding name. When present, the operation's return
  value is captured into the bindings map under this name. Subsequent
  operations and the `expected.json.results` section can reference
  captured values.
- Method-specific fields — the arguments to the method, using
  camelCase field names matching the TS DTOs.

## Supported operations

R2-F v1 supports **19 operation types** covering the full public
CRUD surface of the 6 D4-Core entities (`repos`, `snapshots`,
`files`, `file_versions`, `nodes`, `edges`). This matches the
locked R2-E method set one-to-one: every public method that
`StorageConnection` (Rust) and `SqliteStorage` (TS) expose for a
D4-Core entity is a fixture operation here.

The surface includes both writes and reads so that read-return
parity is part of the cross-runtime contract, not only write-
state parity. An earlier design draft carved out a smaller "writes-
plus-setup" subset of 11 operations; the user's R2-F lock override
explicitly expanded the scope back to the full 19 because excluding
the read-return surface would leave `getRepo` dispatch, `getLatestSnapshot`
READY-only filtering, and `queryFileVersionHashes` map semantics
unverified across runtimes — exactly the classes of bug a parity
harness is supposed to catch.

| op | Arguments | Returns | Notes |
|---|---|---|---|
| `addRepo` | `repo: Repo` | void | |
| `getRepo` | `ref: { uid\|name\|rootPath: string }` | `Repo \| null` | Discriminated by which key is present |
| `listRepos` | — | `Repo[]` | |
| `removeRepo` | `repoUid: string` | void | |
| `createSnapshot` | `input: CreateSnapshotInput` | `Snapshot` | Generates `snapshotUid` + `createdAt`. MUST have `as` to enable binding capture |
| `getSnapshot` | `snapshotUid: string` | `Snapshot \| null` | |
| `getLatestSnapshot` | `repoUid: string` | `Snapshot \| null` | Returns latest `ready` snapshot only |
| `updateSnapshotStatus` | `input: UpdateSnapshotStatusInput` | void | `completedAt` MUST be explicit; the auto-generation path is unsupported in v1 |
| `updateSnapshotCounts` | `snapshotUid: string` | void | |
| `upsertFiles` | `files: TrackedFile[]` | void | |
| `upsertFileVersions` | `fileVersions: FileVersion[]` | void | |
| `getFilesByRepo` | `repoUid: string` | `TrackedFile[]` | Excludes `isExcluded=true` rows |
| `getStaleFiles` | `snapshotUid: string` | `TrackedFile[]` | |
| `queryFileVersionHashes` | `snapshotUid: string` | `Map<fileUid, contentHash>` | Represented as JSON object in the dump |
| `insertNodes` | `nodes: GraphNode[]` | void | Preflight stable-key collision check applies |
| `queryAllNodes` | `snapshotUid: string` | `GraphNode[]` | |
| `deleteNodesByFile` | `snapshotUid: string, fileUid: string` | void | Composite delete (edges then nodes) in one transaction |
| `insertEdges` | `edges: GraphEdge[]` | void | |
| `deleteEdgesByUids` | `edgeUids: string[]` | void | |

Every operation in a fixture must be a real public method call. There
is no raw-SQL setup primitive; the fixture corpus proves method-level
parity, not just state-level parity.

## Symbolic bindings (`as` and `@binding.field`)

Operations that generate non-deterministic values (currently only
`createSnapshot`) or return DTOs worth referencing can be tagged with
`as: "<name>"`. Subsequent operations can reference fields of the
captured value using `@<name>.<field>` string syntax:

```json
[
  { "op": "createSnapshot", "as": "snapA", "input": { "repoUid": "r1", "kind": "full" } },
  { "op": "updateSnapshotStatus", "input": {
      "snapshotUid": "@snapA.snapshotUid",
      "status": "ready",
      "completedAt": "2025-01-01T00:00:00.000Z"
  } }
]
```

At execution time, the harness walks the operation's argument JSON
and replaces any `@<binding>.<field>` string with the corresponding
value from the bindings map.

Only single-level field access (`@binding.field`) is supported.
Nested access (`@binding.field.subfield`) is not needed in v1 because
none of the D4-Core DTOs contain generated values inside nested
objects.

## Expected file shape

```json
{
  "results": {
    "binding-name-1": { ... normalized DTO ... },
    "binding-name-2": null
  },
  "schema": {
    "tables": {
      "repos": [
        { "name": "created_at",    "type": "TEXT", "notnull": true,  "dflt_value": null, "pk": 0 },
        { "name": "default_branch","type": "TEXT", "notnull": false, "dflt_value": null, "pk": 0 },
        { "name": "metadata_json", "type": "TEXT", "notnull": false, "dflt_value": null, "pk": 0 },
        { "name": "name",          "type": "TEXT", "notnull": true,  "dflt_value": null, "pk": 0 },
        { "name": "repo_uid",      "type": "TEXT", "notnull": false, "dflt_value": null, "pk": 1 },
        { "name": "root_path",     "type": "TEXT", "notnull": true,  "dflt_value": null, "pk": 0 }
      ],
      "...other tables...": [ ]
    },
    "indexes": [
      "idx_annotations_kind",
      "idx_annotations_target",
      "..."
    ]
  },
  "tables": {
    "repos": [ { ... row 1 ... }, { ... row 2 ... } ],
    "schema_migrations": [
      { "version": 1, "name": "001-initial", "applied_at": "<TIMESTAMP>" },
      ...
    ]
  }
}
```

Three top-level keys:

- **`results`** — map from `as` binding name to the expected captured
  return value, after normalization. Bindings from operations without
  `as` are not captured.

- **`schema`** — canonical **logical** schema representation, NOT a
  raw `sqlite_master.sql` dump. Two sub-keys:
  - `tables`: map from table name to a list of column descriptors.
    Each descriptor has `{name, type, notnull, dflt_value, pk}`
    matching the output of `PRAGMA table_info(<table>)`. Columns
    within each table are sorted by column name (NOT by creation
    order, which differs across runtimes because of the
    outdated-001-plus-migration-002 path).
  - `indexes`: sorted list of index names (e.g., `idx_repos_root_path`).
    Index definitions are NOT dumped; only presence is compared.

  **Why not `sqlite_master.sql` text:** Rust embeds the outdated
  `001-initial.sql` per R2-C D12, so migrations 002/005/010 add
  columns via `ALTER TABLE`. The resulting `sqlite_master.sql` text
  shows columns appended with ALTER-style formatting. TS uses the
  up-to-date `001-initial.ts` constant so the same columns are
  inline in the original `CREATE TABLE` body. Both end states are
  logically identical but the text dump differs. PRAGMA table_info
  gives a canonical column-shape representation independent of
  creation path, and is therefore the stable parity contract.

- **`tables`** — map from table name to a list of row objects. Only
  tables with at least one row are listed. Each table's rows are
  sorted by a stable identity key (see "Canonical ordering" below).
  A fixture that does not modify a particular table omits that table
  from `tables` entirely; the harness's comparison treats an absent
  table key as "expected to be empty".

## Normalization rules

Both halves apply the same normalization pass to their dumps before
comparison. Without normalization, generated values (timestamps, UIDs)
would break byte-equality comparison.

### Blanket column normalization

These columns are always dynamic (generated by the adapter regardless
of fixture input) and are replaced with the sentinel `"<TIMESTAMP>"`
in every row of every table:

| Table | Column |
|---|---|
| `schema_migrations` | `applied_at` |

### Binding-based UID substitution

For every `createSnapshot as X` operation, the harness captures:

- The actual `snapshotUid` returned → mapped to `"<SNAP:X>"`
- The actual `createdAt` returned → mapped to `"<TS:X:createdAt>"`

After execution, the harness scans every string value in the dump
(and in the captured results map) and replaces occurrences of any
captured actual value with its corresponding placeholder. This
handles foreign key references automatically: `file_versions.snapshot_uid`,
`nodes.snapshot_uid`, `edges.snapshot_uid` etc. all store the same
literal string, so a single global substitution maps them all.

If multiple `createSnapshot as X` operations run in the same fixture,
each gets its own binding (`<SNAP:X>`, `<SNAP:Y>`, ...).

### What is NOT normalized

- Literal timestamps and UIDs provided by the fixture itself (e.g.,
  `repos.created_at` from an `addRepo` operation with an explicit
  `createdAt` field, or `updateSnapshotStatus` with an explicit
  `completedAt` field). These are deterministic inputs and comparison
  uses their literal values.

- All other columns, including `repos.created_at`, `files.*`,
  `nodes.*`, `edges.*`, `file_versions.indexed_at`, etc. Fixtures
  MUST provide deterministic values for these in the operation
  inputs.

## Canonical ordering

JSON array order is part of the expected contract. Without canonical
ordering, parity failures become harness noise. Both halves sort
before dumping.

### Schema

The schema dump has deterministic ordering at two levels:

1. **Outer table map**: `schema.tables` is a JSON object. Both
   runtimes iterate `SELECT name FROM sqlite_master WHERE type =
   'table' ORDER BY name` and insert into the map in that order.
   The comparison is structural (`serde_json::Value` equality on
   Rust, `expect.toEqual` on TS), which treats JSON object keys
   as a set — iteration order does not affect parity, but both
   halves still produce the same insertion order for reviewability.

2. **Per-table column list**: columns within each table are sorted
   by column name (ASCII ascending). This is critical because
   Rust-side migration 002 appends `ALTER TABLE` columns at the
   end of the table's schema, while TS-side columns are inline in
   declaration order — different creation-order sequences, same
   logical schema. Sorting by name normalizes out the creation-order
   divergence.

3. **Index list**: `schema.indexes` is a sorted array of index
   names. `sqlite_master` rows of type `index` are filtered
   (excluding SQLite-internal `sqlite_*` indexes), their `name`
   column is extracted, and the resulting list is sorted
   alphabetically.

### Per-table data

Each table's rows are sorted by a stable identity key in the harness:

| Table | Sort key |
|---|---|
| `repos` | `repo_uid` |
| `snapshots` | `snapshot_uid` |
| `files` | `file_uid` |
| `file_versions` | `(snapshot_uid, file_uid)` (composite PK) |
| `nodes` | `node_uid` |
| `edges` | `edge_uid` |
| `schema_migrations` | `version` |

For tables outside the D4-Core set that happen to have rows (shouldn't
happen in R2-F v1 fixtures but the harness handles it defensively), the
sort key is the table's primary key column as reported by
`PRAGMA table_info`.

### Captured results

The `results` top-level object uses insertion order (binding order).
JavaScript and Rust both can produce insertion-order JSON output; the
harness relies on this. The `results` object keys match the `as`
binding names in the order they appear in `operations.json`.

## Constraints summary

- All operations use real public method calls (no raw SQL).
- Operations that return non-deterministic values (currently only
  `createSnapshot`) use `as` to capture return values into a binding.
- Fixtures reference captured values via `@<binding>.<field>` syntax.
- `updateSnapshotStatus` fixtures MUST provide explicit `completedAt`.
- Fixture inputs use camelCase field names matching the TS DTOs.
- Expected.json uses placeholders for dynamic values and literal
  values for deterministic ones.
- Both halves compare structurally against the committed expected.json.
  Neither half regenerates expected.json at test time.

## Curation discipline

Each fixture proves one coherent set of behaviors. Adding a new fixture
requires naming which methods it exercises and why the existing corpus
does not already cover the case. Fixtures are not verbatim extractions
of R2-E unit tests; they are cross-runtime parity checks on the
composed behavior of the methods they call.

R2-F v1 ships with 6 fixtures (see individual directory READMEs or
the `description` field in each `operations.json`):

1. `repos-ref-and-list` — full Repo CRUD surface.
2. `create-snapshot-and-latest-ready` — exercises the binding
   substitution machinery for generated UIDs/timestamps, plus the
   READY-only filter in `getLatestSnapshot`.
3. `files-and-versions` — file and file_version CRUD, including
   `getStaleFiles` and `queryFileVersionHashes` read paths.
4. `nodes-and-edges` — bulk node and edge insert, `queryAllNodes`
   read.
5. `delete-nodes-by-file-cascade` — composite delete behavior.
6. `update-snapshot-counts` — recomputation of counter columns from
   actual graph state.
