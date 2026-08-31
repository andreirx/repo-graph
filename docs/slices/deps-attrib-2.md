# DEPS-ATTRIB-2 — glamCRM deps: true attribution or a true excuse, never a false one

Status: SPECIFIED (2026-08-31) · Track: Usefulness audit v0.11.0 fix queue, item #2. CODE
slice. Maturity: MATURE (deps rewrite shipped as DEPS-LIST-REWRITE-1).

## 1. Problem (measured — audit run 2026-08-30T20-44-57Z, glamCRM)

`deps list` on glamCRM renders zero content with a FALSE diagnosis:
- "0 of 7 npm manifests attributed to a module (**7 govern no indexed source**)" — while the
  SAME run indexed 188 serverless + 168 frontend files under exactly those manifests.
- "4336/4336 refs unattributed" with no statement of why.
- ZERO mention of Java/Gradle on a repo whose backend half is Java with a `build.gradle`
  (VERIFIED: no java/gradle token anywhere in the capture) — the audited house rule is that a
  missing reader gets an honest no-reader sentence (leveldb's C/C++ line is the model).
The manifests-to-module attribution worked on every other repo (django, FRAKTAG, repo-graph)
— the failure is specific to glamCRM's shape (nested workspaces: `serverless/packages/*`,
`frontend/*` — monorepo sub-packages, likely npm workspaces).

## 2. Contract

1. **Diagnose before fixing (binding first step, in the report).** Reproduce on an isolated
   index and identify WHY the 7 manifests attribute to no module: the attribution join
   (manifest→module mapping — path containment? module identity?) versus glamCRM's nested
   workspace layout. Name the exact predicate that fails, with evidence.
2. **Fix the attribution for the demonstrated shape** (nested/workspace manifests whose
   governed source IS indexed): those manifests attribute to the modules that own their
   files, and the per-ecosystem tables render (declared/used/undeclared as the rewrite
   defines them, npm builtins bucket per ecosystem). No attribution heuristics from NAMES —
   containment/ownership facts only.
3. **The excuse must be true when it renders.** "governs no indexed source" may only render
   when the manifest's subtree truly contains zero indexed files (computed, not assumed);
   otherwise the honest line states what actually failed ("N files indexed under this
   manifest, not attributed: <reason>").
4. **Java truth in the default view — AMENDED (ruling DR-JAVA-NOREADER = Option 2,
   2026-08-31)**: the spec's original no-reader premise was FALSE — `resolve_gradle_deps`
   exists (repo-index compose.rs:589, manifest_deps.rs). DIAGNOSE why glamCRM's audit capture
   contained zero java/gradle content despite the reader (where do its results go?), then make
   the DEFAULT `deps list` view state Java's truth: attributed Gradle deps render like any
   ecosystem; a failed/empty read renders unknown-with-reason or computed-true absence. A
   materially-present ecosystem may never be silently absent from the default view.
5. JSON additive; exit codes unchanged.

## 3. Stop conditions

Frozen: storage schema, module identity/ownership computation (module_file_ownership is the
attribution basis — extending a QUERY over it is in scope; changing ownership itself is not),
exit codes, trust. STANDING HONESTY RULES. New public APIs beyond additive DTO fields →
DECISION_REQUIRED. Unmet DoD → STOP + DECISION_REQUIRED. Never touch the operator's real
state root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Reproducing fixture FIRST: a nested-workspace fixture reproducing the zero-attribution
  (fails pre-fix, passes post-fix); the false-excuse predicate test (excuse only when truly
  zero indexed files under the manifest).
- Unit: attribution for nested manifests; true-excuse gating; Gradle/Maven no-reader line
  materiality; per-ecosystem tables unchanged on the already-working repos.
- Live proof (isolated state root, registry sha unchanged): glamCRM — deps list shows real
  npm attribution for serverless/frontend packages AND the Gradle no-reader sentence;
  byte-parity spot check on django + FRAKTAG deps output vs the audit captures (no
  regression). Captures in the report.
- Chunked cargo gates; consolidation witness; dogfood-isolated green.

## 5. Definition of done

glamCRM's deps surface tells the truth twice over: real attribution for the npm halves, an
honest no-reader sentence for the Java half; the "governs no indexed source" excuse can only
render when it is computed true; other repos byte-stable; gates green.
