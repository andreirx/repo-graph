# FIND-KIND-MISLABEL-1 — a C++ class is not a FUNCTION

Status: SPECIFIED (2026-09-03) · Track: v0.15.0 audit queue #3 (trust-class, narrow).
CODE slice, diagnose-then-fix. Maturity: MATURE (symbol kind feeds find labels, stable
keys, downstream classification).

## 1. Problem (OBSERVED in v0.15.0 audit; CAUSE UNVERIFIED — diagnosis is step 1)

On vcmi (C++), `find` renders classes/structs (`HeroClassID` et al.) tagged
`SYMBOL:FUNCTION`. Operator trace so far: cpp-extractor's class/struct/enum paths emit
correct subtypes (`extract_class` → `subtype: Some(subtype)`, extractor.rs ~644), so the
label is NOT an obvious constant. Competing hypotheses, both unverified:
- H1: a different extraction path claims those declarations (tree-sitter misparse of
  macro-heavy/template C++, or a fallback that stamps FUNCTION);
- H2: a downstream serve/render default substitutes FUNCTION when subtype is
  absent/unrecognized (would violate STANDING HONESTY RULES — a defaulted kind).

## 2. Contract

1. **Diagnose FIRST, on the real corpus.** Index vcmi in an isolated state root; query the
   DB for the mislabeled symbols' rows (stable_key, subtype, extraction provenance).
   State which hypothesis holds WITH THE EVIDENCE in the build report before fixing.
2. **Fix the true cause at its site.**
   - H1 (extractor path): correct the kind mapping for the demonstrated construct class;
     no name-based classification; a construct the parser genuinely cannot classify gets
     an honest unknown/generic subtype, never FUNCTION.
   - H2 (downstream default): remove the default; absent subtype renders as
     `SYMBOL` (kind unknown) — visible, never invented.
3. **Stable-key impact is a decision gate.** If the fix changes stable_keys of existing
   nodes (subtype is part of the key), report the blast radius (how many nodes on vcmi;
   what refresh/copy-forward does with the changed keys) — if changed keys break identity
   continuity beyond one reindex, STOP + DECISION_REQUIRED with options.
4. Downstream movement measured (deep-vertical): find on vcmi renders the true kinds;
   before/after counts of FUNCTION-tagged symbols on vcmi in the report. Non-C++ corpus
   repos byte-stable.

## 3. Stop conditions

Frozen: storage schema, exit codes, find ranking semantics. STANDING HONESTY RULES (no
kind defaults; unknown visible). Tree-sitter grammar version bumps → DECISION_REQUIRED.
Stable-key continuity per §2.3. Unmet DoD → STOP + DECISION_REQUIRED. Never touch the
operator's real state root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Reproducing test FIRST: a minimal C++ fixture reproducing the demonstrated construct
  (from the vcmi diagnosis) that pre-fix mislabels as FUNCTION and post-fix carries the
  true kind.
- Live proof (isolated state root, registry sha unchanged): vcmi reindex — HeroClassID et
  al. render true kinds in find; before/after FUNCTION counts; openxcom (C++) spot-check;
  leveldb byte-parity or explained movement (it is C++ too — movement there is EXPECTED
  if H1 constructs exist; report it, do not hide it).
- Chunked cargo gates; consolidation witness; dogfood-isolated green.

## 5. Definition of done

The diagnosis names the mechanism with DB evidence; the demonstrated construct class
labels truthfully; no kind is ever defaulted; key-continuity impact reported; gates green.

CORPUS PATHS: vcmi at ../legacy-codebases/vcmi; openxcom at ../legacy-codebases/openxcom;
leveldb at ../legacy-codebases/leveldb; repo-graph is THIS repo.
