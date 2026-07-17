# LIVEGRAPH-PARTIAL-FIX-1 — fail-soft the Partial-invariant panic (RECON-SPIKE-1 finding #0)

Status: SPECIFIED (2026-07-17, ratified as the immediate next slice during RECON-SPIKE-1
review-0) · Track: Serving/trust invariants · Priority: P1
Origin: RECON-SPIKE-1 §5.0 — empirically fired 3× on a real-producer fixture.

## 1. Problem

`repo-graph-livegraph::finalize_envelope` panics (`.expect("partial invariant holds")`,
`lib.rs:338`) when a `Partial` answer arises from a call-graph-incomplete defining basis
(`AstFileScope` — e.g. the FILE node SCIP materializes for a top-level import) because no
`DegradationReason` is mapped (documented precondition, `lib.rs:303-306`). The SHIPPED
cert's comparison calls the same `callers()`/`callees()` and escapes only by
data-dependent luck (short-circuit order). A repo whose first walked FILE symbol panics
before any divergence would crash the serve path: a daemon panic on a serving/trust
invariant.

## 2. Contract

1. Map the call-graph-incomplete `AstFileScope` `Partial` basis to an honest
   `DegradationReason` (reader-frame: the answer is partial because the symbol's defining
   basis carries no call-graph content — a structural fact, not an error), so
   `finalize_envelope` degrades fail-soft instead of panicking. The asymmetry RECON-SPIKE-1
   recorded (callees degrade cleanly, callers panic) becomes symmetric clean degradation.
2. No panic path remains reachable from `callers()`/`callees()` for ANY basis: a named
   test walks a fixture containing the affected FILE symbols (reuse RECON-SPIKE-1's
   real-producer fixture) BOTH directions with no catch_unwind, plus a unit test for the
   basis→reason mapping.
3. Cert/serving verdict semantics unchanged for measured cases (GREEN/RED as today);
   the previously-panicking symbols now yield honest Partial answers wherever served.

## 3. Stop conditions

- `repo-graph-livegraph` (+ its tests) only; no cert logic changes beyond what the
  now-total mapping removes (the spike's catch_unwind may be retained as belt-and-braces
  or removed — builder's call, recorded). No schema/trust-vocabulary changes.
- Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

Cargo gates from `rust/` (fmt/clippy/livegraph + daemon-runtime crates + chunked
workspace); the named no-panic walk test; a live isolated re-run of the RECON-SPIKE-1
diff emission on the same fixture showing `livegraph_panic: 0` and the two FILE symbols
reporting measured/Partial instead of null (raw artifact retained).

## 5. Definition of done

The panic is unreachable (tested); the spike's artifact on the same fixture shows 0 panics
with honest Partial degradation; gates green.

---

## 6. Delivery record (2026-07-17)

**DELIVERED** (see the fail-soft commit; operator close on review-1's explicit
"acceptance otherwise satisfied" + two ratifications: AST-FILE-SCOPE-REASON — the additive
`StructuralNodeNoCallGraphContent` variant, after the reviewer correctly rejected the
UnresolvedAlias reuse as contract-falsifying; REAL-WALK-TEST-SCOPE — the walk test lives
with its fixture in livegraph-feed). livegraph_panic: 0 on the spike's re-run; the
data-dependent daemon panic is unreachable (tested, no catch_unwind); RECON-DESIGN-1's
exhaustive-walk precondition is cleared.
