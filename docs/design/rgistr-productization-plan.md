# rgistr Productization Plan

Status: PLANNING

Owner surface: `tools/rgistr`

Maturity target:
- Current: PROTOTYPE
- Near-term target: MATURE
- Long-term target: PRODUCTION

## Purpose

Tighten `rgistr` from a useful script into a productized documentation generator
that works out of the box across cloud and local inference backends, while
remaining separate from repo-graph's deterministic extraction core.

**rgistr is secondary to rmap.** The product center is pragmatic documentation
generation with provenance, not a substrate for other tools. Architecture
decisions optimize for stability and bounded scope, not maximum abstraction
purity.

This document records the intended product shape, support modules, sequencing,
constraints, and non-negotiable rules.

## Product Goal

`rgistr` should generate an inverted-pyramid documentation map for real
repositories without silent failure modes, hidden backend assumptions, or file
skipping.

The product contract is:

1. Discover available LLM backends before generation starts.
2. Present a clear, deterministic availability report.
3. Never skip source files due to size.
4. Chunk oversized files and generate per-chunk gists.
5. Roll chunk gists into file gists.
6. Roll file gists and child-folder gists into folder `MAP.md`.
7. Preserve explicit provenance for every synthesized level.

## Current State

**Shipped:**
- Provider discovery support module (OpenAI cloud, OpenAI-compatible local, Ollama)
- Model capability support module (registry, budget calculation)
- Chunking support module (planning, identity, artifact serialization)
- Two-mode file routing: whole-file (≤200KB) or chunked (>200KB)
- No silent file skipping — all code files processed
- `rgistr discover` CLI command
- Discovery-assisted preflight in `generate` command (fail-closed, no auto-selection)

**Remaining prototype constraints:**
- No MLX-specific adapter (uses OpenAI-compatible)
- No llama.cpp-specific adapter (uses OpenAI-compatible)

## Non-Negotiable Product Rules

### 1. Never skip files

If a file is too large for single-shot summarization, it must be chunked.

Forbidden behavior:
- silently skipping large files
- omitting a file from folder synthesis because of size
- pretending a folder summary is complete when some source files were dropped

Required behavior:
- generate chunk-level summaries
- synthesize chunk summaries into a file-level summary
- include file-level summary in folder synthesis
- mark uncertainty and basis explicitly where chunking limited global visibility

### 2. Chunking is the large-file strategy

The product must treat chunking as a first-class support module, not as an
incidental fallback.

Chunking responsibilities:
- split files on structurally meaningful boundaries where possible
- preserve order
- preserve source path and line-span provenance
- allow overlap when needed for continuity
- emit deterministic chunk identifiers
- support chunk regrouping into a file synthesis stage

### 3. Discovery before generation

Before generation, `rgistr` should probe available providers in a fixed order
and print findings.

Initial discovery surfaces:
- OpenAI via `OPENAI_API_KEY`
- OpenAI-compatible local servers
  - LM Studio
  - MLX server
  - llama.cpp server
- Ollama

If nothing is available, the tool must stop and print exact missing
requirements.

### 4. OpenAI-compatible local backends are one transport family

LM Studio, MLX server, and llama.cpp should be treated as one transport family
for discovery and request execution, with backend flavor metadata layered on
top.

Rationale:
- shared `/v1/models`
- shared `/v1/chat/completions`
- shared OpenAI-compatible request/response concepts
- duplication is architectural waste

Ollama remains separate because its transport and model enumeration differ.

### 5. Context policy must be explicit

`rgistr` must not bury model-context assumptions inside file-size heuristics.

The product should expose and use explicit capability metadata:
- maximum input context
- maximum output tokens
- safe synthesis budget
- JSON-mode support
- streaming mode
- transport family

Planning defaults:
- OpenAI preferred cloud model: `gpt-4.1-mini`
- Local planning budget: target 200k-token working budget on 256k-class models

This planning target is for product policy and chunk sizing. Runtime capability
detection should still override defaults when the backend exposes stricter
limits.

### 6. Provenance must survive summarization

Every generated layer should know what it was built from:

- chunk summary -> source file + line span
- file summary -> ordered list of chunk summaries
- folder summary -> file summaries + child folder maps

This is required for staleness detection, trust marking, and future repo-graph
ingestion.

## Target Product Architecture

`rgistr` remains separate from repo-graph, but internally it should adopt a
cleaner layered structure.

### Core policy/support modules

These own stable product rules.

1. **Provider discovery support module**
   - provider candidates
   - backend probes
   - normalized discovery report
   - ranking and preferred-model resolution

2. **Model capability support module**
   - model capability registry
   - safe prompt budget calculation
   - transport family metadata
   - context budget policy

3. **Chunking support module**
   - deterministic chunk planning
   - structural splitting when possible
   - chunk line-span tracking
   - overlap rules
   - chunk identity/versioning

4. **Synthesis orchestration support module**
   - chunk -> file rollup
   - file -> folder rollup
   - future folder -> repo rollup
   - uncertainty aggregation rules

5. **Artifact/provenance support module**
   - frontmatter contract evolution
   - chunk/file/folder basis recording
   - freshness and invalidation rules

### Outer adapters/mechanisms

These perform volatile work.

1. OpenAI cloud adapter
2. OpenAI-compatible local adapter
3. Ollama adapter
4. Backend probe adapters
5. Filesystem artifact writer/reader
6. Future repo-graph context adapter

## Proposed Artifact Model

Current artifact model is too thin for chunk-first generation.

### Required generated scopes

1. **Chunk scope**
   - one artifact per chunk
   - contains line-span provenance
   - contains chunk order
   - machine-readable basis for file rollup

2. **File scope**
   - synthesized from chunk artifacts or whole-file pass
   - must record whether it was direct or chunk-derived

3. **Folder scope**
   - synthesized from file artifacts plus child folder artifacts

4. **Repo scope**
   - reserved for later

### Naming and storage

Product direction:
- keep `MAP.md` for folder scope
- keep file-level map artifacts
- add chunk-level generated artifacts with stable deterministic naming

The exact naming convention can be finalized during implementation, but it must:
- be deterministic
- survive source filenames with dots/underscores
- support staleness checks
- avoid ambiguity between file and chunk artifacts

## Discovery and Selection Flow

### Discovery order

1. Check `OPENAI_API_KEY`
2. Probe local OpenAI-compatible endpoints
   - `http://127.0.0.1:1234/v1/models`
   - `http://127.0.0.1:8080/v1/models`
   - additional configured endpoints later
3. Probe Ollama
   - `http://127.0.0.1:11434/api/tags`

### Output contract

Before generation, print:
- discovered backends
- endpoint
- backend flavor if known
- discovered models
- preferred model matches
- effective selected model
- planning context budget

If the user explicitly supplies `--adapter` and `--model`, discovery is skipped
and generation proceeds directly with the specified provider.

### Preferred models

Initial preference list:

- Cloud:
  - `gpt-4.1-mini`

- Local aliases:
  - `qwen3.6`
    - `qwen/qwen3.6-35b-a3b`
    - `qwen/qwen3.6-27b`

Exact discovered IDs should be preserved. Alias preference is only ranking
logic.

## Chunking Strategy

### Why chunking is mandatory

Large source files are common in legacy repositories. Skipping them creates
false silence exactly where the product is most needed.

### Chunk planning rules

1. Prefer structural boundaries first:
   - top-level functions
   - methods
   - classes
   - impl blocks
   - logically grouped declarations

2. Fall back to line-window chunking when structure extraction is weak.

3. Preserve ordered chunk indices.

4. Preserve line spans.

5. Allow bounded overlap to avoid seam loss at chunk borders.

6. If a single function or class still exceeds safe budget:
   - recursively subchunk it
   - never drop it

### Chunk synthesis stages

For files > 200KB (chunked path):

1. source file -> chunk plan
2. each chunk -> chunk gist
3. ordered chunk gists -> file gist (rollup)
4. file gist -> folder synthesis input

For files ≤ 200KB (whole-file path):

1. whole file -> file gist

Both paths produce the same file-summary shape for folder synthesis.

## File Routing Policy

**Two-mode routing (implemented):**

- Files ≤ 200KB: whole-file prompt
- Files > 200KB: chunked generation

No intermediate digest band. No silent file skipping.

The 200KB threshold (`WHOLE_FILE_THRESHOLD`) is a conservative cutoff that
ensures whole-file prompts fit comfortably in model context with room for
system prompt, output, and optional graph context.

### Scanner policy

The CLI passes `maxFileSize: Number.MAX_SAFE_INTEGER` to the scanner, ensuring
all code files are included regardless of size. The generator then routes them
to whole-file or chunked path based on the 200KB threshold.

### Chunked path budget

The capability module's budget calculation is used when:
- A file exceeds the 200KB threshold
- The chunking support module plans how to split it
- Chunk sizing respects model context limits

Token-budget-aware planning for the chunked path:
- estimate prompt footprint
- reserve output budget
- reserve system-prompt and wrapper overhead
- plan chunk boundaries within safe budget

This is a support module concern, not a CLI concern.

## Frontmatter and Contract Evolution

Chunk-first generation requires contract extension.

Likely additions:
- `scope: chunk`
- `source_span`
- `chunk_index`
- `chunk_count`
- `chunk_basis`
- `file_basis`
- `synthesis_mode: whole_file | chunk_rollup`

Contract rule:
- additive evolution only
- preserve compatibility for existing folder `MAP.md` consumers
- document all shape changes in `tools/rgistr/README.md` and repo-graph docs if
  ingestion semantics change

## Implementation Sequence

Build support modules first. Then wire behavior.

### Phase 1: Product contract and support modules

1. Define discovery DTOs and capability DTOs
2. Define chunk artifact contract
3. Define file rollup contract
4. Define budget policy contract
5. Define deterministic chunk identity scheme

### Phase 2: Provider discovery

1. Implement OpenAI env detection
2. Implement OpenAI-compatible local probing
3. Implement Ollama probing
4. Implement backend/model report rendering
5. Implement preferred-model ranking

### Phase 3: Chunking support module

1. Implement chunk planner
2. Implement structural chunking heuristics
3. Implement fallback line-window chunking
4. Implement per-chunk gist generation
5. Implement chunk -> file rollup

### Phase 4: Generator integration (SHIPPED)

**Implemented:** Two-mode file routing with no silent exclusions.

1. All code files are included (scanner has no size cap)
2. Files ≤ 200KB: whole-file prompt
3. Files > 200KB: chunked generation
   - Chunk planner splits file
   - Per-chunk gist generation
   - Rollup into file artifact
4. Freshness checks include chunk artifact integrity
5. Deterministic traversal order preserved

**What was removed:**
- Digest fallback path (100KB-500KB band)
- Hard file size limits that caused silent skipping
- `MAX_FILE_SIZE_WHOLE` and `MAX_FILE_SIZE_FOR_SUMMARY` constants

**Current constants:**
- `WHOLE_FILE_THRESHOLD = 200KB` — routing decision point

The capability module's budget calculation is used by the chunked path
for chunk sizing.

### Phase 5: CLI productization (SHIPPED)

**Implemented:** Discovery-assisted preflight with fail-closed behavior.

1. If no `--adapter` specified, run provider discovery
2. Print discovery report showing all probed endpoints and available models
3. If 0 providers available: exit 1 with guidance
4. If 1+ providers available: print example commands, exit 2 (require explicit selection)
5. Never auto-proceed — user must always specify `--adapter` and `--model`

**Exit codes:**
- 1: No providers available (nothing to choose from)
- 2: Providers available but explicit choice required

This enforces the "fail closed" product rule: rgistr does not silently pick a backend.

### Phase 6: Documentation and validation

1. Update `tools/rgistr/README.md`
2. Add fixture-based tests for chunk rollup
3. Add adapter tests for backend discovery normalization
4. Add smoke validation against real repositories

## Testing Expectations

### Support-module tests

- chunk planner determinism
- chunk line-span correctness
- recursive subchunk behavior
- discovery normalization
- preferred-model ranking
- token-budget decisions

### Integration tests

- whole-file generation path
- chunked-file generation path
- file rollup correctness
- folder rollup with mixed whole/chunked files
- stale chunk invalidation
- local backend discovery scenarios

### Smoke validation

Run on:
- `repo-graph`
- one medium repo with oversized files
- one legacy repo with very large C/C++ translation units

## Packaging and Releaseability Constraint

**Non-functional product constraint:** rgistr must be architected for future
downloadable release artifacts (GitHub Actions builds distributable binaries
for `rmap` and `rgistr`).

### What this constrains now

1. **No dev-environment assumptions in product architecture**
   - Local defaults are fine; hard assumptions are not
   - No fixed localhost ports as truth
   - No repo-relative writable paths assumed
   - No hidden dependence on local Node tooling beyond packaged runtime

2. **Backend discovery must be runtime-configurable**
   - Discovery/probing must work from: packaged binary, CI, local workstation
   - Endpoints from config/env/defaults
   - No adapter that assumes "LM Studio always means `:1234`" as hard rule
   - Flavor metadata must be explicit in discovery output

3. **Stable machine-facing output**
   - Deterministic exit codes
   - Machine-readable discovery report
   - Machine-readable provider/model selection report
   - Explicit version/build metadata
   - Required for: GitHub Actions smoke, release verification, issue triage

4. **Dependency footprint matters**
   - Support modules must be: pure, transport-neutral, easy to bundle
   - Avoid: multiple near-identical adapters, backend-specific branching
     spread everywhere, packaging-time conditional logic in core policy

### Adapter architecture decision

**Resolved:** Single `OpenAICompatibleAdapter` with flavor profiles.

Rationale:
- Differences between LM Studio, MLX server, and llama.cpp are flavor metadata,
  not transport-level divergence
- Matches design doc's "one transport family" rule
- Smaller adapter surface, less duplication
- Easier packaging and release testing
- Easier discovery/reporting (one transport family owns one contract)

Implementation shape:
- `OpenAICompatibleAdapter` — one transport-family adapter
- `OpenAICompatibleProbe` — probe/discovery layer
- `BackendFlavor = lmstudio | mlx | llamacpp` — flavor enum
- Flavor profile: default base URL, probe behavior, capability quirks, labeling

This keeps one transport family without collapsing into spaghetti.

## Assumptions

1. `rgistr` remains a separate tool from repo-graph.
2. Folder `MAP.md` remains the canonical folder artifact.
3. OpenAI-compatible local backends share enough transport contract to justify
   one local adapter family.
4. Local planning will target a conservative 200k-token working budget on
   256k-class backends unless runtime capability says otherwise.
5. Chunk-first generation is preferable to omission, even if it increases
   artifact count.

## Architecture Constraints

**rgistr is not a substrate product.** It is a pragmatic documentation tool.

Architecture decisions follow these priorities:
1. Protect existing operator-facing behavior where it already works
2. Isolate new features (chunking) from existing stable paths
3. Keep support modules separate from generator feature layer
4. Minimize blast radius across unrelated code

**What this means concretely:**

- Support modules own policy (capability, chunking, discovery)
- Generator consumes support modules where needed
- Two-mode routing: whole-file (≤200KB) or chunked (>200KB)
- No silent file exclusions
- The goal is bounded stability, not maximum abstraction purity

**What is explicitly NOT a goal:**

- Making capability/budget policy the single decision center for routing
  (the 200KB threshold is a simple constant, not a dynamic budget check)
- Treating rgistr as a first-class policy center like rmap

## Divergences From Current Prototype

1. Large files will no longer be skipped.
2. Byte-size heuristics will no longer be the decision center.
3. Adapter choice will no longer be purely manual-first.
4. Local OpenAI-compatible backends will be normalized instead of treated as
   separate ad hoc special cases.
5. Provenance will become multi-level rather than only file/folder level.

## Immediate Next Slice

The next implementation slice should be:

1. discovery support module
2. model capability support module
3. chunk artifact contract

That sequence establishes the stable seams before changing generator behavior.
