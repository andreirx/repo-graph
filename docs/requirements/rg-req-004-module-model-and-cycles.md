<!-- requirements-assurance-v1
{
  "formatVersion": 1,
  "kind": "requirement",
  "requirementId": "RG-REQ-004",
  "sources": [
    { "kind": "document-section", "path": "docs/VISION.md", "fragment": "value-frontier" },
    { "kind": "document-section", "path": "docs/VISION.md", "fragment": "primary-use-case" },
    { "kind": "document-section", "path": "docs/slices/modules-method-1.md", "fragment": "2-contract" },
    { "kind": "document-section", "path": "docs/slices/cycle-honesty-1.md", "fragment": "2-contract" },
    { "kind": "document-section", "path": "docs/slices/coherence-3.md", "fragment": "2-contract" },
    { "kind": "document-section", "path": "docs/slices/import-resolution-rust-1.md", "fragment": "2-contract" }
  ],
  "lowLevelRequirements": [
    { "id": "RG-REQ-004-L01", "parentId": "RG-REQ-004" },
    { "id": "RG-REQ-004-L02", "parentId": "RG-REQ-004" },
    { "id": "RG-REQ-004-L03", "parentId": "RG-REQ-004" },
    { "id": "RG-REQ-004-L04", "parentId": "RG-REQ-004" },
    { "id": "RG-REQ-004-L05", "parentId": "RG-REQ-004" },
    { "id": "RG-REQ-004-L06", "parentId": "RG-REQ-004" },
    { "id": "RG-REQ-004-L07", "parentId": "RG-REQ-004" },
    { "id": "RG-REQ-004-L08", "parentId": "RG-REQ-004" },
    { "id": "RG-REQ-004-L09", "parentId": "RG-REQ-004" },
    { "id": "RG-REQ-004-L10", "parentId": "RG-REQ-004" },
    { "id": "RG-REQ-004-L11", "parentId": "RG-REQ-004" }
  ]
}
-->
# RG-REQ-004 — Modules are found by a stated method, related by resolved imports, and agree across every surface

Status: DRAFT authored by the in-place manager 2026-09-12. Independent requirements review: PERFORMED 2026-09-12 by Codex gpt-5.6-terra (standalone, read-only) — verdict REFINE; its findings are applied in this revision (record: reviews/2026-09-12-requirements-review-0-terra.txt). Human approval: PENDING.
Maturity: PROTOTYPE (requirements record).

## Source and intent

Sources: [VISION — Value Frontier](../VISION.md#value-frontier) item 1 (modules — declared, operational and inferred; file ownership; inter-module relationships: the primary orientation layer); [VISION — Primary Use Case](../VISION.md#primary-use-case); the normative `docs/architecture/module-graph-contract.txt` and `module-discovery-layers.txt`; MODULE-MODEL-1 (D2 one population — binding 2026-07-11; D4 logical package per toolchain; D5 output is the product; D7 bounded human, complete JSON); MODULES-IDENTITY-2; MODULE-EDGES-1; MODULE-OWNERSHIP-DUPLICATE-1; MODULES-METHOD-1 and the human ruling of 2026-09-07 (no further module discovery; state the method; recommend the repo's docs/tree); CYCLE-HONESTY-1; CYCLES-OUTPUT-CONTRACT-1 (D1 = B: cycles are sets); CYCLES-COMPLETENESS-CERT-1 (no Exact "no cycle" without a Complete certificate); COHERENCE-2/3; TYPE-ONLY-IMPORTS-1; IMPORT-RESOLUTION-RUST-1/JAVA-1; the v0.18.0 root causes RC-5 and RC-10.

## High-level requirement

An agent asking `rmap modules list|deps|files` and `rmap cycles` shall receive the repository's modules with the method that produced them (declared per manifest family, or inferred from directories and said so), the files each owns with a reconciled total, the module dependency edges derived only from resolved file-to-file imports with every unresolved import counted, and the strongly connected components as sets with their size and runtime classification — with one edge computation behind `modules` and `trust` (L01) and one identifier space across `modules list`, its edge list, `cycles` and `stats` (L11, a separate obligation).

**Scope:** Layer 1–2 module model and its surfaces. Import resolution itself is RG-REQ-006; the trust consumer is RG-REQ-009-L02.

**High-level acceptance:** all L entries hold on repo-graph, kafka, grpc-java, hadoop, vcmi, poco, leveldb, FRAKTAG, django, codegraph; the trust↔modules seam (L01) exists.

## Low-level requirements

### RG-REQ-004-L01 — Trust and modules read one module-edge computation

Fan-in/fan-out on `trust`, `modules list` and `modules deps` shall be counted over the same derived module-dependency edge set (resolved file→file IMPORTS aggregated through file ownership to the module candidate on both endpoints); `trust`'s zero-connectivity list and `modules list` can never contradict; module identity is not changed by this obligation (MODULES-IDENTITY-2 freezes it) — only the source of the fan counts.

**Verification criterion:** `modules_list_tests.rs::shared_derivation_matches_orient`; a cross-surface seam test asserting `trust --json modules[].fan_*` == `modules deps` per module (to be added); rows-with-fan>0 > 0 on repo-graph/kafka/hadoop/FRAKTAG; the Import-graph level stays LOW while unresolved imports > 0 — only the false `alias_resolution_suspicion` reason disappears.

**Evidence (v0.18.0):** NOT MET — RC-5 (trust reads a population whose fan counts are structurally zero on Cargo/Gradle/Maven/TS; regression 28126a2). Queue Q2. The stats and grpc identifier reconciliation is a SEPARATE obligation (L11), deliberately not part of Q2.

### RG-REQ-004-L02 — Module dependency edges derive only from resolved, cross-module, file→file imports

For each resolved IMPORTS edge whose source and target files are owned by different modules, one aggregated `ModuleDependencyEdge {importCount, sourceFileCount}`; unresolved and intra-module imports are excluded; derivation is query-time, never persisted.

**Verification criterion:** `modules_list_tests.rs` (`two_crate_fixture_renders_a_to_b_edge_verbatim`, `list_render_edge_list_count_equals_rows`, `list_render_edges_sorted_by_refcount_then_name`); `scripts/compare-module-cycles.sh` (SQLite vs LiveGraph equivalence).

**Evidence (v0.18.0):** OBSERVED MET as a derivation rule.

### RG-REQ-004-L03 — Declared modules are named per manifest family; unparsed families are stated

The method line opens `modules list` and orient's module section from stored facts: `N declared in Cargo.toml (workspace members)` · `N Gradle projects from settings.gradle (M relocated via projectDir)` · `N npm workspaces from package.json` · `N declared in pyproject.toml`; "Maven manifests present but not parsed on this build" when present; mixed repos list each family; unreadable → named unavailable; absent → `method not recorded on this index` and STOP. A logical package is defined per toolchain (Rust = nearest Cargo.toml crate; TS/JS = nearest package.json workspace package; JVM = logical package with `src/main|test/<lang>` merged; C/C++ and manifest-less = directory groups); basename merging across unrelated roots is forbidden.

**Verification criterion:** `modules_list_tests.rs` (`method_line_renders_single_family`, `method_line_renders_mixed_families`, `method_line_renders_maven_diagnostics_degradation`, `unavailable_method_renders_distinct_reason`, `not_recorded_method_renders_canonical_sentence`, `inequality_across_fixtures`); `orient_tests.rs::orient_method_*`.

**Evidence (v0.18.0):** OBSERVED MET (MODULES-METHOD-1: repo-graph 56 declared · 3 inferred; kafka 63 Gradle · 4 inferred, Maven named).

### RG-REQ-004-L04 — Inferred directory modules are stated as inferred, with confidence — and there is no further discovery

Inferred rows render `inferred (0.7)`; an all-inferred repo's method line carries "boundaries are a guess from directory names" and the documentation recommendation renders before the rows; inferred module identities are recomputed per snapshot (no cross-snapshot identity is claimed); no new module-inference algorithm ships (human ruling 2026-09-07); the memo layer is declined.

**Verification criterion:** `modules_list_tests.rs` (`list_render_shows_kind_confidence`, `all_inferred_renders_recommendation_before_rows`, `declared_renders_recommendation_after_rows`); `module_shared.rs::format_kind_confidence_*`.

**Evidence (v0.18.0):** OBSERVED MET (hadoop, vcmi).

### RG-REQ-004-L05 — Σ owned files reconciles to the grouped total, or the non-reconciliation is printed

When Σ owned across modules exceeds the directory-grouped total, the footer names the excess as root-level files only when the store proves that exact count; otherwise it prints the residual and the inequality and points at `rmap check`; either count unknown → no footer; an under-count (Σ owned < grouped) is also stated.

**Verification criterion:** `modules_list_tests.rs` footer tests (`…names_root_level_when_owned_exceeds_grouped`, `…surfaces_residual_when_excess_does_not_match_proven_root_level`, `…django_shape_names_one_root_level`, `modules_list_rendered_file_totals_sum_to_the_check_indexed_basis`); an under-count test (to be added — currently silent).

**Evidence (v0.18.0):** OBSERVED MET for over-count (codegraph's self-reported non-reconciliation is the keep-and-imitate pattern); under-count silent.

### RG-REQ-004-L06 — File ownership is deterministic and a collision degrades honestly

Ownership resolves by longest prefix with kind precedence only as a tie-break; an npm-package claim beats an inferred claim at candidate generation; if double ownership recurs, `modules list/violations` report the defect and the affected files as a labeled degradation, never an InternalError that kills the surface; non-colliding ownership is byte-identical.

**Verification criterion:** `daemon_dispatch.rs::modules_family_duplicate_ownership_degrades_across_all_surfaces`; MODULE-OWNERSHIP-DUPLICATE-1 tests; `module-discovery-layers.txt` §5 rules.

**Evidence (v0.18.0):** OBSERVED MET (vscode 11,878-file incident closed).

### RG-REQ-004-L07 — Cycles are SCCs rendered as sets with size; a walk is drawn only over verified edges

Each cycle is a canonical set (rotated to its lexicographically smallest member; size = unique member count); the count line and each size come from the shared exclusion-aware partition; with intra-SCC edges present, a DFS walk renders a ring and off-walk members `+ N more members in this cycle`; with no, empty or truncated edges, `members (unordered): …` with zero arrows; an Exact "no module cycle" answer is never given without a Complete certificate.

**Verification criterion:** `cycles/walk.rs` (`walk_follows_real_edges_not_member_order`, `no_edges_renders_unordered_with_no_arrows`, `offwalk_members_reported_as_plus_n_more`, `truncated_edges_render_unordered_no_arrows`); `cycles/tests.rs` (`render_shows_cycle_count`, `render_shows_large_cycle_size`, `headline_counts_come_from_the_shared_partition`); `daemon-runtime/tests/cycle_honesty_route_consistency.rs`.

**Evidence (v0.18.0):** OBSERVED MET on `cycles` and `orient` (the best-graded surface); NOT MET on `explain` (RG-REQ-003-L01).

### RG-REQ-004-L08 — Type-only classification is SCC-wide, three-state, identical on `cycles` and `orient`

`BreaksAtRuntime {k of n}` only when removing the SCC's type-only edges leaves no directed cycle in the runtime subgraph; otherwise `HasRuntimeEdges` with the type-only count; `TypeOnly`; `Unknown {reason}` visible and never demoted to runtime; one shared label function; TS/JS only (other languages carry no label).

**Verification criterion:** `cycles/tests.rs` (`type_only_cycle_is_labeled_vanishes_at_runtime`, `has_runtime_edges_missing_counts_fails_to_decode_not_defaults_to_zero`, `mixed_scc_with_surviving_runtime_cycle_states_the_runtime_truth`, `breaks_at_runtime_cycle_states_no_runtime_cycle_remains`, `unknown_cycles_narrow_the_caveat_and_name_the_count`).

**Evidence (v0.18.0):** OBSERVED MET (FRAKTAG BreaksAtRuntime 3 of 10).

### RG-REQ-004-L09 — Every module-edge zero-state carries resolved + unresolved counts

An empty edge array renders `No cross-module dependencies detected` with the denominators (`… N imports did not resolve to a file`), suppressed when unresolved = 0; an absent edges field renders `Cross-module edge list unavailable` with the reason and labels any rollup figure as a rough estimate; no repo wears another repo's gap sentence.

**Verification criterion:** `modules_list_tests.rs` (`list_render_empty_edges_is_zero_state`, `list_render_absent_edges_field_is_unavailable_with_reason`, `list_render_unresolved_read_failure_does_not_blame_older_daemon`); `cycles/tests.rs` (`zero_state_states_module_and_edge_counts`, `zero_state_names_empty_graph_when_no_resolved_edges`).

**Evidence (v0.18.0):** OBSERVED MET (keep-and-imitate); poco's zero-state is faithful to a never-resolved include graph (RG-REQ-006-L03).

### RG-REQ-004-L10 — Bounded human output, complete JSON

`modules list` renders a bounded top-N by file count with lexicographic tie-break and an explicit omission line; headline counts count ALL groups, never only the displayed ones; JSON is complete; display names are collision-safe after prefix collapse; scale is accepted past 100 groups.

**Verification criterion:** MODULE-MODEL-1 D7 dogfood arithmetic (20+261, 50+231 = 281, 50+267 = 317); `modules_list_tests.rs` (`list_render_edges_budget_and_full`, `list_render_disambiguates_twin_names_by_manifest`, `list_render_twin_same_manifest_falls_back_to_path`).

**Evidence (v0.18.0):** OBSERVED MET.

### RG-REQ-004-L11 — One module identifier space across `modules list`, its edge list, `cycles` and `stats`

An edge list and the table above it shall use one identifier space (grpc `grpc-core`, never `core`/`.`); `stats`' module rows shall be reconciled with `modules list`'s population or state their differing population in the user's term ("directory groups"); `cycles` names its population likewise. This obligation is separate from L01 and is not part of Q2.

**Verification criterion:** a render test asserting edge-list keys equal table keys on the grpc fixture (to be added); a `stats`↔`modules list` population statement test (to be added); field: `ir-grpc-modules` shows `grpc-core → grpc-api`, not `core → api`.

**Evidence (v0.18.0):** NOT MET — D-N10 (grpc identifier spaces); `stats`' module rows come from `queries.rs::compute_module_stats` (the directory-node population), a fourth population RC-5's fix does not close.

## Preservation obligations named by the ratifying specifications

- `module-graph-contract.txt` (NORMATIVE): derivation from resolved IMPORTS × ownership only; cross-module only; query-time, no persistence; unresolved belongs to diagnostics/trust; boundary declarations target discovered candidates.
- `module-discovery-layers.txt` (NORMATIVE): confidence ladder declared > operational > inferred; longest-prefix ownership; no ownership leakage beyond a promoted root; pre-persistence dedup; roots from `surface.rootPath` exactly — no heuristic climbing.
- MODULE-MODEL-1 D2 (one population behind package groups: the indexed-file set behind the per-directory MODULE nodes/OWNS edges — orient and stats derive from the same read), D4 (logical package per toolchain), D5 (output is the product; only the governance surface is frozen), D7 (bounded human, complete JSON).
- MODULES-IDENTITY-2 §3: module identity computation is frozen — rendering disambiguates; identity does not change; RC-5's fix re-sources the fan counts, never the identity.
- CYCLE-HONESTY-1 §3: Tarjan/SCC semantics; CYCLES-OUTPUT-CONTRACT-1 D1=B (sets; the additive `edges` divergence between backends is a ratified, accepted divergence — certified fields byte-identical); the CYCLES-B byte-parity certificate.
- TYPE-ONLY-IMPORTS-1 §3: no `unwrap_or` on the fact read; additive migration only.
- MODULES-METHOD-1 §3: no new inference; an unnameable method → `method not recorded on this index` + STOP.
- IMPORT-RESOLUTION-RUST-1 §4: the cycles-over-directory-groups vs modules-over-declared-modules divergence is a recorded follow-up, named in the user's term.

## Review and approval

Independent requirements review: PERFORMED (Codex gpt-5.6-terra, REFINE; findings applied).
Human approval: PENDING.
Implementation and verification: evidence lines are the manager's OBSERVED reading; they are not acceptance.
