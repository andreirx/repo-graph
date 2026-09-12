<!-- requirements-assurance-v1
{
  "formatVersion": 1,
  "kind": "requirement",
  "requirementId": "RG-REQ-015",
  "sources": [
    { "kind": "document-section", "path": "docs/VISION.md", "fragment": "the-core" },
    { "kind": "document-section", "path": "docs/VISION.md", "fragment": "operational-architecture" },
    { "kind": "document-section", "path": "docs/VISION.md", "fragment": "honesty-rules" },
    { "kind": "document-section", "path": "docs/contracts/exit-codes.md", "fragment": "non-verdict-commands" }
  ],
  "lowLevelRequirements": [
    { "id": "RG-REQ-015-L01", "parentId": "RG-REQ-015" },
    { "id": "RG-REQ-015-L02", "parentId": "RG-REQ-015" },
    { "id": "RG-REQ-015-L03", "parentId": "RG-REQ-015" },
    { "id": "RG-REQ-015-L04", "parentId": "RG-REQ-015" },
    { "id": "RG-REQ-015-L05", "parentId": "RG-REQ-015" },
    { "id": "RG-REQ-015-L06", "parentId": "RG-REQ-015" },
    { "id": "RG-REQ-015-L07", "parentId": "RG-REQ-015" },
    { "id": "RG-REQ-015-L08", "parentId": "RG-REQ-015" },
    { "id": "RG-REQ-015-L09", "parentId": "RG-REQ-015" },
    { "id": "RG-REQ-015-L10", "parentId": "RG-REQ-015" },
    { "id": "RG-REQ-015-L11", "parentId": "RG-REQ-015" }
  ]
}
-->
# RG-REQ-015 — Indexing, maps, contracts and enrichment are explicit operations with stated outcomes

Status: DRAFT authored by the in-place manager 2026-09-12 to close a coverage gap named by the independent review of 2026-09-12 (no H owned `index/refresh`, `map`, `contracts`, enrichment as an operation, `perf`). Independent requirements review: NOT PERFORMED on this file (the review that named the gap preceded it). Human approval: PENDING.
Maturity: PROTOTYPE (requirements record).

## Source and intent

Sources: [VISION — The Core](../VISION.md#the-core) (deterministic extraction, never model output — `map` exists to end the LLM structure path); [VISION — Operational Architecture](../VISION.md#operational-architecture); [VISION — Honesty Rules](../VISION.md#honesty-rules); the [exit-code contract](../contracts/exit-codes.md) (code 3 = still running); the ratified contracts of INDEX-QUIET-1, INDEX-DISCONNECT-1, MAP-FROM-INDEX-1, ECONOMY-2 (map caps), SELF-POLLUTION-1 (the generated marker), CS-2A (Java generated-code mapping; CS-2 is PLANNED), ENRICH-LIFECYCLE-1, ENRICH-ROOT-1, ENRICH-YIELD-1/2/3, ORIENT-SMALL-ENRICH-1, DEV-INSTALL-DOCTOR-WAIT-1, PERF-OBS-1 (1B PENDING); `docs/cli/rmap-contracts.md`, `docs/cli/rmap-map-command.md`.

## High-level requirement

A user running `rmap index|refresh`, `map`, `contracts`, `enrich` or `perf` shall see what the operation did, what it only queued, and what it could not do — an index that is quiet, time-bounded and survives its client; a map that is deterministic, model-free, marked as generated and bounded in dry-run; contract and generated-code facts with their mapping basis and confidence, stating what is planned rather than shipped; enrichment as a visible, skippable, switchable operation; and a heavy storage probe kept off the readiness path.

**Scope:** the operations and their completion reports. Store lifecycle is RG-REQ-011; the `imports` CLI outcome is RG-REQ-006-L12 (cross-referenced, not duplicated).

**High-level acceptance:** all L entries hold on the isolated dogfood and the smoke corpus; the two "writes nothing" and "doctor never calls perf" guarantees gain executed verification before they are reported satisfied.

## Low-level requirements

### RG-REQ-015-L01 — A long index is quiet, time-bounded, and says "still running" rather than "failed"

`index`/`refresh` shall print one start line ("indexing <repo> — follow progress with `rmap doctor`") and the completion report; no inline progress by default (`--progress` restores frames; frames still flow so the stall deadline resets); after 300 s of daemon silence (`RMAP_LONG_OP_READ_TIMEOUT_SECS` overrides; 0 clamped to 1) the client re-probes and reports still-running with exit 3, a just-completed operation as success, an unreachable daemon as exit 2; `doctor` renders phase and current/total on request.

**Verification criterion:** `rgr/src/commands/index.rs` (`progress_sink_quiet_emits_nothing_even_as_frames_flow`, `default_command_surface_prints_start_line_and_stays_quiet_opt_in_renders`, `still_running_timeout_yields_distinct_non_failure_exit_status`); `daemon-runtime/tests/daemon_visibility.rs::inflight_refresh_reported_with_phase_and_counters_by_status_surface`.

**Evidence (v0.18.0):** OBSERVED MET as honesty; the capability gap — no per-repo index timeout override, so a kernel-scale index never fits the client window (`SMOKE_SKIP=linux`) — is an open ROADMAP item, not a violation of this L.

### RG-REQ-015-L02 — An index survives its client and leaves no half state

Registration is persisted before indexing; a progress-emit failure logs once and the index CONTINUES detached; the ready flip is independent of the response path; a real failure marks a terminal snapshot state with a reason; explicit cancel is the only deliberate stop.

**Verification criterion:** `daemon-runtime/tests/index_disconnect.rs` (`disconnect_during_index_completes_to_ready`, `disconnect_during_refresh_completes_to_ready`, `failed_index_after_registration_leaves_repo_registered`, `failed_index_leaves_terminal_snapshot_and_reader_surface_agree`).

**Evidence (v0.18.0):** OBSERVED MET (INDEX-DISCONNECT-1).

### RG-REQ-015-L03 — The completion report states what was indexed, what was queued, and every degradation

One summary line (files, nodes "all kinds", edges, unresolved) plus repo and snapshot; then only when applicable: enrichment queued|disabled, retention queued, contracts (`N schemas, M elements (K failed, storage error: …)`, failures truncated after five), generated-code mappings (persisted / high-confidence with named errors), copy-forward families. Asynchronous outcomes are never fabricated; absent data prints no line rather than a zero. `--include-root` and `--alias` are honoured and visible in the report.

**Verification criterion:** `commands/index.rs` summary/format tests; `rgr/tests/index_contract_summary.rs` (nine tests); CLI tests for `--include-root` and `--alias` (to be added — none exist at the CLI; alias is tested only in `registry.rs`).

**Evidence (v0.18.0):** OBSERVED MET for the report; the two flags are UNVERIFIED at the CLI.

### RG-REQ-015-L04 — `map` is deterministic and model-free

`rmap map` shall render MAP.md from the current READY snapshot's extracted facts with no LLM anywhere in the path; a total order on every rendered field makes identical facts byte-identical regardless of row arrival order; a response missing a required fact fails closed, never renders empty-as-zero.

**Verification criterion:** `rgr/src/presentation/map.rs` (`render_is_byte_identical_across_two_renders`, `render_is_stable_under_permuted_input`, `symbol_ordering_is_total_over_tied_name_and_line`, `golden_directory_map_exact_bytes`, `partial_daemon_payload_fails_closed_never_empty_facts`); `scripts/byte-compare-map-modules-surfaces.sh`.

**Evidence (v0.18.0):** OBSERVED MET.

### RG-REQ-015-L05 — Every sidecar carries the generated marker, and the marker is the sole evidence of generation

First line `<!-- generated by rmap map from snapshot <short-uid>; do not hand-edit -->` (short-uid from the snapshot's content-hash segment, ≤ 12 chars, `unknown` never fabricated); per-directory `<dir>/MAP.md`, per-file `<base>_<ext>_MAP.md`; writes anchored at the resolved repo root, never the cwd. The marker string is consumed by the one shared exhaust predicate (RG-REQ-011-L07, RG-REQ-008-L03) — changing it silently reclassifies users' hand-authored maps.

**Verification criterion:** `presentation/map.rs` (`every_rendered_file_carries_marker_with_snapshot_provenance`, `short_uid_uses_snapshot_content_hash_not_repo_prefix`); `commands/map.rs::output_path_is_anchored_at_repo_root_not_cwd`; `doc-facts/src/self_generated.rs` marker tests.

**Evidence (v0.18.0):** OBSERVED MET.

### RG-REQ-015-L06 — `--dry-run` writes nothing and is bounded

`--dry-run` prints the count header (source files mapped / sidecars / bytes), the per-file bytes manifest and content capped at a stated line budget with an elision line naming the remainder and the exact `--full` / `--limit <n>` / `--json` command; `--full` uncaps; `--full` with `--limit` is rejected; NO file is written. Without `--dry-run`, `map` writes and overwrites (a regenerable artifact) — every script and smoke uses `--dry-run`.

**Verification criterion:** `presentation/map.rs` dry-run cap tests; `commands/map.rs` parse tests; a process-level test asserting an unmodified tree after `--dry-run` (to be added — NONE exists; `byte-compare-map-modules-surfaces.sh:125` only comments it); `daemon-runtime/src/handlers/map.rs` has no tests.

**Evidence (v0.18.0):** OBSERVED MET for the bound (gstreamer 12.99 MB → 24.6 KB); "writes nothing" UNVERIFIED — the 2026-08 incident (bare `map` wrote 79,325 sidecars across the corpus) is the reason this L exists.

### RG-REQ-015-L07 — `map` coverage is honest and names its basis

Files present in the index but not mapped for symbols are listed with distinguishable reasons (size cap / no extractor / parse failure), by full repo-relative path, capped with `+N more`; the summary states its basis ("files mapped for symbols") and marks it distinct from `check`'s UNPARSED_FILES; each file map is self-contained, repeating the per-language complexity coverage caveat or its unavailability reason; a file's import section splits resolved intra-repo from external/unresolved.

**Verification criterion:** `presentation/map.rs` (`unmapped_files_listed_with_distinct_reasons_never_omitted`, `summary_names_unmapped_files_and_marks_distinct_notion`, `summary_caps_long_unmapped_list_with_more_tail`, `coverage_unavailable_reason_is_rendered_and_reaches_unmeasured_files`, `file_imports_list_resolved_and_external`).

**Evidence (v0.18.0):** OBSERVED MET (map --dry-run EVIDENCE/HONESTY A in the audit).

### RG-REQ-015-L08 — `contracts` answers with schema facts and states its mapping basis and confidence

`contracts list|show|elements|usages` shall return, from facts the index populates automatically, schemas (path, kind, package, syntax version, content hash, extractor), elements (kind, name, full name, parent, line span, per-kind metadata) and usages (generated symbol key and file, language, `mapping_basis`, `confidence`, evidence), each with snapshot scope, count and a `stale` flag; filters `--kind`, `--file`, `--element`, `--min-confidence`; exit 0/1/2. `contracts` is JSON-only today — a recorded exception to RG-REQ-012-L06, to be stated in the CLI contract, not hidden. `schema_uid` is a fresh UUID per snapshot by design and is not a cross-snapshot identity.

**Verification criterion:** `rgr/tests/contracts_command.rs` (argument validation and daemon precondition, 13 tests); `contract-schema/src/mapping.rs` basis/uid tests; rendered-content, coverage and degradation-state tests (to be added — `commands/contracts.rs` has no tests; no dispatch test covers the four handlers).

**Evidence (v0.18.0):** PARTIALLY MET — a working JSON fact surface with no content-level verification and no human renderer.

### RG-REQ-015-L09 — Generated-code mapping is Java, top-level, basis-and-confidence labeled; the rest is planned and said so

Checked-in Java protobuf/gRPC artifacts are mapped to top-level messages, enums and services with five explicit bases and default confidences (`exact_option_match` 0.95, `option_package_match` 0.90, `filename_convention` 0.85, `symbol_normalized_match` 0.75, `weak_wrapper_match` 0.50) plus evidence fields; the mapping pass reports at index and refresh. Field/method-level mapping, other languages, build-output inference, non-proto IDLs and gRPC-specific detection are NOT claimed (CS-2 is PLANNED); any surface that renders mappings states this coverage.

**Verification criterion:** `indexer/src/java_code_mapper.rs` tests (`test_mapping_basis_confidence`, `test_find_java_mappings_exact_option_match`, `test_find_java_mappings_grpc_stub`, `test_find_java_mappings_grpc_outer_class_skipped`); `storage/src/generated_code_mapping_read_impl.rs`; `rgr/tests/index_contract_summary.rs::index_with_proto_and_java_shows_mapping_summary`; CS-2A acceptance 0 false positives above 0.75 on hadoop (index path; proto schemas re-indexed, not copied forward, at refresh — by design).

**Evidence (v0.18.0):** OBSERVED MET within the stated scope.

### RG-REQ-015-L10 — Enrichment is an operation the user can see, skip and switch off

Enrichment shall run after every completed index/refresh when the language's resolver toolchain is present (rust-analyzer, tsserver, jdtls), one pass per daemon with a newer trigger superseding a queued older one; a missing toolchain is a NAMED skip with a reader-frame next action, never an error and never silent; `RMAP_AUTO_ENRICH` opt-out renders "disabled"; it is activity-stamped (visible in `doctor`), cancellable, yields to explicit writes, survives client disconnect, resolves the repo root against the store's location not the daemon's cwd; the promotion funnel (resolved → promoted + top rejecting classes in the reader's language) reaches `doctor`; the in-flight posture is identical on every budget tier and never promises rising figures on a repo whose languages have no resolution path. C/C++ has no semantic-resolution path (BC-1 PLANNED; SE-1 superseded) — no obligation is claimed.

**Verification criterion:** `daemon-runtime/tests/enrich_lifecycle.rs` (auto trigger, supersede, absent toolchain skip, opt-out, yield, explicit index preempts, cwd-resolved manual enrich, mixed context, python-dominant skip naming); `enrich_in_flight_coherence.rs`; `doctor/daemon_info.rs::enrichment_probe_*` (12 tests incl. the promotion funnel and the toolchain skip).

**Evidence (v0.18.0):** OBSERVED MET (ENRICH-LIFECYCLE-1, ENRICH-ROOT-1, ENRICH-YIELD-1/2/3, ORIENT-SMALL-ENRICH-1). Doc drift: `docs/cli/rmap-contracts.md` still shows the positional `enrich <db_path> <repo_uid>` form.

### RG-REQ-015-L11 — `perf` is the deliberate heavy probe, kept off the readiness path

`rmap perf` shall measure storage volume on demand (DB size, page size/count, per-table rows, tier/layer aggregates and classification coverage, retention breakdown with its baseline stamp), declared a diagnostic and JSON-only; `doctor` shall NOT call it — the readiness probe uses the cheap `storage_health` summary (the ~80 s `doctor` wait DEV-INSTALL-DOCTOR-WAIT-1 removed).

**Verification criterion:** `commands/perf.rs` (`perf_parses_baseline_stamp_in_retention_breakdown`, `perf_tolerates_daemons_without_baseline_stamp`); a dispatch/probe test asserting `doctor` selects `storage_health`, never `perf` (to be added — today a code comment at `doctor/mod.rs:253-255` is the only guard); the dev-install phase timing.

**Evidence (v0.18.0):** PARTIALLY MET — the exclusion holds in code but is unguarded; PERF-OBS-1B (phase timing, RSS, baselines) is PENDING and not claimed; `perf` itself exceeds 300 s on a 9.5 GB store (known limit).

## Preservation obligations named by the ratifying specifications

- INDEX-QUIET-1: frames keep flowing when nothing renders; a display change never touches transport or stall semantics; the 300 s stall window and its single override.
- MAP-FROM-INDEX-1 / VISION: no model call in the map path; the total ordering; fail-closed on missing facts; `GENERATED_MARKER` text and sidecar naming are a frozen contract consumed by the exhaust predicate.
- ECONOMY-2: dry-run caps, `--full`/`--limit` semantics and their mutual exclusion.
- CS-2A scope boundaries (Java, top-level, five bases); `schema_uid` per-snapshot by design; proto schemas re-indexed at refresh.
- ENRICH-LIFECYCLE-1: one pass per daemon; supersession; yields; opt-out name `RMAP_AUTO_ENRICH`; the funnel vocabulary is an open tail (FUNNEL-VOCAB-1), not shipped.
- `doctor`'s readiness path stays light (`storage_health`); `perf` stays diagnostic.
- Doc drift to fix alongside: `rmap-contracts.md` enrich form; `rmap-map-command.md` lacks `--full`/`--limit` and the cap; `contracts` JSON-only exception unstated.

## Review and approval

Independent requirements review: NOT PERFORMED on this file.
Human approval: PENDING.
Implementation and verification: evidence lines are the manager's OBSERVED reading; they are not acceptance.
