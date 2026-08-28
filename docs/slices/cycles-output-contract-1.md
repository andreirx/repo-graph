# CYCLES-OUTPUT-CONTRACT-1: the default `rmap cycles` identity + ordering contract

Slice ID: CYCLES-OUTPUT-CONTRACT-1
Status: **IMPLEMENTED + LIVE-VALIDATED 2026-06-07 (D1=B canonical qualified human identity + deterministic
order; D2=B additive `qualified_name` JSON; D3=A full fastpath unblocks AFTER byte-identity is proven).
Output-contract migration ONLY; NO fastpath this slice — FASTPATH-1 is now UNBLOCKED (byte-identity proven on
xpart + amodx) and may resume as a separate slice.** Gate green (build/fmt/clippy/`test --workspace` 0 failures);
byte-identity proven LIVE (default == `--engine livegraph --kind module-import`). Decided (DONE) the DEFAULT
`rmap cycles` output contract — node IDENTITY (short vs
qualified) and cycle/ring ORDERING (Tarjan-discovery vs canonical-deterministic) — for the HUMAN text and,
SEPARATELY, for the JSON. This is the explicit blocker surfaced by CYCLES-LIVEGRAPH-DEFAULT-FASTPATH-1 (ratified
C: hold): a LiveGraph cycles fastpath CANNOT be byte-transparent under today's contract, so the contract must be
decided BEFORE any cycles default migration. Weigh HUMAN compatibility vs AGENT correctness. NO raw decommission,
NO resolver change, NO module-identity change to the GRAPH (this is an OUTPUT/presentation contract only).
Depends: CYCLES-LIVEGRAPH-DEFAULT-FASTPATH-1 (BLOCKED — the discovery), MODULE-CYCLES-CLI-1 (the qualified
identity already exists on `--engine compare/livegraph`). Gates: any future cycles default fastpath/migration.
Track: Stage D, QUERY-MIGRATION-1.

## Why now (priority path)
```text
FASTPATH-1 proved the cycles default CANNOT migrate to LiveGraph without changing human-visible identities/order
(short `src`+Tarjan -> qualified `packages/a/src`+derivation). That is an OUTPUT MIGRATION, not a fastpath. The
blocker is explicit and SMALL: a single contract decision (identity + order, human vs JSON) unblocks (or
permanently closes) the cycles default decommission. Deciding it FIRST is cheaper than a stats build and is the
direct precondition for ANY cycles default work. The substrate mission is "DETERMINISTIC code-intelligence" —
Tarjan discovery order is an implementation artifact, which is itself an argument the contract is worth pinning.
```

## Grounding (EXECUTED 2026-06-07 — from the FASTPATH-1 build halt)
```text
HUMAN default render (rgr cycles.rs:60-100): reads ONLY cycle.nodes.len() and node.name; ignores extra JSON
  fields (cycles.rs:44-56, no deny_unknown_fields). So the SHAPE is permissive; the TEXT is driven by name+order.
DEFAULT SQLite identity (queries.rs find_cycles:1024-1054 + docstring 1059-1063): CycleNode.name = the bare
  `name` column = the SHORT module name ("src"), node_id = the unique node_uid ("repo:packages/a/src:MODULE").
  Ordering: Tarjan SCC discovery order, sorted by cycle_id; ring starts at Tarjan member order.
QUALIFIED identity (queries.rs module_qualified_names:1064-1077): COALESCE(qualified_name, name) = "packages/a/
  src" — exists TODAY, used by `--engine compare`. The short "src" COLLIDES across packages (the docstring's
  stated reason qualification exists).
LIVEGRAPH identity (livegraph_feed.rs module_import_cycles_json:1052-1071): node_id = name = the qualified
  dirname member ("packages/a/src"); file = null. This is WHY amodx matched EXACT against sqlite_qualified.
  Ordering: LiveGraph derivation order (NOT Tarjan).
=> For the SAME cert-proven cycle SET: human NAMES differ (short vs qualified) DEFINITELY; ORDER differs
   POSSIBLY (Tarjan vs derivation, not reproducible without re-running SQLite's SCC). JSON node_id ALSO differs
   (uid vs member-path). Agents today CAN disambiguate via the SQLite node_id (unique), but it is NOT stable
   across backends.
```

## Forced decisions — every cell filled (ratify at sign-off)

### D1 — DEFAULT HUMAN contract: identity + ordering (the human-compatibility axis)
```text
A. STATUS QUO — short names + Tarjan/cycle_id order. [max human compat]
   + Zero churn: no human-visible change; existing snapshots/muscle-memory intact.
   - Short names COLLIDE ("src -> ... -> src" is ambiguous across packages) — the human output is already
     lossy/confusing for multi-package repos.
   - PERMANENTLY blocks a transparent cycles default fastpath (LiveGraph cannot reproduce short+Tarjan). The
     cycles default NEVER decommissions off SQLite via this path.
B. CANONICAL — qualified names + deterministic canonical order, applied to BOTH backends (the SQLite default is
   ALSO canonicalized: qualified `name`, cycles sorted by their sorted-member tuple, each ring rotated to start
   at its lexicographically-min member). [enables fastpath]
   + Both backends emit IDENTICAL human bytes for the same set -> a LiveGraph fastpath becomes truly transparent
     -> FASTPATH-1 resumes. Disambiguates the "src" collision. Aligns with the "DETERMINISTIC substrate" mission
     (Tarjan order is an artifact).
   - ONE-TIME human-visible change to `rmap cycles` (short->qualified, order changes). Any human snapshot/test/
     agent-prompt keyed on the old text churns once. `--engine sqlite` does NOT preserve it (it is canonicalized
     too); a frozen-legacy escape hatch would be a separate explicit flag if needed.
C. ORDER-ONLY — canonical order, KEEP short names.
   + Order reproducible across backends.
   - Names STILL diverge (short vs qualified) -> fastpath STILL blocked unless LiveGraph members are basename-
     mapped to short (couples to an unverified ingest "name==basename" assumption; reintroduces the collision).
     Half-measure that does not actually unblock cleanly.
RECOMMENDATION: GENUINE FORK — surface, do not self-decide.
  Lean B IF the cycles default decommission is a goal AND a one-time human-output change is acceptable: it is the
  ONLY option that unblocks a transparent fastpath, it fixes the real "src" collision, and determinism is the
  stated mission. Choose A IF human-output stability is a HARD constraint: then accept the cycles default stays
  SQLite permanently (pursue decommission breadth elsewhere — stats/coherence). C is dominated (does not unblock).
```

### D2 — DEFAULT JSON contract: identity (the agent-correctness axis — decided SEPARATELY from D1)
```text
A. MATCH HUMAN — JSON `name` follows D1 (short if D1=A, qualified if D1=B); node_id stays the backend's native id.
   + Simplest; one contract.
   - If D1=A, agents still get the ambiguous short `name` in JSON (node_id disambiguates but is backend-specific).
B. ALWAYS-QUALIFIED ADDITIVE — regardless of D1, each JSON cycle node gains an explicit `qualified_name` field
   (both backends populate it: SQLite via module_qualified_names, LiveGraph via the member path); `name`/node_id
   unchanged. [agent correctness without forcing the human change]
   + Agents key on a stable, unambiguous, human-meaningful identity NOW, even if D1=A keeps human short. Purely
     additive — no breakage. Decouples agent correctness from the human-compat decision (the user's explicit ask).
   - JSON node_id still differs across backends (uid vs member-path); a JSON-byte-identical fastpath needs C.
C. CANONICAL CROSS-BACKEND ID — restructure the JSON node identity so SQLite and LiveGraph emit BYTE-IDENTICAL
   JSON for the same cycle (node_id := the canonical qualified module path on both; canonical order per D1=B).
   + Enables a JSON-byte-transparent fastpath even if the human path is treated separately.
   - Largest change; changes the existing JSON node_id contract for SQLite consumers.
RECOMMENDATION: B (additive `qualified_name`) as the floor — it delivers agent correctness with zero breakage and
  is independent of D1. Add C ONLY if a JSON-only fastpath (human stays SQLite, JSON served from LiveGraph) is
  the chosen unblock path (see D3). Avoid A-with-D1=A (leaves agents on the ambiguous short name).
```

### D3 — The unblock consequence (sequencing — derived from D1/D2, ratify explicitly)
```text
A. FULL FASTPATH (requires D1=B): both backends emit identical human+JSON bytes -> FASTPATH-1 resumes UNCHANGED
   as a transparent cert-gated fastpath; the cycles default decommissions off SQLite on GREEN. Highest leverage.
B. JSON-ONLY FASTPATH (requires D1=A + D2=C): human default stays SQLite (byte-exact), JSON default served from
   LiveGraph on GREEN. Asymmetric; human path still reads nodes/edges every call; partial decommission.
C. NO FASTPATH (D1=A + D2=A/B): the cycles default stays SQLite-served permanently; decommission pursued via
   OTHER defaults (stats/coherence). FASTPATH-1 is CLOSED (not merely deferred). Accept and move on.
RECOMMENDATION: follows D1/D2. If D1=B -> A (clean, full). If human stability is hard (D1=A) -> C is honest
  (B's asymmetry buys little: the human default still eager-reads SQLite, so nodes/edges stay load-bearing for
  cycles regardless). State the chosen consequence so FASTPATH-1's status is updated correctly (resume vs close).
```

## Build plan (RATIFIED D1=B / D2=B / D3=A — implementation, no fastpath)
```text
DESIGN INVARIANT (byte-identity by construction): the compare matches cycles as canonical SETS -- each cycle
sorted+deduped via BTreeSet<String> (module_cycle_compare.rs:19-30), cycles ordered as BTreeSet<Vec<String>>.
Both are PLAIN lexicographic String order. The output canonicalization uses the SAME lexicographic order over
the QUALIFIED name, so: cert GREEN (sets match) => SQLite-default canonical render == LiveGraph canonical render,
inherently (both String::cmp). An SCC is a SET (scc.rs:21 "members = stack pop order" is an artifact, NOT a ring)
-> "canonical ring rotation" reduces to "sort+dedup the member set". A unit test ALSO asserts equivalence to the
compare's canonical_set (defense in depth).

NEW MODULE (decided+recorded; internal file in daemon-runtime, no new crate/dep edge, SRP = the canonical
module-cycle OUTPUT contract): `rust/crates/daemon-runtime/src/cycle_output.rs`
  - CanonModuleCycleNode { node_id, name, qualified_name }.
  - canonical_module_cycles_json(cycles: &[Vec<CanonModuleCycleNode>]) -> Vec<Value>:
      per cycle: sort+DEDUP nodes by qualified_name (mirror canonical_set; on a qualified-name collision keep the
      lexicographically-smallest node_id -- deterministic) ; sort cycles by their Vec<qualified_name> ;
      emit {cycle_id:"cycle-{i+1}", length:nodes.len(), nodes:[{node_id,name,qualified_name,file:null}]}.
  - sqlite_module_cycles_json(cycles: &[CycleResult], qualified: &HashMap<uid,qual>) -> adapter: node.name (short)
      + qualified_name = qualified.get(node_id).unwrap_or(name) -> canonical_module_cycles_json.
  - module_basename(path) helper for the LiveGraph short `name`.

WIRING (additive; storage find_cycles / CycleResult / CycleNode UNCHANGED -- the contract is a PRESENTATION
concern, applied at the daemon output layer, NOT in the low-level SCC query):
  1. dispatch.rs handle_cycles SQLite default: find_cycles + module_qualified_names -> sqlite_module_cycles_json.
  2. livegraph_feed.rs module_import_cycles_json: build CanonModuleCycleNode from members (node_id=member,
     qualified_name=member, name=basename(member)) -> canonical_module_cycles_json (so --engine livegraph
     --kind module-import is canonical + carries qualified_name too).
  3. rgr presentation/cycles.rs: CycleNode gains `qualified_name: Option<String>` (additive, serde default) ;
     the renderer displays qualified_name.unwrap_or(name) (file-level / absent -> name, unchanged).

JSON (D2=B additive, backward-compatible): cycle_id, length, node_id, name, file ALL preserved ; + qualified_name.
  node_id stays backend-NATIVE (SQLite uid / LiveGraph member) -- the human + qualified_name are the cross-backend
  stable identity; a fully cross-backend-identical JSON node_id is a RESIDUAL deferred to FASTPATH-1 (its proof
  target per the user's validation is the HUMAN output). RECORDED.

TESTS: cycle_output (ordering, dedup, additive fields, basename, equivalence-to-canonical_set) ; an adapter-
  parity test (SQLite-adapter vs LiveGraph-adapter produce IDENTICAL json for the same member sets) ; rgr
  renderer (qualified_name shown ; fallback to name) ; update existing cycles.rs tests (add qualified_name).
LIVE: xpart fixture default-human == `--engine livegraph --kind module-import` human ; amodx (GREEN) the same ;
  `--engine sqlite` now canonical too (no legacy-short promise). Gate (test/clippy/fmt) then completion doc.

ASSUMPTIONS / DIVERGENCES (recorded): (a) module_qualified_names = COALESCE(qualified_name,name); a NULL
  qualified column falls back to short -> would mismatch LiveGraph, BUT such a repo is NOT compare-GREEN so the
  future fastpath would not fire; this slice renders whatever the compare basis is (consistent). (b) LiveGraph
  short `name` = basename(member); if a repo's stored MODULE `name` is NOT the dir basename, the JSON `name`
  cosmetic differs (human uses qualified_name, unaffected). (c) `length`/size = UNIQUE-qualified member count
  after dedup (honest aggregated size).
```

## What this slice does NOT decide / out of scope (hard guardrails)
```text
NO code, NO renderer edit, NO JSON field added, NO canonicalization implemented THIS slice — DECISION ONLY. The
implementation (canonical ordering helper, qualified-name plumbing, the FASTPATH-1 resume/close) is a SEPARATE
ratified slice AFTER this. NO change to the GRAPH's module identity/resolver (presentation/output contract only).
NO change to `--engine compare` (already qualified). NO raw decommission, NO table deletion. The `file`-kind
cycles output is untouched. If D1=B, the implementation slice MUST land the canonical order on BOTH backends
BEFORE any fastpath (so the SQLite default and the LiveGraph fastpath are proven byte-identical first).
```

## Completion (EXECUTED 2026-06-07)
```text
IMPLEMENTED (additive; storage find_cycles / CycleResult / CycleNode UNCHANGED):
  - NEW daemon-runtime/src/cycle_output.rs: CanonModuleCycleNode + canonical_module_cycles_json (sort+dedup by
    qualified_name; cycles sorted by qualified-name vector; emit {cycle_id,length,nodes:[{node_id,name,
    qualified_name,file}]}) + sqlite_module_cycles_json + livegraph_module_cycles_json + module_basename.
  - dispatch.rs handle_cycles SQLite default -> find_cycles + module_qualified_names -> sqlite_module_cycles_json.
  - livegraph_feed.rs module_import_cycles_json -> livegraph_module_cycles_json (same canonical builder).
  - rgr presentation/cycles.rs: CycleNode.qualified_name (additive, serde default); render prefers it.

GATE (EXECUTED): cargo build (daemon-runtime+rgr) clean ; cargo fmt --all --check clean ; cargo clippy
  --workspace --all-targets -D warnings PASS ; cargo test --workspace = 0 failures across 220 test-result lines.
  cycle_output 7/7 (ordering, dedup-keep-min-node_id, additive fields, basename, ADAPTER PARITY, equivalence to
  the compare canonical_set basis). rgr cycles 12/12 (incl. render_prefers_qualified_name_over_short_name).

LIVE (EXECUTED, release rmapd, producer env):
  - xpart-monorepo (refreshed packages/a+b): DEFAULT, --engine sqlite, AND --engine livegraph --kind
    module-import ALL render the SAME chain "packages/a/src -> packages/b/src -> packages/a/src". JSON identical
    on qualified_name/name/cycle_id/length/count (only node_id backend-native: SQLite uuid vs LiveGraph member).
    The fixture's two modules BOTH short-name "src" -> the OLD default would have shown the ambiguous
    "src -> src"; the qualified default disambiguates. PROOF of the D1=B value + byte-identity.
  - amodx (GREEN real repo, 8 partitions): default vs livegraph -> count 3==3; the qualified-name sequences of
    ALL 3 cycles (incl. a 21-member plugins cycle) + cycle_id + length are IDENTICAL; canonical order
    deterministic. No default cycle lost.
  - `--engine sqlite` is now canonical too (no legacy-short-name promise) — verified identical to default.

DIVERGENCES / RESIDUALS (recorded): (1) the EXPLICIT `--engine livegraph --kind module-import` JSON `name`
  changed from the qualified path to the basename, with the qualified path RELOCATED to the new `qualified_name`
  (information-preserving; required for SQLite/LiveGraph consistency + the future fastpath). (2) JSON `node_id`
  stays backend-native (SQLite uuid / LiveGraph member) — a fully cross-backend-identical JSON node_id is a
  RESIDUAL deferred to FASTPATH-1 (its proof target is the HUMAN output, which IS identical). (3) `length`/size =
  unique-qualified member count after dedup. (4) module_qualified_names = COALESCE(qualified_name,name): a NULL
  qualified column would fall back to short, but such a repo is not compare-GREEN so the future fastpath would
  not fire. (5) the agent orient/explain cycle rendering is UNCHANGED (out of scope; separate read path).

CONSEQUENCE: FASTPATH-1 (BLOCKED) is now UNBLOCKED — byte-identity is proven, so a cert-gated LiveGraph cycles
  default fastpath can resume as a SEPARATE slice (no human/JSON regression on GREEN). NOT done here (D3=A: prove
  first, fastpath after).
```

## Amendment — CYCLE-HONESTY-1 (2026-08-28): additive `edges` + `ts_type_only_caveat`

The default cycles output previously rendered an arrow ring drawn from the canonical member SET (a
Tarjan/sort order, NOT a walk) — a fabricated import claim (audit fix queue #7). CYCLE-HONESTY-1
amends this contract ADDITIVELY (operator ruling A1 / C1-repo-level):

- **`edges` (additive, per cycle).** The SQLite-served canonical output
  (`sqlite_module_cycles_json_with_edges`) now carries the REAL intra-SCC directed `IMPORTS` edges
  `[{from_node_id, to_node_id}]` (capped at 200 with `edges_truncated`). The renderer draws a DFS
  walk over these; with no edges it renders `members (unordered)`. An arrow only ever appears between
  a verified edge pair. The LiveGraph fastpath adapter (`livegraph_module_cycles_json`) OMITS the
  field (absent optional = honest); the compare + file/module-import LiveGraph routes carry no edges
  and render unordered.
- **Truncated edges render unordered, NO arrows (§2.2, ruling A1).** `edges_truncated == true` means
  the carried edges are an incomplete subset of the real intra-SCC set. A walk drawn over a capped
  subset could imply a chain the full set does not, so a truncated cycle is a no-arrows fallback case
  — rendered `members (unordered)` exactly like the LiveGraph route and older daemon replies, checked
  BEFORE any walk attempt. The member COUNT line stays complete; `--json` still carries the capped
  `edges` and the `edges_truncated: true` marker for programmatic consumers.
- **Byte-identity scope tightened, NOT broken.** The CERTIFIED fields (`cycle_id`, `length`,
  `nodes[].qualified_name`) stay byte-identical across both default backends — the no-loss cert,
  module-cycle compare, and consolidation witness are UNCHANGED. Only the additive `edges` field
  differs between the SQLite serve (present) and the LiveGraph fastpath (absent). ACCEPTED, RATIFIED
  DIVERGENCE: a TS repo's auto-fastpath serve renders `members (unordered)` while a cert-stale
  auto-SQLite fallback renders a real walk — both honest, neither fabricates. Restoring walks on the
  LiveGraph backend under a strengthened edge-set cert is the named follow-up CYCLE-FACTS-2(a).
- **`ts_type_only_caveat` (additive, repo-level bool).** True iff stored language facts show
  MATERIALLY-present TS/JS AND ≥1 cycle renders; the renderer prints one repo-scoped footer
  (`import type` edges may create cycles that vanish at runtime). No per-cycle claim (the per-cycle
  `type_only` fact is follow-up CYCLE-FACTS-2(c)).
  - **Materiality basis, route-consistent (review-2, operator ruling 2026-08-28 item 1).** "Material"
    is the ESTABLISHED ≥10%-of-code-files gate — the SAME `reader_context::material_code_languages`
    CONTRADICTION-SWEEP-1 uses for the enrich CTA, reused (not a re-derived threshold) via
    `reader_context::repo_has_material_ts_js`. "Any TS/JS file present" is deliberately NOT enough: a
    ~3.7% incidental JS (django) must not trip the caveat. EVERY cycles route — the SQLite serve, the
    forced `--engine sqlite` arm, the module-compare route, AND the three LiveGraph routes (default
    `auto` fastpath, file-import, module-import) — now derives the flag from the SAME stored
    per-language file facts (`query_file_count_by_language`) through the single
    `livegraph_feed::snapshot_has_material_ts_js`. The LiveGraph routes NO LONGER read the in-memory
    answer envelope's `contributing_languages` (which carried no counts, so it could not gate on
    materiality and diverged from the SQLite route). This costs the default `auto` route one cheap
    grouped language-count read; it still avoids the `find_cycles` Tarjan (the fastpath's performance
    rationale). The read is CLASSIFIED — a failure PROPAGATES (never a silent `false`).

Module layout (guardrail): `daemon-runtime/src/cycle_output.rs` (was 515) is split into
`cycle_output/mod.rs` (the certified canonical member-set output) + `cycle_output/edges.rs` (the
additive CYCLE-HONESTY-1 intra-SCC edge attachment); `rgr/src/presentation/cycles.rs` (was 746) is
split into `cycles/mod.rs` (response DTOs + renderers + caveat footer) + `cycles/walk.rs` (the DFS
walk-over-real-edges body renderer). Both halves are crate-private and ≤500 lines; the three
edge-attachment items are `pub(crate)` (no longer public APIs, review-2).

Spec: `docs/slices/cycle-honesty-1.md`.

## References
- `docs/slices/cycles-livegraph-default-fastpath-1.md` (BLOCKED — the discovery + the C ruling)
- `rust/crates/storage/src/queries.rs` (`find_cycles` short name + Tarjan order; `module_qualified_names` qualified)
- `rust/crates/daemon-runtime/src/livegraph_feed.rs` (`module_import_cycles_json` qualified members + derivation order; `module_cycle_compare_response`)
- `rust/crates/rgr/src/presentation/cycles/` (`mod.rs` renderers + `walk.rs` the real-edge walk)
- `rust/crates/daemon-runtime/src/cycle_output/` (`mod.rs` canonical output + `edges.rs` intra-SCC edges)
- `rust/crates/daemon-runtime/src/reader_context.rs` (`repo_has_material_ts_js` — the shared ≥10% gate)
- `docs/slices/sqlite-raw-decommission-readiness-6.md` (cycles = the highest-leverage REMAINING eager-SQLite default)
