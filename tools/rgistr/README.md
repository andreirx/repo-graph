# rgistr - Recursive Gist Runner

Hierarchical documentation synthesis via LLM. Generates `MAP.md` files throughout a codebase using depth-first recursive summarization.

## Purpose

Many codebases lack adequate documentation. `rgistr` fills this gap by:

1. Scanning a codebase depth-first
2. Summarizing code files using an LLM
3. Synthesizing folder summaries from child summaries
4. Writing `MAP.md` files with machine-readable frontmatter

The generated MAPs can be consumed by repo-graph as documentation evidence.

## Architecture

`rgistr` is **separate from repo-graph** by design:

- repo-graph: deterministic engineering intelligence (extraction, queries, graph truth)
- rgistr: AI-generated documentation synthesis (volatile, model-dependent)

repo-graph later **ingests** generated MAP.md files as documentation evidence, but never generates them.

## Installation

```bash
cd tools/rgistr
npm install
npm run build
```

## Usage

### Generate MAP.md files

```bash
# Using LM Studio (default, localhost:1234)
rgistr generate ./path/to/repo

# Using Ollama
rgistr generate ./path/to/repo -a ollama -m llama3.2:3b

# Using OpenAI
rgistr generate ./path/to/repo -a openai -m gpt-4o-mini --api-key sk-...

# Options
rgistr generate ./path/to/repo \
  --max-depth 3 \           # Limit recursion depth
  --output SUMMARY.md \     # Custom output filename
  --force \                 # Regenerate existing MAPs
  --dry-run                 # Show what would be generated
```

### Scan without generating

```bash
rgistr scan ./path/to/repo
```

### Test LLM connection

```bash
rgistr test-connection -a lmstudio
rgistr test-connection -a ollama -m llama3.2:3b
```

## LLM Adapters

| Adapter | Endpoint | Notes |
|---------|----------|-------|
| `lmstudio` | `localhost:1234/v1` | OpenAI-compatible, local inference |
| `ollama` | `localhost:11434` | NDJSON streaming |
| `openai` | `api.openai.com/v1` | Cloud, requires API key |

All adapters support custom endpoints via `--endpoint`.

## MAP.md Frontmatter

Generated files include machine-readable frontmatter:

```yaml
---
generated_by: rgistr
generator_version: 0.1.0
adapter: lmstudio
model: local-model
basis_commit: abc123...
scope: folder
path: src/core
generated_at: 2024-01-15T10:30:00.000Z
synthesis_basis: code_only
confidence: low
child_maps:
  - src/core/types/MAP.md
source_files:
  - src/core/index.ts
  - src/core/utils.ts
---
```

### Frontmatter fields

| Field | Description |
|-------|-------------|
| `generated_by` | Always `rgistr` |
| `generator_version` | Semantic version |
| `adapter` | LLM adapter used |
| `model` | Model name |
| `basis_commit` | Git commit at generation time |
| `scope` | `folder` (v0.1.0; file/module/repo reserved for future) |
| `path` | Relative path from repo root |
| `generated_at` | ISO 8601 timestamp |
| `synthesis_basis` | What was used: `code_only`, `code_and_graph`, `code_graph_and_docs` |
| `confidence` | `high`, `medium`, or `low` |
| `child_maps` | Child MAP.md files used as input |
| `source_files` | Source code files summarized |

## Integration with repo-graph

repo-graph's documentation detector can ingest MAP.md files:

1. `rgistr` generates MAP.md files throughout a codebase
2. repo-graph indexes the codebase
3. repo-graph's doc detector reads MAP.md frontmatter
4. Generated summaries become queryable documentation evidence

This keeps AI-generated content clearly separated from deterministic facts.

## Design Decisions

### Why depth-first?

Child folders must be summarized before parents can synthesize from them.

### Why separate from repo-graph?

- LLM calls are expensive and non-deterministic
- Model/provider concerns should not pollute the graph engine
- Generated docs are advisory, not authoritative
- Provenance must be explicit

### Why frontmatter?

- Machine-readable metadata enables staleness detection
- repo-graph can distinguish authored vs generated docs
- Reproducibility tracking via basis_commit

## Limitations

**v0.1.0 scope:**
- Only folder-level MAPs are generated
- File, module, and repo scopes are reserved but not implemented

**General:**
- No semantic understanding of code (pure LLM synthesis)
- Quality depends on chosen model
- Large codebases may be expensive to process
- Staleness detection uses file mtime only (not content hash)

## Future Work

- Incremental regeneration (only stale folders)
- repo-graph graph context injection (enrich prompts with graph facts)
- File importance scoring (skip trivial files)
- Cost estimation before running
