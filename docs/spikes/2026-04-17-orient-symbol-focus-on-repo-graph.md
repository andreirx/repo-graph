# Spike: `rmap orient --focus <symbol>` on repo-graph itself

**Date:** 2026-04-17
**Slice context:** Rust-45 validation. Symbol focus shipped;
spike validates focus resolution, caller/callee grouping,
inherited module context, and the SignalScope marker before
moving to `check`.

## Method

1. Release binary with Rust-45 changes.
2. Reused `/tmp/rgspike2.db` from the Rust-44 spike (495 files,
   3630 nodes, 2091 edges, 4556 unresolved).
3. Five scenarios:
   - Stable-key symbol focus: `computeTrustReport`
   - Exact-name symbol focus: `evaluateObligation`
   - Ambiguous name: `constructor` (12+ matches, capped at 5)
   - Inherited module context via name: `computeTrustReport`
     (module `src/core/trust` — no cycles/boundaries)
   - Symbol in cycle-participating module: `maskCommentsCStyle`
     (module `src/core/seams` — has cycle with
     `src/core/seams/detectors`)

## Results

### Scenario 1: Stable-key symbol focus

```
rmap orient /tmp/rgspike2.db repo-graph --budget large \
  --focus "repo-graph:src/core/trust/service.ts#computeTrustReport:SYMBOL:FUNCTION"
```

- Focus: resolved, kind=symbol, input=stable key (preserved
  verbatim, not rewritten to name), path=src/core/trust/service.ts
- CALLERS_SUMMARY: count=2, top_modules:
  `[src/cli/commands: 1, test/trust-parity: 1]`
- CALLEES_SUMMARY: count=14, top_modules:
  `[src/core/trust: 13, src/core/classification: 1]`
- Trust: TRUST_NO_ENRICHMENT (repo-wide)
- No inherited module signals (module `src/core/trust` has no
  cycles, no boundaries, no gate obligations)
- Limits: DEAD_CODE_UNRELIABLE, GATE_NOT_CONFIGURED,
  COMPLEXITY_UNAVAILABLE
- No MODULE_DATA_UNAVAILABLE (P2-2 fix confirmed)
- No `scope` field on any signal (all direct, backward compat)

### Scenario 2: Exact-name symbol focus

```
rmap orient /tmp/rgspike2.db repo-graph --budget large \
  --focus "evaluateObligation"
```

- Focus: resolved by name, input="evaluateObligation"
- CALLERS_SUMMARY: count=1, module=src/core/evaluator
- CALLEES_SUMMARY: count=3, module=src/core/evaluator
- Same limits pattern as scenario 1

### Scenario 3: Ambiguous name

```
rmap orient /tmp/rgspike2.db repo-graph --budget large \
  --focus "constructor"
```

- Focus: resolved=false, reason=ambiguous
- 5 candidates (max per contract), all kind=symbol,
  sorted by stable_key lexicographically
- Zero signals, zero limits

### Scenario 4: Inherited module context (no-cycle module)

```
rmap orient /tmp/rgspike2.db repo-graph --budget large \
  --focus "computeTrustReport"
```

- Same symbol as scenario 1, resolved by name
- Module `src/core/trust` has no cycles, no boundaries, no
  gate obligations → inherited signals correctly absent
- Only direct signals fire

### Scenario 5: Symbol in cycle-participating module

```
rmap orient /tmp/rgspike2.db repo-graph --budget large \
  --focus "repo-graph:src/core/seams/comment-masker.ts#maskCommentsCStyle:SYMBOL:FUNCTION"
```

- Focus: resolved, kind=symbol
- **IMPORT_CYCLES fires with scope=module_context** — cycle:
  `src/core/seams ↔ src/core/seams/detectors` (qualified
  names, exact-module match)
- TRUST_NO_ENRICHMENT: direct scope (no `scope` field)
- CALLERS_SUMMARY: direct scope
- SNAPSHOT_INFO: direct scope
- Limits: DEAD_CODE_UNRELIABLE, GATE_NOT_CONFIGURED,
  COMPLEXITY_UNAVAILABLE

This is the key scenario. The inherited cycle signal has
`"scope": "module_context"` in the JSON. The direct signals
do NOT have the `scope` field. The contract's scope marker
works correctly.

## Validation matrix

| Check | Status |
|---|---|
| Stable-key symbol focus resolves | Correct |
| Exact-name symbol focus resolves | Correct |
| Ambiguous candidates bounded at 5 | Correct |
| Candidates sorted by stable_key | Correct |
| No signals on ambiguous resolution | Correct |
| CALLERS_SUMMARY groups by OWNS module | Correct |
| CALLEES_SUMMARY groups by OWNS module | Correct |
| Inherited IMPORT_CYCLES uses exact module | Correct |
| Inherited signals have scope=module_context | Correct |
| Direct signals omit scope field (backward compat) | Correct |
| No MODULE_SUMMARY at symbol scope | Correct |
| No MODULE_DATA_UNAVAILABLE at symbol scope | Correct |
| DEAD_CODE_UNRELIABLE reliability gate works | Correct |
| focus.input preserves original query string | Correct |

## Scenarios NOT exercised on this repo

- Inherited BOUNDARY_VIOLATIONS (no boundary declarations in DB)
- Inherited gate signals beyond GATE_NOT_CONFIGURED (no
  requirement declarations in DB)
- `"(unknown)"` module bucket in callers/callees (all callers
  have OWNS-edge module ownership on this repo)

These paths are covered by the fake-based integration tests
in `orient_symbol_signals.rs` (tests 7, 10, 13). The spike
confirms the real-adapter path for the exercisable scenarios.

## Conclusion

No bugs found. Rust-45 is output-correct on representative
real-repo scenarios. `check` can proceed.
