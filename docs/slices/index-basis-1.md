# INDEX-BASIS-1 — say which commit the facts describe, and how far the working tree has moved

Status: SPECIFIED (2026-08-23) · Track: Product-surface honesty (operator direction 2026-08-23:
"index HEAD; the diff is the diff"). CODE slice. Maturity: MATURE (orient/check/explain contracts).

## 1. Problem (measured)

`snapshots.basis_commit` exists in the schema (`storage/src/types.rs:262,290`) and is **NULL on
every real snapshot** (OBSERVED on glamCRM's 2026-08-08 and 2026-08-23 rows): no index records
which commit it was built from. Meanwhile every surface's `Serving:` footer says **"freshness
fresh"** and `check` says **"STALE_FILES: No stale files"** — both are driven by
`get_stale_files` = rows whose *parse status* is stale (`storage/src/crud/files.rs:113`,
`agent/src/check/evaluate.rs:97`), NOT by working-tree drift since the index. An agent that
edited 30 files since the last index reads "fresh" and acts on facts that no longer hold — the
name does not match the semantics (a defect class, not cosmetics). The operator's model for the
quick-changing side is: **repo-graph owns the structure of the last indexed commit; git owns the
delta; the agent orients on facts + `git diff`** — which is only honest if the facts say what
commit they are anchored to and how much has moved.

## 2. Contract

1. **Record the basis at write time.** Every index/refresh (daemon `index`/`refresh` arms →
   `IndexOptions.basis_commit`, already plumbed to the snapshot row) records `basis_commit` =
   `git rev-parse HEAD` of the repo at index start; `None` when the path is not a git repo (and
   the surfaces say "not a git repo", not nothing). No schema change (the column exists).
2. **Render the basis + drift on the three agent surfaces** (orient / check / explain — the
   Serving footer and check's conditions): `index basis: <sha7>` plus, computed at query time
   from git (cheap: `git rev-list --count <basis>..HEAD`, `git diff --name-only <basis>` +
   `git status --porcelain`): "HEAD is N commits ahead of the index; M files changed in the
   working tree (K of them indexed files, in modules X, Y)" with the next action `rmap refresh`.
   When the basis is unknown (pre-slice snapshots), say "index basis unknown (indexed before
   basis tracking) — `rmap refresh` to stamp it". Git errors render as unknown with the reason.
3. **Rename the misleading labels to what they measure**: the parse-status condition becomes
   `UNPARSED_FILES` ("N files could not be parsed") and the footer's `freshness` is split into
   `parse: ok|N unparsed` and `basis: <sha7>, drift: clean|M files` — the word *fresh* is only
   used for drift. Contract-doc + JSON field updates are part of the slice (governance surfaces:
   check's JSON is CI-facing — add the new condition ADDITIVELY and keep `STALE_FILES` emitted
   for one release with a deprecation note, per the versioning model).
4. **`check` gains `INDEX_DRIFT`** (informational → `Incomplete`, never `Fail` by itself): the
   facts may be stale for the M changed files. Exit codes unchanged.
5. Hook: `rmap hook post-commit` (if the hook family has a commit event) or the existing
   `post-edit` marks the drift count; NOT a refresh trigger (out of scope — the operator's model
   is explicit refresh).

## 3. Stop conditions

Frozen: storage schema (no new columns), exit-code semantics, LiveGraph/witness/union/reconciliation,
trust. If `basis_commit` cannot be stamped on the daemon path without touching the coordinator
contract, STOP + DECISION_REQUIRED. If renaming `STALE_FILES` breaks a CI consumer named in
`docs/architecture/gate-contract.txt`, keep the old code emitted alongside and record it. Never
touch the operator's real state root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Unit: basis stamped on index + refresh; None on non-git dir; drift computation (N ahead, M
  changed, K indexed) with a fixture repo; unknown-basis rendering; git-error rendering.
- Live proof (isolated state root): index repo-graph's own tree, touch 3 files + 1 commit →
  `orient`/`check`/`explain` footers show `basis <sha7>`, "1 commit ahead, 3 files changed (3
  indexed, modules …)", `check` shows `INDEX_DRIFT` Incomplete + `UNPARSED_FILES` Pass; after
  `rmap refresh` → clean. Captures in the report.
- Contract docs updated (`docs/architecture/agent-orientation-contract.md`, check's contract);
  JSON additive; chunked cargo gates; consolidation witness 15/15; `./scripts/dogfood-isolated.sh` green.

## 5. Definition of done

Every snapshot knows its commit; every agent surface states the basis and the working-tree drift
with a next action; "fresh/stale" means drift and only drift; parse failures have their own
honest name; gates green.
