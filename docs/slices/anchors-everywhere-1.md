# ANCHORS-EVERYWHERE-1 — every symbol citation opens at a line

Status: SPECIFIED (2026-09-04) · Track: v0.16.0 audit queue #4 (human directive
2026-09-04: "include line numbers in other outputs, not only find"; sequenced AFTER
CPP-SPAN-FIDELITY-1 so C++ anchors land on correct spans). CODE slice, presentational +
plumbing. Maturity: MATURE.

## 1. Problem (SURVEYED 2026-09-04 — renderer-by-renderer, DTO-by-DTO)

FIND-EVIDENCE-1 anchored `find`; the rest of the product still cites symbols an agent
must then search for. Survey result (rgr presentation + agent/storage DTOs):

| surface | anchored? | line on the wire? | plumbing |
|---|---|---|---|
| explain target header (symbol) | no | YES (`ExplainIdentityEvidence.line_start`, unread) | none — render |
| explain → Symbols section | no | YES (`ExplainSymbolItem.line_start`, unread) | none — render |
| explain → Callers / Callees | no | no (`ExplainCallerItem{stable_key,name,module}`) | DTO additive + storage read |
| explain → ambiguous candidates | no | no (`FocusCandidate` has file, no line) | DTO additive + storage read |
| orient complexity centers | no | no — the SQL already JOINs `nodes` but never SELECTs the line | storage read |
| boundaries list rows | no | no in DTO; YES in storage (`boundary_interaction_surfaces.line_start`, unselected) | storage read through 4 structs |
| surfaces show evidence | no | no column exists | OUT OF SCOPE (schema) unless `payload_json` already carries it — CHECK, don't build |
| cycles, modules, trust modules, docs, stats, hotspots, churn | n/a | module/file-level entities | none — honest: not everything has a line |

Already anchored and correct: find, inferences, resources, violations-source, map.
No surface renders a computed/guessed line today (honesty rule holds) — keep it so.

## 2. Contract

1. **Tier 0 (render only):** explain's target header and Symbols section render
   `path:line` from the line already on the wire; absence renders no line (never 0/1).
2. **Tier 1 (additive plumbing):** explain Callers/Callees/candidates and orient's
   complexity centers gain `line` via additive DTO fields fed by the storage reads that
   already touch `nodes` (SELECT the column; no new joins). boundaries list: thread
   `line_start` from `boundary_interaction_surfaces` through `HttpSurfaceRow →
   HttpSurfaceInput → UnifiedHttpSurface → BoundaryListEntry` — rendered on individual
   rows / `boundaries show`, NOT on grouped `file × direction (×N)` headlines (a group
   has no single line; never pick one).
3. **Source-of-truth rule (the survey's warning):** a rendered `file:line` pair comes
   from ONE store. explain's LiveGraph rebuild path recomputes `name` from the live IR
   but keeps `module` from SQLite — `line` MUST come from the same source as `file`
   (SQLite/node-store), never a LiveGraph name paired with a SQLite line from a different
   snapshot. Assert this by test.
4. **Tier 2 gate:** surfaces-show evidence: inspect `payload_json` per detector; if a line
   is already carried, render it; if not, STOP — a schema column is a separate decision,
   report the finding and ship Tiers 0–1.
5. Anchor shape is uniform with find (`path:line`); JSON additive; exit codes unchanged;
   economy: anchors add bytes — report before/after per surface; no other output growth.

## 3. Stop conditions

Frozen: storage schema (reads only; the Tier-2 column is DECISION_REQUIRED), exit codes,
ranking/semantics of every surface (this slice adds locations, changes nothing else).
STANDING HONESTY RULES (no invented lines; single-source file+line). Unmet DoD → STOP +
DECISION_REQUIRED. Never touch the operator's real state root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Unit per surface: present line renders; absent line renders nothing; the single-source
  assertion for explain callers on the LiveGraph path; grouped boundaries headline never
  carries a line.
- Live proof (isolated state root, registry sha unchanged): repo-graph `explain` on a
  symbol with callers/callees (anchors on header, symbols, callers, callees); leveldb
  `orient --budget medium` complexity centers anchored (after CPP-SPAN-FIDELITY-1 the
  lines must be TRUE — spot-check three against source); FRAKTAG `boundaries show`;
  before/after bytes per surface.
- Chunked cargo gates; consolidation witness; dogfood-isolated green.

## 5. Definition of done

Every symbol-level citation on explain, orient complexity, and boundaries rows opens at a
true line from a single source of truth; file/module-level surfaces are untouched (and
say nothing false); the Tier-2 finding is reported; gates green.

CORPUS PATHS: repo-graph is THIS repo; leveldb at ../legacy-codebases/leveldb; FRAKTAG at
../FRAKTAG.
