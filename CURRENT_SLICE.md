# CURRENT_SLICE.md

## Current Priority

**CLI-OUT-2B: First-Contact Discovery Output** — CURRENT

Slice doc: `docs/slices/cli-out-2b-output-redesign.md`

Renderer-only implementation. Data already exists in daemon responses.

### In Scope

- `orient` — repo identity, cycle topology, evidence-bearing degradation
- `trust` — new human renderer
- `cycles` — new human renderer
- `check` — optional evidence refinement

### Out of Scope

- Module count fix (ORIENT-BUG-1 — data bug)
- Daemon timeouts (RMAPD-PERF-1 — runtime issue)
- stats renderer (deferred to CLI-OUT-2C pending timeout investigation)
- explain renderer (deferred to CLI-OUT-3)

---

## Handoff Complete

**CLI-OUT-2A: Cross-Repo Output Audit** — HANDOFF COMPLETE

Slice doc: `docs/slices/cli-out-2a-output-audit.md`

Audit sufficient to drive first implementation wave. Findings in `docs/audits/cli-out-2a/`.

### Completed

- 5 of 7 repos audited (gstreamer/hadoop blocked by RMAPD-PERF-1)
- orient, trust, cycles, stats, check audited (explain deferred)
- Contracts proposed for first-contact discovery commands

### Gaps Handed Off

| Gap | Tracked As |
|-----|------------|
| gstreamer/hadoop not audited | RMAPD-PERF-1 |
| explain not audited | CLI-OUT-3 |
| Module count mismatch | ORIENT-BUG-1 |
| stats/check timeout | RMAPD-PERF-1 |

### Key Defects Found

1. Repo identity shows internal UID, not name
2. Module count wrong in orient vs trust (ORIENT-BUG-1)
3. Cycle severity hidden (69 modules = "4 cycles")
4. Resolution rates hidden behind "LOW"
5. trust/cycles/stats are JSON dumps

---

## Bug Slices Created

**ORIENT-BUG-1: Module Count Mismatch** — QUEUED

Orient shows 2-17 modules, trust shows 19-240+. Data/query bug.
See `docs/slices/orient-bug-1-module-count.md`.

**RMAPD-PERF-1: Large Repo Timeout** — QUEUED

Indexing/stats/check timeout on large repos. Daemon runtime issue.
See `docs/slices/rmapd-perf-1-timeout.md`.

---

## Output Program Wave Model

| Wave | Slice | Commands | Notes |
|------|-------|----------|-------|
| 1 | CLI-OUT-2B | orient, trust, cycles, check | CURRENT |
| 1b | CLI-OUT-2C | stats | After RMAPD-PERF-1 timeout fix |
| 2 | CLI-OUT-3 | callers, callees, path, imports, explain | Graph drilldown |
| 3 | CLI-OUT-4 | modules *, surfaces *, boundaries * | Module/architecture |
| 4 | CLI-OUT-5 | docs *, resource *, policy | Inventory |
| 5 | CLI-OUT-6 | churn, hotspots, risk, coverage | Quality/risk |
| 6 | CLI-OUT-7 | violations, gate, assess | Governance |

---

## Recently Implemented

**CLI-OUT-1: Presentation Layer** — IMPLEMENTED (2026-05-18)

**REG-1: Repo Registry + CWD Auto-Discovery** — IMPLEMENTED (2026-05-17)

**RMAPD-2: Unix Socket Transport** — IMPLEMENTED (2026-05-15)
