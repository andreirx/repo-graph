# FIND-GREP-1 — text search that tells the truth about what it is

Status: SPECIFIED (2026-09-03) · Track: zg head-to-head fix queue #2 (human-ratified
direction: imitate the output shape). CODE slice. Maturity: MATURE.

## 1. Problem (MEASURED — docs/audits/2026-09-03-zg-vs-rmap-find.md)

Four straight F's on literal/text hunts (`fsync`, `TODO`, `unwrap_or(0)`): find searches
fact tables only — libc calls, comment markers, and expressions are not symbols — and it
papers over the gap with a FALSE sentence: "the concept may not have a distinct home in
this repo" (wrong 3/3; the true statement is about capability, not the repo). Meanwhile
the compared tool's ripgrep route was complete AND its enclosing-symbol annotation was its
single best feature — built on kind data WORSE than ours.

## 2. Contract

1. **`find --text <pattern>`** (regex; `-F` for fixed string): a live working-tree scan
   scoped by the repo's ignore rules, implemented on the ripgrep library crates
   (`grep`/`ignore` — reuse known-good, do not reinvent matching or walking). Output
   grouped by file with `path:line` + the matching line.
2. **Enclosing-symbol annotation from OUR spans**: each hit inside a stored symbol span is
   annotated `[kind qualified_name]` from the snapshot DB. Hits outside any span carry no
   annotation (visible absence, never a guess).
3. **Freshness honesty (the thing zg advertises but does not deliver)**: annotation joins
   a LIVE scan against SNAPSHOT spans. Per file, if the working-tree content differs from
   the snapshot's file version (hash compare against stored file_versions), its hits say
   so once ("file changed since snapshot — symbol context may be stale") and the output
   header states the snapshot identity. No silent mixing of live text with stale spans.
4. **Retire the false sentence.** In classic find, when fact tables miss and the seeds
   floor is unmet, the closing line states CAPABILITY, not repo absence: facts indexed,
   nothing matched; for text/comments/expressions → `find --text`. The "may not have a
   distinct home" sentence is deleted everywhere it can render on a capability miss.
5. **Volume honesty**: total match count in a header; default cap (e.g. 200 lines) with
   an explicit "showing N of M — --full for all" line (our own disclosure pattern; the
   compared tool dumped 23.9KB uncapped and that graded D).
6. JSON additive; exit codes: match/no-match semantics follow existing find conventions
   (state them in the report); classic find behavior otherwise byte-stable except the
   retired sentence.

## 3. Stop conditions

Frozen: fact-table match semantics, seeds computation, storage schema (reads only), exit
codes beyond the stated convention. STANDING HONESTY RULES (no guessed annotation; stale
context labeled). New crate dependencies limited to the ripgrep library family
(`grep`/`ignore`/regex already in tree?) — anything else → DECISION_REQUIRED. New public
APIs → DECISION_REQUIRED (additive precedent chain citable). Unmet DoD → STOP +
DECISION_REQUIRED. Never touch the operator's real state root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Unit: hit-in-span annotated; hit-outside-span bare; changed-file staleness note; cap
  disclosure; retired-sentence absence on capability miss.
- Live proof (isolated state root, registry sha unchanged): the three audit F probes
  re-run — `--text fsync` on leveldb (must find env_posix.cc:411, annotated to
  STORED-SPAN FIDELITY: the annotation renders `[kind qualified_name]` exactly as
  indexed, never implying a function — AMENDED 2026-09-03 after the operator verified the
  stored span at :411 is the mis-extended `class Limiter` 73–806 with the real enclosing
  function unextracted; that defect is CPP-SPAN-FIDELITY-1, a named follow-up, and the
  build report must cite it), `--text TODO` on FRAKTAG (3 first-party, vendored
  suppressed by ignore rules), `--text 'unwrap_or\(0\)'` on repo-graph (count header +
  cap disclosure); one deliberately-edited file demonstrating the staleness note.
- Chunked cargo gates; consolidation witness; dogfood-isolated green.

## 5. Definition of done

The three audit F's would grade A-range on re-run; no output claims repo absence for a
capability gap; annotation never guesses; stale symbol context is labeled; gates green.

CORPUS PATHS: leveldb at ../legacy-codebases/leveldb; FRAKTAG at ../FRAKTAG; repo-graph is
THIS repo.
