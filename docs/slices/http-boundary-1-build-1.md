# HTTP-BOUNDARY-1 — build-1.md (incremental build report)

Builder run. Uncommitted working tree for reviewer `git diff`. Evidence labels:
EXECUTED / OBSERVED / INFERRED / NOT RUN.

## Task packet

- **task type:** feature (implementation).
- **active priority:** VISION Value Frontier §2 "Boundaries and seams — API provider/consumer
  structure … Mechanism detectors (HTTP, gRPC, …) are evidence tracks feeding the seam model."
  Boundary-interaction track; HTTP is the one common mechanism `ChannelKind` lacks.
- **definition of done (slice §5):** HTTP/REST is a first-class `ChannelKind`; Spring + serverless
  routes emit provider surfaces; fetch/axios (+ Java clients) emit consumer surfaces; route-template-
  aware linking produces the inter-module API map on `rmap boundaries`; on glamCRM ≥1 real
  frontend→backend/serverless API edge shown; ambiguous/absent matches stay honestly unlinked; gRPC
  track + existing channel kinds unchanged; trust denominator untouched; gates green.
- **why on priority path now:** usefulness run 2026-08-23 shows `boundaries summary` = 0 on glamCRM,
  whose three subsystems talk over HTTP/REST — the mechanism the boundary track was blind to.
- **files in scope (confirmed against real tree):**
  - `boundary-interaction/src/types.rs` — add `ChannelKind::Http` + taxonomy.
  - `storage/src/boundary_interaction_read_impl.rs` — `parse_channel_kind` `"http"` arm (REQUIRED:
    manual `&str` match, no compiler exhaustiveness guard — reads of http surfaces error without it).
  - `repo-index/src/http_boundary.rs` (NEW pub(crate) module) — Spring provider + TS consumer + TS
    CDK provider detectors + `persist_http_boundary_interactions` wiring; called from `compose.rs`.
  - `indexer/src/http_link.rs` (NEW module next to `grpc_link.rs`) — template-aware linker.
  - `indexer/src/storage_port.rs` + `storage/src/*` — `HttpLinkReadPort` (read http surfaces).
  - `indexer/src/refresh_dispatch.rs` — Phase B http-link invocation (before the proto gate).
  - `rgr/src/commands/boundaries/links.rs` + `presentation/` — human render for links.
  - tests/fixtures.
- **files OUT of scope:** Flask/FastAPI/Express-beyond-glam providers (extension points),
  witness/union/reconciliation, trust, ROADMAP/VISION/TECH-DEBT, agent-manager.
- **storage/refresh/trust/CLI impact:** NO schema change (route+method ride in `evidence_json`;
  `link_kind`/`match_basis` are free TEXT; `contract_element_uid` written NULL for http). Reuses the
  existing surface + link write paths. CLI: `boundaries summary`/`links` show http automatically.
- **validation commands:** chunked `cargo build/test/fmt/clippy` in `rust/`; unit tests per detector +
  linker; isolated live index of glamCRM under `/private/tmp` state root; leveldb byte-parity;
  `./scripts/dogfood-isolated.sh`; `SMOKE_ONLY="glamCRM leveldb"` smoke run.
- **stop conditions (slice §3):** gRPC track frozen (http is a SIBLING); no schema write beyond the
  surface family; trust FROZEN; matching rule is the one ratification-class heuristic — ambiguous/
  absent → UNLINKED, never fabricated; NEVER touch the operator's real state root; do NOT commit.

## FINDINGS (surfaced, per slice §3 "if the real glam route shapes make confident linking impossible
for a class of routes, that is a FINDING")

- **F1 (slice premise correction):** `react_detector.rs` does NOT detect `fetch`/`axios` — it detects
  React components/hooks only. The real (method, path) analog is `express_detector.rs` (emits
  `ProjectSurface http_provider`). Slice FILES_IN_SCOPE named `react_detector.rs (extend)`; the honest
  technically-correct move is a NEW cohesive detector module (`http_boundary.rs`) modelled on
  `express_detector.rs`, not jamming HTTP-client logic into the React detector. Pre-ratified new
  `pub(crate)` module used. [OBSERVED — react_detector.rs has no fetch/axios matcher.]
- **F2 (two surface stores):** `boundaries` (list/show/summary/links) reads
  `boundary_interaction_surfaces` + `boundary_interaction_links` (the boundary-interaction crate store,
  where gRPC's 116 surfaces live). `surfaces list` reads the SEPARATE `project_surfaces` store (where
  express_detector's `http_provider` rows go). Slice §5 DoD names `boundaries`; the packet's item 5
  also names `surfaces list`. This build targets `boundaries` (the binding DoD surface). `surfaces list`
  HTTP parity would require dual-emission to `project_surfaces` — a separate concern (see split).
- **F3 (glam serverless framework):** the serverless TS is NOT express and has NO serverless.yml — it
  is **AWS CDK apigatewayv2** `this.api.addRoutes({ path, methods: [apigateway.HttpMethod.*] })` in
  `serverless/packages/infra/lib/constructs/api.ts`, static literal paths. Named as the detected
  serverless form; other frameworks (express/Flask/FastAPI) recorded as extension points.
- **F4 (Phase B proto gate):** `dispatch_recompute_relationships` returns early when there are no
  `generated_code_mappings` (proto). HTTP linking must run BEFORE that gate but still honor the
  `BoundaryInteractionLinks` contract-policy check. [OBSERVED refresh_dispatch.rs:123-131.]
- **F5 (naming):** `GrpcLinkStorePort::insert_boundary_interaction_links` is correctly (generically)
  named; the enclosing trait `GrpcLinkStorePort` is narrowly named but the method is channel-agnostic.
  HTTP reuses this method (empty `contract_element_uid` → NULL). Proposed rename of the TRAIT to
  `BoundaryInteractionLinkStorePort` is boundary-touching (call sites) → NOT done here; surfaced.

## Scope decision for this run (recorded, not asked — slice is ratified; packet authorizes reporting a
split if it cannot converge in 3 cycles)

Building the coherent CORE VERTICAL that proves the DoD on glamCRM:
`ChannelKind::Http` → Spring provider + TS axios/fetch consumer + TS CDK provider → template-aware
linker → Phase-B wiring + read port → `boundaries` render → tests → live glam proof.

DEFERRED (named split, informed by what lands):
- Java `RestTemplate`/`WebClient`/`HttpClient` consumer (glam's cross-subsystem edge is axios→Spring;
  not needed for the DoD link; extension point).
- `surfaces list` HTTP parity (separate `project_surfaces` store — F2).
- `modules list` "boundaries may not be meaningful" hint (keys off module-IMPORT counts, independent of
  boundary links; fixing needs link-presence plumbed into `ModulesListResponse` — a boundary-crossing
  data-shape change; assessed below).

## Abstraction ledger (one line each)

- `http_boundary.rs` (repo-index, NEW pub(crate) module): HTTP detection + surface emission for the
  boundary-interaction store. Concrete callers: `compose.rs` fresh + refresh index paths. Axis:
  operations growing over a fixed detection concern (added alongside express/react/ts-boundary
  postpasses). Rejected simpler: inline in compose.rs — violates the 500-line/oversized-file guardrail.
- `http_link.rs` (indexer, NEW module next to grpc_link.rs): route-template-aware provider↔consumer
  linking. Concrete caller: `refresh_dispatch.rs::dispatch_recompute_relationships`. Axis: a second
  concrete link mechanism beside gRPC contract-match (2 callers of the link table now). Rejected
  simpler: extend grpc_link — its match is string-equality on proto UID, cannot express path templates,
  and the gRPC track is frozen.
- `HttpLinkReadPort` (indexer storage_port): read http surfaces (method+route from evidence_json) for
  linking. Concrete caller: http_link. Axis: a second surface-read shape beside `GrpcLinkReadPort`
  (which is proto-contract-joined). Rejected simpler: reuse `GrpcLinkReadPort` — it requires a
  `boundary_contracts` grpc_service join http surfaces don't have.

---

## Build log (chronological; appended AS work lands)
</content>

### Cycle 1 — detectors + types (EXECUTED)

- `ChannelKind::Http` + `ProtocolFamily::Http` added to `boundary-interaction/src/types.rs`
  (as_str "http", `default_transport_class`→CustomProtocol, `is_http()`, From, snake_case serde).
  `emit.rs` `protocol_for_channel_kind` Http arm (exhaustiveness). Storage read
  `parse_channel_kind`/`parse_protocol_family` "http" arms (REQUIRED — manual `&str` matches).
- NEW `repo-index/src/http_boundary.rs`: Spring `@RestController` provider detector (metadata_json
  annotations + parent_node_uid class-join), TS axios/fetch consumer detector, TS AWS-CDK
  apigatewayv2 `addRoutes({path,methods})` provider detector; `persist_http_boundary_interactions`
  wired into compose.rs fresh + refresh index paths (isolated postpass, extractor-scoped cleanup).
- Persistence: reuses `insert_boundary_surfaces_and_channels` (channels empty; route+method in
  `evidence_json`). Refresh = full recompute per snapshot (no copy-forward path for a new family).
- `cargo build -p repo-graph-boundary-interaction -p ...-extractor` → Finished [EXECUTED].
- `cargo build -p repo-graph-repo-index` → Finished [EXECUTED].
- `cargo test -p repo-graph-repo-index --lib http_boundary` → **14 passed; 0 failed** [EXECUTED]:
  join_route, first_string_literal, mapping_verb, source_file_from_stable_key,
  route_literal (query+interpolation strip, dynamic→None), spring provider class+method compose,
  non-rest-controller→nothing, axios instance/dotted consumers, fetch method+default,
  dynamic-url→unknown-route, non-client-receiver ignored (Map.get/app.get), CDK one-per-method.

### Cycle 2 — linking + wiring fix + LIVE glam proof (EXECUTED)

- Linker `indexer/src/http_link.rs`: route-template match (`{id}`/`:id`/`*` wildcards), unambiguous-only,
  ambiguous/no-match/dynamic counted honestly; `HttpLinkReadPort` (reads http surfaces, parses
  method+route from `evidence_json`); reuses the link write path with empty `contract_element_uid`→NULL.
- **Wiring correction (root-caused live):** first placed the linker in `dispatch_recompute_relationships`,
  but that is proto-gated (`orchestrator.rs:303 if !contract_files.is_empty()`) AND runs before the
  compose http-surface postpass — so it never ran for proto-less glamCRM (0 links). Reverted the
  dispatch/orchestrator wiring (gRPC track left frozen) and run the linker as the final step of the
  `persist_http_boundary_interactions` postpass, after all http surfaces are persisted (FK ON DELETE
  CASCADE handles cleanup). This is the correct ordering + no proto gate.
- `boundaries links` gained a human renderer (was JSON-only) + `--json`; link DTO gained additive
  `evidence_json` so the edge (method+route) renders.
- Unit tests: http_link 7 passed; http_boundary 14 passed; storage null-contract round-trip 1 passed;
  rgr links render 2 passed [EXECUTED].
- **LIVE glamCRM (isolated /private/tmp state root, release binaries):**
  - `boundaries summary`: **0 → 234 http surfaces** (221 provider [106 Spring annotation + 115 CDK
    convention], 13 consumer [axios/fetch]); scope all `unknown` (never fabricated). [OBSERVED]
  - `boundaries links`: **1 CONFIRMED cross-subsystem link** [OBSERVED]:
    `POST /api/v2/projects/{projectSelector}/etape`
    `frontend/workspace/src/api/projects.ts:99 → serverless/packages/infra/lib/constructs/api.ts:1173`
    `[http_route_match, confidence 0.75]` — a real frontend→serverless API edge.
  - Consumer breakdown: 13 = 8 dynamic (route null, never linkable) + 5 concrete → 1 linked (etape) +
    4 ambiguous. **FINDING F6:** glam runs Spring AND serverless exposing the SAME `/api/v2/*` routes
    (in-progress backend migration), so 4/5 concrete consumer routes match BOTH backends → AMBIGUOUS →
    left UNLINKED, never guessed (slice's ratified rule; VISION unknown-never-fabricated). The single
    serverless-only route (`POST .../etape`) links cleanly. This is honest, correct behavior, and the
    reason the count is 1 not 5.
- **leveldb byte-parity (control):** `boundaries summary` body IDENTICAL to baseline
  smoke-runs/2026-08-23T18-38-37Z (0 surfaces / 0 channels) — HTTP is additive, no regression on a
  repo without HTTP. [EXECUTED diff → IDENTICAL]

### Cycle 3 — gates, parity, cleanup (EXECUTED)

**Cargo gates (from rust/):**
- `cargo fmt --check` → clean [EXECUTED].
- `cargo clippy -p boundary-interaction -p repo-index -p indexer -p storage -p rgr` → clean (one
  `manual_find` on `first_call_argument` annotated `#[allow(clippy::manual_find)]`, matching
  express_detector's identical helper) [EXECUTED].
- `cargo test --workspace --no-run` → compiles (DTO `evidence_json` addition breaks no constructor)
  [EXECUTED].
- Test suites [EXECUTED]: boundary-interaction 32 + extractor 45; storage lib 604 + http-null-contract
  round-trip 1; indexer lib 328 (incl. http_link 7); repo-index lib 128 (incl. http_boundary 14) +
  integration bi_*/mb_*/fd_* suites green; rgr lib 683 (incl. links render 2); daemon-runtime
  consolidation_witness **15/15**.
- **PRE-EXISTING FAILURE (not mine):** `repo-index::integration::mixed_lang_language_isolation`
  (integration.rs:391 — asserts App.java has no `package_dependencies_json`) fails IDENTICALLY on clean
  HEAD 467c0df (verified via `git stash`). Unrelated to this slice (touches Java package-dep signals,
  not boundaries). Reported, not fixed.

**End-to-end / isolation:**
- `./scripts/dogfood-isolated.sh` → **OK** (operator registry read-only, non-pollution PASS) [EXECUTED].
- **gRPC parity (grpc-java, isolated):** `boundaries summary` = **116 surfaces (86 consumer / 30
  provider), grpc_channel / rpc** — EXACTLY the slice's baseline; HTTP additive (0 for grpc-java).
  gRPC track UNCHANGED. [OBSERVED]
- **glamCRM (isolated):** 234 http surfaces; 1 confirmed frontend→serverless link rendered; 4 ambiguous
  + 8 dynamic consumers honestly unlinked (F6). [OBSERVED]
- **leveldb (isolated):** boundaries summary body byte-identical to baseline (0 surfaces). [EXECUTED]

**Persistence completeness:** write ✓ (reuse insert_boundary_surfaces_and_channels + links insert,
empty contract_element_uid→NULL); read ✓ (channel-agnostic summary/links + `parse_channel_kind`/
`parse_protocol_family` http arms); refresh ✓ (full recompute per snapshot — no copy-forward path for a
new family; matches persist_express); trust — untouched (FROZEN, as required); CLI ✓ (boundaries
summary auto + boundaries links human render); validation ✓ (fresh index live-proven; refresh path
wired identically).

### DoD status
- HTTP/REST first-class `ChannelKind` ✓ · Spring + serverless providers ✓ · fetch/axios consumers ✓ ·
  route-template-aware linking with honest ambiguity ✓ · renders on `rmap boundaries` (summary + links)
  ✓ · ≥1 real glam cross-subsystem edge ✓ · ambiguous/absent unlinked ✓ · gRPC + existing kinds
  unchanged ✓ · trust denominator untouched ✓ · gates green (modulo the pre-existing unrelated failure).
- **DEFERRED (reported split, not built):** Java RestTemplate/WebClient consumer (extension point;
  glam's edge is axios→serverless/Spring); `surfaces list` HTTP parity (separate project_surfaces store,
  F2); `modules list` "not meaningful" hint (needs boundary-link presence plumbed into
  ModulesListResponse — a boundary-crossing data-shape change; F-note below). These do not block the
  slice's binding §5 DoD (which names `boundaries`).

### DECISION_REQUIRED (non-blocking; for reviewer/operator)
- ID: modules-list-boundary-hint
  QUESTION: `modules list` still prints "Module boundaries may not be meaningful yet" purely from
  module-IMPORT counts, independent of HTTP/boundary links. Suppressing it when boundary links exist
  needs link-presence plumbed into `ModulesListResponse` (a boundary-crossing data-shape change).
  OPTIONS:
  - A: build it now (plumb boundary-link count into modules read/render) — extra cross-boundary data flow.
  - B: defer (this build) — the hint is about module-import graph, a different sense of "boundary"; the
    HTTP API map is fully visible via `rmap boundaries`.
  RECOMMENDED: B — the two "boundary" senses differ; conflating them risks a misleading claim, and the
  §5 DoD is satisfied by `boundaries`.
  BLOCKING_REASON: none — surfaced for governance, not blocking.

---

## SUPERSEDING NOTE (2026-08-24): the "DEFERRED" split above is STALE

Cycles 1–3 above describe the FIRST builder run (build-0). The three items marked DEFERRED were then
BUILT in the revision run (build-1), and the DECISION_REQUIRED `modules-list-boundary-hint` was
RESOLVED (build it — the operator ratified an honest Layer-3 hint). The AUTHORITATIVE final
working-tree state is the "FINAL STATE (iteration 5)" section at the bottom of THIS file; the
per-cycle live counts in the Cycle 1–3 log (0→234 http / 13 consumer) are the STALE build-0 numbers
kept only for provenance — the current counts are **244 http / 23 consumer / 1 link** (see below).
Corrected status:

- **Java HTTP consumers (RestTemplate/WebClient/HttpClient)** — BUILT (`repo-index/src/http_boundary/
  java_consumer.rs`, tree-sitter-java re-parse; static + dynamic-URL tests). No longer deferred.
- **`surfaces list` HTTP parity** — BUILT. The daemon `surfaces list` handler now reads the
  `channel_kind='http'` surfaces as a DISTINCT section (never mixed into `project_surfaces`) and the
  `rgr` presentation renders "HTTP/REST API surfaces: N provider(s), M consumer(s)" with `<dynamic>`
  for unreadable routes. No longer deferred.
- **`modules list` "not meaningful" hint** — BUILT + made Layer-3-honest (see below). No longer deferred.

## Cycle 4 — iteration 2: operator rulings applied (2026-08-24)

Review-1 escalated on two architecture-boundary decisions; the operator ratified both. Applied here:

- **Ruling 1 — `daemon_indexer_production_edge` → OPTION B (direct edge REJECTED).** The pure
  route-template matcher + its raw DTOs moved OUT of `indexer` INTO the zero-dep policy crate
  `repo-graph-boundary-interaction` (`http_link` module: `find_http_links`, `route_matches`,
  `HttpSurfaceRow`, `HttpLink`, `UnlinkedCounts`). The HTTP surface READ moved onto
  `BoundaryInteractionReadPort::query_http_surfaces` (impl in storage, backed by the crate-private
  `storage/src/http_surface_read.rs`). `daemon-runtime` no longer depends on `indexer` in
  `[dependencies]` (restored to a dev-dependency for the DAEMON-CANCEL-3 fixtures only). Both the
  index-time linker (`indexer::http_link::run_http_link_detection`) and the read-time renderer
  (`daemon-runtime::http_boundary_read`) now call the SAME matcher across the index/serve split.
- **Ruling 2 — `shared_spring_annotation_parse` → OPTION A.** The RAW annotation parser is now exposed
  from `classification::spring_liveness` (`parse_node_annotations` → `ParsedAnnotation{simple_name,
  args_raw}`); `classify_spring_liveness` consumes it internally and the HTTP Spring provider detector
  (`repo-index/src/http_boundary/spring.rs`) consumes it too. The duplicated parser
  (`spring.rs::node_annotations`/`simple_name`/`Annotation`) is DELETED. One parser, no drift.
- **Layer-3 honesty (review-1 observation).** The `modules list` note no longer says a route match
  "connects these modules at runtime" / "boundaries ARE meaningful". It now reads: *"imports are
  intra-module, but these modules are likely connected via HTTP route match (heuristic, N links;
  Layer-3 discovery, not runtime-proven) — see `rmap boundaries links`."*
- **Structural guardrail (review-1 observation).** The read-time additions were moved OFF the oversized
  files into crate-private modules: `daemon-runtime/src/http_boundary_read.rs` (was inline in
  dispatch.rs @8.9k), `storage/src/http_surface_read.rs` (was inline in grpc_impl_hint_port_impl.rs
  @1.9k). No `witness/dispatch_fact_classes.txt` change (no dispatch arm added — reuses `boundaries_*`
  / `surfaces_*`).

### Abstraction ledger — iteration-2 additions/changes (one line each)

- `boundary-interaction::http_link` (pub module, zero-dep policy crate): the PURE route-template
  matcher + raw DTOs. Users: `indexer` index-time linker + `daemon-runtime` read-time renderer (2
  concrete callers). Axis: one matcher, two call sites across the index/serve split. Rejected: the
  direct `daemon-runtime→indexer` edge (wrong direction — serving must not depend on indexing
  orchestration) and persisting counts (frozen storage scope).
- `BoundaryInteractionReadPort::query_http_surfaces` (added method): the single HTTP-surface read path
  for both callers, so index-time and read-time never drift. Rejected: keeping the indexer-owned
  `HttpLinkReadPort` (would force a second read path in daemon or the rejected edge).
- `classification::spring_liveness::parse_node_annotations` (pub fn + `ParsedAnnotation`): the shared
  RAW Java annotation parser. Users: `classify_spring_liveness` + the HTTP Spring provider detector.
  Axis: one parse, two Spring consumers. Rejected: the local duplicate in repo-index (drift risk;
  contradicts the slice's stated reuse).
- `storage/src/http_surface_read.rs` (crate-private module): HTTP-surface SQL + `evidence_json` parse,
  backing `query_http_surfaces`. User: the read-impl method (1 caller). Axis: 500-line guardrail (both
  `boundary_interaction_read_impl.rs` @1631 and `grpc_impl_hint_port_impl.rs` @1921 exceed it).
  Rejected: inline in the read-impl (grows an over-limit file).
- `daemon-runtime/src/http_boundary_read.rs` (crate-private module): read-time HTTP boundary render
  helpers reusing the domain matcher. Users: `surfaces list` / `boundaries links` / `modules list`
  handlers (3 callers). Axis: 500-line guardrail (dispatch.rs @8.9k). Rejected: inline in dispatch.rs.

---

## FINAL STATE (iteration 5, 2026-08-24) — AUTHORITATIVE working-tree state

Supersedes every earlier live count in this file. Iterations 2–5 applied the operator rulings +
reviewer-required corrections on top of the build-0 core vertical.

### What each review cycle changed (net)

- **review-1 (rulings):** matcher moved to the zero-dep policy crate (`boundary-interaction::http_link`);
  `daemon-runtime → indexer` edge REJECTED; shared Spring annotation parser exposed from
  `classification::spring_liveness`; the `modules list` note reframed as an honest Layer-3 heuristic.
- **review-2:** static absolute URLs (`https://host/path`) reduced to their path in the ONE shared
  `route_from_raw` normalizer, so a `fetch("https://host/offers")` consumer matches a `/offers` provider.
- **review-3:** `run_http_link_detection` COLLECTS (never fail-fast) a surface-query / link-write error;
  the postpass now maps a collected error to `ComposeError::Index` (`link_result_into_postpass_error`),
  so `isolate_postpass` drops this extractor's partial surfaces+links (FK ON DELETE CASCADE) rather than
  serving a false-complete API map.
- **review-4 (this iteration):**
  1. **No false Java HTTP-consumer facts.** `java_consumer::classify_java_invocation` now requires
     receiver/builder EVIDENCE for `.exchange(` (RestTemplate receiver) and `.uri(` (arg-free WebClient
     verb builder, or `HttpRequest`/`newBuilder` chain) — a bare method-name match is no longer a
     consumer. Prefilter no longer admits bare `.exchange(`/`.uri(`. Negative tests added
     (`queue.exchange`, `config.uri`, `items.get(0).uri`).
  2. **No read-failure → zero collapse.** The three read helpers return `Result`; a failed HTTP-surface
     / link read renders a reader-framed DEGRADATION in `surfaces list`, `boundaries links`, and the
     `modules list` hint — never an empty map, a silent footer, or a restored "boundaries may not be
     meaningful" claim. Failure-path render tests added in all three renderers + the read module.
  3. **Spring provider verb matrix.** Added `spring_provider_all_verbs_compose_with_base_path` covering
     Get/Post/Put/Delete/PatchMapping + method-level `@RequestMapping(method=…)` with class-base
     composition.
  4. This section (the stale-doc fix).

### LIVE glamCRM (isolated `/private/tmp` state root; operator `registry.json` sha256 IDENTICAL
before/after — `98fb3ba8…a002449`; RMAP_TRANSPORT=stdio, never the operator daemon)

- `boundaries summary`: **244 http surfaces** — 221 provider (106 Spring `annotation` + 115 CDK
  apigatewayv2 `convention`) / 23 consumer (`api_call`: axios/fetch + Java RestClient); all scope
  `unknown` (never fabricated).
- `boundaries links`: **1 CONFIRMED cross-subsystem link** —
  `POST /api/v2/projects/{projectSelector}/etape`
  `frontend/workspace/src/api/projects.ts:99 → serverless/packages/infra/lib/constructs/api.ts:1173`
  `[http_route_match, confidence 0.75]`. Honest unlinked: 23 consumers, 1 linked, 4 ambiguous
  (route exposed by BOTH Spring + serverless during the in-progress migration → never guessed),
  0 unmatched, 18 dynamic (URL not statically readable). **FINDING F6 stands:** the low link count is
  correct behavior, not a miss — glam's dual-backend migration makes most routes ambiguous.
- `surfaces list`: renders "HTTP/REST API surfaces: 221 providers, 23 consumers" as a DISTINCT section,
  `<dynamic>` for unreadable routes.
- `modules list`: 6 modules, 0 cross-module IMPORTS → honest Layer-3 note "likely connected via HTTP
  route match (heuristic, 1 link; Layer-3 discovery, not runtime-proven)" — NOT "may not be meaningful".

### Gates (EXECUTED this iteration)

- fmt clean · clippy clean (repo-index, daemon-runtime, rgr) · repo-index lib **148** · daemon-runtime
  lib **471** · rgr lib **692** · consolidation witness **15/15** (no dispatch arm added —
  `dispatch_fact_classes.txt` untouched) · `./scripts/dogfood-isolated.sh` OK + non-pollution PASS ·
  `SMOKE_ONLY="glamCRM leveldb"` smoke run **2 passed / 0 failed**.
- **leveldb byte-parity:** `boundaries summary` body BYTE-IDENTICAL to
  `smoke-runs/2026-08-23T18-38-37Z` (0 surfaces) — HTTP is additive, no regression on a non-HTTP repo.

### Validation limitations (honest)

- **grpc-java gRPC parity (116 surfaces) NOT RE-RUN this iteration** — the repo is present but re-indexing
  it was not needed: this iteration's diff touches only (a) Java-consumer receiver evidence, (b) read-time
  degradation propagation, (c) tests/docs. None touch the gRPC track, `ChannelKind` variants, or the
  shared link/summary read path; gRPC output is byte-identical by construction. Prior iterations verified
  116 surfaces (86 consumer / 30 provider) unchanged. [NOT RUN — reason recorded.]
- The read-failure DEGRADATION paths are proven by renderer/mapper UNIT tests (injected `Err`/degraded
  fields), not by inducing a live storage fault — a live fault-injection harness is not present and was
  not built (out of scope). The render honesty is fully covered by the added tests.

---

## FINAL STATE (iteration 6, 2026-08-24) — SUPERSEDED by iterations 7–8 (see final section below)

> NOTE (added iteration 8): this section is NO LONGER the authoritative working-tree state. Iterations
> 7 and 8 tightened import evidence and unknown-route honesty on top of it. The AUTHORITATIVE state is
> the "FINAL STATE (iterations 7–8)" section at the END of this file. The 244/23/1 live counts here
> still hold (the honesty tightening removed no true glam fact), but the code descriptions below omit
> the iteration-7/8 corrections.

Iteration 6 addresses review-5's five required changes. The
live counts are UNCHANGED from iteration 5 (244/23/1) because the honesty tightening removed no true
glam facts — glam's real HTTP code carries the required imports — while it removes false-positive
surface classes on other codebases (proven by new negative unit tests).

### What review-5 required, and what changed (net)

1. **Eliminate name-only HTTP classification (STANDING HONESTY RULE 2) — DONE across all three
   detectors.** Each framework path now requires FILE-LEVEL IMPORT/TYPE/RECEIVER evidence, not a bare
   method/annotation/receiver NAME:
   - **Spring provider** (`http_boundary/spring.rs`): a `@RestController` class emits routes only if its
     `.java` file imports `org.springframework.web.bind.annotation` (the import-evidence set is built in
     `http_boundary/mod.rs` and threaded in). New negative test:
     `rest_controller_without_import_evidence_emits_nothing`.
   - **Java consumer** (`http_boundary/java_consumer.rs`): `getForObject`/`postForObject`/`exchange`/
     `put`/`delete` require `org.springframework.web.client`; the WebClient `.uri` path requires
     `org.springframework.web.reactive.function.client`; the HttpClient path requires `java.net.http`.
     New negatives: `resttemplate_call_without_import_is_not_consumer`,
     `webclient_chain_without_webclient_import_is_not_consumer`.
   - **TS/JS** (`http_boundary/typescript.rs`): the axios/api-client consumer path requires a file import
     of `axios` or an `api-client` module AND a receiver that is exactly `axios` or an `apiClient`-named
     instance/factory (the loose `*client*` substring is GONE — `dbClient`/`s3Client` no longer match);
     the CDK provider path requires an `apigatewayv2` import. New negatives:
     `axios_call_without_import_is_not_consumer`, `non_apiclient_client_receiver_is_not_consumer`,
     `addroutes_without_apigw_import_is_not_provider`. `fetch` remains the one import-free path (a
     browser/Node global matched by its exact identifier), which review-5 did not flag.
2. **No fabricated GET for a dynamic fetch method — DONE.** `fetch_method` now returns a sum type
   `{Static, Absent, Dynamic}`: absence of a method (no options object, or an options object without a
   `method` key) → GET (fetch's spec default); a supplied-but-non-static method (`{ method: verb }`) →
   `UNKNOWN` (never GET), which matches no provider and therefore does not link. Tests:
   `fetch_dynamic_method_is_unknown_not_get`, `fetch_options_without_method_defaults_get`.
3. **No-false-zero sweep completed — DONE.**
   - `storage/src/http_surface_read.rs`: `parse_http_evidence` returns a typed `HttpEvidenceError`
     (`NotJson`/`MissingMethod`/`RouteNotString`); `query_http_surfaces` propagates it as a
     `FromSqlConversionFailure` so a corrupt surface degrades the WHOLE HTTP map to UNKNOWN (via the
     collected `surface_query_error`) instead of silently classifying corruption as a dynamic-route
     consumer. A valid `route: null`/absent-key stays the one legitimate `None`.
   - `rgr/src/commands/boundaries/links.rs`: an ABSENT/non-array `results` renders "links: unknown —
     malformed response", never "0 links"; a `httpUnlinked` block missing any counter renders "HTTP
     consumers: unknown — incomplete", never silent zeros. Tests:
     `absent_results_renders_unknown_not_zero_links`, `non_array_results_renders_unknown`,
     `incomplete_http_unlinked_counters_render_unknown_not_zero`.
4. **500-line refactor ruling met — DONE.** New crate-private `rgr::presentation::http_boundary`
   holds the HTTP-surface DTO + surface/degraded rendering + the `modules list` Layer-3 note decision;
   `surfaces.rs` (759→~640) and `modules_list.rs` (543→~430) now only wire to it. The storage HTTP
   round-trip test moved from `grpc_impl_hint_port_impl.rs` into the crate-private `http_surface_read.rs`.
   Residual inline HTTP code in `dispatch.rs` / `boundary_interaction_read_impl.rs` is narrow wiring only
   (calling extracted helpers, a delegating trait method, one-line parse arms in existing match fns).

### Abstraction ledger — iteration-6 addition (one line)

- `rgr::presentation::http_boundary` (crate-private module): HTTP-surface DTO + surface/degraded
  rendering + the `modules list` Layer-3 note. Users: `SurfacesListResponse::render_human` +
  `ModulesListResponse::render_human` (2 concrete callers). Axis: one cohesive HTTP-render concern,
  two presenters across the empty/degraded honesty rules. Rejected: leaving ~300 lines inline in the
  two 500+-line files (violates the guardrail the operator ruling enforces).

### Gates (ALL EXECUTED this iteration)

- fmt clean (`cargo fmt --all --check`) · clippy clean (`cargo clippy --workspace --lib --tests`, 0
  warnings) · lib suites: boundary-interaction **38**, repo-index **156**, indexer **325**, storage
  **611**, daemon-runtime **471**, rgr **700** · consolidation witness **15/15** (no dispatch arm —
  `dispatch_fact_classes.txt` untouched) · `./scripts/dogfood-isolated.sh` OK + non-pollution PASS ·
  `SMOKE_ONLY="glamCRM leveldb"` smoke **2 passed / 0 failed** (logged `smoke-runs/2026-08-24T04-59-44Z`).
- **LIVE glamCRM (isolated `/private/tmp`; operator `registry.json` sha256 `98fb3ba8…a002449` IDENTICAL
  before/after):** 244 http surfaces (221 provider / 23 consumer); 1 CONFIRMED cross-subsystem link
  `POST /api/v2/projects/{projectSelector}/etape` frontend→serverless; honest unlinked 4 ambiguous / 0
  unmatched / 18 dynamic; `surfaces list` HTTP section + `<dynamic>`; `modules list` honest Layer-3 note.
- **gRPC parity RE-RUN (EXECUTED this iteration):** grpc-java `boundaries summary` body BYTE-IDENTICAL
  to `smoke-runs/2026-08-24T03-28-19Z` — **116 surfaces (86 consumer / 30 provider), grpc_channel/rpc**.
  HTTP is additive; the gRPC track is unchanged.
- **leveldb byte-parity (EXECUTED):** `boundaries summary` body BYTE-IDENTICAL to
  `smoke-runs/2026-08-23T18-38-37Z` (0 surfaces).

### Validation limitations (honest)

- The read-failure DEGRADATION and corrupt-evidence paths are proven by UNIT tests (typed-error returns
  + injected degraded fields), not by inducing a live storage fault — no live fault-injection harness
  exists and none was built (out of scope). Render/parse honesty is fully covered by the added tests.

---

## FINAL STATE (iterations 7–8, 2026-08-24) — AUTHORITATIVE working-tree state

Supersedes every earlier "AUTHORITATIVE" claim in this file (including the iteration-6 section).
Iterations 7 and 8 were pure HONESTY tightenings of import evidence; they removed no true glam fact,
so the live counts are UNCHANGED from iteration 6: **244 http surfaces (221 provider / 23 consumer),
1 confirmed cross-subsystem link, 4 ambiguous / 0 unmatched / 18 dynamic**.

### What iteration 7 changed (net — review-6 FINAL enumerated list)

- **Import evidence is a real declaration, not `content.contains(...)`.** New crate-private
  `repo-index/src/http_boundary/imports.rs`: a comment/string-aware tokenizer extracts TS/JS module
  specifiers from actual `import`/`export … from`/`import()`/`require()` declarations
  (`ts_import_specifiers`), and a comment-stripped, line-anchored scan reads Java `import`/`import static`
  statements (`java_imports_package`). Keywords inside a comment or a string literal are a single
  non-`Ident` token, so they can no longer satisfy the import gate. Every `content.contains(<pkg>)`
  "import evidence" check in `mod.rs`, `typescript.rs`, and `java_consumer.rs` was replaced by these.
- **Five `unwrap_or_default()` sites on classified/rendered data eliminated** (spring.rs:54,83 via a
  `PathArg{Literal,Absent,Unreadable}` + `BasePath{Known,Unknown}` tri-state → unreadable route is
  UNKNOWN (`route: None`), never a fabricated base path; http_link.rs:171 via `MatchOutcome::Unique`
  carrying the matched route; typescript.rs:214,223 and java_consumer.rs:211,257 → explicit `None →
  false`). An unresolvable annotation arg / route / node text is UNKNOWN, never a silent empty string.

### What iteration 8 changed (net — review-7 enumerated list)

1. **Java import evidence now matches on a PACKAGE-COMPONENT boundary, not a raw textual prefix**
   (`imports.rs::java_imports_package`). review-7 observed that `fqn.starts_with(package_prefix)`
   accepted sibling packages whose name merely extends the prefix's last component — e.g.
   `import org.springframework.web.clientish.Fake;` qualified as evidence for
   `org.springframework.web.client`, and `import java.net.httpx.Foo;` for `java.net.http`, emitting a
   false Layer-3 Java HTTP fact. The check now requires the fqn to continue with `package_prefix + "."`,
   which every real type/wildcard/static import under the package satisfies (`...client.RestTemplate`,
   `...annotation.*`, `...annotation.RequestMethod.GET`) and which the sibling-package false positives
   do not.
2. **Negative regression tests added** (`imports.rs::java_sibling_package_extending_prefix_component_does_not_qualify`):
   `clientish` and `httpx` are proven NOT to match, and a genuine `import java.net.http.HttpClient;`
   is proven to STILL match (the boundary check did not over-reject). The existing direct/wildcard/static
   positives and the comment/string negatives are retained and green.
3. **This stale-doc fix** — the iteration-6 section above is demoted to SUPERSEDED; this section is the
   authoritative working-tree state.

### Gates (ALL EXECUTED this iteration — iteration 8)

- fmt clean (`cargo fmt -p repo-graph-repo-index -- --check`) · clippy clean
  (`cargo clippy -p repo-graph-repo-index --lib --tests`, 0 warnings) · `http_boundary::imports` **10
  passed** (incl. the new sibling-package negative) · `http_boundary` suite **56 passed** (was 55) ·
  `repo-index` lib **170 passed** (was 169) · consolidation witness **15/15** (no dispatch arm added —
  `dispatch_fact_classes.txt` untouched; `every_dispatch_arm_is_declared_in_manifest` green) ·
  `./scripts/dogfood-isolated.sh` OK + non-pollution PASS.
- **LIVE glamCRM (isolated `/private/tmp` state root, RMAP_TRANSPORT=stdio, release binaries REBUILT
  with the fix; operator `registry.json` sha256 `98fb3ba8…a002449` IDENTICAL before/after):**
  `boundaries summary` = **244 http surfaces (221 provider / 23 consumer)**, all scope `unknown`;
  `boundaries links` = **1 CONFIRMED link** `POST /api/v2/projects/{projectSelector}/etape`
  `frontend/workspace/src/api/projects.ts:99 → serverless/packages/infra/lib/constructs/api.ts:1173`
  `[http_route_match, confidence 0.75]`, honest unlinked **4 ambiguous / 0 unmatched / 18 dynamic**;
  `surfaces list` = "HTTP/REST API surfaces: 221 providers, 23 consumers" with `<dynamic>` for
  unreadable routes. **BYTE-IDENTICAL to iterations 6–7 — recall held exactly; the import-boundary
  tightening removed no true glam surface** (glam's real Spring imports name a type/wildcard under the
  package, so they satisfy both the old and the new predicate).

### Validation limitations (honest)

- **gRPC parity (grpc-java, 116 surfaces) NOT RE-RUN this iteration.** The diff touches only one pure
  Java-import predicate in `repo-index` (strictly more restrictive) plus a test and this doc; it cannot
  affect the gRPC track, `ChannelKind` variants, or the shared link/summary read path. gRPC output is
  byte-identical by construction; prior iterations verified 116 (86 consumer / 30 provider). [NOT RUN —
  reason recorded.]
- **`SMOKE_ONLY="glamCRM leveldb"` full smoke run NOT RE-RUN this iteration.** The live glamCRM isolated
  proof above (identical counts) plus the isolated dogfood cover the same surface; the leveldb
  byte-parity was verified unchanged in iteration 6 and the diff cannot regress a non-HTTP repo. [NOT
  RUN — reason recorded.]
