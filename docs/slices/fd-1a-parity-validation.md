# FD-1A-PARITY: Express Detector Parity Validation

Status: COMPLETED (2026-05-12)
Type: Validation
Depends: FD-1A (IMPLEMENTED)
Blocks: None

## Completion Summary

Parity validation executed. See `fd-1a-parity-report.md` for full results.

**Verdict:** Parity achieved for core route detection. Behavioral differences are intentional scope choices, not bugs. No FD-1A-FIX required.

| Metric | Rust | TS Prototype |
|--------|------|--------------|
| Total routes | 16 | 17 |
| Shared routes | 15 | 15 |
| Rust-only | 1 (USE middleware) | — |
| TS-only | — | 2 (dynamic paths) |

---

## Goal

Validate that the Rust Express detector (`express_detector.rs`) achieves behavioral parity with the TypeScript prototype (`express-route-extractor.ts`) when run on the same corpus.

This is **validation work**, not new feature logic — unless the comparison reveals gaps that require fixes.

## Why This Slice Exists

FD-1A is marked IMPLEMENTED but explicitly states:

> This is a first-cut Rust implementation, not a verified parity match against the TS prototype. Parity claim has not been validated (no corpus comparison test).

The "parity" label in FD-1A is aspirational. This slice makes it factual.

## Scope

### In Scope

1. **Corpus selection:** Identify a shared test corpus (existing or new) that exercises:
   - Basic routes (`app.get`, `app.post`, etc.)
   - Router patterns (`express.Router()`)
   - Path parameters (`:id`)
   - Multiple routes per file
   - Negative cases (dynamic paths, non-Express receivers)

2. **Dual-run execution:**
   - Run Rust detector via `rmap index` on corpus
   - Run TS prototype via `rgr` (or isolated test harness) on same corpus
   - Capture route lists from both

3. **Comparison metrics:**
   - Route count match (exact or within tolerance)
   - Method extraction match (GET, POST, etc.)
   - Path extraction match (string equality after normalization)
   - False positive comparison (routes detected by one but not the other)
   - False negative comparison (routes missed by one but caught by the other)

4. **Parity report:**
   - Document observed deltas
   - Classify each delta as:
     - **Acceptable:** intentional scope difference
     - **Bug:** unintentional miss (triggers FD-1A-FIX)
     - **Enhancement:** new capability (triggers follow-on slice)

### Out of Scope

- Handler symbol attribution comparison (FD-1A-4 deferred)
- Router mount composition comparison (FD-1A-2 deferred)
- Performance comparison (parity is about correctness, not speed)

## Existing Artifacts

### Rust Detector
- `rust/crates/repo-index/src/express_detector.rs`
- `rust/crates/repo-index/tests/fd_1a_express_integration.rs`
- Validation corpus: `test/fixtures/typescript/express-routes/`

### TypeScript Prototype
- `src/adapters/extractors/typescript/express-route-extractor.ts`
- `test/adapters/extractors/typescript/express-route-extractor.test.ts`

## Validation Approach

### Option A: Shared Corpus Comparison

Run both detectors on `test/fixtures/typescript/express-routes/` and compare outputs.

**Pros:**
- Uses existing corpus
- Simple execution

**Cons:**
- Corpus was designed for Rust validation, may not exercise TS prototype edge cases

### Option B: TS Prototype Test Cases as Corpus

Extract test cases from `express-route-extractor.test.ts` and use them as comparison corpus.

**Pros:**
- Exercises known TS prototype expectations
- Higher confidence in parity

**Cons:**
- Requires extracting inline test fixtures to files

### Recommendation

**Option A first**, then expand corpus if deltas appear.

## Validation Commands

```bash
# 1. Index corpus with Rust detector
rmap index test/fixtures/typescript/express-routes ./test-artifacts/fd-1a-parity.db

# 2. List Rust-detected routes
rmap surfaces list ./test-artifacts/fd-1a-parity.db express-routes --kind http_provider > rust-routes.json

# 3. Run TS prototype on same corpus (requires harness script)
# Output: ts-routes.json

# 4. Compare outputs
# diff rust-routes.json ts-routes.json
# Or structured comparison script
```

## Acceptance Criteria

1. Both detectors run on identical corpus
2. Route count difference <= 10% OR all deltas classified and documented
3. Method extraction matches for all shared routes
4. Path extraction matches for all shared routes (after normalization)
5. Parity report documents all observed deltas with classification
6. No unclassified deltas remain

## Definition of Done

- Comparison executed (EXECUTED evidence)
- Parity report written
- All deltas classified
- Follow-on slices created for any bugs or enhancements discovered
- FD-1A slice doc updated to reference parity validation results

## Possible Outcomes

1. **Full parity:** No follow-on work needed. Update FD-1A to claim validated parity.

2. **Minor deltas, all acceptable:** Document in parity report. Update FD-1A to claim parity with documented exceptions.

3. **Bugs found:** Create FD-1A-FIX slice for each bug. FD-1A remains "implemented but not parity-validated" until fixes land.

4. **Enhancement gaps:** Create follow-on slices (FD-1A-2, FD-1A-3, etc.) for scope expansions. FD-1A claims parity for current scope.

## Artifacts Produced

- `docs/slices/fd-1a-parity-report.md` — comparison results and delta classification
- Updated `docs/slices/fd-1a-rust-express-detector-parity.md` — parity status

## Estimated Effort

Small slice. Primarily execution and documentation, not implementation.

- Corpus selection: 1 hour
- Dual-run execution: 1-2 hours (including TS harness if needed)
- Comparison and report: 2-3 hours
- Follow-on slice creation (if needed): 1-2 hours

Total: ~1 day
