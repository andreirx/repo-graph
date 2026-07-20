# HTTP-BOUNDARY-1 — HTTP/REST as a first-class inter-module boundary (the API-points map)

Status: SPECIFIED (2026-07-20) · Track: Boundary interaction / IPC (multitrack — see
`docs/design/boundary-detection-multitrack.md`). Third of the CLI→JS→HTTP sequence. Maturity: MATURE.

## 1. Problem (named plainly — a miss, not a scope choice)

The boundary track's stated purpose is to detect the API points modules use to talk to each other.
It works well where it fires: on grpc-java it emitted **116 gRPC surfaces, 86 consumer / 30 provider**
— the provider/consumer map is real. But the `ChannelKind` enum
(`boundary-interaction/src/types.rs:225`) covers Unix sockets, pipes, shared memory, semaphores,
signals, TCP/UDP, gRPC, eRPC, AMQP, ZeroMQ — **every mechanism except HTTP/REST**, the single most
common inter-module API. glamCRM's three subsystems (Spring Java backend, TS lambda serverless,
React frontend) communicate over HTTP/REST; they therefore produce ZERO boundary surfaces. A
boundary track blind to HTTP fails its own promise on the most common case. This slice fixes that.

The detection primitives already exist and are UNWIRED to the boundary surface:
- `classification/src/spring_liveness.rs` already parses Java `@RestController` / `@RequestMapping`
  / `@GetMapping` / `@PostMapping` annotations from `metadata_json` (today only for liveness) — the
  PROVIDER side (Java).
- `repo-index/src/react_detector.rs` already sees `fetch(` / `axios` — the CONSUMER side (JS/TS).
- `indexer/src/grpc_link.rs` (GR-3A) already links provider↔consumer surfaces by shared contract —
  the exact linking model HTTP needs, keyed on URL route instead of proto service.
- The extractor emits surfaces via a TOML binding table (`boundary-interaction/bindings.toml`,
  loaded by `table.rs`) keyed by source language + pattern + provider/consumer role.

## 2. Contract

1. **New channel kind: `ChannelKind::Http`** (REST over HTTP), added to
   `boundary-interaction/src/types.rs` with its `as_str` ("http"), protocol class, and the
   scope/transport taxonomy the multitrack model uses. Scope defaults to `unknown` (localhost vs
   remote is not statically decidable in general — mirror gRPC's 116 `unknown`-scope surfaces;
   never fabricate a scope). Additive to the enum — no existing kind changes.

2. **PROVIDER detection — the routes a module EXPOSES.** Emit an HTTP provider `BoundaryInteractionSurface`
   carrying (HTTP method, route path) for the frameworks glam uses:
   - **Java / Spring:** `@RestController` + `@RequestMapping`/`@GetMapping`/`@PostMapping`/
     `@PutMapping`/`@DeleteMapping` (class-level base path + method-level path composed). REUSE the
     `spring_liveness` annotation parse over `metadata_json` — do not re-implement annotation reading.
   - **TS/JS serverless:** the route declaration form glam's lambda/API-Gateway handlers actually use
     (VERIFY against `serverless/packages/**` before coding — express-style `app.get('/x')`, an API
     Gateway event `httpMethod`+`path`, or the framework in use; extract from the real source, not an
     imagined shape). Name any framework NOT found as an extension point.
   Providers name broader frameworks (Flask/FastAPI `@app.route`, etc.) as recorded extension points,
   NOT built here.

3. **CONSUMER detection — the routes a module CALLS.** Emit an HTTP consumer surface carrying the
   called (method, URL/route) for:
   - **JS/TS:** `fetch(url, {method})` / `axios.get|post(...)` / an axios instance — REUSE
     `react_detector`'s existing fetch/axios sighting; extend it to capture the URL + method for the
     surface (today it only detects presence).
   - **Java:** `RestTemplate` / `WebClient` / `HttpClient` call sites (the consumer side of Spring).
   Honest: a dynamically-built URL (template string, variable) that can't be statically read →
   surface the consumer with an UNKNOWN/partial route, never a fabricated path.

4. **LINKING — the payoff (provider↔consumer route match).** Link a consumer surface to a provider
   surface when their (method, route) match, PATH-TEMPLATE-AWARE: a Spring `/offers/{id}` provider
   matches a `fetch('/offers/123')` consumer by segment-wise comparison treating `{id}`/`:id`/`*`
   as wildcards. Model this on `grpc_link` (GR-3A: "discovery slice, not connection-proof" —
   same honesty posture). **The load-bearing decision:** matching is a HEURISTIC — link only when
   the (method + template) match is unambiguous; when a consumer URL matches multiple providers or
   none, surface BOTH sides UNLINKED with the reason, never guess a link (VISION: unknown never
   fabricated). Cross-subsystem links (frontend→backend, frontend→serverless) are the goal, but a
   link is asserted only on route evidence, not on module adjacency.

5. **Deep vertical — it must RENDER (no dormant capability).** The `boundaries` CLI
   (`rgr/src/commands/boundaries.rs`, `run_boundaries`) shows the HTTP surfaces AND the linked
   provider↔consumer map, so on glamCRM an agent sees "frontend `GET /offers` → backend
   `@GetMapping /offers`" as a concrete inter-module API edge. The DoD names this output; validation
   proves it on glam's real routes. (If the linked map warrants a distinct view/flag beyond
   `boundaries summary`, the builder adds it — least-new-surface, recorded.)

## 3. Stop conditions

Frozen: existing `ChannelKind` variants + their detection, the gRPC track (grpc_link/grpc_*_hint —
HTTP is a SIBLING, not a modification), the binding-table loader/validator contract, the state-boundary
slice, storage schema unless surface persistence genuinely needs a column (trace the gRPC surface
persistence first — reuse it), witness/union/reconciliation, trust ratio. Route matching is the one
ratification-class heuristic — an ambiguous or absent match surfaces UNLINKED, never a fabricated
link; if the real glam route shapes make confident linking impossible for a class of routes, that is
a FINDING (surface it), not a forced link. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- **Live proof on glamCRM** under an ISOLATED state root (/private/tmp — NEVER the operator registry;
  sha256 before/after): `rmap boundaries` shows HTTP provider surfaces for the Spring backend routes
  AND the serverless routes, consumer surfaces for the frontend `fetch`/`axios` calls, and at least
  one CONFIRMED cross-subsystem link (frontend consumer ↔ backend/serverless provider) by route
  match. The build report SHOWS the real routes linked and names any left honestly unlinked.
- Provider unit tests: Spring annotation compositions (class base path + method path, all verbs) →
  correct (method, route); the TS/serverless route form found in glam → correct surface.
- Consumer unit tests: fetch/axios static URL → surface with (method, route); dynamic URL → UNKNOWN
  route, not fabricated.
- Linking tests: template match (`/offers/{id}` ↔ `/offers/123`) links; ambiguous (one URL, two
  providers) → UNLINKED with reason; no-match → both surfaces, no link. A negative test proving no
  fabricated link on module adjacency alone.
- gRPC parity: grpc-java's 116 gRPC surfaces UNCHANGED (HTTP is additive; a named byte/parity check).
- Chunked cargo gates (standing pattern); consolidation witness 15/15; isolated dogfood; SMOKE_ONLY
  logged run on glamCRM.

## 5. Definition of done

HTTP/REST is a first-class `ChannelKind`; Spring + serverless routes emit provider surfaces, fetch/
axios + Java HTTP clients emit consumer surfaces, and route-template-aware linking produces the
inter-module API map on `rmap boundaries`; on glamCRM at least one real frontend→backend/serverless
API edge is shown, ambiguous/absent matches stay honestly unlinked; the gRPC track and existing
channel kinds are unchanged; the trust denominator untouched; gates green. The API-points map the
operator built repo-graph for exists for the mechanism glam actually uses.
