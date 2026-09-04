# CURSOR-ROUNDTRIP-1 — a cursor rmap prints is a cursor every command accepts

Status: SPECIFIED (2026-09-04) · Track: v0.16.0 regression pair found by the self-model
experiment (docs/audits/2026-09-04-self-model.md). CODE slice, SMALL. Maturity: MATURE.

## 1. Problem (VERIFIED live on the v0.16.0 isolated index, 2026-09-04)

1. `find` prints short cursors (`rust/…/coordinator.rs#RepoCoordinator:SYMBOL:CLASS`,
   FIND-EVIDENCE-1's cursor diet) that `explain` accepts via the syntax-gated alias — but
   `callers`/`callees` reject the identical string: `InvalidRequest: symbol not found`.
   An agent following find's own output into the next natural command hits a dead end.
2. That not-found fallback then renders seed candidates through a pre-SEED-CHUNK-1 DTO
   shape and prints `(malformed candidate: missing file/stable_key/score/model_id/source)`
   ×N — literal garbage in user output, on a path SEED-CHUNK-1's tests never covered.

## 2. Contract

1. The short-cursor alias (`dispatch_explain_alias::reattach_repo_uid_prefix`, syntax-
   gated, storage-free) applies to EVERY symbol-cursor-taking command: `explain`,
   `callers`, `callees`, `path` endpoints, and any other handler that resolves a
   `stable_key` argument — one resolution site, not per-handler copies; the nodes-free
   green path preserved and asserted as in FIND-EVIDENCE-1.
2. The not-found fallback's semantic candidates render through the CURRENT seed DTO
   (SEED-CHUNK-1's `RankedCandidate`: path:line, qualified name, score, model, is_test
   partition) — the same renderer `find` uses, not a second copy. A candidate that
   genuinely fails validation renders ONE honest line ("N candidates unreadable: <reason>"),
   never per-row malformed placeholders.
3. `find`'s printed cursor is shell-quoted for humans; the JSON output carries the raw
   cursor (additive field if absent) so agents never have to strip quotes.
4. JSON additive; exit codes unchanged.

## 3. Stop conditions

Frozen: cursor grammar, seed ranking, storage. STANDING HONESTY RULES. Unmet DoD → STOP +
DECISION_REQUIRED. Never touch the operator's real state root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Reproducing tests FIRST: short cursor → callers/callees resolve; not-found fallback
  renders current-DTO candidates; malformed → single honest line.
- Live proof (isolated state root, registry sha unchanged): `find RepoCoordinator` →
  copy its printed cursor into `callers` and `callees` → results; a nonexistent symbol →
  fallback with real candidates, zero "malformed" strings.
- Chunked cargo gates; consolidation witness; dogfood-isolated green.

## 5. Definition of done

Every cursor rmap prints round-trips into every cursor-taking command; the fallback
renders current seeds honestly; gates green.

CORPUS PATHS: repo-graph is THIS repo.
