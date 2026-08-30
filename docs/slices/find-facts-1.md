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
   inference identifiers, boundary/entrypoint declarations. The corpus is the FACT TABLES —
   never the rendered text of other commands (a hit must not depend on another renderer's
   budget or phrasing).
2. **Every hit is labeled with its fact class and the command that renders it** — e.g.
   `[http-surface → rmap boundaries] GET /api/offers … serverless/src/handlers/offer.ts`.
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
6. JSON: additive (facts tier as a new array; existing seed fields unchanged). Contract doc +
   VISION § Semantic Seeding amended in-slice (facts-first wording — the VISION edit is
   pre-ratified by the human direction of 2026-08-30).

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
