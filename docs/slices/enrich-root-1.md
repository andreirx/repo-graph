# ENRICH-ROOT-1 — enrichment resolves the repo root against the DB, not the daemon's cwd

Status: SPECIFIED (2026-08-31) · Track: Field finding P1 (diagnosed 2026-08-31, controlled-
experiment proven). CODE slice, small. Maturity: MATURE (enrich lifecycle).

## 1. Problem (proven — controlled experiment, isolated roots)

`EnrichmentPipeline` resolves `repos.root_path` — stored RELATIVE to the DB file's directory
(`compute_storage_root_path`, pathdiff convention) — against the DAEMON PROCESS'S CWD
(`pipeline.rs:183-184` via `enrichment_impl.rs:800` `get_repo_root`, verbatim). Enrichment
therefore works only when the daemon's cwd happens to make the relative path resolve:
- daemon cwd = agent-manager sibling dir → FRAKTAG enriched 522/983, promoted 76;
- daemon cwd = deep/other (launchd runs at `/`) → 0/983, promoted 0 — SILENT.
Every launchd-served enrichment has always been 0. Two honesty defects hid it:
1. `client.rs:281-288` (`resolve_groups`): a group whose project context finds no tsserver is
   skipped with edges marked NEITHER enriched NOR failed — `eligible=983, enriched=0,
   failed=0, top_failure_reasons=[]`, no skip entry in the report.
2. The tsserver-resolver crate's `debug!`/`warn!` never reach the daemon log at any level.
This is a RECURRENCE of a fixed bug class: `storage/src/agent_orient_reads.rs:28-43` fixed
the same cwd-relative resolution for the doc inventory (resolve against the DB parent), and
`rgr/src/cli/context.rs::resolve_repo_root` is the client-side canonical implementation.

## 2. Contract

1. **Resolve against the DB parent.** `get_repo_root` (enrichment path) returns the repo
   root resolved against the DB file's parent directory per the established
   `agent_orient_reads.rs` pattern — one shared helper if one can serve both sites without a
   new crate edge (prefer reusing/moving the existing pattern; do not duplicate a third
   implementation). Resolution failure is an ERROR with the attempted path — never a silent
   fallback to the raw relative path (`client.rs:175-177`'s canonicalize-or-raw fallback
   becomes canonicalize-or-error at the pipeline boundary).
2. **Not-attempted is a rendered state.** Per-context locate-misses surface in the enrich
   report as skip entries with the context path and reason (`skipped_contexts` additive
   field), counted in a new `not_attempted` total so `eligible = enriched + failed +
   not_attempted` ALWAYS holds (an invariant test). doctor's enrichment line renders the
   breakdown when not_attempted > 0.
3. **Audit every other `get_repo_root`/`root_path` consumer** (deterministic grep, list in
   the report) for the same cwd-relative resolution; fix in-slice ONLY those on the
   enrichment path; others become named findings (report, not silent fixes).
4. JSON additive; exit codes unchanged; no schema change (root_path storage convention is
   frozen and correct — the READ side resolves).

## 3. Stop conditions

Frozen: storage schema, root_path storage convention, enrich single-pass doctrine, promotion
semantics, trust. STANDING HONESTY RULES. New public APIs beyond additive DTO fields →
DECISION_REQUIRED. Unmet DoD → STOP + DECISION_REQUIRED. Never touch the operator's real
state root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Regression test FIRST: pipeline run with the harness daemon's cwd set somewhere hostile
  (tempdir) against a relative-root DB → FAILS pre-fix (0 attempted), PASSES post-fix
  (edges attempted). The eligible == enriched+failed+not_attempted invariant test.
- Unit: DB-parent resolution (incl. error-with-path on unresolvable); skip entries rendered;
  doctor breakdown.
- Live proof (isolated state root, registry sha unchanged): daemon launched with cwd=/ (the
  launchd shape), index FRAKTAG, auto-pass enriches >0 and promotes >0; capture doctor line.
  (LM Studio live — auto-seed guards in harnesses; the ENRICH pass is the subject here, so
  use the real pass in the isolated daemon, not the disabled-guard shape.)
- Chunked cargo gates; consolidation witness; dogfood-isolated green.

## 5. Definition of done

Enrichment yield is independent of the daemon's cwd; a launchd-served daemon enriches;
not-attempted edges are visible in the report and doctor with reasons; the accounting
invariant holds by test; other cwd-relative consumers are enumerated; gates green.
