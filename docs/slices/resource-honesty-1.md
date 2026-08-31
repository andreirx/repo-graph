# RESOURCE-HONESTY-1 — resource-list stops blaming the codebase and claiming totality

Status: SPECIFIED (2026-09-01) · Track: Usefulness audit v0.11.0 fix queue, tail item.
CODE slice, small. Maturity: MATURE surface.

## 1. Problem (measured — audit run 2026-08-30T20-44-57Z)

`resource-list` is the last surviving member of the audit's "blames the codebase" class:
- django/leveldb/FRAKTAG: "hint: no resource access patterns detected in this codebase" —
  on an ORM, a storage engine, and a persistence tool. The tool's detector coverage is the
  cause; the sentence indicts the repo.
- Non-zero renders assert totality with near-zero recall: repo-graph's ONE resource is
  `/private/tmp/embed-spike/results.json`; glamCRM's is `boen_products.csv` — "1 resource"
  reads as the repo's resource inventory.

## 2. Contract

1. **The zero-state names the tool's coverage, per language** (the leveldb-pattern sentence
   every other surface now has): which resource-access patterns this build detects (and for
   which languages — from the detector registry, one source of truth, never a hardcoded
   list), and the honest no-path sentence for materially-present languages with no detector.
   "No resource access patterns detected" may render only WITH that coverage statement.
2. **Non-zero renders carry the same coverage header** — "N resource(s) via <detector
   families> (coverage: <langs>)" — so 1 result never reads as an inventory.
3. Reuse the existing materiality gate and capability-fact plumbing (CS-1/CHECK-SIGNAL-1
   pattern); no new fact classes.
4. JSON additive; exit codes unchanged.

## 3. Stop conditions

Frozen: resource detection itself (what is detected — this slice changes what is SAID),
storage schema, exit codes, trust. STANDING HONESTY RULES. New public APIs beyond additive
DTO fields → DECISION_REQUIRED. Unmet DoD → STOP + DECISION_REQUIRED. Never touch the
operator's real state root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Unit: zero-state with coverage statement per language mix; non-zero header; detector list
  from the registry (a registry change propagates without touching this surface — test it).
- Live proof (isolated state root, registry sha unchanged): leveldb + django zero-states
  name the coverage gap honestly; repo-graph/glamCRM non-zero renders carry the coverage
  header. Before/after captures.
- Chunked cargo gates; consolidation witness; dogfood-isolated green.

## 5. Definition of done

resource-list never blames the codebase for the tool's coverage and never lets one detected
file pose as an inventory; coverage statements derive from the detector registry; gates
green.
