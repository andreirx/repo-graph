# DEPS-ECOSYSTEM-PARTITION-1 — `deps list` partitions what it observed by ecosystem and names the readers it has

Status: SPECIFIED (2026-09-12) · Track: audit round six, Q5 (HIGH; RC-6 never worked / visibility regressed via DEPS-CLASSIFIER-1B; RC-7 never worked). CODE slice: `module-queries/src/deps/compose.rs`, `storage/src/crud/module_edges_support.rs` (carry the source path), `daemon-runtime/src/deps_headline.rs`, `daemon-runtime/src/reader_context.rs`, `rgr/src/presentation/deps_list.rs`. Builder: Codex gpt-5.6-sol; reviewer: Codex gpt-5.6-terra.

## 0. Requirements allocation

**Implements:** RG-REQ-006-L08 (observed side partitioned by the importing file's ecosystem; skipped references named), RG-REQ-006-L09 (reader-absence claim names only reader-less languages and lists parsed manifests), RG-REQ-006-L10 (every materially present ecosystem named), RG-REQ-002-L06 (coverage stated where it renders).

**Changes:** django `deps list --ecosystem npm`: undeclared 102 → 0, with the cross-ecosystem count named; gstreamer `deps list` header names C/C++ only and lists 4 cargo / 1 npm / 8 gradle manifests with the flag that renders them.

**Preserves (§3):** RG-REQ-006-L06/L07 (head reduction, per-row basis), the default-view headline and the ≥10% materiality gate, byte-stability of repo-graph ("294 of 17654"), FRAKTAG ("30 of 733"), storybook, nginx/poco/OpenXcom/sqlite/leveldb; the `none-detected` view exemption.

## 1. Problem (ROOT-CAUSED — RC-6, RC-7)

RC-6: `compose.rs:181-230` groups EVERY external reference into the module bucket; only the DECLARED side is gated by `path_ecosystem` (`:240`). On django, 13,956 of 13,967 external refs come from `.py`; under `--ecosystem npm` they reconcile against package.json's 6 devDeps → "undeclared 102 … abc, argparse, asgiref.local". The observed filter never existed (f634618); DEPS-CLASSIFIER-1B's `insert_module_key_preferring_ecosystem` made the npm row render at all. RC-7: `reader_context.rs:134-147 deps_reader_context_note` joins the display names of ALL repo languages without checking `language_deps_ecosystem`; the §2.4 secondary line is suppressed by the ≥10% gate (rust 0.28%); the same store row records `deps_manifests_present {"cargo":4,"npm":1}` and 13 parsed records with `error: null`.

## 2. Contract

1. `ExternalImportFact` carries the source file path (storage read joins `files`); `compose.rs:181` skips a reference when `ecosystem_prefix.is_some() && path_ecosystem(source_path) != Some(input.ecosystem)` — the ONE existing predicate, not a copy; skipped references are counted in `cross_ecosystem` on `ComposeDependenciesResult`.
2. `compute_unattributed` names them: "13,956 of 13,967 external references are Python-file imports, outside this npm view — see `deps list --ecosystem python`" — never folded into "outside a parsed manifest scope". The `none-detected` view is exempt (`ecosystem_prefix.is_none()`).
3. `deps_reader_context_note` filters languages by `language_deps_ecosystem(tok).is_none()` and appends, from the provenance already read (`dispatch.rs:6748`), "N manifests of other ecosystems were parsed (4 cargo, 1 npm, 8 gradle) — see `deps list --ecosystem cargo`". The ≥10% gate is unchanged.
4. Outward proof: django `--ecosystem npm` → `used 0 · no static import found 6 · undeclared 0`, builtins unchanged, the ⚠ line names the split; gstreamer header names C/C++ only and the parsed-manifest inventory; `--ecosystem cargo` on gstreamer renders the 4 Cargo modules with `[<path>/Cargo.toml]`.

## 3. Regression watch

| Preserved L | What would regress | Proof |
|---|---|---|
| RG-REQ-006-L06/L07 | head reduction; per-row basis; asgiref row | `dep_reduce.rs`, `reconcile.rs`, `deps_list.rs::asgiref_reads_used_with_computed_basis_tzdata_stays_unobserved` green; django default `deps list` byte-identical |
| RG-REQ-006-L10 | Maven refusal; provenance family | `deps_list.rs::maven_capability_limit_names_the_gap_and_suppresses_downgraded`, `present_but_unparsed_manifest_renders_honest_no_record_never_assumed_no_source`; hadoop `deps list` byte-identical |
| Byte-stability | repo-graph "294 of 17654"; FRAKTAG "30 of 733"; storybook; nginx/poco/OpenXcom/sqlite/leveldb headers | isolated indexes (repo-graph via retained copy; FRAKTAG; nginx) — the headline lines identical; the store counts cited in RC-6 are the basis |
| `none-detected` views | gstreamer's 7 rows keep their imports | the exemption test (write first) |
| RG-REQ-002-L08 | reader-frame wording of the new sentences | review against the rule; no basis codes leak |
| RG-REQ-012-L06 | JSON carries `cross_ecosystem` additively | deps JSON test updated additively |

## 4. Stop conditions

Frozen: the ≥10% gate, manifest parser shapes, head reduction, storage schema (the source path is read, not stored anew), exit codes, the per-row `[no manifest]` rendering in the none-detected view (answered by the documented `--ecosystem` route — fixing it there needs four composes and is out of scope). STANDING HONESTY RULES. Do NOT commit.

## 5. Validation (ORDERED)

1. Failing tests FIRST: observed-side gate (a Python binding does not enter an npm reconciliation); `cross_ecosystem` count rendered; reader-note filter; parsed-manifest inventory clause; `none-detected` exemption.
2. `cargo test -p repo-graph-module-queries`, `-p repo-graph-storage`, `-p repo-graph-daemon-runtime --lib deps`, `-p repo-graph-rgr --lib presentation::deps_list`.
3. Live proof: isolated django index (`RMAP_TRANSPORT=stdio`) for the npm view; gstreamer from a COPY of its retained store (2.6 GB — copy, do not reindex) for the header; FRAKTAG and nginx isolated for byte-stability; worktree before-binary on the same roots.
4. `build-N.md`.

## 6. Definition of done

§2.4 holds; §3 green; gates green.

CORPUS PATHS: django, gstreamer (retained copy), nginx at ../legacy-codebases/<name>; FRAKTAG at ../FRAKTAG; repo-graph retained copy under ~/repo-graph-retained/audit-v0.18.0 (copies only).
