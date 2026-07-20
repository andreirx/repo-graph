# JS-ENRICHMENT-1 — admit JavaScript/JSX into tsserver enrichment (make JS dependencies visible)

Status: SPECIFIED (2026-07-20) · Track: Resolution & attribution — JS-seriousness (ROADMAP candidate
track, operator-surfaced 2026-07-20). Second of the CLI→JS→HTTP sequence. Maturity target: MATURE.

## 1. Problem (measured)

glamCRM's React frontend is JavaScript/JSX (45 `.jsx` + 7 `.js`, 0 `.tsx`). At the pipeline level
JSX resolves 24.2% — but that is its CEILING: JS/JSX gets NO semantic enrichment, while TypeScript
climbs via tsserver enrichment + SCIP reconciliation. The gap the operator named ("treat JS
seriously"): JS dependencies stay invisible because JS files never reach the resolver.

**The resolver is already JS-capable** (verified): `tsserver-resolver` is documented "TypeScript/
**JavaScript** type resolver"; it already resolves per `jsconfig.json`/`package.json` context
(`project.rs:26 CONFIG_FILES = [tsconfig.json, jsconfig.json, package.json]`), already treats
`.jsx` as JSX (`client.rs:882 is_tsx = .tsx || .jsx`), and already knows Node.js built-ins. tsserver
itself resolves `.js` when the project has `allowJs`.

**The gap is admission, not capability:** `EnrichmentLanguage` (`daemon-runtime/src/enrich_pass.rs`)
has exactly three variants — `Rust`, `TypeScript`, `Java`. There is NO `JavaScript`. The enrichment
file-set for `TypeScript` gathers TS files (language == "typescript"); `.js`/`.jsx` files
(language "javascript"/"jsx" in `files.language`) are never included, so tsserver never sees them
even though it could resolve them.

## 2. Contract

1. **Admit JS/JSX into tsserver enrichment.** The cleanest shape (builder verifies + records): the
   TypeScript enrichment language's file-set includes `.js`/`.jsx`/`.mjs`/`.cjs` (language
   "javascript"/"jsx"), routed to the SAME `TsServerResolver` — NOT a new resolver, NOT a new
   `EnrichmentLanguage` variant unless the toolchain-presence/skip-message semantics genuinely
   require distinguishing "JS present but no allowJs" from "TS present" (if a new variant IS
   needed, it shares the tsserver resolver — record the one-line rationale either way). This
   respects the enrich_pass doctrine (header lines 17-20: "one enrichment pass; per-language
   resolver choice is DATA, not a plugin axis").
2. **allowJs / project-context correctness — the load-bearing decision.** tsserver resolves `.js`
   only when its owning project enables `allowJs` (jsconfig with `allowJs`, or a package.json
   Node context). The slice must handle three real cases HONESTLY:
   - **jsconfig / allowJs-enabled project** (glam frontend, if it has one) → resolve, deps visible.
   - **tsconfig WITHOUT allowJs, mixed TS+JS** → tsserver will not resolve the `.js`; degrade that
     file honestly (labeled "JS not resolved: project has no allowJs"), NEVER fabricate.
   - **bare JS, no config** → a package.json Node context (already a `CONFIG_FILES` entry) or an
     honest skip with an install/config next-action, mirroring the toolchain-absent skip vocabulary.
   Whether the slice SYNTHESIZES a permissive `allowJs` context for bare JS or degrades honestly is
   the ratification-class decision — recommended: degrade honestly first (no fabricated resolution),
   name the synthesize-a-context option as a follow-up extension point. Record the choice.
3. **Promotion + honesty parity with TS.** JS-resolved receiver types flow through the SAME
   enrichment promotion path as TS (the call-graph actually upgrades; ENRICH-YIELD funnel accounting
   applies to JS the same way). Unknowns stay unknown. The trust denominator is untouched.
4. **Deep vertical — it must RENDER (no dormant capability).** After this slice, running enrichment
   on glamCRM's frontend makes JS dependencies VISIBLE: the attribution surface ("Unresolved
   references — where they go") names the frontend's JS deps (react, etc. from resolved receivers,
   not just the import-specifier match), and the RESOLUTION-BREAKDOWN-CLI-1 per-language JSX figure
   MOVES UP from its 24.2% pipeline ceiling. The DoD names this surface; validation proves the lift.

## 3. Stop conditions

Frozen: the enrich_pass single-pass doctrine + the between-language/before-resolver cancellation
checks (enrich_pass header lines 66-70), the promotion path semantics, the tsserver session model
(one per context — do not thrash sessions), the toolchain-absent skip vocabulary, trust ratio.
If tsserver cannot be made to resolve a real glam JS file even with a correct allowJs context, that
is a FINDING (the JS-enrichment ceiling is lower than hoped — surface with evidence), not a silent
partial. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- **Live lift proof on glamCRM frontend** under an ISOLATED state root (/private/tmp — NEVER the
  operator registry; sha256 before/after), with tsserver on PATH (the builder runs unsandboxed):
  enrich the frontend, show the JSX/JS per-language resolution RISING above the 24.2% pipeline
  baseline, and at least one frontend JS dependency newly NAMED on the attribution surface via a
  resolved receiver (not a pre-existing import-specifier match). Before/after numbers recorded.
- allowJs-absent honest-degradation test: a mixed TS+JS project with no allowJs → the `.js` file
  degrades labeled, no fabricated resolution.
- Bare-JS honest handling test (per the recorded §2.2 decision).
- Promotion parity: JS resolutions promote through the same funnel; ENRICH-YIELD accounting holds.
- Byte-parity: TS-only repos' enrichment output unchanged (this ADDS JS, does not alter TS).
- Chunked cargo gates (standing pattern); consolidation witness 15/15; isolated dogfood; SMOKE_ONLY
  logged run on glamCRM (or a JS repo).

## 5. Definition of done

`.js`/`.jsx` files reach the tsserver resolver; JS-resolved receivers promote and become visible on
attribution + the per-language breakdown (JSX rises above its pipeline ceiling on glamCRM, proven
live); allowJs-absent and bare-JS cases degrade honestly with named next-actions; TS enrichment
unchanged; the trust denominator untouched; gates green. JS dependencies are no longer invisible.

## 6. RATIFICATION + DELIVERY (2026-07-20) — DoD amended per measured evidence

The builder escalated honestly; the operator ratified two decisions:

**A — the tsserver path fix is RETAINED (governance: TS/trust touch ratified).** The builder
found a real defect: tsserver associates files with a project by ABSOLUTE path, but repo-graph
passed a RELATIVE repo root — silently breaking tsserver's inferred project AND costing configured
TS a resolution (measured 0→1). The canonicalization fix RECOVERS a compiler fact the bug was
dropping (not fabricated). The slice's "TS enrichment unchanged" clause was a guardrail against
scope-creep/fabrication, NOT against fixing a shared bug — lifting it is correct. The false
"byte-neutral for TS" comment is corrected to the truth. (Restrict-to-JS was rejected: it would
knowingly retain the TS defect.)

**B — the DoD is AMENDED (the untyped-JS ceiling is a measured FINDING, not a failure to hide).**
The original DoD ("JS dependencies visible / JSX rises off its ceiling") is UNACHIEVABLE on untyped
JS and is retracted. Measured on glamCRM (preserved: `.agent-manager/slices/JS-ENRICHMENT-1/
measurements-preserved/`): admission works mechanically (57 receivers resolved, 2 promotions) but
the lift is +1 call (jsx 112→113, 37%→37%) because tsserver returns `quickinfo_no_type` for ~148
untyped-JSX receivers, and the few resolved types are DOM/JS built-ins, not app dependencies — the
attribution surface is byte-identical. **The ceiling is a property of untyped JS, not a repo-graph
gap.** The real lever for "treat JS seriously" is porting the frontend to TS (operator direction,
now data-justified) — typed JS is the only thing tsserver can resolve.

**AMENDED DoD (what this slice actually delivers):** (1) the tsserver relative-root bug is fixed,
recovering real TS + enabling the JS inferred-project path; (2) `.js/.jsx` admission to tsserver
enrichment is landed as a documented foundation; (3) the untyped-JS resolution ceiling is measured
and recorded as the finding. JS failures are recorded honestly (`quickinfo_no_type`), never
fabricated.

**DEFERRED DEBT (recorded, per the escalate):** the `allowJs`-absent *labeled* degradation path
(a `.js` in a tsconfig-without-allowJs project → "JS not resolved: no allowJs") was not built.
Honesty is preserved without it (failures record as `quickinfo_no_type`, not fabricated
resolutions); the labeled variant is a nicety deprioritized under decision B (JS stays limited;
TS port is the lever). File if a real repo needs the distinction.
