# Spike: `rmap explain` on repo-graph itself

**Date:** 2026-04-17
**Slice context:** Explain certification. The command triad
(orient/check/explain) is structurally complete; this spike
validates explain preserves the substrate boundary and meets
the locked invariants.

## Method

1. Reused `/tmp/rgspike2.db` (495 files, 3630 nodes).
2. Six scenarios:
   - File target: `src/core/trust/service.ts`
   - Module/path target: `src/core`
   - Symbol by name: `computeTrustReport`
   - Symbol by stable key
   - Ambiguous symbol: `constructor`
   - No-match: `nonexistent_thing`
   - Bonus: symbol in cycle-participating module:
     `maskCommentsCStyle` (in `src/core/seams`)

## Invariant verification

| Invariant | Status |
|---|---|
| File targets direct-only | **Correct.** No cycles, boundary, gate, callers, callees at file scope. |
| Symbol targets direct + exact-module context | **Correct.** `maskCommentsCStyle` shows EXPLAIN_CYCLES with `scope: module_context`. Direct signals omit scope field. |
| One evidence shape per signal code | **Correct.** EXPLAIN_DEAD uses unified shape (`count`, `is_target_dead`, `reliability_level`, `items`). |
| Bounded lists with truncation metadata | **Correct.** EXPLAIN_FILES at `src/core`: 15 of 60, `items_truncated: true`, `items_omitted_count: 45`. |
| EXPLAIN_IDENTITY first | **Correct.** Rank 1 in every response. |
| Measurements from measurement queries | **Correct.** (No measurements exist on this repo — verified via empty items list.) |
| EXPLAIN_SYMBOLS / EXPLAIN_FILES as explain-native inventory | **Correct.** Both emit bounded lists with counts. |
| No orient/check signal leakage | **Correct.** Zero orient-only or check-only signal codes in any response. |

## Findings

### S-EX-1 — Design question. EXPLAIN_DEAD fires at LOW reliability

`rmap explain ... src/core` shows EXPLAIN_DEAD with
`count=1421, reliability_level="low"`. Orient would suppress
this entirely (DEAD_CODE_UNRELIABLE limit). Explain shows the
data with the reliability caveat in the evidence.

For a deep dive, this is defensible: the agent sees
`reliability_level: "low"` and can decide whether to act.
But it is behaviorally different from orient. The question:

| Option | Behavior |
|---|---|
| A. Keep as-is | Explain shows dead data with reliability caveat. Agent has more information than orient provides. The `reliability_level` field communicates the caveat. |
| B. Gate explain like orient | Suppress EXPLAIN_DEAD when reliability is not HIGH. Emit a limit instead. Consistent with orient but loses deep-dive detail. |

### S-EX-2 — Bug. EXPLAIN_MEASUREMENTS emitted with empty items

The plan says: "When neither [coverage nor complexity] is
present, EXPLAIN_MEASUREMENTS is not emitted."

The implementation emits `EXPLAIN_MEASUREMENTS` with
`{"items": []}`. This is unconditional noise — the section
has no data but still appears in the output. Fix: skip the
section when `items` is empty.

### S-EX-3 — Minor. Zero-count callers/callees omitted but zero-count dead emitted

`maskCommentsCStyle` has 0 callees. EXPLAIN_CALLEES is absent.
But EXPLAIN_DEAD with `count=0, is_dead=false` IS emitted.

For a deep dive, "this symbol is not dead" is useful positive
information. "This symbol has 0 callees" is also useful.

The inconsistency is minor but real. Two options:
- Emit zero-count callers/callees as empty sections
  (consistent with dead, provides positive signal)
- Omit zero-count callers/callees (current behavior)

### S-EX-4 — Confirmed correct. Module-context scope marker

`maskCommentsCStyle` (in `src/core/seams`) produces:
- EXPLAIN_CYCLES with `scope: module_context` ✓
- Direct signals (EXPLAIN_IDENTITY, EXPLAIN_CALLERS,
  EXPLAIN_DEAD, EXPLAIN_TRUST) with NO scope field ✓

The exact-module matching works: only cycles involving
`src/core/seams` appear, not descendants or unrelated modules.

### S-EX-5 — Confirmed correct. All other scenarios

- Ambiguous `constructor`: 5 candidates, 0 signals ✓
- No-match `nonexistent_thing`: resolved=false, 0 signals ✓
- Stable-key resolution: preserves input in focus.input ✓
- File sections correctly scoped: 7 imports, 8 symbols, 6 dead ✓
- Module sections correctly scoped: 60 files, 2 cycles ✓

## Summary

| Finding | Type | Action |
|---|---|---|
| S-EX-1 EXPLAIN_DEAD at LOW reliability | Design question | User decides A or B |
| S-EX-2 Empty EXPLAIN_MEASUREMENTS | Bug | Fix: omit when items empty |
| S-EX-3 Zero-count callers/callees omitted | Minor inconsistency | User decides |
| S-EX-4 Module-context scope marker | Confirmed correct | None |
| S-EX-5 All other scenarios | Confirmed correct | None |

## Substrate preservation assessment

The spike confirms explain is a structured bridge, not a
replacement for the substrate:

- Every section maps to an existing substrate surface
  (EXPLAIN_SYMBOLS and EXPLAIN_FILES are explain-native
  inventory, honestly classified)
- Bounded lists with truncation metadata signal "there is
  more; use `rmap callers` / `rmap dead` for the full list"
- Trust caveats (reliability_level in EXPLAIN_DEAD,
  EXPLAIN_TRUST section) give the agent the context to
  decide whether to trust the data
- No signal code from orient or check leaks into explain

The detailed substrate commands remain the authoritative
source of truth. Explain organizes and bounds that truth for
agent consumption.
