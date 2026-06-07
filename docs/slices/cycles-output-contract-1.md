# CYCLES-OUTPUT-CONTRACT-1: the default `rmap cycles` identity + ordering contract

Slice ID: CYCLES-OUTPUT-CONTRACT-1
Status: **SPEC — AWAITING RATIFICATION (D1–D3). DECISION-ONLY; NO code, NO renderer change, NO fastpath, NO
default flip until ratified.** Decide the DEFAULT `rmap cycles` output contract — node IDENTITY (short vs
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

## What this slice does NOT decide / out of scope (hard guardrails)
```text
NO code, NO renderer edit, NO JSON field added, NO canonicalization implemented THIS slice — DECISION ONLY. The
implementation (canonical ordering helper, qualified-name plumbing, the FASTPATH-1 resume/close) is a SEPARATE
ratified slice AFTER this. NO change to the GRAPH's module identity/resolver (presentation/output contract only).
NO change to `--engine compare` (already qualified). NO raw decommission, NO table deletion. The `file`-kind
cycles output is untouched. If D1=B, the implementation slice MUST land the canonical order on BOTH backends
BEFORE any fastpath (so the SQLite default and the LiveGraph fastpath are proven byte-identical first).
```

## References
- `docs/slices/cycles-livegraph-default-fastpath-1.md` (BLOCKED — the discovery + the C ruling)
- `rust/crates/storage/src/queries.rs` (`find_cycles` short name + Tarjan order; `module_qualified_names` qualified)
- `rust/crates/daemon-runtime/src/livegraph_feed.rs` (`module_import_cycles_json` qualified members + derivation order; `module_cycle_compare_response`)
- `rust/crates/rgr/src/presentation/cycles.rs` (the human renderer — reads name + nodes.len only)
- `docs/slices/sqlite-raw-decommission-readiness-6.md` (cycles = the highest-leverage REMAINING eager-SQLite default)
