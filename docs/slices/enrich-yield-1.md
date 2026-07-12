# ENRICH-YIELD-1 — make the promotion filter's rejections visible, then explain the 3.5% yield

Status: SPECIFIED (2026-07-12) · Track: Resolution & attribution (`docs/ROADMAP.md` § ENRICH-YIELD-1)
Origin: ENRICH-LIFECYCLE-1 delivery (2026-07-07): the auto-pass resolves ~74% of unknown
receiver types, but promotion banks only **~3.5% of resolutions** (261/7,435 on self-index;
+0.3pp call resolution). Call-graph reliability stays LOW until yield improves — which caps
the value of the entire enrichment investment.

## 1. Problem

Enrichment does the expensive work (LSP resolution) and then the 8-gate promotion filter
(TECH-DEBT § 8-Gate Promotion Filter) rejects ~96.5% of the results — **silently**. Nobody
can say which gate rejects what share, whether those rejections are defensible
(honesty-preserving) or over-conservative (e.g. gate 5/6 uniqueness, gate 4 internal-only),
or what the recoverable upside is. Deciding gate changes blind would be guesswork; this
slice makes the funnel a measured fact FIRST.

## 2. Contract (instrumentation + analysis — NO promotion-behavior change)

1. **Per-gate rejection accounting.** The promotion pass counts, per gate (1–8): candidates
   entering, rejected (first-rejecting-gate attribution), plus promoted and total resolved.
   Deterministic, cheap (counters, no extra queries per candidate beyond what gates already
   read).
2. **Visible on the output surface** (deep-vertical rule): the enrichment completion report
   (doctor / oplog outcome line already shipped by ENRICH-LIFECYCLE-1) carries the funnel
   headline (resolved → promoted, top rejecting gates), and the full per-gate breakdown is
   queryable (`rmap enrich --report` or the doctor detail — builder picks the least-new-
   surface option and records it). Labels speak the reader's language ("rejected: type
   resolves to 2+ classes (ambiguous)" not "gate 5").
3. **Analysis + ratification packet.** From a real self-index run + one external fixture
   repo (nginx-scale not required): per-gate shares, the dominant rejection classes with
   3-5 concrete cited examples each, and a DECISION_REQUIRED list proposing (a) which gates
   could relax into DIRECT promotion safely, (b) which rejected classes could land as
   **Layer-2 inferences with basis** (per the certainty model — "inferred from LSP
   resolution, ambiguous target set") instead of vanishing, (c) which rejections are
   correct and stay. Each option with honesty rationale. The IMPL of any change is
   ENRICH-YIELD-2 after operator ratification.

## 3. Stop conditions

- NO change to what gets promoted (counters + reporting only); enrich_pass semantics
  frozen except the additive accounting/reporting seam.
- No new config surface; no schema migration (reuse the existing enrichment
  outcome/diagnostics storage — if the breakdown cannot land there additively → STOP +
  DECISION_REQUIRED).
- Do NOT commit.

## 4. Validation (SYNCHRONOUS; TEST REPORT INLINED)

- Cargo gates green from `rust/` (build / full workspace test — the machine-environmental
  `execute_repo_request_handles_daemon_states` exclusion is documented / fmt / clippy).
- Named tests: funnel counters sum correctly (entering = promoted + Σ rejected);
  first-rejecting-gate attribution; report rendering with reader-frame labels; zero-work
  pass renders honestly ("no candidates", never 0-vs-unknown confusion).
- Isolated self-dogfood (/private/tmp state root + stdio; NEVER the real registry): index
  repo-graph, let auto-enrichment run, show the funnel in the completion report + detail
  surface; transcript inlined. The analysis packet (§2.3) inlined in the build report.

## 5. Definition of done

The 3.5% is decomposed into named, cited, per-gate facts visible on the product surface;
the ratification packet proposes concrete yield options with honesty rationale; nothing
about promotion behavior changed.
