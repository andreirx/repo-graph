# CLI-OUT-2A: Cross-Repo Output Audit

**Status:** HANDOFF COMPLETE  
**Type:** Product Surface / Audit  
**Priority:** Current (2026-05-18)  
**Prerequisite:** None (SMOKE-1 deferred; current harness sufficient for manual review)  

## Problem Statement

CLI-OUT-1 established the rendering boundary (human-default, --json opt-in).
It did not prove product usefulness.

Current human output is cleaner than raw JSON but thinner than useful:

1. **Repo identity is internal** — shows `repo_01kr...` not meaningful name/path
2. **Signal compression loses operational detail** — "1 import cycle" with no members
3. **Degradation is repetitive boilerplate** — same LOW text, no repo-specific evidence
4. **No "where to look first"** — no top offenders, no representative examples
5. **Intentional truncation** — `[Output truncated. Use --json for full results.]`

This slice is **audit only**. Implementation is CLI-OUT-2B.

## Scope

### This Slice (2A)
- Run audit corpus
- Document findings per repo
- Compare current human output vs JSON
- Synthesize defects by command
- Define revised output contracts

### Not This Slice (Deferred to 2B)
- Renderer implementation
- New human renderers for JSON-dump commands
- Before/after smoke comparisons

## Audit Corpus

### Commands

| Command | Current State | Audit Priority |
|---------|---------------|----------------|
| `orient` | Human renderer, truncates | HIGH — primary orientation surface |
| `trust` | JSON dump | HIGH — core diagnostic surface |
| `cycles` | JSON dump | HIGH — structural problem surface |
| `stats` | JSON dump | MEDIUM — module metrics |
| `check` | Human renderer | MEDIUM — pass/fail verdict |
| `explain` | Human renderer | MEDIUM — entity drilldown |

### Repos

| Repo | Languages | Shape |
|------|-----------|-------|
| OpenXcom | C++ | Medium game, known cycle |
| duckdb | C++ | Large database |
| django | Python | Large framework |
| buildroot | C/Make/Shell | Build system, mixed |
| grpc-java | Java | RPC framework |
| gstreamer | C | Large media framework |
| hadoop | Java | Large distributed system |

## Audit Methodology

### Phase 1: Corpus Runs

For each repo, run orientation bundle:
```bash
# After SMOKE-1 implements --cmd:
./scripts/smoke-rmap.sh cli-out-2a-audit <repo> \
  --cmd "orient" --cmd "trust" --cmd "cycles" \
  --cmd "stats" --cmd "check"

# Until then, run separately with --retain
```

Store outputs in `smoke-runs/cli-out-2a-audit-<repo>/`.

### Phase 2: Per-Repo Documentation

For each repo, create `docs/audits/cli-out-2a/<repo>.md`:

1. **What the user learns** from each command output
2. **What is missing** compared to JSON
3. **What is noise** (repeated boilerplate, internal IDs)
4. **Specific improvement recommendations**

### Phase 3: Command-Level Synthesis

Create `docs/audits/cli-out-2a/synthesis.md`:

For each command:
- Common defects across repos
- Decision-relevant information currently lost
- Drilldown anchors currently missing
- Proposed contract changes

## Audit Dimensions

For each command output, evaluate:

1. **Orientation value**
   - Does it tell what matters?
   - Does it tell why it matters?
   - Does it tell where to look next?

2. **Decision-relevant information retention**
   - Concrete entity names (files, modules, symbols)
   - Representative examples
   - Counts with diagnostic context
   - Not: full JSON envelope preservation

3. **Evidence vs. interpretation separation**
   - Are facts visually distinct from derived rollups?
   - Are degradation warnings grounded in visible evidence?

4. **Repetition vs. specificity**
   - Are boilerplate warnings drowning repo-specific insight?

5. **Scanability under line budget**
   - Can an agent reading first 20-40 lines get useful orientation?
   - (Agent decides to cut lines; command preserves substance)

## Output Contract Principles (Draft)

To be validated/refined by audit findings:

### 1. No silent deletion of decision-relevant facts
The renderer must not hide facts that would change user decisions.
Full machine detail remains available in `--json`.

### 2. Concrete entities over abstract counts
Bad: `178 symbols exceed complexity threshold`
Good: `178 symbols exceed complexity (top: Foo::bar 42, Baz::qux 38, ...)`

### 3. Evidence-bearing degradation
Bad: `Call-graph reliability is LOW on this repo.`
Good: `Call-graph reliability is LOW (33% call resolution, 29672 unresolved).`

### 4. Fact-triggered next-step guidance
- Only when directly grounded in visible findings
- At most a few commands
- Not canned noise

### 5. Human-meaningful repo identity
Show repo name or alias, not internal UID.

## Definition of Done

- [x] Corpus review sufficient for first implementation wave
  - 5 of 7 repos audited
  - 2 repos (gstreamer, hadoop) blocked by indexing timeout (RMAPD-PERF-1)
- [x] Per-repo audit docs in `docs/audits/cli-out-2a/`
- [x] Command-level synthesis doc complete
- [x] Revised output contracts proposed for first-contact discovery commands
- [x] Handoff to CLI-OUT-2B complete

**Not achieved (handed off):**
- Full 7-repo audit (blocked by RMAPD-PERF-1)
- explain command audit (deferred to CLI-OUT-3)

**Explicit non-goals for this slice:**
- No renderer code changes
- No new presentation modules
- No before/after implementation comparison

## Files Produced

- `docs/audits/cli-out-2a/openxcom.md`
- `docs/audits/cli-out-2a/duckdb.md`
- `docs/audits/cli-out-2a/django.md`
- `docs/audits/cli-out-2a/buildroot.md`
- `docs/audits/cli-out-2a/grpc-java.md`
- `docs/audits/cli-out-2a/gstreamer.md`
- `docs/audits/cli-out-2a/hadoop.md`
- `docs/audits/cli-out-2a/synthesis.md`
- `smoke-runs/cli-out-2a-audit-*/` (audit artifacts)

## Files Out of Scope

- Any Rust code
- Presentation modules
- CLI command implementations

## Risk Assessment

**Scope risk:** Low. Audit-only slice with clear deliverables.

**Dependency risk:** Medium. SMOKE-1 improves repeatability but is not blocking.
Manual runs work; they're just less clean.

## Relationship to CLI-OUT-2B

This slice produced:
- Evidence of what's wrong
- Proposed contract changes for first implementation wave

CLI-OUT-2B consumes that and implements renderer changes.

**Handoff complete.** CLI-OUT-2B is now current.
