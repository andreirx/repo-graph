# CONTRADICTION-SWEEP-1 — one snapshot, one story

Status: SPECIFIED (2026-08-28) · Track: Usefulness-audit fix queue #6
(`docs/ROADMAP.md` § Usefulness audit v0.9.0; evidence `smoke-runs/2026-08-25T22-41-37Z`).
CODE slice. Maturity: MATURE surfaces.

## 1. Problem (measured — five contradictions between commands on ONE snapshot)

1. **doctor "healthy (28/28)" while `check` returns FAIL** on the same snapshot (glamCRM/amodx);
   doctor also marks a 0/881-promotion enrichment `[ok]`.
2. **`check: UNPARSED_FILES: none` while `map` reports "14 file(s) … (not parsed)"** (glamCRM;
   amodx 17) — two different "parsed" notions rendered under one word; map's unparsed files are
   also UNNAMED (unactionable).
3. **trust "Suspicious Modules (zero connectivity): include/" while `stats` reports
   `include/leveldb fan_in=6`** (leveldb) — two connectivity computations disagree.
4. **orient (FRAKTAG) said "Enrichment phase did not run" while check said "executed"** — stale
   duplicated wording paths (partially retired since; verify and close the class).
5. **Python repos are told `run rmap enrich`** while doctor's own line says the skip was
   `no typescript` — a remedy that cannot apply (no Python enricher exists); C++ already gets
   the honest "no semantic-resolution path exists" sentence.

## 2. Contract

1. **doctor's verdict framing**: doctor reports DAEMON/INSTALL health; it must not imply snapshot
   quality. Its summary line becomes "daemon healthy (N/N checks); snapshot verdicts: <repo>
   check FAIL/INCOMPLETE/PASS" for the cwd repo when resolvable (one added read), and the
   enrichment line drops `[ok]` in favor of a neutral marker when the outcome is degraded
   (0-promotion → `[note]` with the funnel's top rejection, one line).
2. **One "parsed" story**: `map`'s unparsed set and `check`'s UNPARSED_FILES read the SAME fact
   source; map NAMES the files (capped list + count). If two legitimate notions exist
   (extractor-parse vs map-render), they get DISTINCT names and both surfaces say which they
   mean.
3. **One connectivity story**: trust's zero-connectivity suspicion consults the same
   module-graph facts stats renders (or states its own basis inline: "zero RESOLVED IMPORT
   connectivity (header-only inclusion not counted)"). The leveldb contradiction must render
   coherently — either the suspicion disappears or it explains itself against the stats number.
4. **One enrichment story**: every surface that words enrichment state (orient/check/doctor/
   trust) reads ONE shared state accessor; wording variants retire.
5. **Per-language remedy honesty**: the enrich CTA renders only for languages with an existing
   enrichment path (TS/JS via tsserver, Rust via rust-analyzer, Java via jdtls); others get the
   C++-style "no semantic-resolution path exists for <lang> on this build". Python's false CTA
   dies.

## 3. Stop conditions

Frozen: storage schema, trust/stats COMPUTATION (this slice aligns basis/wording/labels, not
math — if the two connectivity computations genuinely disagree on facts, that is a FINDING +
DECISION_REQUIRED, not a silent recompute), witness/union/reconciliation, exit codes. Never
touch the operator's real state root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Unit: doctor verdict line (PASS/FAIL/INCOMPLETE/unresolvable-cwd); shared parsed-fact source +
  map naming; connectivity coherence (the leveldb fixture shape); shared enrichment accessor
  (all four surfaces, one wording); per-language CTA matrix.
- LIVE isolated proofs: glamCRM — doctor names the check verdict; map names its unparsed files
  and check agrees or the two notions are distinctly named; leveldb — trust's suspicion line
  coherent with stats; django — the enrich CTA replaced by the honest no-path sentence.
- Chunked cargo gates incl. --all-targets clippy; witness green; dogfood green; logged smoke
  SMOKE_ONLY="glamCRM leveldb" green.

## 5. Definition of done

No two commands disagree about one snapshot without one of them explaining the difference in the
reader's language; remedies are only offered where they exist; gates green.
