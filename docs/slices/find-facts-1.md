# FIND-FACTS-1 — `find` searches the fact tables first; embeddings demoted to a labeled lower tier

Status: SPECIFIED (2026-08-30, human-ratified direction same day) · Track: Semantic seeding /
orientation. CODE slice. Maturity: `find` is PROTOTYPE-fresh — contract change is cheap NOW.

## 1. Problem (ratified 2026-08-30)

`rmap find` answers only through embeddings: ranked *guesses*, dependent on the local LM Studio
endpoint being up. For an **exact identifier** (`bnr-service`, a route template, a dependency
name, a module path) the knowledge exists in the fact tables but is scattered across fact
classes, each behind a different command (`explain`/`callers` for symbols, `boundaries` for
routes, `deps` for dependencies, `map` for modules) — the reader must already know which
command renders a fact class before they can look for the thing. And when LM Studio is down,
`find` has nothing at all. Deterministic facts must outrank similarity guesses; today the verb
has only guesses.

## 2. Contract

1. **A FACTS tier, rendered FIRST.** `find <query>` performs a deterministic lexical match
   (case-insensitive substring; SQL `LIKE` — no FTS index unless measured slow, not
   preemptively) over the CURRENT snapshot's fact tables: symbol names/qualified names, file
   paths, module names/paths, HTTP surface route templates, dependency names, framework
   inference identifiers, governance boundary/requirement/quality-policy declarations. The
   corpus is the FACT TABLES — never the rendered text of other commands (a hit must not
   depend on another renderer's budget or phrasing).
   - **`boundary` class corpus** (review-6, operator-ratified 2026-08-30): the governance
     DECLARATIONS store (`declarations`, active rows), NOT `surface_entrypoints`.
     `surface_entrypoints` is EXCLUDED from `find`'s corpus — no serving surface renders that
     table, so any next-command for such a hit is unfulfillable and dead-ends the reader; it is
     eligible again only if a renderer ever ships. Declarations DO render: a `boundary`-kind
     declaration via `rmap violations`, a `requirement`/`quality_policy`-kind one via `rmap
     gate` — so the `boundary` group's per-hit next-command points at the command that renders
     THAT declaration kind (a per-hit renderer; no single class-level verb).
2. **Every hit is labeled with its fact class, its source certainty, and the command that
   renders it**, and every hit carries a RUNNABLE next command — e.g. a group header
   `[http-surface · inferred → rmap boundaries list]` over `provider GET /api/offers …
   serverless/src/handlers/offer.ts` with a per-hit `→ rmap boundaries list`; a symbol hit
   `bnrService — src/bnr.ts` with a per-hit `→ rmap explain <stable_key>`.
   - **Certainty tag** (review-1 honesty defect): each class is tagged by the certainty LAYER
     of its SOURCE table (VISION § Fact Certainty Model / architecture Product Layer Stack) —
     `extracted` (Layer 0–1: symbol/file `nodes`/`files`, dependency declared manifest names),
     `inferred` (Layer 2: module `module_candidates`, http-surface runtime surfaces), `hint`
     (Layer 3: framework `inferences`), `governance` (Layer 4 governance/policy overlay:
     boundary = the authored `declarations` store — review-6 re-home). The lexical RETRIEVAL is
     deterministic; the content's certainty is the table's layer. A discovered module boundary
     is NEVER presented as an extracted fact, and a Layer-4 governance DECLARATION is never
     tagged `extracted` (that would describe an authored policy overlay as Layer-0 code truth).
   - **Runnable next command** (review-1 item 1): the group header shows the class VERB (a bare
     `rmap boundaries`/`deps`/`inferences`/`surfaces` prints usage — the runnable form is the
     `list` subcommand; `explain` is a read-only top-level verb that takes a target). Every hit
     ALSO renders an executable, **NON-MUTATING** invocation: `explain <key>` for symbol/file,
     `map <path> --dry-run` for module (review-2 item 1: `rmap map` WRITES `MAP.md` into the tree
     by default — a discovery next-step must never mutate on paste, so the rendered/header form is
     `map --dry-run`, which prints the map to stdout and writes nothing), each shell-quoted; the
     whole-listing command for the `… list` classes. Each emitted form is probed exit-0 against
     the release binary; the e2e proof runs them verbatim and never writes into the target checkout.
   The label teaches the next move; hits are grouped by fact class, capped per class with an
   explicit `(+N more — --full)` per the audit's budget-honesty standard, deduped across
   classes by (fact class, path, identity-within-class).
3. **The embeddings tier is DEMOTED below the facts tier** and renamed in-output to what it
   is: `Semantic seeds (embedding similarity — ranked guesses, not facts):`. Its existing
   labeling/pins are unchanged. When the endpoint is unavailable, the tier renders
   `semantic seeds unavailable (<reason — e.g. endpoint connection refused>)` — the verb no
   longer dies with the endpoint; the facts tier always answers.
4. **`--exact`** renders the facts tier alone (no endpoint touched at all) with deterministic
   ordering — the grep-like scriptable form; `grep` is NOT added as a verb or alias (ruling
   2026-08-30: the name promises determinism a semantic-capable verb cannot keep).
5. Zero facts hits + zero/unavailable seeds → an honest empty: what was searched (the fact
   classes, by name) and what was not (semantic, with reason if unavailable).
6. JSON: additive (facts tier as a new array; existing seed fields unchanged). Each fact group
   additionally carries `certainty` (`extracted` | `inferred` | `hint` | `governance` — the
   last is the Layer-4 label for authored declaration hits, which are policy statements, not
   extracted facts; amendment 2026-08-30, review-8) and each hit a `next` (the runnable
   invocation) — both additive, byte-compatible for existing seed consumers. Contract
   doc + VISION § Semantic Seeding amended in-slice (facts-first + certainty wording — the
   VISION edit is pre-ratified by the human direction of 2026-08-30).

## 3. Stop conditions

Frozen: seed sidecar format/pins, seed ranking formula, storage schema (read-only queries
only; new dispatch arm or extension of `find`'s arm is in scope with the witness line), exit
codes. STANDING HONESTY RULES (every fallible read rendered → unknown-with-reason; a failed
fact-class query renders that class as `unavailable (<reason>)`, never silently absent from
the searched-classes list). New PUBLIC APIs beyond additive DTO fields → DECISION_REQUIRED.
Unmet DoD → STOP + DECISION_REQUIRED. Never touch the operator's real state root. Do NOT
commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Unit: per-fact-class hit shape + label + rendering-command name; dedup; per-class caps with
  explicit remainder; `--exact` never touches the endpoint (assert no request); endpoint-down
  → facts still render + labeled seed unavailability; failed single fact-class query →
  unavailable-with-reason for that class only; honest empty.
- Live proof (isolated state root, registry sha unchanged): glamCRM — `rmap find bnr` hits
  the bnr-service symbols/files in the facts tier with correct labels ABOVE any seeds;
  `rmap find "offers"` shows http-surface hits labeled → boundaries; kill LM Studio reachability
  (wrong RMAP_SEED_ENDPOINT) → facts tier intact, seeds tier says unavailable with reason.
  Captures in the report.
- Chunked cargo gates; consolidation witness; dogfood-isolated green.

## 5. Definition of done

`find` answers exact-identifier queries deterministically from the fact tables with
fact-class + next-command labels, above demoted, clearly-named semantic seeds; the verb
survives LM Studio being down; `--exact` is the scriptable deterministic form; VISION and
contract docs match; gates green.
