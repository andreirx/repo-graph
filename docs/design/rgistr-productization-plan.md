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

Current implementation in `tools/rgistr/src/` has these prototype constraints:

- CLI requires direct adapter selection (`lmstudio`, `ollama`, `openai`).
- No discovery layer exists.
- No MLX adapter exists.
- No llama.cpp adapter exists.
- OpenAI-compatible local backends are not unified.
- `src/core/generator.ts` skips files above a hard size threshold.
- Whole-file vs digest behavior is based on byte size, not prompt budget.
- No per-chunk artifact layer exists.
- No model capability registry exists.
- No backend/model selection workflow exists.

These are prototype shortcuts, not acceptable product behavior.

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

If the user explicitly supplied adapter/model, discovery still runs as a
validation/reporting step but should not silently override explicit user input.

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

For oversized files:

1. source file -> chunk plan
2. each chunk -> chunk gist
3. ordered chunk gists -> file gist
4. file gist -> folder synthesis input

For non-oversized files:

1. whole file -> file gist

The orchestration contract must unify both paths so folder synthesis consumes a
single file-summary shape.

## Context Budget Policy

Current byte-based thresholds are a prototype shortcut and must be removed from
product logic.

### Planning policy

The product should plan around these working assumptions:

- local generation should fit within a conservative 200k-token working budget
  on 256k-class local backends
- OpenAI `gpt-4.1-mini` may use much larger whole-file windows when useful

### Required replacement

Introduce token-budget-aware planning:

- estimate prompt footprint
- reserve output budget
- reserve system-prompt and wrapper overhead
- reserve graph-context overhead when enabled
- decide:
  - whole file
  - chunked file
  - recursive subchunking

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

### Phase 4: Generator refactor

1. Remove hard file skipping
2. Replace byte-threshold branching with budget policy
3. Unify whole-file and chunked file outputs
4. Extend freshness checks to chunk artifacts
5. Preserve deterministic traversal order

### Phase 5: CLI productization

1. Run discovery automatically before generation
2. Print findings and selected execution plan
3. Respect explicit user override flags
4. Fail clearly when no viable backend/model exists

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

## Assumptions

1. `rgistr` remains a separate tool from repo-graph.
2. Folder `MAP.md` remains the canonical folder artifact.
3. OpenAI-compatible local backends share enough transport contract to justify
   one local adapter family.
4. Local planning will target a conservative 200k-token working budget on
   256k-class backends unless runtime capability says otherwise.
5. Chunk-first generation is preferable to omission, even if it increases
   artifact count.

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
