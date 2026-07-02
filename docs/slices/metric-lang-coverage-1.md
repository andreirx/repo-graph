# METRIC-LANG-COVERAGE-1 — Quality signals state their language coverage; Rust joins the measured set

Status: SPECIFIED (2026-07-02) · Track: Product-surface honesty + measurement depth
Origin: fresh-eyes v0.4.0 self-dogfood (2026-07-02), operator review

## 1. Problem — the quality signals are silently blind to entire languages

Evidence (self-index of repo-graph, v0.4.0, isolated state):

- `rmap metrics --kind cyclomatic_complexity --limit 500` on the repo-graph
  snapshot returns **zero Rust functions** — all 500 entries are legacy
  TypeScript. `orient`'s "Complexity centers" and `hotspots` likewise show
  only TS files, on a repo whose product code is ~250k LOC of Rust
  (`dispatch.rs` alone: 7,935 lines, 131 match arms — absent).
- Root cause (corrected 2026-07-02 by the build's evidence — the original
  "Rust/Java/Python all unmeasured" premise was wrong): **only the Rust
  extractor lacked complexity emission**. `java-extractor` has `metrics.rs`
  and `python-extractor` computes cyclomatic inline; they simply have no
  files in this repo. Part A is therefore honesty *infrastructure* guarding
  the general case (future unsupported languages, bodyless-heavy snapshots),
  not a patch for a specific Java/Python gap.
- No rendering surface says so. `orient` presents "Complexity centers (by
  cyclomatic complexity)" as repo-wide fact; `hotspots` scores
  `lines_changed × sum_complexity` where unmeasured languages contribute
  complexity 0 and silently vanish from the ranking.

This is the same violation class HONEST-DEGRADATION-1 (D1–D5, ratified
2026-07-01) closed elsewhere: a partial measurement rendered as a total one.
VISION (2026-07-02 rewrite) now names it explicitly: *"a quality signal
computed for only some supported languages must say so wherever it renders"*
and *"coverage is part of the fact."*

## 2. Contract

**A. Language-coverage honesty (mechanism, not hardcoded list).**
Quality-signal surfaces that render complexity-derived content (`orient`
complexity centers, `hotspots`, `metrics`) derive per-language measurement
coverage **from the snapshot**: group function/method symbols by file
language; compute, per language, the share carrying a complexity measurement.
Any language holding a non-trivial share of the repo's functions (threshold:
≥5% — builder may adjust with rationale) with zero/near-zero measured share
triggers a caveat on the rendered section, reader-frame wording, e.g.:

> Complexity is measured for C and TypeScript files only on this snapshot —
> Rust (72% of functions) is not yet measured; rankings omit it.

Requirements: data-driven (the caveat must disappear by itself when a
language gains measurements — no hardcoded language list); present in human
and `--json` output (a `measurement_coverage` block); reader's language per
the VISION labels rule.

**B. Rust cyclomatic complexity emission.**
`rust-extractor` emits `cyclomatic_complexity` for functions/methods,
following the existing `c-extractor/src/metrics.rs` pattern (tree-sitter
walk; same counting rules as the C/TS implementations so values are
comparable — document any Rust-specific constructs counted, e.g. `match`
arms, `?`, `if let` chains). Emit the same measurement kinds c-extractor
emits where they transfer (at minimum cyclomatic; nesting/length/params if
the shared pattern provides them cheaply).

**Out of scope:** Java and Python emission (already shipped, per the corrected
premise above — the part A mechanism reports any future gap honestly);
cognitive complexity for Rust;
any change to hotspot formula; enrichment; the TS prototype (separate slice).

## 3. Stop conditions

- If comparable counting rules for Rust require diverging from the C/TS
  counting semantics in a way that would make cross-language rankings
  misleading → STOP + DECISION_REQUIRED (comparability is a contract).
- If the coverage computation cannot be done from existing snapshot data
  (needs schema migration) → STOP + DECISION_REQUIRED before migrating.
- Do NOT hardcode language names into caveat logic.

## 4. Validation (end-of-slice, synchronous; TEST REPORT)

- `cargo build`, `cargo test`, `cargo fmt --check`, `cargo clippy -- -D warnings`
  green (from `rust/`).
- **Emission proof:** named test — indexing a Rust fixture yields
  `cyclomatic_complexity` measurements for its functions with expected values
  (hand-computed for 2–3 fixture functions, including a `match`).
- **Coverage-caveat proof:** named test — a mixed-language fixture where one
  language is unmeasured renders the caveat; a fixture where all languages
  are measured renders none.
- **Self-dogfood proof:** isolated self-index (`RMAP_STATE_ROOT` under
  `/private/tmp`) — Rust functions now appear in the complexity top list
  (e.g. `dispatch` handlers), and remaining unmeasured languages (Java/Python
  if present in fixtures) are caveated.
- `./scripts/dogfood-isolated.sh` green.

## 5. Definition of done

Complexity-bearing surfaces state their per-language measurement coverage
(data-driven), Rust functions carry cyclomatic complexity comparable to C/TS,
and the repo-graph self-index ranks its own Rust code in complexity
centers/hotspots. Cargo gates green (EXECUTED + reported) + the three named
proofs + dogfood green.

## 6. Files in scope (expected)

- `rust/crates/rust-extractor/src/` (new `metrics.rs` + wiring)
- `rust/crates/classification/` or the shared measurement plumbing if the
  coverage computation lives there
- `rust/crates/rgr/src/presentation/` (orient complexity section, hotspots,
  metrics renderers) + their tests
- Out of scope: `src/` (legacy TS), governance surfaces, gate contracts.
