<!-- requirements-assurance-v1
{
  "formatVersion": 1,
  "kind": "requirement",
  "requirementId": "RG-REQ-007",
  "sources": [
    { "kind": "document-section", "path": "docs/VISION.md", "fragment": "value-frontier" },
    { "kind": "document-section", "path": "docs/VISION.md", "fragment": "honesty-rules" },
    { "kind": "document-section", "path": "docs/slices/headline-truth-1.md", "fragment": "2-contract" },
    { "kind": "document-section", "path": "docs/slices/audit5-minors-1.md", "fragment": "2-contract" },
    { "kind": "document-section", "path": "docs/slices/anchors-everywhere-1.md", "fragment": "2-contract" },
    { "kind": "document-section", "path": "docs/slices/inferences-surface-1.md", "fragment": "2-contract" },
    { "kind": "document-section", "path": "docs/slices/honesty-gate-2.md", "fragment": "5-definition-of-done" }
  ],
  "lowLevelRequirements": [
    { "id": "RG-REQ-007-L01", "parentId": "RG-REQ-007" },
    { "id": "RG-REQ-007-L02", "parentId": "RG-REQ-007" },
    { "id": "RG-REQ-007-L03", "parentId": "RG-REQ-007" },
    { "id": "RG-REQ-007-L04", "parentId": "RG-REQ-007" },
    { "id": "RG-REQ-007-L05", "parentId": "RG-REQ-007" },
    { "id": "RG-REQ-007-L06", "parentId": "RG-REQ-007" },
    { "id": "RG-REQ-007-L07", "parentId": "RG-REQ-007" },
    { "id": "RG-REQ-007-L08", "parentId": "RG-REQ-007" },
    { "id": "RG-REQ-007-L09", "parentId": "RG-REQ-007" },
    { "id": "RG-REQ-007-L10", "parentId": "RG-REQ-007" },
    { "id": "RG-REQ-007-L11", "parentId": "RG-REQ-007" }
  ]
}
-->
# RG-REQ-007 — Boundaries, surfaces, resources and inferences are evidence tracks, labeled as such

Status: DRAFT authored by the in-place manager 2026-09-12 from the ratified slice specifications, the v0.18.0 audit and its root-cause record. Independent requirements review: PERFORMED 2026-09-12 by Codex gpt-5.6-terra (standalone, read-only) — verdict REFINE; its findings are applied in this revision (record: reviews/2026-09-12-requirements-review-0-terra.txt). Human approval: PENDING.
Maturity: PROTOTYPE (requirements record).

## Source and intent

Sources: [VISION — Value Frontier](../VISION.md#value-frontier) items 2 and 3 (boundaries and seams; runtime/build environment — "mechanism detectors are evidence tracks feeding the seam model, not Layer 0/1 substrate"); [VISION — Honesty Rules](../VISION.md#honesty-rules); the Certainty Model's Layer-3 claim ("these files likely contain X usage; coverage is partial; open the files"); the ratified contracts of HTTP-BOUNDARY-1, HTTP-SURFACE-COHERENCE-1, HEADLINE-TRUTH-1 (D6), ANCHORS-EVERYWHERE-1, AUDIT5-MINORS-1 (F2, F7), RESOURCE-HONESTY-1, RESOURCE-RECALL-1, HONESTY-GATE-2, INFERENCES-SURFACE-1, FD-1B, ZEROSTATE-SCOPE-1, TC-1 (planned). Links name origins, not proof that the refinement is ratified.

## High-level requirement

An agent asking `rmap surfaces`, `boundaries`, `resource` or `inferences` about a repository shall receive the detected API surfaces, state boundaries, resource accesses and framework inferences as evidence-backed hints whose coverage, partition and detection basis are stated on the surface that renders them — never as counts that imply completeness, and never as rows the detector did not observe.

**Scope:** the Layer 2–3 detector tracks and their renderers (human and `--json`). Does not cover the module model (RG-REQ-004), symbol relationships (RG-REQ-005) or the trust/reliability computation (RG-REQ-009), which consume these tracks.

**High-level acceptance:** every L below holds on the smoke corpus (`scripts/smoke-validation-repos.sh`) and on the supplemental probes of the current audit round; a partial slice does not satisfy this H.

## Low-level requirements

All L entries refine RG-REQ-007; their parent is this H. IDs are stable across wording changes.

### RG-REQ-007-L01 — Provider surfaces carry method, route and anchor, or say unknown

For every supported framework (Spring `@RestController` and `@Controller` MVC with composed class+method paths, Next.js App Router verb exports with `[param]` rendered as `{param}`, Express, TS/JS serverless), each provider row shall render `METHOD /route  path:line  [provider]`. A route that cannot be read statically shall render `unknown` with its reason; no route text shall be composed from anything but the source literals.

**Verification criterion:** `rust/crates/rgr/tests/surfaces_command.rs` and the renderer tests in `rust/crates/rgr/src/presentation/http_boundary.rs`; field probe: FRAKTAG `surfaces list` renders `GET /api/knowledge-bases  packages/api/src/server.ts:45  [provider]` (47/47 provider rows anchored); spring-petclinic renders `POST /owners/new … OwnerController.java:77`.

**Evidence (v0.18.0):** OBSERVED MET — audit round six captures `a5-fraktag-surfaces`, `ht-pc-surfaces`.

### RG-REQ-007-L02 — A surface provided twice is one fact

When the same `(method, route)` is provided by two modules, the surface shall render once with `also provided by <module> (dual implementation)`; it shall never render as two undifferentiated rows, and the note shall appear once per key.

**Verification criterion:** `http_boundary.rs` `dual_providers`/`note_for` tests; field probe: glamCRM `surfaces list` names its serverless/Spring dual routes.

**Evidence (v0.18.0):** OBSERVED MET — capture `ht-glam-surfaces`.

### RG-REQ-007-L03 — Test fixtures are partitioned, disclosed and still shown

Surfaces, boundaries and inferences whose file is a test file shall be excluded from headline counts under an explicit exclusion clause, rendered after production rows and labeled `[test]`. A row whose test status is unknown shall stay in the counts; it shall never be dropped silently. No headline count shall sit above listed fixtures without the clause.

**Verification criterion:** `http_boundary.rs` `from_entries`/`exclusion_clause` tests; `rust/crates/rgr/tests/inferences_command.rs`; `rust/crates/rgr/tests/dead_command.rs` (`Spring: 1 (1 in test fixtures)`); field probe: `ht-rg-inferences` renders `spring_container_managed 1 — App (App.java:5) [test]`.

**Evidence (v0.18.0):** OBSERVED MET — HEADLINE-TRUTH-1 D6 and §2.5 verified in the field.

### RG-REQ-007-L04 — No "0 project surfaces" above real routes; providers before consumers

The `surfaces list` headline shall omit or rename a zero count that would contradict the rows beneath it (`0 non-HTTP surfaces`, never `0 project surfaces` beside 235 routes), and rows shall order providers → consumers → test fixtures.

**Verification criterion:** `rust/crates/rgr/src/presentation/surfaces.rs` ordering tests; corpus assertion `grep -rl "0 project surfaces" <smoke run>` returns no file.

**Evidence (v0.18.0):** OBSERVED MET — corpus grep empty on run `2026-09-08T17-59-32Z`.

### RG-REQ-007-L05 — One count source per surface family

Provider/consumer counts on `surfaces list` (headline and footer), `boundaries summary` and the modules note shall derive from one aggregation of the rows actually rendered, so that two renderers of the same snapshot cannot disagree about the same count.

**Verification criterion:** a test that parses rendered output and cross-checks every count against the rendered rows (`surfaces_command.rs`); the HTTP family has it; any new family must add the same test before shipping.

**Evidence (v0.18.0):** OBSERVED MET for HTTP; UNKNOWN for other families — the audit names coherence under composition as the dominant defect class; see RG-REQ-002.

### RG-REQ-007-L06 — Zero-states name the build's detector inventory, never blame the codebase

An empty surfaces/boundaries result shall state which detectors this build has and which materially-present languages of this repo have none (`No detector for C, C++ on this build — their boundaries are not counted`), computed from one detector roster shared with `surfaces list` and gated by per-repo language materiality; no repo shall render another repo's sentence, and the clause is omitted when no uncovered language is materially present.

**Verification criterion:** `rust/crates/rgr/src/presentation/boundaries_list/tests.rs::list_render_empty_states_coverage_not_codebase_blame` and the unknown-gap case; field probe: vcmi `boundaries summary`.

**Evidence (v0.18.0):** OBSERVED MET — audit keep-and-imitate item "language-named blind spots".

### RG-REQ-007-L07 — Grouped rows carry the set of lines, never one picked line

A grouped boundary row shall render the set of source lines it aggregates (`ximagepool.c ×13 @ 112,140,163,… (+8 more)`, at most 5 shown); `boundaries summary` stays line-free; no renderer shall select one line to stand for a group.

**Verification criterion:** `boundaries_list/tests.rs` line-set tests; field probe: gstreamer `boundaries list` 26/26 rows end in `@ <lines>`.

**Evidence (v0.18.0):** OBSERVED MET — capture `a5-gst-boundaries`.

### RG-REQ-007-L08 — Every anchor comes from one store and is never fabricated

A `path:line` pair shall be read from a single store for both halves; a missing or zero line renders no line (never a guessed one); provider rows read the detector's stored `lineStart`.

**Verification criterion:** ANCHORS-EVERYWHERE-1 single-source test; `daemon-runtime/src/http_boundary_read.rs` provider line read; field probe as L01.

**Evidence (v0.18.0):** OBSERVED MET — AUDIT5-MINORS-1 F7 (0/47 → 47/47 provider lines).

### RG-REQ-007-L09 — Resource access modes come only from detector evidence

`resource list/readers/writers` shall emit a reader or writer only when the detector observed the access mode; an undetermined mode shall render in an explicit `access (mode unknown)` bucket; a literal passed to a path-join/resolve helper is not access evidence. Per-language coverage and the literal-path gate ("detection sees only calls whose path argument is a string literal at the call site") shall render on both the zero-state and the non-zero header, derived from the detector registry.

**Verification criterion:** `rust/crates/rgr/tests/resource_command.rs`; `resources.rs` zero-state/header tests; field: every audit-round resource probe traces each path to a real literal call.

**Evidence (v0.18.0):** OBSERVED MET as honesty (HONESTY A corpus-wide); recall is low by design of the literal gate (HIT D) — RESOURCE-DYNAMIC-PATH-1 unscheduled. A recall change shall keep the gate sentence re-worded, never drop it.

### RG-REQ-007-L10 — Inference inventory is grouped, uncapped in count, and explains emptiness

`inferences list` shall state what inferences are and which detectors apply to this repo's languages (never claiming a detector "ran"), render kind × count with per-kind top files and anchors, report `count` as the true total with `returned`/`truncated`/`limit` always present, and distinguish "detector applies, found nothing" from "no detector for these languages". A framework claim requires structural evidence — a directory or file name alone never detects a framework — and its basis renders with the claim.

**Verification criterion:** `rust/crates/rgr/tests/inferences_command.rs` (FD-SUPPORT-3 cases); HONESTY-GATE-2 standing rule tests; field probe: hadoop never renders `nextjs_app_router_detected`.

**Evidence (v0.18.0):** OBSERVED MET — audit inferences B+/A-/A-/A on Rust/TS; honest `—` on C/C++ and Python.

### RG-REQ-007-L11 — Runtime and build environment per module is stated from persisted provenance

The product shall be able to state, per module, the build system that owns it and the toolchain/evidence provenance it relies on (tool name and version for imported evidence, `compile_commands.json` identity, extractor versions) from persisted lineage — never from probing the host at query time — and shall state incomparability reasons explicitly.

**Verification criterion:** NO EXISTING VERIFICATION — TC-1 (`docs/slices/tc-1-toolchain-inventory.md`) is PLANNED; a `provenance` surface does not exist; NC-1 is DEFERRED on the missing coverage-evidence contract.

**Evidence (v0.18.0):** NOT MET — the one Value-Frontier-3 track with no shipped surface; it is the upstream input gap behind `risk`/`assess` HIT = F (RG-REQ-009-L10) and the generated-code predicate (RG-REQ-009-L01).

## Preservation obligations named by the ratifying specifications

- `ChannelKind` variants and their detection, and the gRPC track (grpc-java's 116 gRPC surfaces are a parity check) — HTTP is a sibling, not a modification.
- `docs/architecture/state-boundary-contract.txt` is NORMATIVE and FROZEN at `state_boundary_version: 1`: READS/WRITES edges only, `CONFIG_KEY` is not a target kind, Form-A matching, no per-call-site pseudo-resources, SQL-string inspection out of scope.
- The binding-table loader/validator contract (`boundary-interaction/bindings.toml`).
- Storage write schema for the boundary-surface family; LiveGraph witness/union/reconciliation; trust ratio and denominator; exit codes — additive only.
- RESOURCE-HONESTY-1 / RESOURCE-RECALL-1 are render slices: what is SAID changes, what is DETECTED does not, unless a detector slice is ratified.
- The surfaces-show evidence line column remains DECISION_REQUIRED (ANCHORS-EVERYWHERE-1 Tier 2).
- Framework-liveness inferences are assessments (confidence-bearing); extracted surfaces are measurements (no confidence) — `docs/architecture/measurement-model.txt`.

## Review and approval

Independent requirements review: PERFORMED (Codex gpt-5.6-terra, REFINE; findings applied).
Human approval: PENDING (the human requested this requirements set on 2026-09-12; approval of a specific revision is a separate record).
Implementation and verification: evidence lines above are the manager's OBSERVED reading of the v0.18.0 audit; they are not acceptance.
