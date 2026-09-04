# Per-command usefulness audit — v0.16.0 (2026-09-04, round four)

Artifact: https://claude.ai/code/artifact/581da1fc-faa1-4972-ae77-2d27579b0493
Smoke: smoke-runs/2026-09-04T00-47-11Z (26/29 passed; linux client-timeout carried;
repo-graph assess + django orient were serial-daemon contention, retested clean).
Supplemental: 15 probes (/private/tmp/audit16/captures at run time). Gate: matrix grader
(29-repo sweep, ground-truth-verified) + no-rubric judge + codex standalone adjudication.
Baselines: v0.15.0 matrix, zg head-to-head (2026-09-03), seed-chunk spike.

## Verdict (codex-adjudicated)

v0.16.0 is more useful — find --text retires all four zg F's with exact ripgrep parity
(3/3, 6/6, 230/230), anchors/evidence land everywhere, chunk seeds hit the spike ranks
with lmstudio retired — but the trust contract regressed in the highest-risk layer: the
product now emits CONFIDENT UNSUPPORTED CLAIMS. v0.15.0's failure mode was silence
(safe); v0.16.0's is confident invention (indefensible for an agent). Verified
fabrications: vscode ".env FS_PATH 2 readers 2 writers" (no literal-path call exists);
hadoop 5 phantom writers (its 6 readers exactly right); "nextjs_app_router_detected" on a
React-Router-7 Java monorepo; map edges invented for a JDK-only file; django "unused:
asgiref" vs 31 import sites; storybook "declared-unused 111" vs a 13-entry manifest.
Trust repair must dominate the next round. The refusal discipline (dead, vacuous-pass,
cause-discriminating zero-states, spring-petclinic surfaces 17/17) remains best-in-class.

## Shipped-fix verdicts

--text: FIXED recall / BROKEN C++ evidence (156/177 mis-attributed; Rust/TS clean) ·
anchors+evidence: PARTIAL (26/26 lines; diet reclaimed 2.2–2.8% on the wrong tier; seed
cursors still 30–53% of bytes) · false sentence: FIXED (0/759) · chunk seeds: PARTIAL
(ranks reproduced; is_test partition INERT on Rust — 4,578 #[test] chunks stored
production, per-file fact vs per-symbol chunks; ≤2-line decl chunks 53% of FRAKTAG corpus
outrank impls) · resource caveat: FIXED disclosure / NEW fabrication · .h routing:
REGRESSED (class <MACRO> <Name> erases names, 770/770 vcmi; LEVELDB_EXPORT API still
FUNCTION) · type-only: NEAR-DORMANT (2/118; predicate ALL should be ANY — any erased edge
breaks a runtime cycle; 5 false negatives verified).

## Proposed queue (codex-consolidated, awaiting ratification)

1. HONESTY-GATE-1 — the no-fabrication invariant: no row whose evidence the printed
   caveat excludes; covers deps-unused (npm/py caveat or gate; dynamic-import + config
   resolution; Maven absence named architectural), resource phantom writers/computed
   paths, trust framework detections, map invented edges.
2. CPP-SPAN-FIDELITY-1 — macro-decorated class extraction + span containment (one root,
   two field regressions: .h routing + --text attribution; Limiter 73–806 already filed).
3. SEED-CHUNK-2 — per-chunk is_test, declaration-chunk demotion, --text referral beside
   seeds always.
4. COHERENCE-2 — orient/cycles walk agreement; type-only ALL→ANY (+ positive "breaks at
   runtime" label); test-only exclusion in surfaces headlines (6 of vscode's 9 providers
   are fixtures); (N test) subset-vs-addend; five file totals reconciled.
5. ECONOMY-2 — seed-cursor diet; map/orient --full caps + truthful truncation markers
   (zvec --full byte-identical to large, unmarked).
6. Carried: qualified-call blindness, check-verdict comparability, boundary coverage
   statement, Martin-on-C, drill-downs ending in zeros.

Infra: serial-daemon contention now MEASURED (301s assess hang, Busy bounce; chunk seed
pass is a new long writer) — DAEMON-CONCURRENCY-1 price rising. Full matrix + all 24
ranked defects + minors: see artifact.
