<!-- requirements-assurance-v1
{
  "formatVersion": 1,
  "kind": "requirement",
  "requirementId": "RG-REQ-009",
  "sources": [
    { "kind": "document-section", "path": "docs/VISION.md", "fragment": "value-frontier" },
    { "kind": "document-section", "path": "docs/VISION.md", "fragment": "honesty-rules" },
    { "kind": "document-section", "path": "docs/VISION.md", "fragment": "discovery-over-enforcement" },
    { "kind": "document-section", "path": "docs/slices/trust-model-rebase-1.md", "fragment": "ratified-decisions-2026-05-31" },
    { "kind": "document-section", "path": "docs/slices/dead-causes-1.md", "fragment": "2-contract" },
    { "kind": "document-section", "path": "docs/slices/headline-truth-1.md", "fragment": "2-contract" }
  ],
  "lowLevelRequirements": [
    { "id": "RG-REQ-009-L01", "parentId": "RG-REQ-009" },
    { "id": "RG-REQ-009-L02", "parentId": "RG-REQ-009" },
    { "id": "RG-REQ-009-L03", "parentId": "RG-REQ-009" },
    { "id": "RG-REQ-009-L04", "parentId": "RG-REQ-009" },
    { "id": "RG-REQ-009-L05", "parentId": "RG-REQ-009" },
    { "id": "RG-REQ-009-L06", "parentId": "RG-REQ-009" },
    { "id": "RG-REQ-009-L07", "parentId": "RG-REQ-009" },
    { "id": "RG-REQ-009-L08", "parentId": "RG-REQ-009" },
    { "id": "RG-REQ-009-L09", "parentId": "RG-REQ-009" },
    { "id": "RG-REQ-009-L10", "parentId": "RG-REQ-009" },
    { "id": "RG-REQ-009-L11", "parentId": "RG-REQ-009" }
  ]
}
-->
# RG-REQ-009 — Quality signals, trust and reliability tell the agent where the risk is now, with their basis

Status: DRAFT authored by the in-place manager 2026-09-12. Independent requirements review: PERFORMED 2026-09-12 by Codex gpt-5.6-terra (standalone, read-only) — verdict REFINE; its findings are applied in this revision (record: reviews/2026-09-12-requirements-review-0-terra.txt). Human approval: PENDING.
Maturity: PROTOTYPE (requirements record).

## Source and intent

Sources: [VISION — Value Frontier](../VISION.md#value-frontier) item 5 (current-state quality signals, honestly labeled with per-language coverage; trends parked by ratification 2026-07-11); [VISION — Honesty Rules](../VISION.md#honesty-rules); [VISION — Discovery Over Enforcement](../VISION.md#discovery-over-enforcement); the Certainty Model's rules 3 and 5 (outer layers surface unknowns; a signal covering some languages says so wherever it renders); the ratified contracts of TRUST-MODEL-REBASE-1, RELIABILITY-REFRAME-1, RESOLUTION-BREAKDOWN-CLI-1, TRUST-FIRSTPARTY-1, METRIC-LANG-COVERAGE-1, CHURN-SHALLOW-1, DEAD-CAUSES-1, CONTRADICTION-SWEEP-1, HEADLINE-TRUTH-1; the v0.18.0 audit root causes RC-1, RC-5, RC-9.

## High-level requirement

An agent asking `rmap orient`, `trust`, `reliability`, `hotspots`, `churn`, `risk` or `dead` shall be pointed at the repository's current risk concentrations by deterministic measurements whose universe, per-language coverage, basis and degradation are stated on the rendering surface; a reliability verdict shall never contradict the edges another surface renders from the same snapshot, and no number shall be inflated by edges the product did not observe.

**Scope:** measurements (complexity, size, hotspots, churn), the trust/reliability model and its renderers, `dead`'s refusal, `risk`/`coverage`/`assess` inputs. Does not define the module model (RG-REQ-004) or call binding (RG-REQ-005), whose outputs it consumes — but it does constrain what it may count from them.

**High-level acceptance:** every L holds on the smoke corpus and the audit's supplemental probes; a headline that moves DOWN when a fabricated input is removed is the expected outcome, not a regression.

## Low-level requirements

### RG-REQ-009-L01 — Complexity centers rank the repository's own production code

`orient`'s complexity headline and section shall rank symbols after excluding generated, vendored and test files, shall compute the above-threshold count from the filtered set, and shall print the excluded count with the flag that shows everything (`N generated/vendored/test symbols excluded — --include-all`). Generated shall be decided from persisted file facts (a first-lines generated marker — a repository-declared exclusion such as `.gitattributes linguist-generated` is a separate, unratified input and is NOT part of this L), vendored from a path-segment rule that includes `dependencies`, `third_party`, `vendor`, `node_modules`; a file distinguishable by no persisted fact stays ranked.

**Verification criterion:** unit test on the aggregator with generated/vendored/test fixtures; corpus: poco's top-5 contain no `dependencies/` path; codegraph's #1 is not a tree-sitter `parser.c`; repo-graph's top-5 contain no `tests/` file; the exclusion line renders with the true count. The filter shall NOT be applied in the storage read consumed by the LiveGraph complexity certificate.

**Evidence (v0.18.0):** NOT MET — RC-9 (never worked; `files.is_generated` hard-coded false; only filter is `complexity >= 20`). Queue Q7 COMPLEXITY-SCOPE-1. A populated `is_generated` also narrows the seed corpus — a stated behaviour change requiring reindex.

Refinement ratified 2026-09-14 (human, option A): generated/vendored/test are decided by rmap's own path and content heuristics. Reading a repository's own exclusion declarations (`.gitattributes linguist-generated`, ignore files, manifest excludes) is a possible future increment and, if taken, lives in one dedicated ecosystem-declarations module so the per-ecosystem parsers stay organized; it is not part of this L.

### RG-REQ-009-L02 — Trust's module connectivity reads the edge set `modules list` renders

`trust`'s per-module fan-in/fan-out, its "Suspicious Modules (zero connectivity)" list and the `alias_resolution_suspicion` downgrade shall be computed from the same module-dependency edge set `modules list` and `modules deps` render (resolved file→file imports aggregated through module ownership to the module candidate on both endpoints). A module with rendered edges shall never be listed as zero-connectivity.

**Verification criterion:** `trust --json modules[].fan_in/fan_out` equals `modules deps` per module on repo-graph, kafka, hadoop, FRAKTAG; `SELECT` over the store shows rows-with-fan>0 > 0 where `modules list` renders edges; the existing `rules.rs::count_suspicious_modules_matches_all_criteria` and `trust_tests.rs::suspicious_modules_state_basis_and_point_at_stats` are rewritten to the true behaviour.

**Evidence (v0.18.0):** NOT MET — RC-5 (regression 28126a2: exact join on the crate-root directory node; structurally zero on Cargo/Gradle/Maven/TS; repo-graph 48/61, kafka 65/65, hadoop 8/9, FRAKTAG 3/4). Queue Q2 TRUST-MODULE-EDGES-1. Import-graph remains LOW while `unresolved_imports_count > 0` — the fix removes the false reason, not the level.

### RG-REQ-009-L03 — Resolution percentages count only evidence-bound edges

The call-resolution percentage on `trust`, `orient`, `stats`, `check` and `reliability` shall count a CALLS edge only when its binding is supported by evidence (a unique name, or receiver/enclosing-type evidence — RG-REQ-005); a call carrying an indirect receiver shall never be counted as resolved to the caller's own class without receiver-type evidence. A headline that falls when fabricated edges are removed is the required outcome.

**Verification criterion:** corpus assertion `SELECT count(*) FROM edges WHERE type='CALLS' AND source_node_uid=target_node_uid AND metadata_json LIKE '%receiver%'` = 0 on leveldb, OpenXcom, vcmi, poco, duckdb, gstreamer; trust's leveldb "calls resolved" reported before/after with the drop attributed.

**Evidence (v0.18.0):** NOT MET — RC-1 (regression 5c3ec2d; self-loops 0 → 155/497/836/1098/2522/321; leveldb 25.3% → 34.7% with ≥1.6 pts fabricated). Queue Q1.

### RG-REQ-009-L04 — Every reliability axis renders with its basis and exact downgrade reasons

Import-graph, Change-impact and call-resolution axes shall render their level, the triggered downgrades that produced it, and per-section source labels; a reason shall never outlive its cause; the headline posture is the snapshot's (Fresh/Stale/…), never a constant derived from a store that is resident only in development. The envelope invariants hold: `Exact` requires `Complete`; `Partial` lists reasons; `Unavailable` is not empty; `null` is not zero.

**Verification criterion:** `rust/crates/rgr/src/presentation/trust_tests.rs` (`render_shows_reliability_levels`, `render_carries_per_section_source_labels`, `render_drops_the_livegraph_posture_section`, `render_headline_is_the_snapshot_posture`, `stale_snapshot_headline_reads_stale`); `trust/src/rules.rs` trigger tests; `trust/src/coherent_tests.rs` envelope invariants.

**Evidence (v0.18.0):** OBSERVED MET structurally (F4 shipped; 28/28 → 0 constant posture lines); content UNMET via L02's false reason (trust HONESTY C..A- across columns).

### RG-REQ-009-L05 — Per-language measurement coverage is stated wherever a signal renders

Every quality measurement shall state, on the surface that renders it and in a `measurement_coverage` JSON block, which of the repository's languages it covers, derived from the snapshot (functions per language × measured share), never from a hardcoded list; the caveat disappears by itself when a language gains measurements; `unavailable` is an explicit state.

**Verification criterion:** `rust/crates/rgr/tests/metrics_command.rs`, `hotspots_command.rs`, `orient_command.rs`; mixed-language fixture renders the caveat, fully-measured renders none; `classification/src/measurement_coverage.rs`.

**Evidence (v0.18.0):** OBSERVED MET — METRIC-LANG-COVERAGE-1 delivered 2026-07-03; the coverage infrastructure must not be removed as "no longer needed".

### RG-REQ-009-L06 — Resolution rate measures the reader's code from one shared computation

Out-of-scope external calls shall leave the denominator and render as a named external share (library, system, dynamic) with its heuristic basis; real in-scope failure shall still read low; `0-of-0` renders "no in-scope calls measured", never 100%; `orient`, `stats`, `trust`, `check`, `explain` and `reliability` shall consume one shared projection (`CallReliabilityView`) and one caveat vocabulary.

**Verification criterion:** the 14 shared-projection tests in `trust_tests.rs` (`render_resolution_is_reader_frame_in_scope` … `render_names_top_external_targets_in_order_with_heuristic_basis`); no second definition of the rate exists in the tree.

**Evidence (v0.18.0):** OBSERVED MET — RELIABILITY-REFRAME-1 (285e62e).

### RG-REQ-009-L07 — First-party workspace packages are internal, from manifest facts only

Calls into workspace members / declared packages shall be labeled internal and excluded from the external-libraries share with the split stated; classification shall come from parsed manifest facts, never name-prefix heuristics; an unmatched family stays unclassified; the suggested next move is in-repo (`rmap explain <name>`).

**Verification criterion:** `trust_tests.rs` first-party tests incl. the two byte-identity guards for repos without workspace packages.

**Evidence (v0.18.0):** OBSERVED MET — TRUST-FIRSTPARTY-1.

### RG-REQ-009-L08 — `reliability --by-language` / `--by-module` reconciles to the aggregate

The decomposition shall render resolved/unresolved/% per language and per module in human and JSON form, reconcile to the aggregate rate, separate test files, and render UNKNOWN (never 0% or 100%) for a bucket with zero measured calls, using the same caveat vocabulary as `check`/`trust`.

**Verification criterion:** `rust/crates/rgr/src/commands/reliability.rs` flag tests; an aggregate-reconciliation test and a zero-measured UNKNOWN test (named by RESOLUTION-BREAKDOWN-CLI-1 §4; no `rgr/tests/reliability_command.rs` exists — add one before relying on this L).

**Evidence (v0.18.0):** OBSERVED command exists; reconciliation and UNKNOWN cases not independently confirmed — UNKNOWN.

### RG-REQ-009-L09 — History signals say what the history is

`churn` and `hotspots` shall render "history is shallow (N commit(s) available; clone depth, not a quiet window, is why nothing changed)" on shallow or single-commit clones, the HEAD date and a derived `--since` when a real history has nothing in the window, "no history" when there is none, and unknown-with-reason on a git read failure; `hotspots`/`risk` consume the same history fact rather than re-deriving it.

**Verification criterion:** `rust/crates/rgr/src/presentation/churn.rs::render_shallow_reframes_and_still_shows_table`; `rust/crates/rgr/tests/churn_command.rs`, `hotspots_command.rs`.

**Evidence (v0.18.0):** OBSERVED MET as honesty (HONESTY A-); signal value is low on shallow corpus clones by construction.

### RG-REQ-009-L10 — `risk`, `coverage`, `assess` and `doctor` are honest about missing inputs

`risk` shall include only files with both hotspot and coverage data and shall name the absent input with the next action — never degrade to `risk = hotspot`; gate/assess verdicts shall preserve the computed verdict beside the effective one across PASS / FAIL / MISSING_EVIDENCE / UNSUPPORTED / WAIVED (WAIVED is effective-only); `doctor` reports daemon health, never snapshot quality, and a degraded probe renders `[note]`, not `[ok]`, with the count line reflecting it.

**Verification criterion:** `rust/crates/rgr/tests/risk_command.rs`, `assess_command.rs`, `gate_command.rs`; `rgr/src/presentation/gate.rs` computed/effective tests; doctor tone test (AUDIT5-MINORS-1 F5); field: production daemon renders `[note] vector store: degraded … 26 ok · 1 note`.

**Evidence (v0.18.0):** OBSERVED MET as honesty; capability UNMET — `risk` and `gate/assess` HIT = F corpus-wide because no coverage evidence can be imported (NC-1 DEFERRED; RG-REQ-007-L11). The F is an input gap, not a dishonesty; degrading risk to hotspot is forbidden.

### RG-REQ-009-L11 — No trend system

The product shall not render snapshot-to-snapshot quality trends; "what changed" compares the current state against a git baseline; snapshot retention biases to the latest full snapshot plus minimal transient comparison state.

**Verification criterion:** absence of any trend surface in `rust/crates/rgr/src/commands/`; `daemon-runtime/tests/snapshot_retention.rs` pins the retention posture; `docs/architecture/measurement-model.txt`'s health-vector section is an aspiration superseded by the 2026-07-11 ratification and shall not be used as a source.

**Evidence (v0.18.0):** OBSERVED MET (by absence).

## Preservation obligations named by the ratifying specifications

- D-T6 JSON semantics: trust root posture = MEET over the LiveGraph half; only the human render dropped the constant lines (pinned by `trust/src/coherent_tests.rs`).
- `docs/architecture/measurement-model.txt`: measurements carry no confidence and no basis ("they are the evidence"); assessments live in `inferences` and cite measurements; waivers are version-scoped and never erase the computed verdict; `docs/architecture/gate-contract.txt` is normative.
- `CallReliabilityView` and `measurement_coverage` are consumed, never forked.
- Trust ratio/denominator, storage schema, exit codes — additive only.
- TRUST-MODEL-REBASE-1 D1–D3: completeness is query-contextual; `AnswerEnvelope` smart constructors make illegal states unrepresentable.
- The `dead` disable decision (2026-04-27) and its refusal shape are not re-litigated; exit code 4 per `docs/contracts/exit-codes.md` (RG-REQ-013).
- Hotspot scoring formula and commits-first sort (QUANT-MECH-1).
- The LiveGraph complexity no-loss certificate compares the storage read's full set — any scope filter stays agent-side.

## Review and approval

Independent requirements review: PERFORMED (Codex gpt-5.6-terra, REFINE; findings applied).
Human approval: PENDING.
Implementation and verification: evidence lines are the manager's OBSERVED reading of the v0.18.0 audit; they are not acceptance.
