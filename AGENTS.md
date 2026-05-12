# CLAUDE.md

## Mission

Deterministic code-intelligence substrate for AI agent orientation.
Discovery is the goal. Trustworthy current-state facts. Honest degradation reporting.

If a rule can be enforced by script, hook, or CI, prefer enforcement over instruction.

You are not constrained by human development timelines. No need to cut corners. Implement full solutions.

## Fact Certainty Model

Not all stored data has the same factual status.

- Layer 0–1: deterministic extracted facts
- Layer 2: bounded inferences derived from facts
- Layer 3: evidence-backed orientation hints
- Layer 4: governance/policy overlays

Never describe Layers 2–4 as if they were Layer 0 truth.
Never collapse unknown, inferred, and extracted into the same certainty class.
See `agent_docs/architecture.md` for the full layer model.

## Decision Hierarchy

When instructions conflict, obey in this order:

1. `docs/VISION.md`
2. Current priority in `docs/ROADMAP.md`
3. `CURRENT_SLICE.md`
4. Relevant `agent_docs/*.md`
5. Other docs in `docs/`
6. Local style

Higher wins. Do not trade priority for polish.

## Mandatory Preflight

Before code changes:

1. Read `docs/VISION.md`
2. Read current priority in `docs/ROADMAP.md`
3. Read `CURRENT_SLICE.md`
4. Read relevant slice doc and relevant `agent_docs/*.md`
5. Use `rmap` to inspect current system state
6. Produce a task packet before editing

## Task Packet

State before editing:

- task type (feature / bug / refactor / validation)
- active priority (quote from roadmap)
- definition of done (from slice)
- why this task is on priority path now
- files in scope
- files explicitly out of scope
- storage / refresh / trust / CLI impact
- validation commands
- stop conditions

## Tool Hierarchy

1. Use `rmap` first for orientation and validation.
2. Raw SQL only after CLI, only for storage diagnostics.
3. Never validate user-facing behavior through SQL alone.

## Evidence Law

Label every validation claim:

- `EXECUTED` — command run, output observed
- `OBSERVED` — artifact inspected
- `INFERRED` — concluded from context
- `NOT RUN` — skipped

Never present inferred output as observed.

## Structural Guardrails

- `main.rs` is wiring only.
- Do not append new responsibilities to files over 500 lines.
- Refactor before expanding mixed-responsibility files.

## Command Execution

- Write bash commands ONE AT A TIME. Compound commands (`&&`, `;`, pipes) trigger permission checks that block execution. Single commands flow faster.

## Persistence Completeness

Persisted feature is incomplete without: write path, read path, refresh behavior, trust impact, CLI visibility, validation.

## Stop Conditions

Stop and report if:

- work conflicts with current priority
- architecture would be violated
- refresh implications unclear
- validation cannot execute
- task drift detected

## Read Next

- `CURRENT_SLICE.md` — what matters now
- `docs/documentation.md` — doc structure and slice lifecycle
- `agent_docs/validation.md` — evidence protocol
- `agent_docs/architecture.md` — full architecture rules
- `agent_docs/rmap-orientation.md` — CLI patterns
