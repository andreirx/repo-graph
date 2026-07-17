# EC-M1-WITNESS-1 — the reader-set + fact-class witness, enforced by script (EC-1 milestone M-1)

Status: SPECIFIED (2026-07-17) · Track: Consolidation milestones (EC-1 §5 M-1; predicate C-6)
Depends: M-0 (complete — ratifications recorded, ledgers corrected, banners placed).

## 1. Problem

The ratified consolidation end-state (EC-1 §4.2 as amended) defines who may read what —
but nothing ENFORCES it. Drift is currently free: a new dispatch arm can read any store,
and a new module can touch the LiveGraph field, without anything going red. C-6 makes the
§7.3 inventory method a permanent, cheap guard: "new features pay ONE integration by
construction."

## 2. Contract

1. **The reader-set witness:** a deterministic script (Rust test or checked script — the
   builder picks the least-new-surface form and records it) that computes the set of
   modules reading the LiveGraph field and asserts it EQUALS the sanctioned list
   (EC-1 §3.3-A modules + the two LG writers, as ratified). Any addition goes red until
   the list (a reviewed, committed manifest) is updated.
2. **The fact-class manifest:** every dispatch arm declares its fact classes in a
   one-line-per-arm committed manifest; the script checks (a) every arm in dispatch.rs
   appears in the manifest (count reconciled — 66 today per EC-1 §3.3), (b) no manifest
   entry is stale (names an arm that no longer exists). Content-level verification of
   WHICH classes an arm reads stays the audit's job — the manifest is the declaration
   surface that makes new arms pay their integration explicitly.
3. **Red/green on today's tree:** the script must PASS against HEAD with the ratified
   sanctioned list + a complete manifest (writing the initial manifest from EC-1 §3.3's
   inventory is part of this slice); a deliberate violation (test fixture) must FAIL.
4. **Wired into CI/smoke:** runs in the standard gate path (the workspace test suite
   and/or smoke script — least-new-surface, recorded) so it cannot be skipped silently.

## 3. Stop conditions

- Guard + manifest + wiring ONLY: no production logic changes, no dispatch changes, no
  serving-path changes. If the computed reader set does NOT match the ratified sanctioned
  list on today's tree → that is a FINDING (surface as DECISION_REQUIRED with the
  evidence), not something to "fix" silently in either direction.
- Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

Cargo gates from `rust/` (chunked); the witness passing on HEAD; the deliberate-violation
fixture failing; manifest completeness reconciled against the live dispatch-arm count;
CI wiring proven (the gate actually executes it).

## 5. Definition of done

C-6 holds and is enforced: reader-set drift and undeclared dispatch arms go red in CI;
the initial manifest matches the ratified inventory; gates green.

## 6. DELIVERY (2026-07-17)

Delivered in 2 relay cycles (opus-4-8 builder, gpt-5.6-sol reviewer; cycle-1 revise
tightened the guard: mechanical `#[cfg(test)]` verification of the test-scaffolding split,
LG-writer fact classes corrected to FC1/FC2a/FC2b/FC4/FC8, RHS taxonomy validation,
preload annotation fix).

- **The witness:** `rust/crates/daemon-runtime/tests/consolidation_witness.rs` — a std-only
  integration test riding the existing CI gate (`cargo test --workspace`, ci.yml:84; zero
  new CI surface). 15 tests: 2 gates + violation fixtures (rogue reader, cfg-drift,
  unknown/empty fact class, stale/missing arm — each proven RED, incl. real injected
  violations reverted byte-exact).
- **Manifests:** `witness/livegraph_reader_set.txt` (15 production reader files → the 12
  ratified §3.3-A surfaces + 2 LG writers; 10 mechanically-verified cfg(test) scaffolding
  files) and `witness/dispatch_fact_classes.txt` (66 lines == live arm count, classes
  sourced from EC-1 §3.3, RHS-validated).
- **STOP-condition result: NO FINDING** — today's tree matches the ratified sanctioned
  list exactly; 66 arms reconcile with §3.3 (10+16+20+7+13).
- Gates: witness 15/15 (builder + reviewer + operator EXECUTED); workspace 5141/0
  (builder); fmt/clippy clean.

**C-6 holds and is enforced**: reader-set drift and undeclared/mis-declared dispatch arms
go red in CI. New features pay ONE integration by construction. M-1 complete.
