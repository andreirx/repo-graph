# Documentation Guidelines

Rules for documentation structure, placement, and lifecycle.

## Directory Structure

```
CLAUDE.md                 # Agent instructions (behavioral contract)
CURRENT_SLICE.md          # Active work pointer
agent_docs/               # Agent-specific reference (validation, architecture, CLI)
docs/
  VISION.md               # Strategic direction (highest priority)
  ROADMAP.md              # Prioritized work items
  TECH-DEBT.md            # Known issues and deferred work
  documentation.md        # This file
  design/                 # Design documents for features
  slices/                 # Active, planned, and partial slice documents
  shipped/
    slices/               # Completed slice documents
    policy-facts/         # Completed policy-fact extraction docs
```

## What Belongs Where

| Content Type | Location | Notes |
|--------------|----------|-------|
| Agent behavioral rules | `CLAUDE.md` | Keep minimal; what to do, not how docs are structured |
| Current work context | `CURRENT_SLICE.md` | Points to active slice |
| Strategic direction | `docs/VISION.md` | Rarely changes |
| Priority order | `docs/ROADMAP.md` | What to work on next |
| Technical debt | `docs/TECH-DEBT.md` | Known issues, deferred decisions |
| Feature design | `docs/design/*.md` | Architecture decisions, API contracts |
| Active/partial slices | `docs/slices/*.md` | Scoped work with acceptance criteria |
| Completed slices | `docs/shipped/slices/*.md` | Archived slice documents |
| Completed policy-facts | `docs/shipped/policy-facts/*.md` | Archived policy-fact extraction docs |
| Agent reference | `agent_docs/*.md` | Validation protocol, architecture rules, CLI patterns |

## Slice Lifecycle

Slice documents track execution units from planning through completion.

### Status Taxonomy

Use exactly one:
- `PLANNED` — not started
- `IN_PROGRESS` — active implementation
- `PARTIAL` — some scope shipped, remainder explicitly documented
- `IMPLEMENTED` — code complete, validation pending
- `SHIPPED` — fully validated, in production
- `SUPERSEDED` — replaced by another slice
- `WITHDRAWN` — abandoned, kept for historical record

### Location Rules

- **Active/planned slices:** `docs/slices/`
- **Completed slices:** `docs/shipped/slices/`
- **Completed policy-fact docs:** `docs/shipped/policy-facts/`

### Completion Rule

When a slice reaches `SHIPPED` status, move it from `docs/slices/` to `docs/shipped/slices/`.

### Partial Status

When a slice is `PARTIAL`:
- Keep in `docs/slices/` (not shipped)
- Document what shipped and what remains in the slice header
- Update scope section to show shipped vs remaining work

Example: `Status: PARTIAL — Rust path shipped for TS/JS source patterns; Python/Java pending`

## Naming Conventions

### Slice Documents

Format: `{prefix}-{number}-{description}.md`

Prefixes:
- `bi-` — Boundary Interaction
- `mb-` — Message Broker
- `sb-` — State Boundaries
- `fd-` — Framework Detection
- `dep-` — Dependencies
- `py-ext-` — Python Extractor
- `pf-` — Policy Facts

Examples:
- `bi-1a-local-ipc.md`
- `mb-2-kafka-topic-detection.md`
- `pf-3-return-fate.md`

### Sub-slices

When a slice is large, split into sub-slices with letter suffixes:
- `mb-1-rabbitmq-amqp-basics.md` — umbrella slice
- `mb-1a-rabbitmq-amqp.md` — first implementation slice (e.g., TS/JS)
- `mb-1b-rabbitmq-python.md` — second implementation slice (e.g., Python)

## Reconciliation Rules

When implementation status changes, update all relevant truth surfaces:

1. **Slice document** — update status, move to `docs/shipped/slices/` if fully shipped
2. **`docs/ROADMAP.md`** — update priority items to reflect completed/remaining work
3. **`docs/TECH-DEBT.md`** — add entry if residual gaps or deferred work remains
4. **`CURRENT_SLICE.md`** — update only if active priority, active slice, or operational validation contract changes (not for every status update)

**Staleness is a bug.** If code ships but docs say PLANNED, the docs are wrong.

**Partial status requires explicit scope documentation.** When a slice is PARTIAL, the slice must state:
- What shipped (with implementation references)
- What remains (with blockers if any)

## CLAUDE.md Boundaries

`CLAUDE.md` contains agent behavioral instructions:
- What to do before code changes
- How to validate
- When to stop
- Decision hierarchy

`CLAUDE.md` does NOT contain:
- Documentation structure rules (put here)
- Detailed architecture (put in `agent_docs/architecture.md`)
- Design decisions (put in `docs/design/`)
