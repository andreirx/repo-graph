# Architecture Rules

## Mandatory Rules

1. **Dependency rule:** inward only. Core never imports adapters or CLI.
2. **Support module first:** build support libraries, then implement features using them.
3. **Storage is adapter:** domain logic never lives in storage or CLI layers.
4. **Docs are primary:** documentation inventory is the primary surface; semantic facts are secondary.
5. **Deterministic output:** same input → same output. No randomness, no order jitter.
6. **Explicit degradation:** `null` = unknown, empty = known-zero. Never conflate.
7. **Document-first authored knowledge:** discovery-oriented knowledge lives in documentation first; DB projections are secondary.
8. **Daemon is coordination authority:** clients must not bypass the daemon for direct storage access.

## Product Layer Stack

| Layer | Name | Certainty | Examples |
|-------|------|-----------|----------|
| 0 | Extraction substrate | Extracted fact | Files, symbols, IMPORTS, CALLS, stable keys |
| 1 | Architectural substrate | Extracted fact | Callers/callees, declared modules, docs inventory, trust |
| 2 | Derived architecture | Bounded inference | Inferred modules, runtime surfaces, risk overlays |
| 3 | Orientation hints | Evidence-backed hints | Framework detectors, IPC detection, gRPC links |
| 4 | Governance | Policy overlay | Declarations, assessments, gate, waivers |

Inner layers (0-1) = deterministic fact. Outer layers (2-3) = partial hints with explicit unknowns. Layer 4 overlays but never erases.

### Layer Rules

1. Never build Layer N before Layer N-1 is queryable.
2. Never describe Layer 3 heuristics as Layer 0 truth.
3. Layer 3 surfaces must include explicit unknowns.
4. Layer 4 overlays, never replaces underlying fact.
5. Maturity claims must specify layer.

## Core Business Logic

The stable product center is **legacy-code relationship modeling**.

Primary relationship families:
- seams and enabling points
- sensing/separation barriers
- module and boundary relationships
- state/resource touchpoints
- policy propagation relationships
- testability constraints
- migration/replacement relationships

Languages are evidence sources for the same relationship substrate, not separate products.

## Build Order

1. Support module (pure domain logic, tested in isolation)
2. Storage port + adapter implementation
3. Feature wiring in CLI
4. Tests (unit → integration → smoke on real repos)
5. Documentation updates

## Persistence Completeness Checklist

Any change that introduces or modifies persisted artifacts is incomplete until:

- [ ] storage schema exists
- [ ] write path implemented
- [ ] read/query path implemented
- [ ] refresh / copy-forward / invalidation behavior handled
- [ ] trust / maturity reporting impact addressed
- [ ] CLI visibility implemented
- [ ] validation covers both fresh index and refresh

**"Feature works on full index" is not sufficient if refresh can lose it.**

## Prohibited Patterns

- Do not put domain logic in CLI handlers or storage adapters.
- Do not erase computed facts under policy overlays — overlay, don't replace.
- Do not default to "mirror TS" without explicit justification.
- Do not append new responsibilities to oversized files.
- Do not claim feature complete without refresh durability.

## Language Direction

**Server/systems track:** TypeScript/JavaScript, Rust, Python, Java, C, C++

**Mobile/client track:** Objective-C, Objective-C++, Swift, Kotlin, Dart

Rust-primary maturity is not uniform. Do not claim parity where it does not exist.
