# HTTP-SURFACE-COHERENCE-1 — one true HTTP story per snapshot

Status: SPECIFIED (2026-08-26) · Track: Usefulness-audit fix queue #3
(`docs/ROADMAP.md` § Usefulness audit v0.9.0; evidence `smoke-runs/2026-08-25T22-41-37Z`).
CODE slice. Maturity: MATURE surfaces (HTTP-BOUNDARY-1 shipped v0.9.0).

## 1. Problem (measured)

- **Spring MVC providers missed entirely** (OBSERVED): spring-petclinic renders `0 surfaces`
  against 6 `@Controller` classes / 17 mapping methods (measured — see §4 note; the audit's
  "18" was an over-count); the only rows shown are 3 integration-TEST
  consumers — an agent learns the app has no HTTP surface, the opposite of the truth. The
  detector keys on `@RestController` only, while `inferences list` on the same snapshot names all
  6 controllers at 0.95 confidence (the spring_container_managed data is already there).
- **Next.js App Router providers misclassified as consumers** (OBSERVED): amodx's 11
  `renderer/src/app/api/*/route.ts` handlers (export `GET`/`POST` — verified) render
  `[consumer]`; the server half of the app points the wrong way.
- **Self-contradicting rendering** (OBSERVED): FRAKTAG `surfaces list` prints 47 provider rows
  under a footer `HTTP/REST API surfaces: 0 providers, 52 consumers`; glamCRM headlines
  `0 surfaces` directly above 244 of them.
- **`boundaries list` is strictly dominated** (OBSERVED): 244/133 rows where column 1 = column 4
  (`http … http`) and scope is `unknown` on every row; amodx: 74% of rows are two strings
  repeated verbatim; the same files WITH method+path live in `surfaces list`.
- **The most interesting architectural facts are invisible** (OBSERVED): glamCRM's dual
  Spring/CDK route implementation is listed twice with no note; petclinic's consumer rows are all
  test scaffolding with no label.

## 2. Contract

1. **Spring MVC providers**: `@Controller` classes' `@RequestMapping`/`@GetMapping`/… methods
   emit `http_provider` surfaces exactly as `@RestController` does today (class base path +
   method path composed; same import-evidence gate HTTP-BOUNDARY-1 ratified). Basis notes MVC
   (view-rendering) vs REST where the annotation distinguishes them — a labeled fact, not two
   pipelines.
2. **Next.js App Router providers**: a `route.ts`/`route.js` under an `app/**` directory that
   exports HTTP-verb handlers (`GET`/`POST`/`PUT`/`PATCH`/`DELETE`/`HEAD`/`OPTIONS`) emits
   `http_provider` surfaces with the route derived from the app-dir path (`app/api/x/route.ts` →
   `/api/x`; dynamic segments `[param]` → `{param}`). Evidence = the app-dir location + the
   exported verb name (structural, not name-guessing). These files stop being consumers unless
   they also make outbound calls.
3. **One aggregation, every renderer**: provider/consumer counts on `surfaces list` (headline AND
   footer), `boundaries summary`, and the modules note derive from ONE shared count of the SAME
   rows being printed — a contradiction becomes impossible by construction, enforced by a test
   that parses the rendered output and cross-checks counts against rows.
4. **`boundaries list` becomes the grouped view of the same truth**: rows grouped per file ×
   direction with `×N` counts, constant-valued columns dropped, methods+routes summarized —
   strictly a summary of `surfaces list`, sharing its data source. (Deleting the command is
   REJECTED: the verb stays, its output earns its place.)
5. **Labels for the facts that matter**: consumer surfaces in test files carry `[test]` (from the
   existing files.is_test flag); when the same (method, route) has two providers in different
   modules, the rendering notes the dual implementation once ("also provided by <module>") —
   glamCRM's Spring/CDK duplication becomes a stated fact.

## 3. Stop conditions

Frozen: storage write schema beyond the boundary-surface family's existing shape, LiveGraph/
witness/union/reconciliation, trust, exit codes. Flask/FastAPI/Express-beyond-current stay named
extension points. If App Router route derivation meets a shape the rule cannot express (parallel
routes, route groups `(group)`), emit the provider with route `unknown` + the honest reason —
never a fabricated path — and record the shape. Never touch the operator's real state root. Do
NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Fixture tests: @Controller provider emission (base+method composition, MVC label); App Router
  provider (static + `[param]` + verb exports; a route.ts with no verb exports emits nothing);
  count-coherence (rendered counts == rendered rows, headline == footer); grouped boundaries
  view; `[test]` labeling; dual-implementation note.
- LIVE isolated proofs: petclinic — `surfaces list` shows the 6 controllers / **17 method-level
  routes** as providers (measured ground truth, operator ruling 2026-08-26 (c): `grep` of
  `src/main` = 12 `@GetMapping` + 5 `@PostMapping` = 17; the only 2 `@RequestMapping` hits are one
  Javadoc comment and one CLASS-LEVEL base path `@RequestMapping("/owners/{ownerId}")` on
  PetController — a prefix composed into its method routes, NOT an 18th servable route. Emitting it
  would fabricate a route the framework does not serve, the exact dishonesty this slice prevents),
  test consumers labeled `[test]`, `boundaries summary` coherent; amodx — the 11
  App Router handlers flip to providers with real routes; FRAKTAG — footer says 47 providers
  under 47 provider rows; glamCRM — headline counts the 244, dual Spring/CDK routes noted.
  Before/after captures vs `smoke-runs/2026-08-25T22-41-37Z` in the report.
- Byte-parity on leveldb (no HTTP). Chunked cargo gates; witness green; dogfood green; logged
  smoke SMOKE_ONLY="spring-petclinic amodx" green.

## 5. Definition of done

One HTTP story per snapshot: every real provider (REST, MVC, App Router) is a provider, every
count matches the rows under it, the grouped view adds signal instead of noise, and the dual-
implementation and test-scaffolding facts are stated; gates green.
