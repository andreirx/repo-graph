# IMPORTS-LIVEGRAPH-DEFAULT-READINESS-1: measure whether LiveGraph can replace SQLite `rmap imports`

Slice ID: IMPORTS-LIVEGRAPH-DEFAULT-READINESS-1
Status: **RATIFIED (D1=A file-level; D2 directional no-loss; D3 precondition+sufficiency; D4 xpart/amodx/non-TS;
D5 default frozen; D6=A `imports --engine compare` — 2026-06-07). BUILD IN PROGRESS.** Measurement/readiness
ONLY. Decide whether the DEFAULT `rmap imports <file>` (SQLite today) can safely serve the LiveGraph import view
for the SAME single-file query. NO default flip, NO decommission, NO resolver changes, NO raw deletion. The
deliverable of THIS slice is the readiness DECISION + (if ratified) a per-file COMPARE harness; the default flip
is a SEPARATE future slice gated on the verdict.
Depends: IMPORTS-LIVEGRAPH-CLI-1 (the `--engine livegraph` surface this compares), the module-cycle completeness
certificate. Track: Stage D, QUERY-MIGRATION-1 (the `imports` default migration question).

## Why now (priority path)
```text
The explicit LiveGraph import surface is complete + live-validated (IMPORTS-LIVEGRAPH-CLI-1). SQLite stays the
default. Before any default migration, MEASURE whether the LiveGraph view can replace SQLite for the SAME
single-file `imports <file>` query without losing an import a user sees today -- exactly the per-file
equivalence the deferred default flip needs. This mirrors CYCLES-DEFAULT-MIGRATION-READINESS for the imports
command family.
```

## Grounding (EXECUTED 2026-06-06) — both engines + an empirical per-file compare

### 1. SQLite `rmap imports <file>` semantics
```text
find_imports (storage/queries.rs:1256): ONE hop, `edges WHERE type='IMPORTS'`, source = the file's
`{repo_uid}:{path}:FILE` node, JOIN target node + its file. Per row: { symbol(=target qualified/name),
file(=target f.path), kind, subtype, resolution, evidence(=extractor), depth=1 }. depth HARDCODED 1 (direct
only). resolution column: e.g. "static" (resolved) ; may be "unresolved"/external (subtype EXTERNAL) for
non-FILE targets. Producer evidence rides per row (e.g. `ts-core:0.2.0`, `cpp-core:0.1.0`).
KEY: the homegrown TS extractor (`ts-core`) resolves PLAIN-RELATIVE imports; it does NOT resolve tsconfig
aliases / workspace packages (see the amodx empirical below).
```

### 2. LiveGraph `imports <file> --engine livegraph` semantics
```text
EDGES (facts) = intra-partition AstImport UNION cross-partition overlay (relative / tsconfig-alias / dynamic),
direct only, FILE->FILE, basis + raw_specifier. OBSERVATIONS (evidence) = the classified NON-edge imports
(ExternalNonLocal/AssetNonRelevant benign ; WorkspaceLocalUnedgeable/UnresolvedPackage/AliasUnresolved/
DynamicUnresolved/UnresolvedAfterOverlay blocking). TS-PRIMARY: a non-TS or non-resident partition contributes
NOTHING (the cert's IncompleteUnsupportedLanguage / missing-partition case).
```

### 3. Key-space mapping (EXECUTED on xpart)
```text
SQLite import target `file` == LiveGraph edge `dst_file` -- the SAME repo-relative path. xpart
packages/a/src/main.ts: SQLite import { file: "packages/b/src/foo.ts", resolution: "static", kind: FILE } ;
LiveGraph edge { dst_file: "packages/b/src/foo.ts", basis: AstImportFileInventoryResolved }. -> the compare
matches by (source file, target FILE path). The source key maps `{repo_uid}:path:FILE` (SQLite) <-> repo-
relative path (LiveGraph, via file_key_path). DIRECT, no heuristic.
```

### 4. Compare feasibility — the DECISIVE empirical (EXECUTED 2026-06-06)
```text
xpart packages/a/src/main.ts:  SQLite count=1 (-> packages/b/src/foo.ts, static)  ;  LiveGraph edge_count=1
  (-> packages/b/src/foo.ts). They AGREE on the one resolved import.
amodx admin/src/components/MediaPicker.tsx:  SQLite count=0 (EMPTY)  ;  LiveGraph edge_count=2 (both
  AstImportTsconfigPathResolved: @/components/ui/dialog, @/lib/api) + 3 observations (react/lucide-react
  ExternalNonLocal benign ; @amodx/shared WorkspaceLocalUnedgeable blocking).
=> TWO decisive facts:
1. LiveGraph is a SUPERSET of SQLite for resolved imports: it captures tsconfig-alias / dynamic / cross-
   partition imports the homegrown SQLite extractor MISSES (amodx: 2 vs 0). SQLite never had them.
2. LiveGraph adds EVIDENCE SQLite cannot express (the classified observations).
=> Equivalence is DIRECTIONAL ("no SQLite import LOST"), NOT set-equality (LiveGraph legitimately has MORE).
   And LiveGraph is TS-only -> for non-TS files SQLite is the SOLE source (LiveGraph empty) -> the fallback
   MUST be language/residency-gated. (Open measurement question: is amodx's SQLite emptiness an extractor-style
   limit or an indexing-method artifact? D4 characterizes it; it does NOT change the directional criterion.)
```

## Forced decisions — every cell filled (ratify at sign-off)

### D1 — Compare shape
```text
A. FILE-LEVEL first (one `imports <file>` per engine, diff the two) ; repo-wide compare LATER. [LEAN -- the
   user's brief; the default question IS per-file; small, decisive, matches the `imports` unit.]
B. REPO-WIDE first (every file's imports, both engines). Strictly more coverage but heavy + the default flip is
   still decided per-file. [defer to a follow-up.]
C. BOTH at once. [over-scoped for the first measurement.]
RECOMMENDATION: A. File-level compare first; repo-wide is a follow-up once the per-file criterion is proven.
```

### D2 — Equivalence criteria (the directional rule the grounding forces)
```text
EDGE EQUIVALENCE (per file) = every SQLite RESOLVED-LOCAL import (kind=FILE, resolution=static, target a repo
  FILE) appears in the LiveGraph EDGES (matched by target FILE path). i.e. SQLite_resolved_local SUBSET-OF
  LiveGraph_edges. A LiveGraph EDGE with NO SQLite peer (an alias/dynamic SQLite missed) is an IMPROVEMENT, not
  a failure (LiveGraph is a proven superset). A SQLite resolved-local import MISSING from LiveGraph edges is a
  REGRESSION -> equivalence FAILS for that file.
EXCLUDED from edge equivalence: SQLite external / unresolved rows (subtype EXTERNAL / resolution!=static) and
  LiveGraph benign observations (external/asset) -- they are not local FILE edges. Reported SEPARATELY.
SEPARATE EVIDENCE: the LiveGraph blocking observations (workspace-local / unresolved-after-overlay / alias-
  unresolved / dynamic-unresolved / unresolved-package) are reported as the file's completeness evidence, NOT
  folded into edge equivalence (a blocking observation does not by itself fail equivalence vs SQLite -- SQLite
  lacks those imports too).
RECOMMENDATION: as written. Directional no-loss + benign/external excluded + blocking reported separately.
```

### D3 — Fallback rule (the user's #3, + the grounding-forced LANGUAGE/RESIDENCY gate)
```text
The default MAY serve LiveGraph for a file ONLY IF a PRECONDITION holds AND a SUFFICIENCY holds.
PRECONDITION (language/residency gate -- grounding-forced): the file's partition is RESIDENT + Fresh + TS-
  primary. ELSE LiveGraph contributes nothing -> the default MUST serve SQLite (non-TS C/C++/Rust files; a
  non-resident partition). This is non-negotiable (LiveGraph is TS-only).
SUFFICIENCY (the user's OR): serve LiveGraph IFF
  (a) per-file EDGE EQUIVALENCE passes (D2: no SQLite resolved-local import lost), OR
  (b) the file is CERTIFIED COMPLETE for its scope -- defined as: the file has NO BLOCKING observation (every
      one of its imports is a captured edge or a benign external/asset) AND the repo-level module-cycle
      certificate is not in a missing-partition/unsupported-language state for the file's partition. (The cert
      is MODULE/repo-level; (b) projects it to the file via "no blocking observation for THIS file".)
ELSE -> SQLite fallback. FORBIDDEN: serving LiveGraph when (a) fails AND (b) fails (would risk losing a SQLite
  import OR hiding a known-incomplete file).
RECOMMENDATION: as written. The language/residency PRECONDITION is mandatory; (a) is the SQLite-relative gate,
  (b) is the absolute gate. Per-file completeness = "no blocking observation for this file" (the projection of
  the repo cert; there is no per-file certificate object today -- defining one is out of scope).
```

### D4 — Validation repo/file set
```text
xpart: packages/a/src/main.ts + packages/b/src/foo.ts (the mutual relative cross-partition imports -- SQLite +
  LiveGraph both populated; the EQUIVALENCE-PASSES baseline).
amodx (the rich cases, all EXECUTED-reachable): a tsconfig-ALIAS file (MediaPicker.tsx -> @/...), a file with a
  WORKSPACE-LOCAL import (@amodx/*), an EXTERNAL-only file (react/lucide-react), an ASSET importer (the 2 CSS),
  a DYNAMIC importer (literal relative). Characterize per file: SQLite count + targets ; LiveGraph edges +
  observations ; edge-equivalence verdict ; the language/residency precondition.
ALSO (the asymmetry control): a NON-TS file (e.g. an OpenXcom .cpp) -> SQLite has imports, LiveGraph empty ->
  the precondition correctly forces SQLite. Confirms the language gate.
RECOMMENDATION: as written. Each amodx case maps to one D2/D3 branch; the non-TS control proves the precondition.
```

### D5 — Output compatibility
```text
The HUMAN default (`imports <file>`, no flags) MUST stay byte-compatible with today IF the default still serves
  SQLite (it does in this slice -- no flip). When a future flip serves LiveGraph for a file, the human default
  must render a SQLite-COMPATIBLE listing (the resolved edges as the import list) -- the read-model already has
  the edges; a compatibility renderer is a FUTURE-slice concern, NOT built here.
JSON: new readiness/compare metadata appears ONLY in the EXPLICIT compare/readiness mode (D6), NEVER on the
  default `imports <file>` response (which stays {file, imports, count}). The default contract is frozen here.
RECOMMENDATION: as written. No change to the default response shape in this slice.
```

### D6 — The readiness HARNESS mechanism (surfaced; determines the build)
```text
A. `imports --engine compare <file>` daemon route (mirror `cycles --engine compare`): runs BOTH engines for the
   file, returns { sqlite_resolved_local, livegraph_edges, matched, missing_in_livegraph (REGRESSIONS),
   extra_in_livegraph (improvements), excluded (external/unresolved/observations), precondition,
   per_file_verdict }. Explicit-only; the default untouched. [LEAN -- the cycles precedent; reusable; the
   readiness measurement consumes it; it is the per-file gate the future flip will call.]
B. A measurement-only SCRIPT (CLI sqlite + CLI livegraph side-by-side, diffed off-process). No daemon code; but
   throwaway + not reusable by the future flip.
C. Manual side-by-side (this grounding already did 2 files). Not a harness; insufficient for the verdict.
RECOMMENDATION: A. The compare route is the reusable per-file gate (D3) the future default flip calls; build it
   read-only (NO default flip, NO resolver change). The READINESS verdict then measures across D4 with it.
```

## Measurement protocol (PROPOSED — gated on ratification)
```text
For each D4 file: run the D6-A compare. Capture: SQLite resolved-local count + targets ; LiveGraph edge count +
targets ; matched ; missing_in_livegraph (the REGRESSION set -- MUST be empty for equivalence) ; extra (the
improvement set) ; the language/residency precondition ; the per-file verdict (SERVE-LIVEGRAPH / FALLBACK).
Aggregate: how many D4 files pass equivalence (a) ; pass completeness (b) ; require fallback (precondition or
neither). Verdict: GREEN (per-file gate proven safe -> the future flip is buildable) / YELLOW (safe but low
coverage) / RED (a regression -- a SQLite import LiveGraph loses).
```

## Out of scope (hard guardrails)
```text
NO default flip (this slice MEASURES + decides) ; NO decommission ; NO SQLite deletion ; NO resolver changes ;
NO repo-wide compare (D1 defers it) ; NO per-file certificate OBJECT (D3-b uses the projection) ; NO change to
the default `imports <file>` response shape (D5). The default flip is the SEPARATE un-deferred slice.
```

## Build contract (PROPOSED — gated on D1–D6 ratification)
```text
1. daemon: an `imports --engine compare` route (D6-A) -- read-only; reuses find_imports (sqlite) +
   live_import_view (livegraph) + the D2 directional diff + the D3 precondition/verdict. PURE diff at the
   boundary; no default touched.
2. CLI: `imports <file> --engine compare [--json]` -- explicit; prints the per-file compare + verdict.
3. MEASURE across D4 ; record the verdict (GREEN/YELLOW/RED) + the per-file table.
4. live + gate + completion doc.
Stop if: a SQLite resolved-local import is MISSING from LiveGraph for any D4 file (a real regression) -> RED,
surface before any flip. Stop if the language/residency precondition is ambiguous for a file (present the case).
```

## References
- `rust/crates/storage/src/queries.rs:1256` (`find_imports` — the SQLite per-file import path)
- `rust/crates/repo-graph-livegraph/src/lib.rs` (`live_import_view` — the LiveGraph per-file read-model)
- `rust/crates/daemon-runtime/src/livegraph_feed.rs` (`module_cycle_compare_response` — the `--engine compare` precedent to mirror)
- `docs/slices/imports-livegraph-cli-1.md` (the explicit surface this measures)
- `docs/slices/cycles-default-migration-readiness-2.md` (the sibling cycles readiness pattern)
