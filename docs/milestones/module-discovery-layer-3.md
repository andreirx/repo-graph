# Module Discovery Layer 3 — Design Slice

Inferred modules from structural patterns, graph analysis, and build-system
artifacts. Extends shipped Layer 1 (declared) and Layer 2 (operational).

## Problem Statement

Layer 1 and Layer 2 cover repos with explicit manifests or detectable project
surfaces. They fail on:

- **Pure C repos** — no package.json, Cargo.toml, or pyproject.toml
- **Kernel-style repos** — modules defined by Kbuild/Makefile, not manifests
- **Legacy monorepos** — directory structure implies modules, no workspace file
- **Mixed-origin repos** — some subdirs have manifests, others are structural

An AI agent working on such repos sees a flat file list or collapsed topology.
Module-level reasoning is unavailable.

## Scope

**In scope:**
- Architectural separation of three inference sources
- Confidence/trust model for inferred modules
- Design constraints before implementation
- Locked decisions for Source A and B

**Out of scope (implementation deferred):**
- Actual algorithms for graph clustering (Source C deferred until A/B validated)
- Kbuild parser implementation
- Migration schema for new module kinds
- CLI surface changes

## Design Principle: Three Distinct Inference Sources

Layer 3 must NOT be a single heuristic blob. Three inference sources have
different reliability profiles and must be tracked separately.

### Source A: Build-System-Derived Modules

**Examples:**
- Linux kernel: `Kbuild`, `Makefile` with `obj-y`, `obj-m` patterns
- GNU projects: `Makefile.am`, `configure.ac` with `SUBDIRS`
- CMake: `add_subdirectory()`, `add_library()`, `add_executable()`

**Characteristics:**
- Explicit artifact: a build file names the module boundary
- HIGH confidence (comparable to Layer 1 manifests)
- Language-specific parsers required
- Evidence is the build file line

**Why separate from Layer 1:**
Layer 1 is package-manager manifests (npm, Cargo, pip, Gradle). Build-system
files are not package managers. They define compilation units, not distribution
units. Collapsing them loses the distinction.

### Source B: Directory-Structure-Inferred Modules

**Examples:**
- `src/core/`, `src/adapters/`, `src/cli/` as implicit modules
- `lib/`, `pkg/`, `internal/` conventions
- Nested directories with coherent file sets

**Characteristics:**
- No explicit artifact: inferred from directory naming and file clustering
- MEDIUM confidence (structural pattern, not declaration)
- Language-agnostic heuristics
- Evidence is "directory pattern" + child file count

### Source C: Graph-Cluster-Inferred Modules

**Examples:**
- Files with high internal IMPORTS cohesion, low external coupling
- Symbol clusters with dense CALLS subgraph
- Structural boundaries from edge density analysis

**Characteristics:**
- No explicit artifact: inferred from edge patterns
- LOW-to-MEDIUM confidence (derived, not observed)
- Requires resolved edge graph (fails on collapsed topologies)
- Evidence is clustering algorithm output + metrics

**Why separate from B:**
Directory structure is observable without graph analysis. Graph clustering
requires a resolved import/call graph. On repos with collapsed topology
(e.g., nginx v1.1), Source B can still work while Source C cannot.

**Implementation status:** Deferred until Source A and B are validated on
real repos. Graph-cluster quality depends on graph quality; validating it
on degraded repos tells you little.

## Confidence/Trust Model

### Ordinal Confidence (Pre-Calibration)

Confidence values are ordinal until calibrated against real repos.
Decimal precision is pseudo-precision without empirical basis.

| Source | Confidence | Coarse Band | Degradation Trigger |
|--------|------------|-------------|---------------------|
| Build-system (A) | HIGH | 0.9 | Ambiguous parse, missing file |
| Directory (B) | MEDIUM | 0.7 | Shallow depth, mixed languages |
| Graph-cluster (C) | LOW | 0.5 | Low edge density, high unresolved rate |

Calibration against real repos will refine these bands. Until then, do not
use decimal ranges like `0.60–0.80`.

### Unavailable vs Low Confidence (Pinned)

Source availability must be distinct from low confidence:

- **LOW confidence:** detector ran, result is weak
- **UNAVAILABLE:** detector could not run meaningfully

Example: Source C on a collapsed graph (nginx v1.1) is `unavailable`,
not `low confidence`. This distinction matters for AI-agent trust:
an agent must know whether to distrust a result vs ignore a missing result.

**Implementation requirement:** `unavailable` is a distinct state, not a
confidence value. Evidence rows indicate why unavailable (e.g., "graph
unresolved rate > 50%").

### Aggregation Rule

When multiple sources agree on the same module root, confidence increases.
When they disagree, the higher-confidence source wins for **module root
ownership/classification**. Lower-confidence candidates remain as evidence,
not discarded.

No policy edges or `--forbids` declarations are inferred from aggregation.

### Trust Signaling Requirements

1. Module kind must indicate inference source: `build_system`, `directory`, `graph_cluster`
2. Evidence must trace to specific artifact or algorithm
3. Confidence must be queryable (not just stored)
4. Degradation flags must propagate to module rollups
5. `unavailable` must be distinct from `low confidence`

## Architectural Constraints

### Constraint 1: Source-Specific Detectors

Each inference source gets its own detector module:

```
src/core/modules/
  detectors/
    kbuild-detector.ts       # Source A: Linux Kbuild
    makefile-detector.ts     # Source A: GNU Makefile (future)
    cmake-detector.ts        # Source A: CMake (future)
    directory-detector.ts    # Source B: structural
    graph-cluster.ts         # Source C: edge analysis (deferred)
```

Detectors are pure functions: `(repoPath, fileSet, edgeGraph?) → ModuleCandidate[]`

Graph-cluster detector requires edge graph; others do not.

### Constraint 2: Evidence Attribution

Every inferred module must have evidence rows linking to:
- Source file (Kbuild, Makefile, etc.) for Source A
- Directory path + pattern name for Source B
- Algorithm name + metrics for Source C

Evidence is not optional. An inferred module without evidence is a bug.

### Constraint 3: Degradation Propagation

If Source C is unavailable (collapsed topology), the orchestrator must:
1. Skip graph-cluster detection (not fail)
2. Record `unavailable` status with reason
3. Propagate flag to `modules list` output

This matches the existing `rollups_degraded` pattern in Rust CLI.

### Constraint 4: Layer Precedence

When Layer 1/2/3 candidates overlap at the same root:

```
Layer 1 (declared) > Layer 2 (operational) > Layer 3 A (build-system) >
Layer 3 B (directory) > Layer 3 C (graph-cluster)
```

Higher-layer candidate wins for **module root ownership/classification**.
Lower-layer evidence is retained for audit, not discarded.

### Constraint 5: No Automatic Boundary Inference

Layer 3 discovers module **existence**, not module **boundaries** (forbids).
Boundary declarations remain human-authored or explicit policy.

A graph-cluster detector may suggest "these modules are loosely coupled"
but it does NOT auto-generate `--forbids` declarations.

## Locked Decisions

### D1: Kbuild Parsing Scope — MINIMAL (Locked)

Parse only:
- `obj-y` assignments
- `obj-m` assignments
- Direct directory/object assignments

Do NOT parse:
- Conditionals (`ifeq`, `ifdef`)
- Kconfig variable resolution
- Variable expansion beyond trivial local forms

**Rationale:** Smallest explicit-artifact slice. Highest signal. Cleanest
validation target on Linux-style repos. Conditional compilation is a v2
concern with high complexity cost.

### D2: Directory Heuristic Rules — FIXED with Anchored Roots (Locked)

**Anchored root directories (explicit list):**
- `src/`
- `lib/`
- `pkg/`
- `internal/`
- `drivers/` (C-family repos)
- `arch/` (C-family repos)

**Minimum thresholds:**
- File count >= 5
- Language coherence >= 80% (same primary extension)

**Explicit exclusions (never infer as modules):**
- `test/`, `tests/`, `__tests__/`
- `vendor/`, `vendors/`
- `third_party/`, `third-party/`
- `external/`
- `deps/`
- `node_modules/`

**Rationale:** Fixed thresholds are explainable. Anchored roots prevent
junk modules from arbitrary deep directories. Exclusions match the
vendored/test heuristics already shipped in hotspot filtering.

### D3: Graph Clustering Algorithm — DEFERRED (Locked)

Source C implementation is deferred until Source A and B are validated.

When Source C implementation begins:
- Use Louvain (community detection), not custom algorithm
- Custom work applies to: edge weighting, cohesion/coupling metrics,
  confidence derivation from cluster quality
- The clustering algorithm itself should not be reinvented

**Rationale:** Known solution exists. Custom clustering is reinvention
without evidence. Louvain is well-understood and explainable.

### D4: Confidence Calibration Method — DUAL (Locked)

Calibration requires both:

1. **Declared-module comparison** — where Layer 1/2 modules exist, compare
   Layer 3 inferences to ground truth
2. **Manual review** — for C/Kbuild repos without declared modules, human
   review is required

Neither alone is sufficient. Layer 1/2 repos provide partial ground truth.
C/Kbuild repos (the primary target) often lack declared modules entirely.

## Implementation Order (Locked)

```
A1: Kbuild detector only
    ↓
    validate on linux kernel or similar Kbuild repo
    ↓
B1: Directory detector
    ↓
    validate interactions and precedence on mixed repos
    ↓
C1: Design Source C (only after A/B validated)
```

Each step is a separate slice with its own tests and validation checkpoint.
Source C design does not begin until A and B are validated on real repos.

## Success Criteria (Design Slice)

Design slice complete:

1. Three inference sources are architecturally distinct
2. Confidence model uses ordinal/coarse bands (not pseudo-precise decimals)
3. `unavailable` is distinct from `low confidence`
4. Precedence rules use "module root ownership" wording (not "boundary placement")
5. D1–D4 decisions are locked
6. Implementation order is locked with validation checkpoints

## Progress

- [x] A1: Kbuild detector implemented and validated on Linux kernel
- [ ] A1: Wire into module discovery orchestrator
- [ ] B1: Directory detector
- [ ] B1: Validate interactions and precedence
- [ ] C1: Design Source C (after A/B validation)

## Shipped (A1)

### Kbuild Detector (2026-04-23)

Pure function implementation: `src/core/modules/detectors/kbuild-detector.ts`

**Scope (minimal, as locked):**
- `obj-y` directory assignments
- `obj-m` directory assignments  
- `obj-$(CONFIG_...)` directory assignments
- Line continuation handling
- Nested path resolution

**NOT implemented (as locked):**
- Conditionals (ifeq, ifdef) — skipped with diagnostic
- Kconfig variable resolution
- Variable expansion — skipped with diagnostic

**Unit tests:** 25 tests in `test/core/modules/detectors/kbuild-detector.test.ts`

**Linux kernel validation:**

| Makefile | Modules Detected |
|----------|------------------|
| drivers/Makefile | 153 |
| fs/Makefile | 76 |
| net/Makefile | 71 |
| samples/Makefile | 25 |
| sound/Makefile | 25 |
| kernel/Makefile | 21 |
| lib/Makefile | 19 |
| security/Makefile | 13 |
| Total (11 Makefiles) | **410** |

The detector correctly:
- Identifies subdirectory module boundaries from obj-y/obj-m assignments
- Ignores object file assignments (`.o`) — not module boundaries
- Skips assignments inside conditional blocks (ifeq/ifdef) — 8 skipped
- Records conditional directive lines as diagnostics — 36 recorded

Conditional block tracking ensures HIGH confidence is only assigned to
modules whose existence does not depend on Kconfig evaluation.

## Deferred Items

- Directory detector implementation (B1 slice)
- Graph clustering design (C1 slice, after A/B validation)
- Orchestrator integration (wire Kbuild detector into module discovery)
- Migration schema for inferred modules
- CLI changes
- Confidence calibration study
- GNU Makefile detector
- CMake detector

## Risk Assessment

Design risk: low. This slice defines structure, not implementation.

Implementation risk (future): medium. Kbuild parsing is language-specific
complexity. Directory heuristics may produce false positives on unusual
repo layouts. Graph clustering on large repos may be expensive.

Mitigation: per-source implementation slices with validation checkpoints.
No Source C work begins until A and B are proven.
