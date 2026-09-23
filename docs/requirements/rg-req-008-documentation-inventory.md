<!-- requirements-assurance-v1
{
  "formatVersion": 1,
  "kind": "requirement",
  "requirementId": "RG-REQ-008",
  "sources": [
    { "kind": "document-section", "path": "docs/VISION.md", "fragment": "value-frontier" },
    { "kind": "document-section", "path": "docs/VISION.md", "fragment": "primary-use-case" },
    { "kind": "document-section", "path": "docs/slices/docs-list-2.md", "fragment": "2-contract" },
    { "kind": "document-section", "path": "docs/slices/audit5-minors-1.md", "fragment": "2-contract" },
    { "kind": "document-section", "path": "docs/slices/self-pollution-1.md", "fragment": "2-contract" },
    { "kind": "document-section", "path": "docs/slices/modules-method-1.md", "fragment": "2-contract" }
  ],
  "lowLevelRequirements": [
    { "id": "RG-REQ-008-L01", "parentId": "RG-REQ-008" },
    { "id": "RG-REQ-008-L02", "parentId": "RG-REQ-008" },
    { "id": "RG-REQ-008-L03", "parentId": "RG-REQ-008" },
    { "id": "RG-REQ-008-L04", "parentId": "RG-REQ-008" },
    { "id": "RG-REQ-008-L05", "parentId": "RG-REQ-008" },
    { "id": "RG-REQ-008-L06", "parentId": "RG-REQ-008" },
    { "id": "RG-REQ-008-L07", "parentId": "RG-REQ-008" },
    { "id": "RG-REQ-008-L08", "parentId": "RG-REQ-008" },
    { "id": "RG-REQ-008-L09", "parentId": "RG-REQ-008" }
  ]
}
-->
# RG-REQ-008 — Documentation is found where the authors put it, classified by decidable rules, and never authored by the tool

Status: DRAFT authored by the in-place manager 2026-09-12. Independent requirements review: PERFORMED 2026-09-12 by Codex gpt-5.6-terra (standalone, read-only) — verdict REFINE; its findings are applied in this revision (record: reviews/2026-09-12-requirements-review-0-terra.txt). Human approval: PENDING.
Maturity: PROTOTYPE (requirements record).

## Source and intent

Sources: [VISION — Value Frontier](../VISION.md#value-frontier) item 4 ("documentation as first-class evidence — the docs themselves are the data; repo-graph finds what exists, what is missing and what likely drifted; it is not a documentation authoring system"); [VISION — Primary Use Case](../VISION.md#primary-use-case) step 2 (the agent repairs docs in the target repo); the ratified contracts of DOCS-LIST-2, AUDIT5-MINORS-1 F1 (amended twice), SELF-POLLUTION-1, FIXTURE-POLLUTION-1, MODULES-METHOD-1 §2.2–2.3, CLI-OUT-5; the v0.18.0 root cause RC-8 (no ratified spec exists for discovery scope — this H is authored against the audit and names that).

## High-level requirement

An agent asking `rmap docs list` shall receive the repository's documentation as its authors laid it out — README and orientation files at any root by stem, prose under documentation trees including the `src/site` convention, in Markdown, reStructuredText, plain text and AsciiDoc — classified by decidable name and directory rules, with generated, vendored, unreadable and unscanned documents excluded or admitted out loud and counted; and the orientation recommendation on `modules`/`orient` shall be derived from that same inventory.

**Scope:** docs discovery, classification, rendering and the recommendation line. Query-time only; no store, no reindex.

**High-level acceptance:** hadoop, django and buildroot discovery counts move as measured in RC-8 with eight other corpora byte-stable; the renderer covers every kind.

## Low-level requirements

### RG-REQ-008-L01 — Discovery scope is one stem list × one extension list × one doc-tree rule

STATUS: RATIFIED 2026-09-14 (human, option A: the rule as proposed replaces DOCS-LIST-2 §3's discovery freeze; authored from the round-six root cause RC-8). Queue Q8 DOCS-DISCOVERY-1 is unblocked. A file is a document if its stem is in `{readme, contributing, changelog, architecture, design, overview, install, building, authors, news}` (with or without a documentation extension), or its extension is in `{.md, .markdown, .txt, .rst, .adoc}` inside a doc tree, where a doc tree is an ancestor component `docs`/`doc`/`design` or the `src/site/**` convention (component `site` whose parent is `src`); matching is on whole path components, and both the stem and the extension are compared case-insensitively — the stem being the name minus a recognised documentation extension, or the whole name when none is recognised — so `README.TXT`, `Readme.md`, `README.MD` are documents and `README.html` is not (ratified 2026-09-22 by the human, D-DD1-003).

**Verification criterion:** `doc-facts/src/discovery.rs` tests (existing: `discovers_readme_at_root`, `discovers_docs_directory_markdown`, `admits_txt_and_rst_under_docs_tree`, `skips_node_modules`; to add: stem rule, `.adoc`, `src/site`); field (CC-8, measured 2026-09-21 by walking all 17 local checkouts with the ratified rule; revised 2026-09-22 for the case-insensitive matching the human ratified, D-DD1-003 — headline counts exclude vendored and generated): MOVING — hadoop 22 → 556 (519 `src/site/markdown` guides, root `README.txt`/`BUILDING.txt`, 9 module `README.txt`, GNU-style `AUTHORS`/`ChangeLog`/`NEWS`; +1 vendored), django 616 → 622 (`README.rst`, `CONTRIBUTING.rst`, `AUTHORS`, `INSTALL`, `tests/README.rst`, `extras/README.TXT`), buildroot 4 → 310 (73 `.adoc` manual files under `docs/`, 233 `board/**/readme.txt`), leveldb 7 → 9 (`AUTHORS`, `NEWS`), grpc-java 49 → 50 (`AUTHORS`), repo-graph 657 → 658 (`agent_docs/architecture.md`), langchain4j +1 (`langchain4j-milvus/README.MD`), gstreamer +8, vscode +3, OpenXcom +5, poco +2, FRAKTAG headline unchanged with +6 vendored (all under `packages/engine/fraktag-env/.../site-packages`); BYTE-STABLE — kafka, storybook, codegraph, spring-petclinic, mempalace. Known residual of the ratified stem list: a bare `install` stem admits Debian packaging's `debian/install` (OpenXcom `install/debian/install`, a path list, not prose — 1 file in the 17 corpora).

**Evidence (v0.18.0):** NOT MET — RC-8 (never worked: seven exact names, three extensions, three tree names). Queue Q8.

### RG-REQ-008-L02 — Kinds are decided by name and directory rules

A config extension beats any path rule; `architecture` only by explicit name (`ARCHITECTURE*`, `DESIGN*`, `OVERVIEW*`) or an architectural directory (`design/`, `docs/architecture*`, `docs/design*`); every other doc-tree prose file is the neutral `doc`; `license` by file name only (`LICENSE*`, `COPYING*`, `NOTICE*`) — a license header never upgrades a document; `release-notes` by a release-named ancestor with an index manifest; the `readme` kind is decided by stem so `README.rst`/`README.txt` classify.

**Verification criterion:** `doc-facts/src/classification.rs` tests (`config_extension_beats_docs_directory`, `plain_prose_outside_docs_tree_is_neutral_doc_not_architecture`, `is_license_document_by_name_only`); `lib_tests::inventory_license_is_name_only_content_body_is_not_upgraded`; a readme-stem test (to be added); renderer coverage for `config`/`license`/`doc` kinds (to be added — only `readme`/`architecture` are exercised).

**Evidence (v0.18.0):** PARTIALLY MET — F1's three bases hold (repo-graph architecture 590 → 28; vscode `docker-compose.yaml → config`); the readme stem rule is NOT MET (RC-8 set D) and the renderer covers only `readme`/`architecture`.

### RG-REQ-008-L03 — Generated documents (rmap's own maps) are excluded by default and counted out loud

Exclusion is evidence-based — the first-line generated marker on name-matched sidecars — and renders `N generated maps excluded (tool-generated map summaries; use --include-generated to show)`; an authored `MAP.md` without the marker is never excluded; when every document is generated the surface says so rather than claiming no documentation.

**Verification criterion:** `docs_tests.rs` (`list_default_excludes_generated_maps_and_states_the_count`, `list_include_generated_shows_maps_and_no_exclusion_line`, `list_all_generated_does_not_claim_no_docs`, `json_default_excludes_generated_and_reports_excluded_count`); `self_generated.rs` tests (`marker_confirms_generated`, `markerless_user_map_is_not_self_generated`).

**Evidence (v0.18.0):** OBSERVED MET.

### RG-REQ-008-L04 — Vendored documents are excluded from the headline and counted out loud

Using the index's one vendored-path predicate (never a second definition), the line reads `+N vendored docs (excluded; third-party/dependency directories — see --json for the full set)`; vendored takes precedence over every kind; an all-vendored repo does not claim no documentation; `--json` stays complete.

**Verification criterion:** `docs_tests.rs` (`vendored_demoted_from_headline_and_listing_but_stated`, `all_vendored_does_not_claim_no_docs`); `docs_list_overlay::tests::vendored_override_clears_release_family`.

**Evidence (v0.18.0):** OBSERVED MET; widening the vendored list (RG-REQ-001-L08) changes these counts.

### RG-REQ-008-L05 — Unreadable documents are admitted, counted and never asserted

`+N unreadable, counted (content unreadable — kind refinement unverifiable)`; a license-named unreadable file stays `license` by name and is counted; nothing is silently dropped or silently asserted authored; a document whose bytes could not be read at all carries `content_hash: null` in JSON (L08); a document whose bytes were read but do not decode as UTF-8 carries the hash of its bytes.

**Verification criterion:** `docs_tests.rs::list_render_surfaces_unreadable_count`; `lib_tests::inventory_unreadable_sidecar_is_admitted_and_counted_never_asserted_generated`, `::inventory_unreadable_license_named_doc_is_license_by_name_and_still_counted`; to be added by DOCS-UNREADABLE-DECODE-1: a doc-facts test that a document whose bytes are not UTF-8 is admitted with the hash of its bytes and counted unreadable, and that a readable document's hash is unchanged by the byte read.

**Evidence (v0.18.0):** OBSERVED MET.

### RG-REQ-008-L06 — What remains unscanned is stated

Beside the generated and vendored lines, the header names the prose the discovery rule refused (`+N markdown/prose files outside a docs tree not scanned (--json for the rule)`), so a widened scope counts its own refusals.

**Verification criterion:** a renderer test for the unscanned line (to be added — the string does not exist; only `N files scanned` renders).

**Evidence (v0.18.0):** NOT MET — RC-8.

### RG-REQ-008-L07 — The orientation recommendation is derived from the same inventory

`modules list` and `orient` render one line naming the repo's top orientation documents from the classified inventory (root README by stem, `ARCHITECTURE`/`DESIGN`/`OVERVIEW`/`CONTRIBUTING`, the `docs/` and `design/` directories), at most 4 paths, root first, vendored excluded: `For module boundaries as the authors describe them, read: README.md, docs/ — or the tree.`; with none, `No README or architecture doc found — the tree is the best orientation`; a read failure renders a named unavailable reason; the stem list is the same constant discovery and classification use; the line never reads the same on every repo.

**Verification criterion:** `modules_list_tests.rs` (`orientation_recommendation_renders`, `orientation_no_docs_recommendation_renders`, `unavailable_orientation_docs_renders_reason`, `inequality_across_fixtures`); field: hadoop prints `read: README.txt`; django `read: README.rst, CONTRIBUTING.rst, docs/`.

**Evidence (v0.18.0):** PARTIALLY MET — correct on 26/29 repos; blind to root `README.rst`/`.txt` (RC-8 set E).

### RG-REQ-008-L08 — `docs list` is budgeted in human form and complete in JSON; `docs extract` states its scan

`docs list` human output caps entries with `(+N more — --full)` while `--json` carries every entry with `path, kind, generated, content_hash` — `content_hash` is always present and is `null` when the document's bytes could not be read (an unknown is marked, never omitted, never fabricated — RG-REQ-002-L11; amended 2026-09-23, CC-10, D-DU-CC10) — plus `count`, `counts_by_kind`, `generated_count`; `docs extract` renders header, files scanned, files by kind, facts extracted/inserted/deleted, generated count and warnings (or an explicit no-warnings line).

**Verification criterion:** `docs_tests.rs` (`entry_list_budgeted_with_remainder`, `family_lines_and_entries_share_one_budget`, `extract_render_shows_*`); `rgr/tests/cli_out_5_inventory.rs::docs_extract_*`; to be added by DOCS-UNREADABLE-DECODE-1: a daemon-side serialization test that an entry whose bytes could not be read serializes `"content_hash": null` (the key present) and a non-UTF-8 readable entry serializes its byte hash; a CLI decode test that `DocEntry` accepts both a string and `null` and that the filtered `--json` view re-emits the key in both cases.

**Evidence (v0.18.0):** OBSERVED MET (docs list ECONOMY A on every class).

### RG-REQ-008-L09 — repo-graph is not a documentation authoring system

Every docs surface is a read-only, query-time inventory (no store read, no reindex); the only files rmap writes into a target tree are its own marked `MAP.md` sidecars via an explicit `map` run, which the inventory excludes and counts; every script or smoke invokes `map --dry-run`.

**Verification criterion:** the usefulness protocol's VISION-alignment gate; `scripts/smoke-validation-repos.sh` uses `map --dry-run` (fixed after bare `map` wrote 79,325 sidecars across the corpus); `.env*` files are excluded from the inventory while still available to the extractor (`lib_tests::env_files_excluded_from_inventory`, `discovery::tests::env_files_still_candidates_for_extraction`).

**Evidence (v0.18.0):** OBSERVED MET, with the sidecar incident as the recorded counter-example.

## Preservation obligations named by the ratifying specifications

- DOCS-LIST-2 §3: extraction/inventory computation frozen except where RC-8/Q8 explicitly unfreezes discovery; a kind assignment needs a stated deterministic basis and unknown keeps the old kind; new public APIs beyond additive DTO fields are DECISION_REQUIRED; `docs list --json` stays complete.
- AUDIT5-MINORS-1 F1: config-extension precedence; architecture by explicit name or architectural directory; license by file name only (the content-marker path was removed, not deprecated).
- SELF-POLLUTION-1 / FIXTURE-POLLUTION-1: the generated marker is the evidence, the name a candidate; the two definitional name-only exceptions (`.rgr/`, OS noise).
- MODULES-METHOD-1 §2.2: the recommendation is derived, repo-specific, never dropped (memo layer declined).
- The discovery walk stays query-time; `MAX_DEPTH` is not the cause of any known miss and is not to be "fixed".

## Review and approval

Independent requirements review: PERFORMED (Codex gpt-5.6-terra, REFINE; findings applied).
Human approval: PENDING (RG-REQ-008-L01/L06 are authored against the audit's root cause, not a ratified spec — approval of this file ratifies the scope rule).
Implementation and verification: evidence lines are the manager's OBSERVED reading; they are not acceptance.
