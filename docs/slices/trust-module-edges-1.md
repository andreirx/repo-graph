# TRUST-MODULE-EDGES-1 — trust's module connectivity reads the edge set `modules list` renders

Status: SPECIFIED (2026-09-12) · Track: audit round six, Q2 (CRITICAL; REGRESSION from ORIENT-BUG-1 28126a2, 2026-05-22). CODE slice: one SQL statement in `storage/src/trust_impl.rs::compute_module_stats` + its tests + one cross-surface seam test. Maturity: MATURE (`trust`, `orient` reliability, `assess`). Builder: Codex gpt-5.6-sol; reviewer: Codex gpt-5.6-terra.

## 0. Requirements allocation

**Implements:** RG-REQ-009-L02 (trust's module connectivity reads the edge set `modules list` renders), RG-REQ-004-L01 (trust and modules read one module-edge computation), RG-REQ-002-L02 (the trust↔modules coherence seam), RG-REQ-009-L04 (a reason never outlives its cause — the false `alias_resolution_suspicion` reason goes).

**Changes (pre-authorised):** `trust` on repo-graph loses its "Suspicious Modules (zero connectivity)" section (48 → 0) and the `alias_resolution_suspicion` downgrade; the Import-graph line moves from `LOW (alias resolution suspected; 4903 unresolved imports)` to `LOW (4903 unresolved imports)` and Change-impact from `LOW (alias resolution suspected)` to `LOW (import_graph_reliability_low)` — the LEVEL does not change while unresolved imports > 0 (`rules.rs:215-243`); kafka/hadoop/FRAKTAG lose their false lists likewise; `orient`'s reliability caveat and `assess` inherit the wording.

**Explicitly NOT in scope (separate obligation RG-REQ-004-L11):** `stats`' module rows (`queries.rs::compute_module_stats`, the directory-node population) and the grpc table/edge-list identifier space. Module IDENTITY does not change (MODULES-IDENTITY-2 §3); only the source of the fan counts.

**Preserves (§3):** RG-REQ-004-L02/L03/L04/L09/L10 (modules list/deps unchanged), RG-REQ-009-L03/L06/L07 (the other trust computations), RG-REQ-002-L08 (reader-frame labels), RG-REQ-003-L03 (file universe), RG-REQ-001-L03 (unresolved rows untouched), RG-REQ-012-L01 (exit codes), RG-REQ-011-L06 (isolation), D-T6 JSON semantics, the envelope invariants of RG-REQ-009-L04 other than the downgrade-reason lines this slice changes, `dead`'s overlay. RG-REQ-009-L04 is IMPLEMENTED (its reason lines move), not preserved.

## 1. Problem (ROOT-CAUSED — RC-5, measured on the retained stores)

`trust` lists `rust/crates/agent`, `boundary-interaction`, `contract-schema` (48 of repo-graph's 61 module candidates) as zero-connectivity on the snapshot where `modules list` renders 129 edges among them; kafka 65/65, hadoop 8/9, FRAKTAG 3/4; rows with any fan > 0 = **0** on every Cargo/Gradle/Maven/TS layout. `trust_impl.rs:755-822 compute_module_stats` rows FROM `module_candidates` but takes fan_in/fan_out from MODULE→MODULE IMPORTS edges between per-DIRECTORY nodes joined by `m.qualified_name = mc.canonical_root_path` — the crate ROOT (`rust/crates/agent`, owns 0 files) while the edges attach to the LEAF directory (`rust/crates/agent/src`, fan_in 14). The join misses, COALESCE yields 0, `rules.rs:392` fires on `file_count >= 2`, and `alias_resolution_suspicion` downgrades Import-graph and Change-impact. Introduced by 28126a2 (ORIENT-BUG-1 moved the row source and left the fan subqueries keyed on directory nodes); CONTRADICTION-SWEEP-1 later added the "cross-check stats" basis line — a wording patch that names the wrong identity out loud. `modules list` reads `derive_module_dependency_edges` over `module_file_ownership` + resolved file→file IMPORTS keyed by `canonical_root_path` — the correct set.

## 2. Contract

1. **One edge computation.** `compute_module_stats` computes fan_in = COUNT(DISTINCT source module candidate) and fan_out = COUNT(DISTINCT target module candidate) over resolved file→file IMPORTS edges whose endpoints are attributed through `module_file_ownership` to different module candidates — the same set `derive_module_dependency_edges` renders — and drops the `nodes kind='MODULE'` join. A prefix-`LIKE` bridge is REJECTED (double-counts nested candidates; wrong population). `file_count` stays from ownership.
2. **The seam.** A test asserts, on the two-crate fixture and on the twin-names fixture, that `trust --json modules[].fan_in/fan_out` equal `modules deps`' per-module counts; `rules.rs::count_suspicious_modules_matches_all_criteria` and `trust_tests.rs::suspicious_modules_state_basis_and_point_at_stats` are rewritten to the true behaviour (a module with rendered edges is never suspicious); the "cross-check stats" basis sentence is replaced by a true one or removed.
3. **Honest verdict movement.** The packet pre-states: Import-graph stays LOW wherever `unresolved_imports_count > 0`; only the false REASON disappears. A repo with zero unresolved imports may go HIGH — say so if observed, never claim it otherwise.
4. **Outward proof on repo-graph (isolated copy of the retained store or an isolated index; `RMAP_TRANSPORT=stdio`):** `trust` shows no "Suspicious Modules (zero connectivity)" section; `Triggered Downgrades` no longer lists `alias_resolution_suspicion`; `trust --json modules[]` fans match `modules deps` for `rust/crates/agent` (fan_out ≥ 1 → gate; fan_in ≥ 2 from daemon-runtime/storage); the Import-graph and Change-impact lines read exactly as §0 Changes; the store assertion rows-with-fan>0 > 0. Cross-repo: the same SQL on copies of kafka, hadoop, FRAKTAG shows rows-with-fan>0 > 0; vcmi's 11 connected rows remain connected.

## 3. Regression watch

| Preserved L | What would regress | Proof |
|---|---|---|
| RG-REQ-004-L02/L03/L04/L09/L10 | `modules list`, `modules deps`, `modules violations` byte-identical (they already read the correct set) | `modules_list_tests.rs` all green; human outputs of `modules list`/`deps` on repo-graph byte-identical before/after (worktree before-binary, same isolated store) |
| RG-REQ-009-L03/L06/L07 | call-resolution %, reader-frame resolution, first-party classification untouched | `trust_tests.rs` resolution and first-party families green; trust's "calls resolved" line byte-identical before/after |
| RG-REQ-009-L04 | envelope invariants; snapshot headline posture | `trust/src/coherent_tests.rs` (all); `trust_tests.rs::render_headline_is_the_snapshot_posture` |
| D-T6 | root posture MEET in JSON unchanged | `coherent_tests.rs::root_never_exceeds_the_weakest_leaf` and siblings |
| RG-REQ-002-L08 | no internal diagnostic leaks into the new basis sentence | review of the replaced sentence against the reader-frame rule; `render_omits_raw_pipeline_diagnostic_sections_from_human_surface` |
| `dead` overlay | `unresolved_import_pressure` unchanged (level stays LOW) | `trust/src/overlay.rs` tests; `dead_command.rs` substrate pins |
| `orient`/`assess`/`stats` reliability axes | inherit the wording change only | `orient_reliability*` tests green; `stats` module rows UNCHANGED (L11 is separate — assert `stats` human output byte-identical) |
| RG-REQ-003-L03 | file universe untouched | `header_renders_indexed_source_tracked_only_split` |
| RG-REQ-012-L01 | exit codes | `exit_code_contract.rs` |
| RG-REQ-011-L06 | isolation | throwaway roots; registry sha256 unchanged |

## 4. Stop conditions

Frozen: module identity computation (MODULES-IDENTITY-2), `module-graph-contract.txt` (query-time derivation, no persistence), storage schema, wire protocol (JSON fields additive), D-T6, exit codes, `stats`' module population (out of scope — RG-REQ-004-L11). If the derived edge set cannot be computed inside `compute_module_stats`'s SQL without persisting module edges, STOP + DECISION_REQUIRED (the contract forbids persistence). If any verdict LEVEL changes on the four proof repos, report it as a finding — do not adjust rules to hold it. STANDING HONESTY RULES. Unmet DoD → STOP + DECISION_REQUIRED. Do NOT commit.

## 5. Validation (SYNCHRONOUS; ORDERED; `build-progress.md` after EACH step)

1. Failing tests FIRST: the seam test (two-crate + twin-names fixtures), the two rewritten tests.
2. The SQL change; chunked gates `cargo test -p repo-graph-storage`, `-p repo-graph-trust`, `-p repo-graph-rgr --lib presentation::trust_tests`, `-p repo-graph-agent`.
3. Outward proof (§2.4) on a COPY of the retained repo-graph store (`~/repo-graph-retained/audit-v0.18.0`, copy it — never open the original) served with `RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off`, before-binary from a `git worktree`; cross-repo SQL on copies of kafka/hadoop/FRAKTAG/vcmi stores.
4. Byte-stability proofs (§3: modules list/deps, stats, calls-resolved line).
5. Cleanup; `build-N.md` with evidence labels.

## 6. Definition of done

§2.4 outputs as stated on repo-graph; the seam test exists and is green; the two defect-pinning tests rewritten; rows-with-fan>0 > 0 on all four proof repos; every §3 row EXECUTED green; the verdict-level statement (LOW stays) recorded; gates green.

CORPUS PATHS: repo-graph is THIS repo; kafka, hadoop at ../legacy-codebases/<name>; FRAKTAG at ../FRAKTAG; retained stores under ~/repo-graph-retained/audit-v0.18.0 (copies only).
