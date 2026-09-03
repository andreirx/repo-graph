# RESOURCE-RECALL-1 — the zero-state cites the literal-path gate

Status: SPECIFIED (2026-09-03) · Track: v0.15.0 audit queue #2 (honesty-class,
full-capture basis). CODE slice, presentational, SMALL. Maturity: MATURE.

## 1. Problem (MEASURED — FINAL-POLISH-1 §6 measurement + v0.15.0 audit)

`resources` zero-states on leveldb/django read as "this repo touches no files" when both
repos are SATURATED with exactly the covered calls (fopen/open) — whose path arguments are
COMPUTED, and the extractor's arg0 gate only accepts STRING LITERALS (measured in
FINAL-POLISH-1: the fstream support delivered for literal paths only; the extractor-side
fix is RESOURCE-DYNAMIC-PATH-1, unscheduled). RESOURCE-HONESTY-1's zero-state names
mechanisms and language gaps but NOT this gate — the dominant false-negative cause on
C/C++/Python repos. An agent reading the current zero-state gains false certainty.

## 2. Contract

1. The zero-state (`resources.rs` §2.1 path) adds ONE honest sentence naming the gate:
   detection sees only calls whose path argument is a string literal at the call site;
   computed/variable paths are invisible on this build. Plain language, cause-discriminating
   (the churn zero-state template — audit "keep and imitate").
2. The NON-zero coverage header (§2.2 path) carries the same limitation (a partial listing
   misleads identically); phrasing may be compact ("literal-path calls only").
3. No behavior change beyond copy: counts, JSON values, exit codes unchanged (a JSON
   additive coverage note field is permitted if the text has a JSON twin today — mirror the
   existing coverage/gap fields' pattern; nothing else).
4. The sentence states the LIMITATION, not a promise ("until RESOURCE-DYNAMIC-PATH-1" is a
   roadmap fact, not output copy — no ticket names in user output).

## 3. Stop conditions

Frozen: detector/extractor behavior (this slice is COPY, not recall), storage schema, exit
codes. STANDING HONESTY RULES. Any urge to widen the arg0 gate → out of scope
(RESOURCE-DYNAMIC-PATH-1). Unmet DoD → STOP + DECISION_REQUIRED. Never touch the
operator's real state root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Unit: zero-state and non-zero header both carry the literal-path sentence; existing
  RESOURCE-HONESTY-1 tests updated, not weakened.
- Live proof (isolated state root, registry sha unchanged): leveldb — zero-state renders
  the gate sentence; a repo with literal-path hits (repo-graph or FRAKTAG) — non-zero
  header carries the compact form; JSON diff confined to any mirrored note field.
- Chunked cargo gates; consolidation witness; dogfood-isolated green.

## 5. Definition of done

No resource output can be read as a repo inventory claim when the literal-path gate is the
reason it is short; gates green.

CORPUS PATHS: leveldb at ../legacy-codebases/leveldb; django at
../legacy-codebases/django; FRAKTAG at ../FRAKTAG; repo-graph is THIS repo.
