# DOCS-DISCOVERY-1 — documentation is found where the authors put it

Status: SPECIFIED (2026-09-12) · Track: audit round six, Q8 (MEDIUM; never worked). DEPENDS ON HUMAN RATIFICATION of RG-REQ-008-L01 (PROPOSED — it unfreezes DOCS-LIST-2 §3's discovery freeze). CODE slice: `doc-facts/src/discovery.rs`, `doc-facts/src/classification.rs`, `daemon-runtime/src/modules_method.rs`, `rgr/src/presentation/docs.rs`. Query-time only; no reindex. Builder: Codex gpt-5.6-sol; reviewer: Codex gpt-5.6-terra.

## 0. Requirements allocation

**Implements:** RG-REQ-008-L01 (scope: one stem list × one extension list × one doc-tree rule incl. `src/site/**`), RG-REQ-008-L02 (readme by stem), RG-REQ-008-L06 (unscanned count stated), RG-REQ-008-L07 (recommendation derived from the same inventory and the same stem constant), RG-REQ-003-L09 (orient's recommendation line).

**Changes (measured in RC-8):** hadoop `docs list` 23 → ~545 (+520 `src/site/markdown`, +2 root stems), `By kind` gains `doc`, `modules list` prints `read: README.txt`; django 670 → ~674, readme 4 → 6, recommendation `read: README.rst, CONTRIBUTING.rst, docs/`; buildroot 4 → ~77 (`.adoc`); the header gains `+N markdown/prose files outside a docs tree not scanned`.

**Preserves (§3):** RG-REQ-008-L03/L04/L05 (generated/vendored/unreadable handling), RG-REQ-008-L08 (budget, JSON completeness, extract), RG-REQ-008-L09 (read-only), byte-stability on django/kafka/grpc-java/langchain4j/vscode/storybook/gstreamer/repo-graph for the existing counts (prose under `src/site` = 0; `.adoc` in doc trees = 0; root names already matched), F1's kind rules, the `.env*` exclusion.

## 1. Problem (ROOT-CAUSED — RC-8)

`doc-facts/src/discovery.rs:120-160 is_doc_candidate` admits a file only by (A) seven exact names (`README.md`, `README`, `ARCHITECTURE.md`, `CONTRIBUTING.md`, `CHANGELOG.md`, compose files) or (B) extension in `{.md,.txt,.rst}` under an ancestor literally `docs`/`doc`/`design`; (D) `classification.rs:50` decides `readme` by exact `readme.md`/`readme`; (E) `modules_method.rs:447-457` re-spells the orientation stems. hadoop's 520 guides under `hadoop-*/src/site/markdown/`, root `README.txt`, django's `README.rst`/`CONTRIBUTING.rst`, buildroot's 73 `.adoc` manual files all fall outside. Never worked; AUDIT5-MINORS-1 F1 changed kinds only.

## 2. Contract

1. `DOC_NAME_STEMS = {readme, contributing, changelog, architecture, design, overview, install, building, authors, news}` matched on the stem with an optional documentation extension or none; ONE constant read by discovery, by `classification.rs` (`readme` = stem `readme`) and by `modules_method.rs` (keeps its depth ≤ 1 bound).
2. `DOC_EXTENSIONS += .adoc, .markdown`; `in_docs_tree` additionally opens on the `src/site/**` convention (component `site` whose parent is `src`), whole-component matching as `classification.rs:110-119` does.
3. The header states what the rule still refuses: `+N markdown/prose files outside a docs tree not scanned (--json for the rule)` beside the generated/vendored lines.
4. Outward proof: the §0 numbers on hadoop, django, buildroot; eight other corpora byte-stable; `modules list`/`orient` recommendations as stated; the recommendation line still differs across repos (`inequality_across_fixtures`).

## 3. Regression watch

| Preserved L | What would regress | Proof |
|---|---|---|
| RG-REQ-008-L02 (F1) | config-extension precedence; architecture name/dir-only; license name-only | `classification.rs` tests `config_extension_beats_docs_directory`, `plain_prose_outside_docs_tree_is_neutral_doc_not_architecture`, `is_license_document_by_name_only`; repo-graph `architecture 28` unchanged |
| RG-REQ-008-L03 | generated maps excluded by marker | `docs_tests.rs::list_default_excludes_generated_maps_and_states_the_count`; `self_generated.rs` tests |
| RG-REQ-008-L04/L05 | vendored and unreadable handling | `vendored_demoted_from_headline_and_listing_but_stated`; `list_render_surfaces_unreadable_count` |
| RG-REQ-008-L08 | budget + complete JSON; `docs extract` | `entry_list_budgeted_with_remainder`; `cli_out_5_inventory.rs::docs_extract_*` |
| RG-REQ-011-L07 | `.env*` never a document; self-exhaust excluded | `lib_tests::env_files_excluded_from_inventory`; `env_files_still_candidates_for_extraction` |
| Byte-stability | eight corpora | `docs list` counts identical on isolated runs of repo-graph, FRAKTAG, leveldb (small) — others operator-run |
| Fixture pull-in | `.txt` under `doc/` must not admit fixtures silently | the unscanned/admitted counts render; a fixture test with `doc/fixtures/x.txt` states the admission |
| RG-REQ-003-L09 | orient renders the same line as modules | orient-side assertion added (was missing) |

## 4. Stop conditions

Frozen: F1's kind bases; the generated-marker rule; budget/JSON contract; `MAX_DEPTH` (not the cause). If the widened scope admits more than 10% non-prose files on any proof corpus, STOP and report the list — do not add exclusion heuristics. This packet is NOT launched until RG-REQ-008-L01 is ratified. STANDING HONESTY RULES. Do NOT commit.

## 5. Validation (ORDERED)

1. Failing tests FIRST: stem rule (`README.txt`, `README.rst`, `CONTRIBUTING.rst`, `INSTALL`); `.adoc`; `src/site/markdown/x.md`; readme-by-stem classification; recommendation reads the shared constant; the unscanned line.
2. `cargo test -p repo-graph-doc-facts`, `-p repo-graph-daemon-runtime --lib modules_method`, `-p repo-graph-rgr --lib presentation::docs`.
3. Live proof (query-time, no reindex): `docs list`/`modules list` on hadoop, django, buildroot from retained store copies served with enrichment/retention off; byte-stability on repo-graph/FRAKTAG/leveldb.
4. `build-N.md`.

## 6. Definition of done

§2.4 holds; §3 green; gates green.

CORPUS PATHS: hadoop, django, buildroot, leveldb at ../legacy-codebases/<name>; FRAKTAG at ../FRAKTAG; retained copies under ~/repo-graph-retained/audit-v0.18.0.
