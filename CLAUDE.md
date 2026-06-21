# CLAUDE.md

## Mission

Deterministic code-intelligence substrate for AI agent orientation.
Discovery is the goal. Trustworthy current-state facts. Honest degradation reporting.

If a rule can be enforced by script, hook, or CI, prefer enforcement over instruction.

You are not constrained by human development timelines. No need to cut corners. Implement full solutions.

## VISION: Orientation over Perfection

80% right with 20% effort beats 100% right with 100% effort. We beat the command line tools like grep, sed, awk — we don't replace compilers. Precision matters for call graphs; fuzziness is fine for module discovery (agents can refine). Build with effort where it matters — ask for keeping it simple where the informational gains are not that good.

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

## Generated OS Cruft

- `.DS_Store` (and kin: `Thumbs.db`, editor swap/lock files) are gitignored. macOS regenerates `.DS_Store` on directory access; it cannot enter a commit (`git add <dir>` honors `.gitignore`).
- Do not delete, flag, or mention them. Do not interfere with OS-generated noise — macOS does its thing, we do not fight it.

## Local Development Build

After code changes that need testing against the installed daemon:

```bash
./scripts/dev-install-local.sh
```

This builds release binaries, restarts the daemon, and validates the installation. macOS only.

## End-of-Slice Procedure

When a slice's code work is done, run the three phases defined in
`docs/testing/end-of-slice-procedure.md`: **Test → Install/deploy → Cleanup**.

- **Test** (always, before handoff): `cargo build/fmt/clippy/test` in `rust/`, the
  smoke scripts (`docs/testing/rmap-test-protocol.md`), AND the isolated live `rmap`
  dogfood `./scripts/dogfood-isolated.sh` — runs `orient`/`explain`/`check` on a
  fixture in a throwaway state root, never touching the operator's daemon/registry.
- **Install / deploy** (`./scripts/dev-install-local.sh`): ONLY after reviewer approval.
- **Cleanup** (`./scripts/clean-build.sh --all`): at slice end (debug artifacts measured ~14 GB).

To run `rmap` when no repo is indexed (the `error: repo not indexed` case), use the
isolated dogfood — never index into the operator's real registry to test.

## Persistence Completeness

Persisted feature is incomplete without: write path, read path, refresh behavior, trust impact, CLI visibility, validation.

## Decision Autonomy

Match ceremony to blast radius. Do not stop for low-stakes calls; do not decide unilaterally on
load-bearing ones. "Decision" in any always-ask rule means an **architecture-boundary or
invariant-affecting** decision — not naming or local mechanism.

**Decide and record (do NOT stop):**

- Naming (crates, types, fns, tests) once a convention exists.
- Local implementation detail with no effect across an architectural boundary.
- Choices the docs, an established convention, or a ratified decision already imply.
- Commit shape / split once the convention is set; which validation commands to run.

Record the call in one line (slice doc, task packet, or commit body). Decided ≠ silent.

**Stop and ask (BLOCKING):**

- Architecture boundary: a new module/crate boundary, dependency edge, or data shape crossing a boundary.
- Contradiction with evidence, `docs/`, or a ratified decision.
- Risk of a false trust claim: anything that could present Layer 2–4 as Layer 0, or mislabel
  certainty / freshness / ownership.
- A discovered mechanism that threatens a ratified invariant.

Spec only the **invariants** up front; discover mechanisms by building/probing; stop only when a
discovered mechanism threatens an invariant — not at every uncertainty. Tie-break: foundational or
irreversible → stop; local or cheap-to-unwind → decide and record. When you do surface a decision,
present it as an exhaustive matrix (every cell filled), not loose bullets — gaps belong at sign-off,
not a later correction.

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
- `docs/testing/end-of-slice-procedure.md` — Test → Install → Cleanup + isolated `rmap` dogfood
