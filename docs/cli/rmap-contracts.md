# Rust CLI (`rmap`) Contract

The Rust CLI is the primary binary for agent-facing commands. This document
specifies intentional contract differences from the original TS CLI (`rgr`).

These are **design decisions**, not gaps or debt.

## Invocation Pattern

### REG-1 Commands (daemon-native)

Most query commands now use CWD-based resolution. The daemon resolves the repo
from the current working directory:

```bash
rmap orient                    # resolves repo from cwd
rmap check                     # resolves repo from cwd
rmap explain <target>          # resolves repo from cwd
rmap callers <symbol>          # resolves repo from cwd
rmap callees <symbol>          # resolves repo from cwd
rmap path <from> <to>          # resolves repo from cwd
rmap imports <file>            # resolves repo from cwd
rmap cycles                    # resolves repo from cwd
rmap stats                     # resolves repo from cwd
rmap trust                     # resolves repo from cwd
rmap gate                      # resolves repo from cwd
rmap modules list              # resolves repo from cwd
rmap modules show <module>     # resolves repo from cwd
rmap boundaries list           # resolves repo from cwd
rmap surfaces list             # resolves repo from cwd
rmap contracts list            # resolves repo from cwd
rmap docs list                 # resolves repo from cwd
rmap deps list                 # resolves repo from cwd
rmap inferences list           # resolves repo from cwd
```

Indexing commands:
```bash
rmap index [repo_path]         # daemon allocates db_path, defaults to cwd
rmap refresh                   # resolves repo from cwd
```

### Legacy Commands (positional-args form)

`assess`, `policy`, and `violations` have **migrated to REG-1 cwd-resolution** — each handler
(`commands/assess.rs` `run_assess`, `commands/policy.rs` `run_policy`,
`commands/modules/violations.rs` `run_violations`) calls `execute_repo_request(...)` with no
storage positionals:

```bash
rmap assess [--baseline <snapshot>]            # resolves repo from cwd
rmap policy [--kind <kind>] [--file <path>]    # resolves repo from cwd
rmap violations [--json]                       # resolves repo from cwd
```

The quality commands `churn`, `hotspots`, and `risk` are **also migrated** — each handler
(`commands/quality/{churn,hotspots,risk}.rs`) calls `execute_repo_request(...)` under a
"Migrated from legacy `<db_path> <repo_uid>` contract to REG-1" doc comment. There is **no
`rmap quality` subcommand namespace**: `main.rs` routes `churn` / `hotspots` / `risk` /
`coverage` / `assess` as flat top-level commands.

`declare *` has **NOT** migrated. `declare boundary`, `declare requirement`, and
`declare quality-policy` still require `<db_path> <repo_uid>` positionals at the handler
(`commands/declare/{boundary,requirement,quality_policy}.rs`):

```bash
rmap declare boundary       <db_path> <repo_uid> <module_path> --forbids <target> [--reason <text>]
rmap declare requirement    <db_path> <repo_uid> <req_id> --version <n> --obligation-id <id> ...
rmap declare quality-policy <db_path> <repo_uid> <policy_id> --measurement <kind> ...
```

> **Help-vs-handler mismatch (code-level — flagged, not fixable in a docs slice).** Top-level
> `rmap --help` lists the `declare` subcommands under "Declarations (resolve repo from cwd)" with
> cwd-style signatures (e.g. `rmap declare boundary <module_path> --forbids …`, no
> `<db_path> <repo_uid>`), but the handlers reject that form and require the positional form above
> (confirmed at runtime: `rmap declare boundary` prints `usage: … <db_path> <repo_uid>
> <module_path> …`). The handler is the shipped contract; the `rmap --help` summary lines are
> stale. Repairing the help text is a code change, out of scope for this docs reconcile.

Two more surfaces still require explicit positional paths (verified at the handler):

```bash
rmap enrich <db_path> <repo_uid> [options]                       # commands/enrich.rs
rmap modules boundary <db_path> <repo_uid> <source> [options]    # commands/modules/boundary.rs
```

> **Reconciled (OUTPUT-DOC-TRUTH-AUDIT-1, was PRIORITY-DOCS-RECONCILE-6).** The deeper per-command
> signature blocks later in this file (`policy`, `boundaries`, `contracts`) previously still showed
> pre-REG-1 `<db_path> <repo_uid>` forms — `policy`'s handler was already cwd-migrated, so its block
> was stale doc residue. They have now been corrected to the cwd-resolved (REG-1) shape, each verified
> against the command's handler usage string (`commands/policy.rs`, `commands/boundaries/mod.rs`,
> `commands/contracts.rs`). **`rmap <cmd> --help` is NOT a reliable
> verification surface:** most commands have no `--help` handler — `rmap churn --help` /
> `rmap cycles --help` print usage only as a side effect of the unknown-flag path
> (`error: unknown flag: --help`), and symbol-taking commands like `rmap callers --help` treat
> `--help` as a positional symbol argument and then fail through normal repo/symbol resolution.
> The exact message is **state-dependent**, not a recognition of `--help`: repo resolution runs
> first, so an unindexed cwd yields `error: repo not indexed` (observed), and only an indexed repo
> reaches symbol resolution and a `symbol not found` failure. To verify an argument form, run the
> bare command to trigger its usage error, or read the command's handler.

## Engine Routing — LiveGraph-first defaults (`--engine`)

Six graph commands — `callers`, `callees`, `path`, `imports`, `cycles`, `stats` —
accept an `--engine` selector that chooses the answer backend. This is the user-visible
surface of the SCIP-first / LiveGraph substrate (the daemon holds a current-state
in-memory LiveGraph beside the SQLite snapshot).

| Value | Behavior |
|-------|----------|
| `auto` (default) | Cert-gated **LiveGraph-first**: serve the in-memory LiveGraph answer when a daemon-side **no-loss certificate** is GREEN at the current fingerprint (Exact + Fresh + TypeScript-primary partitions), else a **labelled SQLite fallback**. Human output is byte-identical whichever backend served. |
| `sqlite` | Force the SQLite backend — the proven, byte-identical baseline and the escape hatch. |
| `livegraph` | Force the LiveGraph backend (diagnostic / read-model surface). |
| `compare` | Run both; the SQLite answer stays PRIMARY. `callers`/`callees`/`path`/`cycles`/`stats` write the LiveGraph-vs-SQLite report to a classified `.rgr/livegraph-compare/…` **file sidecar** (its path is returned in the `--json`); `imports` rides the comparison **inline** in the `--json` (no file sidecar). |

Source: `rust/crates/rgr/src/commands/graph.rs` (`extract_engine_flag`, default `"auto"`);
daemon routing in `rust/crates/daemon-runtime/src/dispatch.rs`. Five of these six — `callers`,
`callees`, `path`, `cycles`, `stats` — are the "SQLite-free migrated default paths": on a GREEN cert
the default serves no `nodes`/`edges` SQLite read for the migrated answer. `imports` is the
exception (IMPORTS-UNRESOLVED-REMAINDER-1, D-IUR-FASTPATH): its default answer serves the resolved
rows from the LiveGraph on a GREEN cert but reads SQLite for the rows without a confirmed target on
every route, and a failed read fails the request; `backend_used` names the source of
`imports`/`count` only, and `unresolved_source: "sqlite"` names the source of
`unresolved`/`unresolved_count`.

**`--json` routing metadata.** Under `--engine auto`, the JSON envelope carries
`backend_used` and (on fallback) `fallback_reason`; the human renderer strips them so the
default text is backend-independent. Under `--engine compare` the SQLite answer stays PRIMARY
and the LiveGraph-vs-SQLite report rides alongside it: `callers`/`callees`/`path`/`cycles`/`stats`
add an inline `*_compare` block **and** a `*_compare_sidecar` path (the report is also written to
a `.rgr/livegraph-compare/…` file — e.g. `livegraph_compare` + `livegraph_compare_sidecar`,
`livegraph_stats_compare` + `livegraph_stats_compare_sidecar`); `imports` adds an inline
`comparison` block (per-file) or a readiness aggregate (repo-wide) with **no** file sidecar.

### `cycles` — `--engine` + `--kind`

```
rmap cycles [--engine auto|sqlite|livegraph|compare] [--kind file-import|module-import] [--json]
```

- **Default** (`auto`, no `--kind`): cert-gated LiveGraph-first **module-import** cycles,
  labelled SQLite fallback. The default module-cycle output was **canonicalized**
  (qualified module identities + deterministic ordering) — a ratified, visible
  human-output change versus the older render.
- `--engine sqlite`: forced SQLite module-import cycles (escape hatch).
- `--engine livegraph --kind file-import`: captured resolved-relative **FILE**-import
  cycles (a *different* graph; no SQLite peer, so no fallback — scope is labelled).
- `--engine livegraph --kind module-import`: directory-aggregated module-import cycles
  from the LiveGraph.
- `--engine compare --kind module-import`: SQLite primary + a LiveGraph-vs-SQLite compare
  summary and sidecar.
- Invalid combinations are rejected with an explicit error (e.g. `--kind file-import`
  requires `--engine livegraph`; `--engine compare --kind file-import` is unsupported —
  FILE-import has no SQLite peer graph).

### `imports` — optional `<file>` under LiveGraph / compare

```
rmap imports [<file>] [--engine auto|sqlite|livegraph|compare] [--json]
```

- `auto` (default) / `sqlite`: require **exactly one** `<file>` (single-file import view).
- `livegraph`: `<file>` is **OPTIONAL** — omit it for a repo-wide import view.
- `compare`: with `<file>` → per-file readiness; without `<file>` → a repo-wide readiness
  aggregate.

### `imports <file>` — unresolved rows (IMPORTS-UNRESOLVED-REMAINDER-1)

RG-REQ-006-L12: the single-file `auto` (default) and `sqlite` answers list the file's imports that
never bound to an indexed file (`unresolved_edges` rows of type IMPORTS) beside the resolved rows.
Source: `storage/src/queries.rs` `find_unresolved_file_imports`; the one attach
`daemon-runtime/src/import_partition_view.rs` `attach_unresolved_imports`; the render
`rgr/src/presentation/imports.rs`.

**Engine scope (D-IUR-ENGINE-SCOPE).** With `<file>`, `--engine livegraph` (the in-memory import
read model) and `--engine compare` (the LiveGraph-vs-SQLite readiness view) answer that one file but
are diagnostic views: they carry no `unresolved` key and their human render says
`view limit: this view does not list imports without a confirmed target — rmap imports <file> does`
(directly after the render's first line; `<file>` shell-quoted). The rows without a confirmed target
are listed by the `auto` (default) and `sqlite` answers. The repo-wide livegraph view and the
readiness report print no such line.

**Source (D-IUR-FASTPATH).** `imports`' default answer reads SQLite for the rows without a confirmed
target on every route, including a GREEN-certified LiveGraph serve, and a failed read fails the
request (`InternalError`, naming `find_unresolved_file_imports`) — never an empty array.
`backend_used` names the source of `imports`/`count` only; `unresolved_source: "sqlite"` names the
source of `unresolved`/`unresolved_count`. The human render shows neither routing field.

**JSON keys (additive):**

| Key | Value |
|-----|-------|
| `unresolved` | array of `{target_key, recorded_specifier, decoded_target_path, line, category, classification, basis_code, basis, candidates}` ordered by line then target key; every key present |
| `unresolved[].target_key` | the stored key, verbatim |
| `unresolved[].recorded_specifier` | the extractor's recorded `rawPath` (a C/C++ quoted include `"x"` is recorded `./x`; a TS/JS import as spelled), `null` when none is stored |
| `unresolved[].decoded_target_path` | the `<path>` of a key `<repo_uid>:<path>:FILE` of this repo, else `null` — never a confirmed target |
| `unresolved[].line` | the stored line, `null` when not stored |
| `unresolved[].category` / `classification` / `basis_code` | the stored codes, verbatim |
| `unresolved[].basis` | the candidate-set reason (metadata `basis`), `null` when not recorded |
| `unresolved[].candidates` | `null` (not recorded), an array of paths (every candidate, stored order), or `{"unreadable": "<text>"}` |
| `unresolved_count` | the number of `unresolved` rows |
| `language` | the file's language, `null` when none |
| `listing_coverage` | `{"omitted_forms": [..] | null}` — the import forms the TypeScript/JavaScript extractor reads but gives no IMPORTS edge (`typescript`/`tsx`: four forms; `javascript`/`jsx`: two); `null` for every other language — no list is recorded for it (never `[]`) |
| `unresolved_source` | `"sqlite"` |

**Human wording** (after the resolved rows and the partition lines, no blank line inserted):

```
{K} imports without a confirmed target:            # `1 import without a confirmed target:`
  {target}  line {n}  {category phrase}{reason clause}{candidate clause} · {classification phrase} ({basis phrase})
```

- `{target}`: `recorded_specifier`; else `decoded_target_path` followed by
  `(decoded from the stored key — not a confirmed target)`; else `target_key` verbatim.
  `line ?` when the line is null. The ` · ` is dropped when the first part is empty; a candidate
  clause that would open the first part drops its ` — ` prefix.
- Category phrases (each the predicate at its assignment site, D-AGENT-USEFULNESS-FRAME-1):
  `imports_file_not_found` → none (the header is its predicate); `imports_ambiguous_match` →
  `several indexed files matched`; `imports_ambiguous_suffix` →
  `several indexed .java files end with the specifier's path (or a prefix of it)`;
  `imports_wildcard` → `wildcard import`; any other →
  `{verbatim} (no phrase for this category in this build)`.
- Reason clause (from `basis`, on any category): `ambiguous_basename` →
  ` (same file name in {m} places)`; `ambiguous_suffix` → ` (same path suffix in {m} places)` —
  `{m}` the number of recorded candidate paths, ` (same file name in several places)` /
  ` (same path suffix in several places)` when none are recorded; `ambiguous_workspace_source_entry`
  → ` (several workspace source entries)`; any other →
  ` ({verbatim} (no phrase for this reason in this build))`; absent → ` (reason not recorded)` after
  `several indexed files matched`, nothing elsewhere. On a category without a phrase the reason
  clause prints bare (no parentheses) as the first part.
- Candidate clause (prefix ` — `; always after the two ambiguity categories, otherwise only for a
  path list or an unreadable carrier): `{m} candidates: {p1}, {p2}, …` |
  `candidate paths not recorded with this import` | `candidates unreadable ("{text}")`.
- Classification phrases (a question, never an assertion): `external library?` |
  `this repository?` | `framework boundary?` | `unknown` |
  `{verbatim} (no phrase for this classification in this build)`.
- Basis phrases: `relative_import_target_unresolved` →
  `recorded specifier starts with ., crate::, super:: or self::, or the specifier or target key contains :FILE`;
  `specifier_matches_package_dependency` → `specifier matches a dependency declared in the manifest`;
  `specifier_matches_runtime_module` →
  `specifier or its part before :: is on the runtime-module list of this build`;
  `specifier_matches_project_alias` → `specifier matches a tsconfig paths alias`;
  `rust_crate_internal_module_heuristic` →
  `metadata carries a specifier and no rawPath, and the specifier's first segment before :: starts with a lowercase letter and holds only lowercase letters, digits or underscores`;
  `no_supporting_signal` → `no classifier signal`; any other →
  `{verbatim} (no phrase for this basis in this build)`.
- Unknown codes: the four `{verbatim} (no phrase for this … in this build)` forms above are the one
  case a stored code reaches the human line — a code this build has no phrase for (a newer daemon's
  vocabulary) prints as stored and marked, so the fact is never dropped and never dressed as a known
  one (RG-REQ-002-L11; VISION Honesty Rules). Every known code prints only its reader phrase.
- Coverage line (exactly one, after the block, outside the zero-state): a non-empty
  `omitted_forms` → `listing limit: this listing omits {items} — open the file for those` (items
  joined by `, ` and the last by ` or `); `null` → `listing coverage: not recorded for {lang} — open the file to confirm`
  (`listing coverage: not recorded (language not recorded) — open the file to confirm` when the
  language is not recorded); absent `listing_coverage` →
  `listing coverage: unavailable from this daemon`; any other shape (an empty array included) →
  `listing limit: unreadable`.

**Field states (RG-REQ-002-L04):** `unresolved` absent →
`imports without a confirmed target: unavailable from this daemon — upgrade rmapd`; malformed →
`imports without a confirmed target: unreadable ({why})`; `unresolved_count` missing, not a
non-negative integer, or ≠ rows → `imports without a confirmed target: unreadable (count {c} disagrees with {n} rows)`;
zero rows outside the zero-state → `0 imports without a confirmed target` (a measured zero).

**Zero-state** (no listed row, a stated partition with no inferred remainder, zero rows without a
confirmed target): the count line becomes
`0 imports (language: {lang}; this index lists no imports for this file{coverage})` with `{coverage}`
one of ` — this listing omits {items}`, ` — listing coverage not recorded for {lang}`,
` — listing coverage not recorded (language not recorded)`,
` — listing coverage unavailable from this daemon`, ` — listing coverage unreadable`; no separate
coverage line.

**`explain <file>`:** the Imports evidence carries `unresolved_count` (the same storage read); the
section is emitted when the file has resolved, inferred or unresolved imports. Bullet:
`+{K} imports without a confirmed target — rmap imports {file}`
(`+1 import without a confirmed target — rmap imports {file}`) when K > 0; nothing when K = 0;
`imports without a confirmed target: unavailable from this daemon — rmap imports {file}` when the
field is absent; `imports without a confirmed target: unreadable — rmap imports {file}` when it is
not a non-negative integer. The LiveGraph-served Imports leaf carries the same count.

### `imports <file>` — tsconfig `paths` imports (TS-ALIAS-RESOLUTION-1)

RG-REQ-006-L04 (first clause), as ruled by D-TSA-RECORD-CONFLICT-1 and D-TSA-BOUNDED-SCOPE-1. An
import whose recorded non-relative specifier is matched by the `paths` of the sole inspected
tsconfig project covering its file — exactly one inspected project covers it (the inspected
projects: the nearest `tsconfig.json` and its one-level `references`; a project covers a file by its
`files`, `include` and `exclude`, `allowJs` for a JavaScript file, never under its `outDir`) — and
whose deciding substitution has exactly one indexed hit, is a static import of that file. The
selection is TypeScript's: the exact pattern, else the matching wildcard pattern with the longest
prefix; its substitutions in order, the first with at least one indexed hit deciding. A
substitution's candidates are the substituted path itself when it ends in one of `.ts`, `.tsx`,
`.d.ts`, `.mts`, `.cts`, `.js`, `.jsx`, `.mjs`, `.cjs`, `.json`, and then the relative-import
candidate set; its hits are every candidate that is an indexed file, none ranked over another (so
`./x.js` with both `x.js` and `x.ts` indexed is two hits). A substitution or `baseUrl` that is
absolute, or whose `..` segments leave the repository, gives no candidate. Static asserts the
language's resolution (the compiler's binding), never a runtime proof; the row renders
`static (resolved through tsconfig paths)`; the alias stage runs before the workspace-source stage.
When the deciding substitution has several indexed hits, or two or
more patterns tie for the longest prefix and the substitutions of the tied patterns match at least
one indexed file, the row stays without a confirmed target and records every candidate and which of
the two happened; when nothing matches an indexed file the workspace-source stage runs and otherwise
the row stays as before. A file that zero or several inspected projects cover, or whose one inspected covering
project has no `paths`, takes no `paths` binding and its rows stay as before, as does a file whose
inspected projects include a config with a malformed field that decides membership or target
selection (`extends`, `references`, `compilerOptions`, `paths`, `baseUrl`, `outDir`, `allowJs`,
`files`, `include`, `exclude`; the index prints one warning naming the file and the config).
**Scope limit:** one level of `references` and relative `extends` only — no package `extends` and no
tsconfig elsewhere in the repository is read — so a file outside the inspected projects is not shown
to be compiled by no project, and a project outside them is not consulted. The stored alias signal
(`file_signals.tsconfig_aliases_json`) carries, beside the nearest tsconfig's `entries` (unchanged,
the classifier's input), the sole inspected covering project's mapping
`soleInspectedCoveringProjectMapping: {anchorDir, baseUrl, entries}` with the directory its substitutions
resolve from; the key is absent when the stage does not act for the file.

Source: `repo-index/src/config.rs` `resolve_tsconfig_aliases` (membership and the mapping);
`repo-graph-import-resolver/src/lib.rs` `tsconfig_alias_hits` (the selection);
`indexer/src/resolver.rs` `resolve_tsconfig_paths_import` (the stage); `storage/src/queries.rs`
`RECORDED_BASES` (`tsconfig_paths`) and `AMBIGUOUS_BASES`; `rgr/src/presentation/imports.rs`.

**Resolved row** (`imports[].reason` = `{"basis": "tsconfig_paths", "candidates": [<the row's file>]}`):
`  {file}  depth=1  static (resolved through tsconfig paths)`; a `tsconfig_paths` reason whose
candidates are not exactly the row's own file → `static: reason unreadable ({what})`; a static row
whose stored reason storage could not read (`imports[].reason` = `{"unreadable": "{what}"}`: a
carrier present but malformed or contradicting its edge, whatever basis it names) →
`static: reason unreadable ({what})` with storage's text; a static row with no reason, or with a
readable reason of another basis, prints `static`, as before.

**Rows without a confirmed target** (category `imports_file_not_found`, so the reason clause prints
bare as the first part; these extend the reason and candidate clauses above):

- `ambiguous_tsconfig_paths` → `the tsconfig paths that apply to this file reach {m} indexed files`
  (`{m}` the recorded candidate count; `… reach several indexed files` without a readable list).
- `tied_tsconfig_paths_patterns` →
  `two or more tsconfig paths patterns tie for the longest prefix; together they reach {m} indexed files`
  (`… reach 1 indexed file` when one is recorded; `… together they reach indexed files` without a
  readable list). The phrase states the tie and the files reached, never which pattern TypeScript
  selects (it takes the first declared; the stored `paths` keep no declaration order).
- The candidate clause always follows these two reasons; one recorded path prints
  `1 candidate: {path}`.

Example: `  @/x  line 1  the tsconfig paths that apply to this file reach 2 indexed files — 2 candidates: lib/x.ts, lib/x/index.ts · this repository? (specifier matches a tsconfig paths alias)`.

### `stats` — `--engine`

```
rmap stats [--engine auto|sqlite|livegraph|compare] [--json]
```

Default `auto` is cert-gated LiveGraph-first (served from the LiveGraph module-stats
substrate when the stats no-loss cert is GREEN at the current fingerprint, else a labelled
SQLite fallback) — byte-preserving human output. This is the 6th SQLite-free migrated
default.

### `callers` / `callees` / `path` — `--engine`

```
rmap callers <symbol> [--edge-types <types>] [--include-inferred] [--engine auto|sqlite|livegraph|compare] [--json]
rmap callees <symbol> [--edge-types <types>] [--include-inferred] [--engine auto|sqlite|livegraph|compare] [--json]
rmap path <from> <to> [--engine auto|sqlite|livegraph|compare] [--json]
```

Default `auto` serves the LiveGraph when a per-call no-loss key-set compare holds, else the
labelled SQLite fallback. Human output is byte-identical.

### `callers` / `callees` — certain rows, the inferred and unresolved remainder (PYTHON-RECEIVER-BINDING-1)

A call edge is **certain** only when its `resolution` is `static` or `dynamic`. A call bound by its
method name alone on an untyped receiver (a Python `obj.m()`) is `inferred` — never a certain
caller or callee (RG-REQ-005-L02, RG-REQ-002-L11).

- **Default:** `callers`/`callees` list the certain rows; `count` is their number.
- **`--include-inferred`** (wire param `include_inferred: true`): the inferred rows follow the
  certain ones, each with `resolution: "inferred"`; `count` is every listed row.
- Caller rows are anchored at the **call site** (`line`/`column` of the call), not the caller's
  declaration (RG-REQ-005-L08). `explain`'s Callers section still anchors each caller's
  declaration (its LiveGraph-served rows carry no call-site line) and says so inline:
  `lines above are where each caller is defined — for the call sites run rmap callers <name>`.
- `include_inferred` must be a boolean when present; any other value (a string, a number, `null`)
  is an invalid request — never read as `false`.
- A stored `CALLS` edge whose `resolution` is outside `static | dynamic | inferred` is unreadable.
  Every read that splits the STORED call rows by certainty refuses with an error naming the value
  and the count — never "0 callers" or a complete-looking rate: `callers`, `callees`, `explain`,
  orient's symbol focus and its file/path dead-code reads, `path`, `reliability`, and trust's live
  certain/inferred split. `dead` and `map`'s call pairs refuse only on their live fallback (a
  snapshot without the persisted call-degree / call-pair families); when those families are
  present they are served as persisted, and so is `trust` when its persisted resolved-call count
  and inferred count are present. The persisted counts were computed by the indexer from its
  own resolver output, not from the stored rows, so these reads do not re-check the rows.

Additive fields (each absent when there is nothing to state — an absent field is never a zero;
JSON routing metadata, RG-REQ-002-L08):

| Field | Where | Meaning |
|---|---|---|
| `inferred` | callers, callees | `{count, by_basis}` — the inferred rows (whatever engine served the certain rows). |
| `unresolved_naming` | callers | `{name, count, by_basis, sites}` — unresolved calls whose target's last `.`/`::` segment is the symbol's name. |
| `unresolved_from` | callees | `{count, by_basis, sites}` — unresolved calls made by the symbol. |
| `sites[]` | both unresolved blocks | at most 20 sites ordered by file, line, column, target key: `{file, line, target_key, basis, candidate_reason?, candidate_count?, candidates?}`. `candidate_reason` is the resolver's recorded reason (`ambiguous_name`, `self_call_hierarchy_miss`, `self_call_without_class_context`, `self_call_receiver_unproven`, `self_call_ambiguous_mro`) or `unreadable` for a malformed recorded pool (no candidates then); `candidate_count` is the full pool size; `candidates` the first 50, sorted. |
| `inference_basis` | an inferred row | `receiver_untyped_name_only`, or `unrecorded` when the row's evidence is absent or malformed. |
| `inference_extractor` | an inferred row | the row's writer; for an `unrecorded` row it names the source (`compiler-promotion:0.1.0` = a compiler-resolved call recorded by an older rmap — `rmap repo rebuild <path>` refreshes it). Never rendered on the human surface. |

Unresolved basis codes introduced by this slice (classification `unknown`, reader class
"couldn't attribute"): `self_call_hierarchy_miss` (a `self.`/`cls.` call whose class hierarchy
declares no such method), `self_call_without_class_context` (a `self.`/`cls.` call with no class the
index recorded), `self_call_receiver_unproven` (a class-contained `self.`/`cls.` call whose receiver
is not proven to be the method's first parameter).

Human lines: `N callers found; M inferred (name-only) — investigate with --include-inferred`
(`(name-only)` only when every inferred row is a name-only binding; in `explain`, a remainder
field that is present but unreadable renders `… : unknown — this answer's … field is unreadable`); with the flag `N+M callers
found; M of them inferred (name-only) — marked below`, each inferred row ending ` (inferred:
name-only binding — investigate)`, ` (inferred: compiler-resolved call recorded by an older rmap —
run rmap repo rebuild <path>)` or ` (inferred: basis unknown — investigate)`; then `U unresolved
calls name `<name>` (<reader class>: n, …)` / `U unresolved calls from this symbol (…)`.

### `path` — certain hops; an inferred-only route is stated

`path` walks certain `CALLS`/`IMPORTS` hops only. When no certain route exists within the search
depth and a route exists once inferred edges are admitted, the answer adds
`inferred_edges_on_route: N` (the inferred call/import edges on the shortest such route) and
`search_depth: D` beside `found: false`; the human line reads `no route through certain edges within
depth D; a route exists using N inferred call/import edges — investigate`. A count without its depth
is a decode error. Among equal-depth routes the one with the fewest inferred edges, then the
smallest stable-key sequence, is chosen.

### Rate surfaces — `inferred_calls`

The calls-resolved share counts certain calls only; the inferred calls stay in its universe
(`resolved + inferred_calls + unresolved in-scope-or-unclassified`) and every rate line states them:
`; +N inferred calls not counted as resolved`. JSON: `trust` `summary.inferred_calls` and the
coherent `resolution.value.inferred_calls`, orient `call_coverage.inferred_calls`, explain trust
evidence `inferred_calls`, `reliability` rows `inferred` — each serialized only when positive.

## Coherence Envelope (`orient` / `check` / `explain` / `trust`)

`orient`, `check`, `explain`, and `trust` return a `CoherenceEnvelope<T>` (the
COHERENCE-LAYER-1 contract). The daemon serves LiveGraph-derivable leaves where a
per-signal **no-loss certificate** holds and **labels every leaf's source**, falling back
to SQLite (labelled) otherwise. Human output hides the envelope detail (signals grouped by
severity); `--json` prints the full wrapped envelope.

Source: `rust/crates/repo-graph-coherence/src/lib.rs` (the `CoherenceEnvelope` / `Provenance`
/ `TrustPosture` structs); daemon handlers `build_{orient,check,explain,trust}_envelope` in
`dispatch.rs`.

**Envelope shape (`--json`).** A representative bounded-fallback serialization: one signal
fell back to SQLite for a cycle divergence (so `source` is the union `livegraph` + `sqlite`
with the cert-ladder `fallback_reason` recorded), while the served SQLite answer is itself
`Exact` / `Fresh` for the snapshot. The optional collection fields carry
`#[serde(skip_serializing_if = …)]`, so empty `basis` / `missing_partitions` /
`degradation_reasons` / `contributing_languages` are **omitted entirely** (not emitted as `[]`)
— see the field semantics below. Field order matches struct declaration order:

```json
{
  "value": { "...": "the command payload; signals live under value" },
  "provenance": {
    "source": ["livegraph", "sqlite"],
    "fallback_reason": "LiveGraphCycleDivergence"
  },
  "trust": {
    "class": "Exact",
    "completeness": "Complete"
  },
  "freshness": "Fresh"
}
```

Field semantics:

- `provenance.source` — set-UNION of contributing leaf sources: `livegraph` | `sqlite` |
  `filesystem` | `declaration`.
- `provenance.fallback_reason` — present only when a LiveGraph-first leaf fell back to
  SQLite (the cert ladder; e.g. `LiveGraphStale`, `LiveGraphCycleDivergence`,
  `LiveGraphBoundedServeDeclined`).
- `provenance.basis` / `provenance.missing_partitions` — omitted when empty.
- `trust.class` — `Exact` | `Partial` | `Unavailable` | `Stale`.
- `trust.completeness` — `Complete` | `Degraded` | `Unknown`.
- `trust.degradation_reasons` / `trust.contributing_languages` — omitted when empty.
- `freshness` — `Fresh` | `Stale` | … (the `repo-graph-trust-model` epoch axis).

The root `trust`/`freshness` are the **MEET** (weakest leaf) and `provenance.source` is the
set-UNION — so a bounded-GREEN orient/explain reports `source: ["livegraph","sqlite"]` (the
(b) leaves from the LiveGraph, the retained trust/structural leaves from SQLite), never a
false "all current-state" claim.

**Eager-read posture (honest).** orient SYMBOL-focus + explain SYMBOL-focus resolve their
focus from the LiveGraph and are `nodes`-free on green; the remaining pipelines (orient
REPO/PATH/FILE, explain FILE/PATH, check, trust) keep bounded SQLite reads **by design** (the
retained-forever `unresolved_edges` + structural-identity reads). See
`docs/slices/coherence-leaf-serve-1.md`.

## Command-Specific Contracts

### `index` / `refresh` — Contract Indexing Summary

When contract files (e.g., `.proto`) are present, the indexing output includes
a contract summary line:

```
indexed 14043 files, 257045 nodes, 210503 edges (1019158 unresolved) → snapshot_uid
  contracts: 81 schemas, 4973 elements
```

**Failure reporting:**

If contract files fail to parse:
```
  contracts: 79 schemas, 4500 elements (2 failed)
    FAILED: path/to/bad.proto: parse error at line 5
    FAILED: other.proto: unexpected token
```

If storage fails during contract indexing:
```
  contracts: 81 schemas, 4973 elements (storage error: connection failed)
```

If both parse failures and storage error occur:
```
  contracts: 79 schemas, 4500 elements (2 failed, storage error: connection failed)
    FAILED: path/to/bad.proto: parse error at line 5
    FAILED: other.proto: unexpected token
```

**No contract line** is printed when:
- No contract files exist in the repo
- Contract indexing produced zero schemas

This surfaces `ContractIndexResult` (schemas_indexed, elements_indexed,
parse_failures, storage_error) at the exact point the system knows the truth,
without requiring post-hoc database queries.

### `index` / `refresh` — Generated Code Mapping Summary (CS-2A)

When contract schemas are indexed and Java generated code exists, the indexing
output includes a mapping summary line after the contract summary:

```
indexed 14043 files, 257045 nodes, 210503 edges (1019158 unresolved) → snapshot_uid
  contracts: 81 schemas, 4973 elements
  mappings: 156 persisted (142 high-confidence)
```

**Failure reporting:**

If element query fails (e.g., missing contract_elements table):
```
  mappings: 0 persisted (0 high-confidence) (element query failed)
    element query: no such table: contract_elements
```

If Java symbol query fails:
```
  mappings: 0 persisted (0 high-confidence) (symbol query failed)
    symbol query: query timeout
```

If storage fails during mapping persistence:
```
  mappings: 5 persisted (3 high-confidence) (storage failed)
    storage: disk full
```

If multiple errors occur:
```
  mappings: 0 persisted (0 high-confidence) (symbol query failed, storage failed)
    symbol query: timeout
    storage: connection lost
```

**No mappings line** is printed when:
- No contract schemas were indexed
- No Java symbols exist in the repo
- Zero mappings were produced and no errors occurred

This surfaces `GeneratedCodeMappingResult` (mappings_persisted, high_confidence_count,
element_query_error, symbol_query_error, storage_error) for explicit degradation
reporting rather than silent failure.

### `gate` — Waiver Overlay

PASS obligations are **not waivable**.

- TS (`rgr`): unconditionally marks WAIVED on any waiver match
- Rust (`rmap`): only suppresses non-PASS verdicts

Rationale: allowing PASS-waiver conflated "no evidence" with "suppressed."
A PASS verdict means the obligation is satisfied; there is nothing to waive.
(Rust-25 deliberate correction.)

### `gate` — Quality-Policy Assessment Integration

Gate consumes pre-computed quality-policy assessments and reduces them into
the gate outcome. Quality assessments are reported separately from obligations
because they have different verdict semantics.

**Report shape:**

```json
{
  "obligations": [ /* requirement-based evaluations */ ],
  "quality_assessments": [
    {
      "policy_id": "QP-001",
      "policy_version": 1,
      "policy_kind": "no_new",
      "severity": "fail",
      "assessment_state": "present",
      "computed_verdict": "PASS",
      "is_comparative": true,
      "violations_count": 0
    }
  ],
  "outcome": {
    "outcome": "pass",
    "exit_code": 0,
    "mode": "default",
    "counts": { /* obligation counts */ },
    "quality_counts": {
      "total": 1,
      "pass": 1,
      "fail": 0,
      "advisory_fail": 0,
      "missing": 0,
      "not_comparable": 0,
      "not_applicable": 0
    }
  }
}
```

**Verdict semantics:**

| Assessment State | Verdict | Severity | Exit Code |
|-----------------|---------|----------|-----------|
| Missing | N/A | Any | 2 (incomplete) |
| Present | NOT_COMPARABLE | Any | 2 (incomplete) |
| Present | FAIL | `fail` | 1 (fail) |
| Present | FAIL | `advisory` | 0 (reported only) |
| Present | PASS | Any | 0 (pass) |
| Present | NOT_APPLICABLE | Any | 0 (pass) |

**Key semantics:**

1. **Missing assessment = incomplete.** Active quality-policy without computed
   assessment is treated as missing required evidence.

2. **NOT_COMPARABLE = incomplete.** Comparative policies (`no_new`,
   `no_worsened`) without a baseline snapshot return NOT_COMPARABLE. This
   blocks gate until a baseline is established.

3. **Severity determines blocking.** `severity: fail` assessments with FAIL
   verdict block gate. `severity: advisory` assessments are reported but do
   not affect exit code.

4. **No waiver overlay.** Quality-policy waivers are explicitly deferred.
   Quality assessments do not participate in the waiver system.

**Mode interaction:**

- `default`: Missing/NOT_COMPARABLE = exit 2; FAIL(blocking) = exit 1
- `strict`: Missing/NOT_COMPARABLE/FAIL(blocking) = exit 1
- `advisory`: Missing/NOT_COMPARABLE ignored; FAIL(blocking) = exit 1

### `violations` — Output Shape

`results` is an object with explicit sections:

```json
{
  "results": {
    "declared_boundary_violations": [...],
    "discovered_module_violations": [...]
  },
  "stale_declarations": [...],
  "declared_count": N,
  "discovered_count": N
}
```

TS uses a flat array. The object shape is more explicit for agent consumption.

### `modules list` — Degradation Contract

Envelope includes `rollups_degraded` (boolean) and `warnings` (string array).

When policy parsing fails:
- Exit 0 (not exit 2)
- `rollups_degraded: true`
- Warning message in `warnings[]`
- `violation_count: null` on each module
- Other rollup fields remain populated

Deliberate: orientation surfaces must survive policy corruption.

### `modules show` — Briefing Shape

This is a briefing, not a list. Envelope differs from QueryResult:

```json
{
  "module": { /* identity */ },
  "rollups": { /* counts */ },
  "outbound_dependencies": [ /* weighted */ ],
  "inbound_dependencies": [ /* weighted */ ],
  "violations": [ /* source-side only */ ]
}
```

No `results`/`count` wrapper.

### `modules violations` — Canonical Command

TS CLI does not have an equivalent command (`rgr arch violations` uses a
different selector domain).

Output includes `diagnostics` object reporting derivation counts so callers
can detect degraded graphs where ownership gaps suppress violations.

### `modules deps` — module-scoped Summary (MODULES-DEPS-SUMMARY-SCOPE-1)

RG-REQ-002-L03 (every measured count states its universe). Query time only; no re-index.

- **Under a module filter** (`rmap modules deps <X>`), `diagnostics` counts the imports of the
  requested view whose source is X (`--outbound`), whose target is X (`--inbound`), or either
  (default, all directions) — the same predicate the answer's edge rows are filtered by:
  - `imports_cross_module` — imports whose source and target files are owned by different
    modules: the sum of `results[].import_count`;
  - `imports_intra_module` — imports whose source and target files are both owned by X (the same
    under every direction);
  - `imports_source_unowned` — imports whose source file has no module owner and whose target
    file X owns (0 under `--outbound`);
  - `imports_target_unowned` — imports whose source file X owns and whose target file has no
    module owner (0 under `--inbound`);
  - `imports_total` — the sum of the four.
  The answer carries `diagnostics_scope: "module"`. The human block's header states the universe:
  `Summary (module X, all directions):` / `…, outbound only):` / `…, inbound only):`.
- **Without a module filter**, `diagnostics` and the human `Summary:` block are unchanged (the
  whole repository's counts of the view); the answer carries `diagnostics_scope: "repo"`.
- **`diagnostics_scope`** is `module` or `repo`. The renderer reads it with `module`:
  - absent or `repo` beside a `module` (a daemon older than the key copies the repository's
    counts under a module filter) → `Summary (whole repository, not module X):`;
  - `module` without a `module` key → `Summary (diagnostics scope "module" but the answer names
    no module):`;
  - any other value → `Summary (diagnostics scope "<value>" — not a scope this build reads):`.
- The counts come from the one module-edge derivation `trust` and `modules list` read
  (RG-REQ-004-L01); `modules violations`' `diagnostics` stay repo-wide and carry no scope key.

### `deps list` — Gradle catalog aliases and Java group matching (DEPS-GRADLE-CATALOG-1B)

RG-REQ-006-L13. Index time (stored; `indexer:1.8.0` re-indexes every Gradle store):

- **Alias reference.** A token `libs.<alias>` or `libraries.<alias>` is an alias reference when it
  stands in a configuration argument position of a direct `dependencies` block: outside every
  string literal and comment, at the block's own brace depth (never inside a closure nested in it),
  in the argument list of the statement's leading word — unparenthesised, inside that word's own
  parentheses, or inside `platform(…)` / `enforcedPlatform(…)` there — continuation lines included.
  A token inside any other call (`because(…)`, `files(…)`), in a statement that assigns, or that is
  itself the statement's leading word is not a declaration.
- **Catalog sources of a build** (the build's directory is its settings directory, or the
  script's directory for a single-script build): the `[libraries]` table of
  `<build dir>/gradle/libs.versions.toml` (a string `"group:artifact[:version]"` or a table with
  `module = "group:artifact"`), a source for the whole build; and the `libs` map of every script
  an `apply from:` statement of the build's scripts names, a source for the projects that
  statement applies to. A map entry `alias: "<group>:<artifact>[:…]"` binds to `<group>` when the
  group (the text before the first `:`) is a literal coordinate segment and the artifact is
  non-empty, whatever interpolation the artifact or version carries; an interpolated group is no
  catalog entry. `libraries = libs` makes `libraries.` resolve through `libs` for the projects it
  applies to. A catalog key's `-` and `_` read as `.`.
- **Statement applicability.** A `libraries = libs` or `apply from:` statement applies to a
  project only where its scope reaches it (`ext { }` blocks are transparent): the root script's
  top level or `buildscript { }` → the whole build; directly inside a top-level `allprojects` /
  `subprojects` / `project('<path>')` block → the projects that scope declares for; a non-root
  script's top level → that script's project; anywhere else (a condition, a callback, a task or
  any other closure) → no project.
- **Map grammar.** A `libs` map is read only when every statement of its script that assigns to
  it is a literal map `libs = [ … ]` or `libs += [ … ]` at the script's top level or directly
  inside a top-level `ext { }`, evaluated in statement order (`=` replaces the map, `+=` adds
  entries). Any other assignment to it anywhere in the script (under `if`/`else`, in a loop or any
  other closure, a non-literal right-hand side, `libs.put(…)`, `libs[…] =`, `ext.libs =`) makes the
  whole map unreadable. Every other use of the map name is one of two kinds. A use PROVEN NOT TO
  WRITE the map is ignored: a map-key label `libs:` in another map or call; one entry read — one
  member access `libs.name` or one string-keyed subscript `libs['name']` that ends the expression
  (`implementation libs.zstd`, `v = libs['x']`, `"${libs.zstd}"`), behind any receiver
  (`ext.libs.zstd`, `ext."libs".zstd`); the map rendered as text (`"${libs}"`, `"$libs"`, the sole
  argument of the script's `print` / `println`); and a string literal `"libs"` that does not name
  the map as a property key (`println "libs"`). Any OTHER use — a write, or a use whose effect on
  the map the reader cannot determine — makes the whole map unreadable, as an assignment outside
  the grammar does: a call or closure on the map (`libs.put(…)`, `libs?.put(…)`), a quoted or
  chained access, the map handed to anything other than text rendering (`f(libs)`,
  `def m = libs`), `def libs`, and `"libs"` as a property key (`ext."libs" = …`,
  `ext["libs"] = …`, `ext.set('libs', …)`, `findProperty("libs")`). The note then names the first
  such statement's line and states that it is not a top-level literal assignment; it does not
  state that the statement assigns to the map.
- **Binding.** For each project its block's scope reaches, an alias reference binds to a group
  only when every source that applies to that project was read, every map that applies to it is
  readable, exactly one group answers the accessor (a `libraries.` accessor through a rename that
  applies to that project), and no `apply from:` that is not shown to apply to that project names
  a script that could not be read, an unreadable map, or a map holding that accessor's key. The
  group joins the declared set of the scope the reference's block declares for (own /
  `allprojects` / `subprojects` / `project('<path>')`).
- **Counted, never guessed.** Every other alias reference is counted once on its build's manifest
  record (`unresolved_alias_refs {count, first}`). `first` is the counted site with the smallest
  path, then line, and states the first test that failed for it, in this order:
  - `<path>:<line> <accessor>: catalog <catalog path> could not be read (<error>)` — a source that
    applies to the project did not read: the TOML catalog did not read or parse, or an applied
    script did not read (missing, outside the repository, `not a literal path`, or a `libs` map
    whose brackets do not close);
  - `<path>:<line> <accessor>: alias map in <script>:<line> is not a top-level literal assignment`
    — a map that applies to the project has a statement that uses it, is not an admitted literal
    assignment, and is not proven not to write it (the first by line; see **Map grammar**);
  - `<path>:<line> <accessor>: rename at <path>:<line> not shown to apply to this project` — a
    `libraries.` accessor, the build holds a `libraries = libs` statement and none applies to the
    project;
  - `<path>:<line> <accessor>: map at <path>:<line> not shown to apply to this project` — an
    `apply from:` that does not apply to the project names a script that could not be read, an
    unreadable map, or a map holding the accessor's key;
  - `<path>:<line> <accessor>: ambiguous accessor (<key>, <key> in <catalog path>)` — two catalog
    keys read as that accessor and name different groups; that reference binds to neither;
  - `<path>:<line> <accessor>: no catalog entry` — every source that applies to the project was
    read and none has the accessor.

Query time (`deps list`, Java rows):

- `used` counts observed packages that a declared group matches on a `.`-segment boundary (the
  package equals the group, or starts with the group followed by `.`: `com.github.luben` matches
  `com.github.luben.zstd`, never `com.github.lubenx`).
- `no static import found` counts declared groups that match no observed package.
- `undeclared` counts observed packages that no declared group matches.
- npm, python and cargo rows keep exact matching.
- When an alias reference of the view's builds could not be bound, one line follows the
  undetermined-blocks line: `declared set may be incomplete: N alias reference(s) could not be resolved (<first>) — investigate`.
  JSON: `declared_unresolved_alias_refs` (count) and `declared_unresolved_alias_note` (the line);
  `0` and `""` when none; both keys absent when the provenance is not tracked. The line adds no row
  suffix.

Stated limits: `[bundles]`, `[plugins]`, `{ group, name }` library tables and custom
`versionCatalogs {}` names are not read (a reference to a `{ group, name }` entry counts as
`no catalog entry`). A rename or `apply from:` at a non-root script's top level reaches that
script's project only, so a descendant project's reference that relies on it is counted, never
bound. A Maven group that is not a
Java package prefix (`at.yawk.lz4` for `net.jpountz.lz4`, `com.google.guava` for
`com.google.common`) reads `no static import found`, and the code that imports those packages is
not counted on the row (its references stay unclassified at index time).

### Measurement Commands (`churn`, `hotspots`, `risk`)

Query-time computation, not persistence-first.

- Git is the authoritative history source
- `repo-graph-git` crate wraps git CLI
- TS implementation is reference, not spec
- Explicit anchoring for gate integration is future opt-in, not automatic

### `dead` — DISABLED

**Status: Deliberately disabled as of 2026-04-27.**

The `dead` command is removed from the CLI surface because current signal
quality produces 85-95% false positive rates on real-world codebases.

**Root causes:**
- Missing framework detectors (Spring, React, Axum, FastAPI)
- Missing entrypoint declarations
- No coverage-backed evidence

**Underlying substrate preserved:**
- `storage::find_dead_nodes()` still works
- `trust::assess_dead_confidence()` still works
- Tests pinning current behavior remain

**Reintroduction plan:**

This surface will be split into TWO separate commands:

1. **`rmap orphans`** — Structural graph orphans with no deadness claim.
   Pure graph heuristic. "Not currently referenced in the graph we built."
   Useful for orientation, not deletion decisions.

2. **`rmap dead`** — Coverage-backed + framework-liveness-backed detection.
   Much stronger evidence required. "Unexecuted under measured scenarios
   AND structurally weakly connected."

**Criteria for reintroduction:**
- Framework entrypoint detection mature (Spring, React, Axum, FastAPI), OR
- Coverage import surface operational, OR
- Entrypoint declaration workflow established

See `docs/TECH-DEBT.md` for full rationale

### `policy` — Policy-Facts Discovery

Query extracted policy-facts from C source files. Facts are populated
automatically during `rmap index` / `rmap refresh`.

**Usage:**

```
rmap policy [--kind STATUS_MAPPING|BEHAVIORAL_MARKER|RETURN_FATE] [--file <path>] [--callee <name>] [--fate <kind>] [--json]
```

**Supported kinds:**

- `STATUS_MAPPING` (default): Status/error code translation functions.
  Functions that switch on an input status and return an output status.
  PF-1 scope. See `docs/shipped/policy-facts/pf-1-status-mapping.md`.

- `BEHAVIORAL_MARKER`: Behavioral patterns in control flow.
  - `RETRY_LOOP`: loops with sleep/delay calls (retry with backoff)
  - `RESUME_OFFSET`: curl `CURLOPT_RESUME_FROM*` patterns
  PF-2 scope. See `docs/shipped/policy-facts/pf-2-behavioral-marker.md`.

- `RETURN_FATE`: What happens to function return values at each call site.
  - `IGNORED`: return value discarded
  - `CHECKED`: return value tested in condition
  - `PROPAGATED`: return value returned from caller
  - `TRANSFORMED`: return value passed to another function
  - `STORED`: return value assigned to variable
  PF-3 scope. See `docs/shipped/policy-facts/pf-3-return-fate.md`.

**Filters (RETURN_FATE only):**

- `--callee <name>`: filter by callee function name
- `--fate <kind>`: filter by fate kind (IGNORED, CHECKED, PROPAGATED, TRANSFORMED, STORED)

**Output (STATUS_MAPPING):**

```json
{
  "repo": "swupdate",
  "snapshot": "swupdate/2026-04-29T...",
  "kind": "STATUS_MAPPING",
  "facts": [
    {
      "symbol_key": "swupdate:corelib/server_utils.c#map_channel_retcode:SYMBOL:FUNCTION",
      "function_name": "map_channel_retcode",
      "file_path": "corelib/server_utils.c",
      "line_start": 72,
      "line_end": 98,
      "source_type": "channel_op_res_t",
      "target_type": "server_op_res_t",
      "mappings": [
        { "inputs": ["CHANNEL_ENONET", "CHANNEL_EAGAIN"], "output": "SERVER_EAGAIN" },
        { "inputs": ["CHANNEL_OK"], "output": "SERVER_OK" }
      ],
      "default_output": "SERVER_EERR"
    }
  ],
  "count": 1
}
```

**Output (BEHAVIORAL_MARKER):**

```json
{
  "repo": "swupdate",
  "snapshot": "swupdate/2026-04-29T...",
  "kind": "BEHAVIORAL_MARKER",
  "facts": [
    {
      "symbol_key": "swupdate:corelib/channel_curl.c#channel_get_file:SYMBOL:FUNCTION",
      "function_name": "channel_get_file",
      "file_path": "corelib/channel_curl.c",
      "line_start": 1359,
      "line_end": 1364,
      "kind": "RETRY_LOOP",
      "evidence": {
        "type": "retry_loop",
        "loop_kind": "for",
        "sleep_call": "sleep",
        "delay_ms": 1000,
        "max_attempts": 4,
        "break_condition": "file_handle > 0"
      }
    },
    {
      "symbol_key": "swupdate:corelib/channel_curl.c#channel_get_file:SYMBOL:FUNCTION",
      "function_name": "channel_get_file",
      "file_path": "corelib/channel_curl.c",
      "line_start": 1437,
      "line_end": 1439,
      "kind": "RESUME_OFFSET",
      "evidence": {
        "type": "resume_offset",
        "api_call": "curl_easy_setopt",
        "option_name": "CURLOPT_RESUME_FROM_LARGE",
        "offset_source": "total_bytes_downloaded"
      }
    }
  ],
  "count": 2
}
```

**Output (RETURN_FATE):**

```json
{
  "repo": "swupdate",
  "snapshot": "swupdate/2026-04-29T...",
  "kind": "RETURN_FATE",
  "facts": [
    {
      "callee_key": "swupdate:corelib/channel_curl.c#channel_map_curl_error:SYMBOL:FUNCTION",
      "callee_name": "channel_map_curl_error",
      "caller_key": "swupdate:corelib/channel_curl.c#channel_get_file:SYMBOL:FUNCTION",
      "caller_name": "channel_get_file",
      "file_path": "corelib/channel_curl.c",
      "line": 1456,
      "column": 12,
      "fate": "STORED",
      "evidence": {
        "type": "stored",
        "variable_name": "result",
        "immediately_checked": false
      }
    },
    {
      "callee_key": null,
      "callee_name": "install_update",
      "caller_key": "swupdate:suricatta/suricatta.c#start_suricatta:SYMBOL:FUNCTION",
      "caller_name": "start_suricatta",
      "file_path": "suricatta/suricatta.c",
      "line": 351,
      "column": 4,
      "fate": "IGNORED",
      "evidence": {
        "type": "ignored",
        "explicit_void_cast": false
      }
    }
  ],
  "count": 2,
  "summary": {
    "by_fate": {
      "CHECKED": 69,
      "IGNORED": 126,
      "PROPAGATED": 7,
      "STORED": 79,
      "TRANSFORMED": 15
    }
  }
}
```

Note: `callee_key` is resolved for same-file direct calls. Cross-file and vtable
calls show `null`. The `summary.by_fate` counts are sorted alphabetically
(deterministic output).

**Exit codes:**

- 0: success (facts found)
- 1: no facts found (not an error, just empty)
- 2: runtime error (invalid args, DB error, missing repo/snapshot)

**Scope:**

- C files only (.c, .h)
- C++ is explicitly out of scope for PF-1/PF-2
- Languages other than C deferred

### `boundaries` — Boundary Interaction Discovery

Query extracted boundary interaction surfaces from C source files. Surfaces
are populated automatically during `rmap index` / `rmap refresh`.

**Scope (Slice 1A):** Local IPC mechanisms only. This slice covers:
- Unix domain sockets (AF_UNIX)
- Named pipes / FIFOs (mkfifo)
- Anonymous pipes (pipe, pipe2)
- POSIX shared memory (shm_open, mmap MAP_SHARED)
- POSIX message queues (mq_open)

**Explicit exclusions (Slice 1A):**
- TCP/UDP sockets (Slice 1B — requires scope heuristics)
- Serial/CAN (Slice 2 — inter_device scope)
- MQTT/ZeroMQ/D-Bus (Slice 3 — library wrappers)
- I2C/SPI/USB (Slice 4 — low-level device protocols)

**Commands:**

```
rmap boundaries list [filters...]
rmap boundaries show <surface_uid>
rmap boundaries summary
```

**List filters:**

| Filter | Values | Description |
|--------|--------|-------------|
| `--kind` | unix_socket, named_pipe, anonymous_pipe, shared_memory, message_queue | Channel mechanism type |
| `--scope` | inter_process, inter_device, unknown | Boundary crossing scope |
| `--direction` | provider, consumer, bidirectional | Role in the interaction |
| `--family` | socket, pipe, shared_memory, message_queue | Protocol family |
| `--file` | path | Exact source file match |
| `--file-prefix` | prefix | Source file path prefix |
| `--symbol` | key | Enclosing symbol stable key (exact match) |

**Output (list):**

```json
{
  "command": "boundaries list",
  "repo": "swupdate",
  "snapshot": "swupdate/2026-04-30T...",
  "results": [
    {
      "surfaceUid": "bi:swupdate:core/network_utils.c:58:13:unix_socket:bidirectional",
      "sourceFile": "core/network_utils.c",
      "lineStart": 58,
      "lineEnd": 58,
      "channelKind": "unix_socket",
      "boundaryScope": "inter_process",
      "direction": "bidirectional",
      "protocolFamily": "socket",
      "protocol": "unix",
      "interactionPattern": "stream",
      "symbolStableKey": "swupdate:core/network_utils.c#listener_create:SYMBOL:FUNCTION",
      "confidence": 0.95,
      "basis": "api_call",
      "channelCount": 1
    }
  ],
  "count": 1,
  "stale": false
}
```

**Output (show):**

```json
{
  "surfaceUid": "bi:swupdate:core/network_utils.c:58:13:unix_socket:bidirectional",
  "snapshotUid": "swupdate/2026-04-30T.../a86b5e96",
  "repoUid": "swupdate",
  "boundaryScope": "inter_process",
  "channelKind": "unix_socket",
  "direction": "bidirectional",
  "protocol": "unix",
  "protocolFamily": "socket",
  "interactionPattern": "stream",
  "endpointLocality": "same_host_named",
  "symbolStableKey": "swupdate:core/network_utils.c#listener_create:SYMBOL:FUNCTION",
  "sourceFile": "core/network_utils.c",
  "lineStart": 58,
  "lineEnd": 58,
  "colStart": 13,
  "colEnd": 38,
  "extractor": "c-ipc:0.1.0",
  "basis": "api_call",
  "confidence": 0.95,
  "evidenceJson": "{ ... binding table match evidence ... }",
  "channels": [
    {
      "channelUid": "ch:bi:swupdate:...:6210e6e330d84b58",
      "channelKind": "unix_socket",
      "channelIdentity": "core/network_utils.c:58"
    }
  ]
}
```

**Output (summary):**

```json
{
  "command": "boundaries summary",
  "repo": "swupdate",
  "snapshot": "swupdate/2026-04-30T...",
  "summary": {
    "totalSurfaces": 14,
    "totalChannels": 14,
    "byChannelKind": [
      { "channelKind": "unix_socket", "count": 5 },
      { "channelKind": "anonymous_pipe", "count": 6 }
    ],
    "byBoundaryScope": [
      { "boundaryScope": "inter_process", "count": 14 }
    ],
    "byDirection": [
      { "direction": "bidirectional", "count": 13 },
      { "direction": "provider", "count": 1 }
    ],
    "byProtocolFamily": [
      { "protocolFamily": "socket", "count": 5 },
      { "protocolFamily": "pipe", "count": 7 }
    ],
    "byBasis": [
      { "basis": "api_call", "count": 14 }
    ],
    "filesWithBoundaries": [
      "core/network_utils.c",
      "core/notifier.c",
      "ipc/network_ipc.c"
    ]
  }
}
```

**Exit codes:**

- 0: success (surfaces found)
- 1: no surfaces found (not an error, just empty)
- 2: runtime error (invalid args, DB error, missing repo/snapshot)

**Architecture notes:**

- Two-level model: surfaces (Level 1) + channel details (Level 2)
- Surfaces capture the architectural relationship (what/where)
- Channel details capture mechanism-specific addressing
- `endpointLocality` distinct from `boundaryScope` — locality is what can
  be determined from the code, scope is the architectural classification
- Shared memory surfaces require dual projection (boundary + state) per
  the design doc, but state projection is not yet implemented

**Design doc:** `docs/design/boundary-interaction-ipc-device.md`

### `contracts` — Contract Schema Discovery

Query extracted contract schemas (protobuf files) and their elements. Schemas
are populated automatically during `rmap index` / `rmap refresh`.

**Scope (CS-1):** Protobuf schema extraction only. This slice covers:
- Proto2 and Proto3 syntax parsing
- Package/namespace extraction
- Message, enum, service, method elements
- Nested message/enum handling
- Import statement tracking (resolution deferred)
- Option extraction (java_package, go_package, etc.)
- Line/column source anchoring

**CS-2A scope (Java generated code mapping):**
- Maps checked-in Java generated protobuf artifacts to schema elements
- Top-level elements only (messages, enums, services)
- Explicit confidence tiers with basis tracking
- Java proto and gRPC file detection

**Explicit exclusions (CS-2A):**
- Field/method-level mapping (future)
- Kotlin/Swift/other language mapping (future)
- Build-output inference (checks repo files only)
- Import resolution across files (future slice)
- gRPC-specific detection (GR-1, GR-2, GR-3)
- Other IDL formats (OpenAPI, GraphQL, Thrift — future)

**Commands:**

```
rmap contracts list [--kind protobuf]
rmap contracts show <file_path>
rmap contracts elements [--kind message|enum|service|method|field] [--file <path>]
rmap contracts usages [--element <element_uid>] [--min-confidence <0.0-1.0>]
```

**List filters:**

| Filter | Values | Description |
|--------|--------|-------------|
| `--kind` | protobuf | Contract schema type (only protobuf supported) |

**Elements filters:**

| Filter | Values | Description |
|--------|--------|-------------|
| `--kind` | message, enum, service, method, field | Element type filter |
| `--file` | path | Exact source file match |

**Output (list):**

```json
{
  "command": "contracts list",
  "repo": "myrepo",
  "snapshot": "myrepo/2026-05-01T.../a86b5e96",
  "snapshot_scope": "full",
  "basis_commit": null,
  "results": [
    {
      "schema_uid": "proto-myrepo:api/v1/user.proto:820819b3",
      "file_path": "api/v1/user.proto",
      "schema_kind": "protobuf",
      "package_name": "api.v1",
      "syntax_version": "proto3",
      "parsed_at": "2026-05-01T10:30:00Z"
    }
  ],
  "count": 1,
  "stale": false
}
```

**Output (show):**

```json
{
  "command": "contracts show",
  "repo": "myrepo",
  "snapshot": "myrepo/2026-05-01T.../a86b5e96",
  "snapshot_scope": "full",
  "basis_commit": null,
  "results": {
    "schema_uid": "proto-myrepo:api/v1/user.proto:820819b3",
    "file_path": "api/v1/user.proto",
    "schema_kind": "protobuf",
    "package_name": "api.v1",
    "syntax_version": "proto3",
    "content_hash": "820819b3...",
    "extractor": "proto-parser:0.1.0",
    "parsed_at": "2026-05-01T10:30:00Z",
    "elements": [
      {
        "element_uid": "elem-proto-myrepo:...:User",
        "element_kind": "message",
        "name": "User",
        "full_name": "api.v1.User",
        "parent_element_uid": null,
        "line_start": 10,
        "line_end": 25,
        "metadata": {
          "fields_count": 5
        }
      },
      {
        "element_uid": "elem-proto-myrepo:...:User.id",
        "element_kind": "field",
        "name": "id",
        "full_name": "api.v1.User.id",
        "parent_element_uid": "elem-proto-myrepo:...:User",
        "line_start": 12,
        "line_end": 12,
        "metadata": {
          "number": 1,
          "label": "optional",
          "type_name": "string",
          "type_kind": "scalar"
        }
      },
      {
        "element_uid": "elem-proto-myrepo:...:UserService",
        "element_kind": "service",
        "name": "UserService",
        "full_name": "api.v1.UserService",
        "parent_element_uid": null,
        "line_start": 30,
        "line_end": 45,
        "metadata": {
          "methods_count": 2
        }
      },
      {
        "element_uid": "elem-proto-myrepo:...:GetUser",
        "element_kind": "method",
        "name": "GetUser",
        "full_name": "api.v1.UserService.GetUser",
        "parent_element_uid": "elem-proto-myrepo:...:UserService",
        "line_start": 32,
        "line_end": 35,
        "metadata": {
          "input_type": "api.v1.GetUserRequest",
          "output_type": "api.v1.GetUserResponse",
          "client_streaming": false,
          "server_streaming": false
        }
      }
    ]
  },
  "count": 1,
  "stale": false
}
```

**Output (elements):**

```json
{
  "command": "contracts elements",
  "repo": "myrepo",
  "snapshot": "myrepo/2026-05-01T.../a86b5e96",
  "snapshot_scope": "full",
  "basis_commit": null,
  "results": [
    {
      "element_uid": "elem-proto-myrepo:...:User",
      "schema_uid": "proto-myrepo:api/v1/user.proto:820819b3",
      "file_path": "api/v1/user.proto",
      "element_kind": "message",
      "name": "User",
      "full_name": "api.v1.User",
      "line_start": 10
    }
  ],
  "count": 1,
  "filter_kind": "message",
  "stale": false
}
```

**Usages filters (CS-2A):**

| Filter | Values | Description |
|--------|--------|-------------|
| `--element` | element_uid | Filter by specific schema element |
| `--min-confidence` | 0.0-1.0 | Minimum confidence threshold (default: 0.0) |

**Output (usages):**

```json
{
  "command": "contracts usages",
  "repo": "hadoop",
  "snapshot": "hadoop/2026-05-01T.../a86b5e96",
  "snapshot_scope": "full",
  "basis_commit": null,
  "results": [
    {
      "mapping_uid": "map-1234...",
      "schema_element_uid": "elem-proto-hadoop:...:RequestHeaderProto",
      "element_name": null,
      "element_full_name": null,
      "generated_symbol_key": "hadoop:proto2-generated/.../ProtobufRpcEngineProtos.java:RequestHeaderProto:CLASS",
      "language": "java",
      "generated_file": "proto2-generated/org/apache/hadoop/ipc/protobuf/ProtobufRpcEngineProtos.java",
      "mapping_basis": "exact_option_match",
      "confidence": 0.95,
      "evidence": {
        "proto_package": "hadoop.common",
        "java_package_option": "org.apache.hadoop.ipc.protobuf",
        "java_outer_classname_option": "ProtobufRpcEngineProtos",
        "java_package_actual": "org.apache.hadoop.ipc.protobuf",
        "java_outer_class_actual": "ProtobufRpcEngineProtos",
        "schema_element_name": "RequestHeaderProto",
        "java_class_name": "RequestHeaderProto"
      }
    }
  ],
  "count": 1,
  "filter_element": null,
  "filter_min_confidence": null,
  "stale": false
}
```

**Mapping basis confidence tiers:**

| Basis | Confidence | Description |
|-------|------------|-------------|
| `exact_option_match` | 0.95 | java_package + java_outer_classname match proto options |
| `option_package_match` | 0.90 | java_package matches, classname follows filename convention |
| `filename_convention` | 0.85 | Generated file pattern + symbol name match |
| `symbol_normalized_match` | 0.75 | Symbol name normalizes to schema element |
| `weak_wrapper_match` | 0.50 | Partial match via outer class wrapper |

**Element metadata by kind:**

| Kind | Metadata fields |
|------|-----------------|
| message | `fields_count`, `oneofs`, `reserved_numbers`, `reserved_names` |
| field | `number`, `label`, `type_name`, `type_kind`, `default_value` |
| enum | `values_count` |
| service | `methods_count` |
| method | `input_type`, `output_type`, `client_streaming`, `server_streaming` |

**Exit codes:**

- 0: success (schemas/elements found)
- 1: usage error
- 2: runtime error (DB error, missing repo/snapshot, schema not found)

**Architecture notes:**

- Dual-pipeline architecture: contract files are indexed in parallel with
  source files under the same snapshot lifecycle
- Contract files are tracked in the file catalog (`tracked_files`,
  `file_versions`) alongside source files
- `files_total` includes both source and contract files
- Parse failures are surfaced via `ContractIndexResult.storage_error` and
  reflected in `file_versions.parse_status`

**Design doc:** `docs/slices/cs-1-protobuf-schema.md`

## Output Format

The agent-facing query commands default to **human-readable** plain-text output and emit the
full machine envelope under `--json` (CLI-OUT-1): `orient`, `check`, `explain`, `trust`,
`callers`, `callees`, `path`, `imports`, `cycles`, `stats` (and the other cwd-resolved query
surfaces). Their human renderers strip JSON-only routing metadata (`backend_used`,
`fallback_reason`, compare blocks/sidecars) so the default text is backend-independent (see
Engine Routing above).

Source: the per-command `run_*` functions in `rust/crates/rgr/src/commands/` parse `--json`
and otherwise call `render_human` (e.g. `graph.rs` `run_callers` / `run_cycles` / `run_stats`).
This is **not** universal: the hidden `rmap dev …` diagnostic family (`graph.rs` `run_dev`) has
no `--json` path, and some legacy/governance surfaces documented above predate CLI-OUT-1.

(Historical note: an earlier contract revision stated `rmap` was JSON-only with no `--json`
flag. That predates CLI-OUT-1 and is superseded by the human-default + `--json` model
described here.)
