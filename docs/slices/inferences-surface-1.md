# INFERENCES-SURFACE-1 — the inference inventory becomes readable (and never silently capped)

Status: SPECIFIED (2026-08-28) · Track: Usefulness-audit fix queue #4
(`docs/ROADMAP.md` § Usefulness audit v0.9.0; evidence `smoke-runs/2026-08-25T22-41-37Z`).
CODE slice, small-medium. Maturity: MATURE surface.

## 1. Problem (measured)

- **The content is gold; the surface is a dump** (OBSERVED): petclinic's 14 Spring beans with
  annotation + reason + confidence are the single most orienting output that repo produced — and
  glamCRM/amodx bury the same class of content in 10,512/10,724 lines of raw JSON (~14
  lines/record; uid + snapshot path + created_at + extractor-version boilerplate on every record;
  no grouping, no header saying what the records are).
- **A silent cap** (OBSERVED): both big repos report exactly `count: 752` — a page/limit bites
  with NO `truncated` flag and no `--limit` echo, so an agent reads 752 as ground truth (an
  honesty violation of the first order).
- **Empty is unexplained** (OBSERVED): django/leveldb render empty JSON with `count: 0` and no
  statement of what an inference is or which detectors ran.

## 2. Contract

1. **Grouped summary by default (human)**: header stating what inferences are + which detectors
   ran on this snapshot; then `kind × count` with per-kind top files/symbols (e.g.
   `spring_container_managed 14 — OwnerController (@Controller), …`); ≤ ~40 lines on the audit
   repos. Full detail behind `--limit N` (records rendered compactly: kind, name, file:line,
   basis/reason, confidence — never uid/snapshot boilerplate in human mode).
2. **The cap becomes honest**: wherever a limit applies (default or `--limit`), the payload and
   human output carry `truncated: true`, the limit, and the true total (`count` = TOTAL matching
   records; `returned` = rows in this payload). The silent-752 case becomes impossible: find the
   existing cap, name it, surface it.
3. **Empty states explain themselves**: zero inferences renders which detectors ran and found
   nothing vs which do not exist for the snapshot's languages ("no inference detectors exist for
   C/C++ on this build" — the leveldb-style honesty line), reusing the reader's-language pattern.
4. **JSON stays machine-first and additive**: existing fields keep meaning; `returned`/
   `truncated`/`limit`/`detectors` are additive. `--json` without `--limit` keeps full records
   (boilerplate MAY remain in JSON — machine consumers may want uids), but carries the same
   headline fields.

## 3. Stop conditions

Frozen: storage schema, detectors/extractors (this is a SURFACE slice — no new inference kinds),
trust, witness/union/reconciliation, exit codes. If the 752 cap turns out to live in storage
pagination whose change would alter other consumers, surface the finding — do not silently raise
it; the fix may be reading pages to completion for the count. Never touch the operator's real
state root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Unit: grouping/rendering; truncation flags (cap hit and not hit); count-vs-returned; empty
  states (ran-and-found-nothing vs no-detector-for-language); additive JSON.
- LIVE isolated proofs: glamCRM — the default output is the grouped summary (≤ ~40 lines, was
  10,512) and the TRUE total renders with `truncated` when the cap bites; petclinic — the 14
  beans render as the summary's showcase; django/leveldb — the explanatory empty states.
  Before/after line counts vs `smoke-runs/2026-08-25T22-41-37Z` in the report.
- Chunked cargo gates; witness green; dogfood green; logged smoke SMOKE_ONLY="glamCRM spring-petclinic" green.

## 5. Definition of done

An agent running `inferences list` learns what was inferred, from which detectors, at a glance —
with true totals, explicit truncation, compact detail on demand, and explained emptiness; gates
green.
