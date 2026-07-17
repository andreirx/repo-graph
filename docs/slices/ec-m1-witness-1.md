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
