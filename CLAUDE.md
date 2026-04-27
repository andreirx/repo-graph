# Repo-Graph

Deterministic code graph tool for AI agent consumption. Multi-language. CLI over SQLite.

Two CLI binaries: `rmap` (Rust, primary) and `rgr` (TypeScript, legacy).

## Mission

**Discovery is the primary goal.** See `docs/VISION.md` §Absolute Priority.

Current-state agentic discovery. Not history accumulation. Not vector search.
The graph answers: "What exists now? What changed? What is risky? Where should I look?"

Enforcement (policies, gate verdicts, waivers) exists as available substrate, not product center.

## Mandatory Architecture Rules

1. **Dependency rule:** inward only. Core never imports adapters or CLI.
2. **Support module first:** build support libraries, then implement features using them.
3. **Storage is adapter:** domain logic never lives in storage or CLI layers.
4. **Docs are primary:** documentation inventory is the primary surface; semantic facts are secondary derived hints.
5. **Deterministic output:** same input → same output. No randomness, no order jitter.
6. **Explicit degradation:** `null` = unknown, empty = known-zero. Never conflate.

## Implementation Workflow

Before changing code:
1. Read relevant design docs in `docs/architecture/` or `docs/design/`.
2. Use `rmap` to understand callers, callees, imports of what you're touching.
3. Confirm which support module owns the logic vs. which adapter/CLI wires it.

Build order:
1. Support module (pure domain logic, tested in isolation)
2. Storage port + adapter implementation
3. Feature wiring in CLI
4. Tests (unit → integration → smoke on real repos)
5. Documentation updates

After changing code:
1. Run relevant test suite (`pnpm test`, `cargo test -p <crate>`).
2. Re-index with `rmap refresh ./repo-graph ./repo-graph.db`.
3. Document any debt immediately in `docs/TECH-DEBT.md`.

## Quality Bar

- Correct first. Fast is a given with agentic coding.
- No silent contract drift. If output shape changes, update contracts.
- Discovery surfaces must be clear. An agent should see what changed, not just pass/fail.
- No mixing extracted fact with inferred hint. Keep provenance clear.

## Testing Expectations

- Unit tests for pure functions and support modules.
- Integration tests against fixture repos in `test/fixtures/`.
- Smoke validation on real repos before declaring slice complete.

**Validation repos** (use for smoke runs, document results in slice notes):
- Internal: `repo-graph`, `../amodx`, `../glamCRM`, `../hexmanos`
- External: `../legacy-codebases/spring-petclinic`, `sqlite`, `nginx`, `swupdate`, `linux`

## Documentation and Debt Recording

- Record technical debt immediately in `docs/TECH-DEBT.md`.
- Record Rust CLI temporary gaps in `docs/TECH-DEBT.md`.
- Record Rust CLI intentional contracts in `docs/cli/rmap-contracts.md`.
- Update `docs/ROADMAP.md` when shipping or deferring features.
- Do not accumulate slice history in CLAUDE.md.

## Forbidden Patterns

- Do not put domain logic in CLI handlers or storage adapters.
- Do not erase computed facts under policy overlays — overlay, don't replace.
- Do not treat documentation as semantic facts only — docs are primary evidence.
- Do not use history accumulation as product center — current-state discovery only.
- Do not default to "mirror TS" without explicit justification.
- Do not store summaries of obvious code — store surprises, constraints, hazards.

## Canonical Doc Map

| Topic | Location |
|-------|----------|
| Vision and priorities | `docs/VISION.md` |
| What's shipped/next | `docs/ROADMAP.md` |
| Known limitations | `docs/TECH-DEBT.md` |
| CLI contract (Rust) | `docs/cli/rmap-contracts.md` |
| CLI contract (TS) | `docs/cli/v1-cli.txt` |
| Database schema | `docs/architecture/schema.txt` |
| Folder layout | `docs/architecture/project-structure.txt` |
| Measurement model | `docs/architecture/measurement-model.txt` |
| Rust milestone | `docs/milestones/rmap-structural-v1.md` |

## Commands (Quick Reference)

```bash
# Build and test
pnpm run build              # Build TypeScript
pnpm run test               # TS tests (includes CLI integration)
pnpm run test:rust          # All Rust crates
pnpm run test:all           # Full TS + Rust acceptance

# Lint
pnpm run lint               # Run Biome linter
pnpm run lint:fix           # Auto-fix

# Discovery (rmap uses <db_path> <repo_uid>)
rmap orient ./repo-graph.db repo-graph --focus "src/core"
rmap explain ./repo-graph.db repo-graph "src/core/some-file.ts"
rmap check ./repo-graph.db repo-graph
rmap trust ./repo-graph.db repo-graph

# Structural queries
rmap callers ./repo-graph.db repo-graph <symbol>
rmap callees ./repo-graph.db repo-graph <symbol>
rmap imports ./repo-graph.db repo-graph <file>

# After editing
rmap refresh ./repo-graph ./repo-graph.db
```

For full command reference, see `docs/cli/rmap-contracts.md`.

## Conventions

- Use `pnpm` not npm.
- `rmap` (Rust) outputs JSON only. `rgr` (TS) supports `--json` with human-readable default.
- No emojis in code or output.
- Relative paths in DB, never absolute.
- TEXT UIDs everywhere, no auto-increment integers.

## Native Dependency Note

`better-sqlite3` is keyed to Node ABI. If you switch Node versions:
```bash
nvm use && pnpm rebuild better-sqlite3
```
