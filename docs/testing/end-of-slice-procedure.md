# repo-graph End-of-Slice Procedure

Status: **ACTIVE** (governance — relay/review agents MUST follow this).
Slice of record: END-OF-SLICE-PROCEDURE-1 (operator-directed).
Last proven: 2026-06-14 — the isolated dogfood §4 ran on HEAD `ccaad68` (run `20260614T153007Z-62881`; §4.2 quotes its capture files verbatim).

This is repo-graph's contract answer to the agent-manager **target end-of-slice
procedure** requirement: every target must DEFINE the three phases a relay runs
when a slice's code work is done — **Test → Install/deploy → Cleanup** — with
concrete, runnable commands. This document is that definition for repo-graph, plus
the load-bearing piece that was previously missing: a **proven, isolated way to run
the live `rmap` CLI** without an indexed repo and without touching the operator's
real daemon or registry.

> Why the dogfood matters: reviewers kept hitting `error: repo not indexed` because
> the agent sandbox has no indexed repo, and indexing into the operator's real
> registry (`~/Library/Application Support/repo-graph/`) is a pollution hazard.
> §3–§4 close that gap with a throwaway state root + stdio transport.

---

## 1. The three phases (overview)

| Phase | Purpose | Primary commands | When |
|-------|---------|------------------|------|
| **Test** | Prove the change against a FRESH build, in ISOLATION, synchronously | `cargo build/fmt/clippy/test` (in `rust/`); smoke scripts; the **isolated `rmap` dogfood** (§3–§4) | Always, before handoff |
| **Install / deploy** | Promote the build to the operator's running daemon | `./scripts/dev-install-local.sh` | **Only AFTER reviewer approval** |
| **Cleanup** | Reclaim disk; leave no scratch | `./scripts/clean-build.sh --all` | At slice end |

Evidence labels used below follow `agent_docs/validation.md` / CLAUDE.md Evidence
Law: **EXECUTED** (command run, output observed), **OBSERVED** (artifact inspected),
**INFERRED**, **NOT RUN**. Never present inferred output as observed.

---

## 2. Phase details

### Phase 1 — Test (against the FRESH build, synchronous)

Run from the workspace. All paths below are repo-relative. Build first so every
later step exercises the just-built code, not a stale binary.

> Run every test **synchronously, to completion**, in the same session. Do NOT
> background long suites and end the turn — a relay run is one-shot and is never
> re-invoked, so a backgrounded suite orphans and its result never reaches the
> artifact. If a suite genuinely cannot finish, report it `NOT RUN` with the reason.

**1a. Build (fresh).**
```bash
cd rust
cargo build --release --bin rmap --bin rmapd     # the two binaries the dogfood drives
# (or `cargo build` for a faster debug build when you only need the test suite)
```

**1b. Static checks.**
```bash
cd rust
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
```

**1c. Unit + integration tests.** Prefer the script wrappers (they pin manifest
paths and the `repo-graph-rgr` package); see `docs/testing/rmap-test-protocol.md`.
```bash
./scripts/test-rgr-crate.sh                       # rgr crate unit/integration
./scripts/test-rgr-integration.sh <test_name>...  # specific integration files
cd rust && cargo test --workspace                 # full workspace when the change is cross-crate
```

**1d. Smoke / validation-repo runs.** Governed by `docs/testing/rmap-test-protocol.md`
(canonical scratch root `/private/tmp/repo-graph-tests/`, mandatory `smoke-runs/`
provenance artifacts). Use the scripts — manual command lines do NOT count as slice
validation.
```bash
./scripts/smoke-rmap.sh <task> <repo-path> <command> [args...]   # one repo, one command
./scripts/smoke-validation-repos.sh <task> trust orient check    # internal + discovered legacy repos
```

**1e. Isolated live `rmap` dogfood (load-bearing).** Exercise the running CLI on an
indexed fixture WITHOUT touching the operator's daemon/registry. One command:
```bash
./scripts/dogfood-isolated.sh            # runs, asserts isolation, then cleans up
./scripts/dogfood-isolated.sh --keep     # retain the state root for inspection
```
Mechanism, guarantees, and the captured proof are in §3–§4. This is what makes
`rmap orient`/`explain`/`check` runnable for a relay agent that has no indexed repo.

### Phase 2 — Install / deploy (ONLY after reviewer approval)

Do NOT run this during the Test phase. It overwrites the operator's installed
binaries and restarts the resident launchd daemon (`com.repo-graph.rmapd`) — that
is promotion of reviewed code to the operator's real environment, which happens
only after the reviewer approves the diff.
```bash
./scripts/dev-install-local.sh
```
What it does (macOS only): builds release `rmap`/`rmapd`/`rgistr`, `launchctl
bootout` the daemon, installs binaries atomically to `~/.local/bin`, `launchctl
bootstrap` to restart, then validates versions + `rmap doctor` + socket
reachability. See the script header for the full sequence and the
`RMAP_CODESIGN_IDENTITY` note (stable signing to stop repeated macOS prompts).

### Phase 3 — Cleanup

Run at slice end. The debug target tree has reached **40+ GB**; left unchecked it
accumulates across slices.
```bash
./scripts/clean-build.sh             # debug artifacts only (default)
./scripts/clean-build.sh --all       # debug + release (full reclaim)
./scripts/clean-build.sh --dry-run   # preview what would be removed
```
`--all` removes `target/release` too, so the NEXT slice's Phase 1a is a cold build.
If you ran the dogfood with `--keep`, also `rm -rf` the printed state root.

---

## 3. The isolated `rmap` dogfood — mechanism

`rmap index`/`orient`/`explain`/`check` need (a) a daemon and (b) an indexed repo.
The harness supplies both in a throwaway sandbox using two independent env levers,
both load-bearing. [OBSERVED: source paths cited.]

| Lever | Value | Effect | Source of truth |
|-------|-------|--------|-----------------|
| `RMAP_TRANSPORT` | `stdio` | `rmap` spawns its OWN `rmapd --stdio` subprocess over stdin/stdout. **No Unix socket is opened**, so the operator's resident launchd daemon is never contacted. The child exits on EOF when the `rmap` call ends. | `rust/crates/rgr/src/daemon_client/{mod,stdio_transport}.rs` |
| `RMAP_STATE_ROOT` | `/private/tmp/repo-graph-dogfood/<run>` | The spawned daemon puts `registry.json` + `databases/` under this root instead of the operator's data dir. A root under `/private/tmp/` → **SandboxLocal** mode: A1 authority writes (alias/baseline/declaration) blocked; A2/B (index, refresh, queries) allowed. | `rust/crates/daemon-runtime/src/registry.rs` (`state_root_dir`), `…/state.rs` (`state_root_mode`) |

How they compose: `Command::spawn` for `rmapd --stdio` inherits the parent
environment, so both vars flow into the child automatically. State survives the
per-call subprocesses on disk — `handle_index` calls `registry.save()`
(`dispatch.rs`), so an `index` in one `rmap` call is visible to `orient` in the
next, even though each call spawns a fresh, short-lived daemon.

Two deliberate consequences:

- **No `--alias`.** Aliasing is an A1 authority write, blocked in SandboxLocal. The
  harness indexes by path and resolves the repo from **cwd** (REG-1): `orient`,
  `check`, `explain`, `repo list` take no path argument — they resolve the repo
  from the current directory, so the harness `cd`s into the fixture before calling
  them.
- **No SCIP / no `scip-typescript`.** `rmap index` uses the homegrown tree-sitter
  extractor (SQLite-served), and `orient`/`check`/`explain` serve from that SQLite
  base (the `sources: sqlite` line in §4). The SCIP/LiveGraph path is opt-in
  (`rmap dev livegraph-refresh`, `--engine livegraph`) and is NOT what these
  default commands use — so, unlike the `scip-typescript` scratch-provisioning in
  SCIP-UNRESOLVED-CALL-PROBE-1, this harness needs no producer. If a future slice
  dogfoods the LiveGraph path, provision `scip-typescript@0.4.0` in scratch exactly
  as that probe did (`docs/slices/scip-unresolved-call-probe-1.md` §2.1, §9) and
  point `RMAP_SCIP_TYPESCRIPT` at it.

---

## 4. The isolated dogfood — EXECUTED proof

**Harness:** `scripts/dogfood-isolated.sh`. **Binaries:** the current-branch release
build (`rust/target/release/{rmap,rmapd}`, HEAD `ccaad68`, version `rmap 0.2.1`). This
slice changes no Rust, so `cargo build --release --bin rmap --bin rmapd` is a clean no-op
(cargo prints `Finished ... in 0.27s`, nothing rebuilt), confirming the binaries are fresh
for `ccaad68` (they were first built from `ccaad68` earlier the same day). [EXECUTED.]

> Note on binary choice: this run used the **current-branch** binaries (built from
> `ccaad68`), which is stronger than the operator's installed 0.2.1. The selection
> packet allowed using the installed `rmap` and deferring the current-branch
> dogfood to follow-on #13; we proved current-branch here, so #13's remaining scope
> is only re-running this harness as the operator and (optionally) exercising the
> opt-in LiveGraph path.

### 4.1 Commands (exact; reviewer re-runs these)

```bash
cd rust && cargo build --release --bin rmap --bin rmapd     # fresh build
cd ..    && ./scripts/dogfood-isolated.sh --keep            # run + assert isolation
```
The harness sets, per call: `RMAP_TRANSPORT=stdio` and
`RMAP_STATE_ROOT=/private/tmp/repo-graph-dogfood/<run>`, then runs `rmap index
<fixture>` followed by (cwd = fixture) `rmap orient`, `rmap explain src/main.ts`,
`rmap check`, `rmap repo list`. The fixture is a 2-file TypeScript sample the script
writes into the state root (no `node_modules`, no network).

### 4.2 Captured output — EXECUTED (verbatim per-command capture files)

Every block below is the **byte-for-byte content of a harness capture file** from run
`20260614T153007Z-62881` (binaries `rmap 0.2.1`, HEAD `ccaad68`, 2026-06-14). The harness
writes each command's stdout to `out/<cmd>.txt` and its stderr to `out/<cmd>.stderr`; each
block names its source file so a reviewer can diff it against a re-run, e.g.
`diff <(cat "$STATE_ROOT/out/orient.txt") -` against the orient block.

> **Non-deterministic tokens — vary on every run; NOT part of the byte-match claim.**
> A faithful re-run differs ONLY in: the run id / state-root path (`…/<UTCstamp>-<pid>`),
> the repo ULID (`repo_01k…`), the snapshot short-hash and its ISO timestamp, and the
> `LAST INDEXED` time. The **stable semantic fields** — file/node/edge counts,
> `Confidence`, `Verdict`, call-resolution %, and the warning + sandbox notes — reproduce
> verbatim. The reviewer's re-run must diff on these stable fields, not on the IDs.

**`out/index.stderr`** — `index` writes nothing to stdout (`out/index.txt` is empty); the
`--stdio` warning, the sandbox-mode posture, and the index summary all go to stderr:
```text
warning: --stdio mode is for debug/test only, not production
note: running in sandbox-local mode (state root: /private/tmp/repo-graph-dogfood/20260614T153007Z-62881)
note: authority writes (baselines, aliases, declarations) are blocked
note: cache operations (index, refresh, queries) are allowed
indexed 2 files, 7 nodes, 6 edges (0 unresolved)
  repo: repo_01kv3c29rf62st85nrmm989n6x
  snapshot: repo_01kv3c29rf62st85nrmm989n6x/2026-06-14T15:30:07.814Z/13435066
```
Each query call (`orient`/`explain`/`check`/`repo list`) emits the SAME first four lines
(the `--stdio` warning + 3 sandbox notes) to its own `out/<cmd>.stderr`, without the index
summary — verbatim `out/orient.stderr`:
```text
warning: --stdio mode is for debug/test only, not production
note: running in sandbox-local mode (state root: /private/tmp/repo-graph-dogfood/20260614T153007Z-62881)
note: authority writes (baselines, aliases, declarations) are blocked
note: cache operations (index, refresh, queries) are allowed
```

**`out/orient.txt`** (cwd = fixture):
```text
Repo: fixture
Confidence: high

Signals
  Low
    - 4 files, 4 symbols indexed; 1 discovered module.
    - Snapshot …13435066 (full).

Limits
  - No active requirement declarations. Gate has no obligations to evaluate.

Certainty
  - class Exact, freshness Fresh
  - sources: sqlite
```
> `orient` reports **4 files / 4 symbols** while `index` and `check` report **2 files**.
> This is not a transcription error — it is the real, unedited output of each command:
> `orient` counts all four repo files (the two `.ts` sources plus `package.json` +
> `tsconfig.json`) and the four TS symbols (`square`, `clamp`, `computeScore`, `main`);
> `index`/`check` count the two `.ts` source files that produced graph nodes. Recorded
> here as observed; reconciling the two counting surfaces is a Rust concern, out of scope
> for this tooling/docs slice.

**`out/explain.txt`**:
```text
Repo: repo_01kv3c29rf62st85nrmm989n6x
Target: src/main.ts (file)
Language: typescript
Symbols: 2
Confidence: high

Imports (1)
  - src/util.ts

Symbols (2)
  - computeScore (FUNCTION)
  - main (FUNCTION)

Trust
  - Call resolution: 100%
  - Call graph reliability: high
  - Enrichment: not_applicable
```

**`out/check.txt`**:
```text
Repo: fixture
Verdict: PASS@Fresh

Passing conditions
  - SNAPSHOT_EXISTS: READY snapshot available.
  - INDEX_NOT_EMPTY: 2 files indexed.
  - STALE_FILES: No stale files.
  - CALL_GRAPH_RELIABILITY: Call graph reliability is HIGH.
  - ENRICHMENT_STATE: No eligible edges for enrichment.
  - GATE_STATUS: No gate policy configured.
```

**`out/repo-list-isolated.txt`** (isolated registry — the fixture, and nothing else):
```text
ALIAS                PATH                                               LAST INDEXED
------------------------------------------------------------------------------------------
-                    /private/tmp/repo-graph-dogfood/20260614T153007Z-62881/fixture 2026-06-14T15:30:07
```

### 4.3 Isolation verified — OBSERVED

The isolated state root holds everything; the operator's data dir is untouched.
```text
/private/tmp/repo-graph-dogfood/<run>/registry.json            # 1 repo: the fixture
/private/tmp/repo-graph-dogfood/<run>/databases/<hash>.db      # per-repo SQLite; <hash> derives from the run-specific path (this run: cd1076d01a2e0c2d.db)
/private/tmp/repo-graph-dogfood/<run>/fixture/...              # the 2-file TS sample
/private/tmp/repo-graph-dogfood/<run>/out/<cmd>.{txt,stderr}   # captured stdout + stderr, one pair per command
```
The harness asserts both halves and exits nonzero on breach [EXECUTED — both PASS]:
```text
Non-pollution check (operator state root is READ-ONLY here):
  PASS: ~/Library/Application Support/repo-graph/registry.json
        does NOT contain /private/tmp/repo-graph-dogfood/<run>/fixture
  PASS: isolated registry recorded the fixture:
        /private/tmp/repo-graph-dogfood/<run>/registry.json
```

### 4.4 Guarantees and honest scope

- **Operator daemon untouched.** stdio transport opens no socket; the launchd
  daemon is never contacted. [OBSERVED: transport code + the absence of any socket
  under the state root.]
- **Operator registry untouched.** The fixture is absent from the operator's
  `registry.json` and present only in the isolated one. [EXECUTED: §4.3 assertions.]
- **A1 writes are genuinely blocked**, by design, in this mode — so the harness
  cannot (and does not) create aliases/baselines/declarations. This is correct
  isolation, not a defect. A slice that must dogfood A1 governance writes needs a
  **non-`/private/tmp/`** isolated `RMAP_STATE_ROOT` (Global mode) — record that
  divergence explicitly when used.
- **Default (SQLite) serving only.** This proves the SQLite-served orient/explain/
  check path. It does NOT exercise the opt-in LiveGraph/SCIP path (`--engine
  livegraph`, `rmap dev livegraph-refresh`); that is follow-on #13 and needs
  `scip-typescript` provisioning per §3.
- **macOS.** The state-root/sandbox convention and the operator-path probe are
  macOS-specific; the env-lever mechanism is portable.

---

## 5. References

- `docs/testing/rmap-test-protocol.md` — smoke/DB rules, `smoke-runs/` provenance,
  validation-repo inventory.
- `scripts/dogfood-isolated.sh` — the isolated dogfood harness (this doc §3–§4).
- `scripts/dev-install-local.sh` — Phase 2 install/deploy.
- `scripts/clean-build.sh` — Phase 3 cleanup.
- `docs/slices/scip-unresolved-call-probe-1.md` — the `scip-typescript`
  scratch-provisioning precedent (only needed for the opt-in LiveGraph path).
- STATE-ROOT-SEPARATION-1 (ROADMAP) — the A1/A2/B sandbox-mode tier model the
  isolation relies on.
- `agent_docs/rmap-orientation.md` — `rmap` command patterns and the trust workflow.
