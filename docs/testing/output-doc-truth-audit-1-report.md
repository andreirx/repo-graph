# OUTPUT-DOC-TRUTH-AUDIT-1 — validation + truth-map report

Operator-directed pre-merge truth pass. Bar = the CLAUDE.md Fact-Certainty Model:
directionally correct (never steer an agent wrong), claim only what the extraction
backs, place each claim in its honest layer (0/1 extracted fact · 2 bounded
inference · 3 evidence-backed hint · 4 governance overlay), and never render a
Layer 2–4 inference as a Layer-0 fact.

This is the **tracked** deliverable for the DoD lines "the core commands audited
(table delivered)" and "docs verified vs `--help` … reported". It lives under
`docs/testing/` (the repo's committed validation-report home, cf.
`daemon-validation-report.md`) because the `docs/audits/` convention is gitignored
(`.gitignore:37`) and a reviewer who reviews `git diff` would not see it there.

All evidence below was **EXECUTED/OBSERVED first-hand** against a fresh build
(`rmap 0.2.1`) in an isolated dogfood state root (`RMAP_TRANSPORT=stdio`,
ephemeral `RMAP_STATE_ROOT` under `/private/tmp`); nothing is reconstructed.
Ground truth = the **handler's actual behavior** (live usage string / real output),
not the top-level `rmap --help` summary or `rmap <cmd> --help` (stale/absent for
several commands — Follow-up #1, #3). This is the iteration-2 ratified surface.

---

## 1. Scope of the change (working tree)

| File | Part | Change | Class |
|---|---|---|---|
| `agent_docs/rmap-orientation.md` | A | Every command example → cwd-resolved REG-1 shape; `declare *` kept positional with the help-vs-handler note; output-format corrected to human-default + `--json`. | doc-only |
| `docs/cli/rmap-contracts.md` | A | RECONCILE-6 residue resolved: `policy`/`boundaries`/`contracts` deep blocks → REG-1; `--help`-unreliable note retained. | doc-only |
| `rust/.../presentation/module_shared.rs` | B | `format_dead_compact`: `"N dead"` → `"N unref?"` (+ layer-rationale doc comment). | presentation-only |
| `rust/.../presentation/modules_list.rs` | B | Caveat footnote; test asserts relabel + caveat + absence of bare `dead`. | presentation-only |
| `rust/.../presentation/modules_show.rs` | B | Same count → "unreferenced symbols" + caveat; test updated. | presentation-only |
| `rust/.../tests/cli_out_4_modules.rs` | B | `#[ignore]` CLI assertion `dead` → `unref?`. | test-only |
| `CLAUDE.md` + `AGENTS.md` | C | Stale "40+ GB" → "~14 GB" (byte-identical mirrors). | doc-only |
| `docs/testing/output-doc-truth-audit-1-report.md` | — | This report (the tracked audit deliverable). | doc-only |

`README.md` — in scope, audited, **not edited** (verified honest; §4).

---

## 2. PART A — CLI doc contract: per-command handler citations (EXECUTED)

The REG-1 migration is **MIXED**, not blanket. The docs document the mix truthfully:
REG-1 (cwd-resolved, no positionals) for migrated commands; explicit
`<db_path> <repo_uid>` for the un-migrated ones. Documenting an un-migrated command
as REG-1 would tell an agent to run a command the handler rejects — the exact
"steer an agent wrong" failure this slice prevents.

### REG-1 — cwd-resolved, NO `<db_path> <repo_uid>` (live usage strings)

| Command | Live handler usage string | Doc form | Verdict |
|---|---|---|---|
| `orient` | `usage: rmap orient [--focus <path>] [--budget small\|medium\|large] [--full] [--json]` | `rmap orient --focus "src/core"` | ✅ match |
| `explain` | `usage: rmap explain <target> [--budget medium\|large] [--full] [--json]` | `rmap explain "…/session.ts"` | ✅ match |
| `check` | `usage: rmap check [--full] [--json]` | `rmap check` | ✅ match |
| `trust` | `usage: rmap trust [--json]` · "Repository is resolved from current working directory." | `rmap trust` | ✅ match |
| `callers` | `usage: rmap callers <symbol> [--edge-types <types>] [--json]` | `rmap callers "AuthService.validate"` | ✅ match |
| `callees` | `usage: rmap callees <symbol> [--edge-types <types>] [--json]` | `rmap callees "AuthService.validate"` | ✅ match |
| `imports` | `usage: rmap imports [<file>] [--engine …] [--json]` | `rmap imports "…/session.ts"` | ✅ match |
| `cycles` | `usage: rmap cycles [--engine …] [--kind …] [--json]` | (contracts doc) | ✅ match |
| `stats` | `usage: rmap stats [--engine …] [--json]` | (contracts doc) | ✅ match |
| `index` | `rmap index [repo_path] [--alias <name>] [--include-root <path>]…` | `rmap index .` | ✅ match |
| `refresh` | `rmap refresh [--include-root <path>]…` | `rmap refresh` | ✅ match |
| `assess` | `usage: rmap assess [--baseline <snapshot_uid>] [--json]` | `rmap assess [--baseline …]` | ✅ match |
| `gate` | `usage: rmap gate [--strict \| --advisory] [--json]` · "resolved from current working directory." | `rmap gate` | ✅ match |
| `policy` | `usage: rmap policy [options]` · "resolved from current working directory." | `rmap policy [--kind STATUS_MAPPING\|BEHAVIORAL_MARKER\|RETURN_FATE] …` | ✅ match |
| `boundaries` | `list [filters] / show <surface_uid> / summary` (no positionals) | same | ✅ match |
| `contracts` | `list [--kind protobuf] / show <file_path> / elements […] / usages […]` (no positionals) | same | ✅ match |

### POSITIONAL — still `<db_path> <repo_uid>` (un-migrated; docs PRESERVE these)

| Command | Live handler usage string | Doc form | Verdict |
|---|---|---|---|
| `declare quality-policy` | `usage: rmap declare quality-policy <db_path> <repo_uid> <policy_id> …` | `rmap declare quality-policy <db_path> <repo_uid> QP-001 …` | ✅ match (positional kept) |
| `declare boundary` | `usage: rmap declare boundary <db_path> <repo_uid> <module_path> --forbids …` | (contracts top block) | ✅ match (positional kept) |

Every corrected doc command maps exactly to the shipped binary. No command was
changed/removed → no STOP_CONDITION hit.

**Under the hood.** REG-1 = the long-lived daemon (`rmapd`) holds repo state; the
handler canonicalizes `cwd` and asks the daemon to resolve the repo from the
registry — no storage path crosses the CLI. Positional = the handler opens SQLite
directly via `crate::cli::open_storage(<db_path>)`, the pre-daemon contract not yet
migrated (`declare/*`, `enrich`, `modules boundary`).

---

## 3. PART B — Output-words audit (core agent-decision commands)

Honest layer per the Fact-Certainty Model; each verdict grounded in the presentation
source AND real output captured this run.

| Command | Notable output line (OBSERVED this run) | Honest layer | Verdict |
|---|---|---|---|
| **orient** | `Confidence: high`; `Certainty` block `class Exact, freshness Fresh` / `sources: sqlite`; conditional `Degradation` block | L2 (certainty/freshness/provenance over L0–1 facts) | **honest** — certainty footer carries class+freshness+source; degradation conditional. Untouched. |
| **trust** | `Resolution (sqlite, snapshot-scoped extraction, Fresh) — Calls 100% (3/3)`; `Reliability … Call-graph: HIGH`; `Current-State Posture (livegraph, current-state, Unavailable) — Resident: no` | L2/L3 | **honest (exemplary)** — every section carries a `(source, scope, freshness)` label, so snapshot diagnostics are never read as current-state. Untouched. |
| **check** | `Verdict: PASS@Fresh`; conditions `CALL_GRAPH_RELIABILITY: HIGH`, `GATE_STATUS`, … | L2 verdict / L0–1 conditions | **honest** — verdict carries MEET-freshness suffix; conditions are extracted facts. Untouched. |
| **explain** | per-section caps; `[Output truncated. Use --full …]` ONLY when `truncated && !full`; `… (N more)` only when items dropped | L0–1 + L2 | **honest** — two independent caps (data + presentation); no false truncation claim. Dogfood-proven. Untouched. |
| **modules list** | `… 3 files  21 unref?  0 violations  declared` + caveat footnote | rows L0–1; **`dead_symbol_count` = L2 low-reliability graph-orphan** | **overclaimed → FIXED**: bare `dead` → `unref?` + caveat → `rmap trust`. `--json dead_symbol_count: 21` unchanged. |
| **modules show** | `Symbols: 21 unreferenced symbols` + caveat | **L2 (same `dead_symbol_count`)** | **overclaimed → FIXED**: `dead symbols` → `unreferenced symbols` + caveat. Other rows L0–1, untouched. |
| **cycles** | `No module-level cycles found.` | L2 (cycles over the modeled import graph) | **honest** — scoped wording, never absolute "no cycles exist". Untouched. |
| **stats** | `Summary modules:1 total_files:3 total_symbols:24`; `By size` / `By fan-in` raw metrics | L0–1 counts; L2 metrics | **honest** — raw metrics, no threshold-based "at risk" labeling. Untouched. |
| modules deps (adjacent) | `No cross-module dependencies detected.` | L1 + L2 | **honest, minor flag** — "detected" states evidence-absence; the older "…exist" wording is a candidate, not a certainty error. Flagged. |
| **Peripheral** (churn, hotspots, risk, coverage, surfaces, docs, resource) | — | — | **NOT audited this slice** — FLAGGED for follow-up (per slice scope). |

**Overclaims found: 2** — `modules list` `dead` column + `modules show` "dead
symbols", both the SAME `dead_symbol_count` rendered as flat Layer-0 fact. **Both
FIXED** (presentation-only relabel + caveat). No new overclaim in the core-7. No
blocking judgment call.

### Why the dead-count fix is correct (tied to VISION)

`dead_symbol_count` is the daemon's **graph-orphan** estimate (no inbound reference
in the *modeled* call graph): a Layer-2, low-reliability inference. VISION: "graph
dead results … should be interpreted as 'graph orphans' … not 'safe to delete.'"
The public `rmap dead` surface was withdrawn for this reason, and `README.md:239`
already withdraws public dead-code claims. Bare "N dead" smuggled the withdrawn
overclaim back through `modules`.

**The live fixture refutes a reliability-only excuse:** this run `trust` reports
**Calls 100% resolved (3/3), Call-graph HIGH** — yet `modules list` reports
**21 unref?**. All 21 are *exported* functions (`square`, `clamp`, `computeScore`,
`main`, `manyFn00..19`) with no in-fixture caller — live library exports, not dead
code. `21 dead` would be an ~87% false-positive overclaim *even at HIGH
reliability*; `21 unref?` claims only what the extraction backs (no inbound
reference), and `?` flags the uncertainty. The label is the load-bearing honesty;
the caveat names a prominent cause. The fix is **presentation-only and proven so** —
`modules list --json` still emits `"dead_symbol_count": 21` (daemon contract
untouched). The count is *preserved* (useful at HIGH reliability and for test-dead).

---

## 4. PART C — README + CLAUDE/AGENTS final pass

- **README.md — audited; NO claim changes required** (OBSERVED first-hand):
  1. Layer 0–4 model (L19–31) confines deterministic claims to Layers 0–1 and labels
     inference explicitly (L25 Layer 2 = "interpretation with explicit basis … one
     step removed from raw extraction"; L27 Layer 3 = "evidence-backed hints with
     explicit unknowns").
  2. Capability claims carry honest maturity markers: L160 "(shipped)", L172
     "(partial)", L180 "(mixed maturity)", per-item "partial"/"incomplete"/"roadmap".
  3. L239 already withdraws public dead-code claims — the `modules` relabel now
     *aligns* output with this (before, `modules` rendered "N dead", contradicting it).
  4. The mixed CLI contract is already correct: L218 `declare quality-policy …
     requires <db_path> <repo_uid>`; L228 `assess` daemon-native REG-1 — both match
     the Part-A handler citations.
  No inference is framed as deterministic. Editing honest content is the churn the
  slice forbids.
- **CLAUDE.md / AGENTS.md** — stale "40+ GB" → "~14 GB" (the one drift). Byte-identical
  mirrors; both fixed in sync. Layer model / evidence law / decision autonomy untouched.

---

## 5. TEST REPORT (all EXECUTED this run; re-runnable verbatim)

From `rust/` against the fresh build.

| Suite / command | Exact command | Result | Evidence |
|---|---|---|---|
| Format | `cargo fmt --check` | clean, exit 0 | EXECUTED |
| Debug build | `cargo build` | `Finished dev profile` | EXECUTED |
| Lint | `cargo clippy --all-targets -- -D warnings` | clean (superset of packet `cargo clippy -- -D warnings`; lints the new test code too) | EXECUTED |
| Module presentation units | `cargo test -p repo-graph-rgr --lib -- presentation::module` | **56 passed, 0 failed** — incl. `format_dead_compact_value`, `list_render_relabels_dead_as_unref_with_caveat`, `show_render_shows_unreferenced_symbols_with_caveat` (all `… ok`) | EXECUTED |
| Full workspace suite | `cargo test` | **4591 passed, 0 failed, 247 ignored** (only `error`-matching lines are passing `error::tests::error_display_* … ok`) | EXECUTED |
| Release binaries | `cargo build --release --bin rmap --bin rmapd` | `Finished release profile` | EXECUTED |
| Isolated dogfood | `./scripts/dogfood-isolated.sh --keep` | exit 0; all `--full` cap proofs PASS; non-pollution PASS (operator registry untouched) | EXECUTED |
| Live `modules list` | isolated state root, cwd=fixture | `21 unref?` + caveat; `--json dead_symbol_count: 21` | EXECUTED/OBSERVED |
| Live `modules show` | isolated state root, cwd=fixture | `21 unreferenced symbols` + caveat | EXECUTED/OBSERVED |
| Live `trust`/`cycles`/`stats`/`orient`/`check` | isolated index | per §3 (all honest) | EXECUTED/OBSERVED |
| Handler citations | `rmap {orient/explain/check/trust/cycles/stats/imports/assess/gate/policy} usage`, `rmap {callers,callees,declare quality-policy,declare boundary}` bare usage, top-level `rmap --help` | all match the doc edits (§2) | EXECUTED/OBSERVED |

### Live captures (OBSERVED this run)

```
$ rmap modules list            (isolated; cwd=fixture)
  rmap-dogfood-fixture               3 files   21 unref?    0 violations  declared

note: unref? = symbols with no inbound reference in the indexed graph (syntactic
estimate); over-counts under low call-graph resolution; run `rmap trust` for reliability.

$ rmap modules list --json | grep dead_symbol_count
  "dead_symbol_count": 21          # daemon field UNCHANGED (presentation-only fix)

$ rmap modules show <uid>          (Symbols section)
  21 unreferenced symbols
  note: unreferenced = no inbound reference in the indexed graph (syntactic estimate);
  over-counts under low call-graph resolution; run `rmap trust` for reliability.
```

No bare `dead` appears on either human surface.

### Coverage gaps / NOT RUN (with reason)

- `cli_out_4_modules.rs::modules_list_human_mode_shows_catalog` is `#[ignore]`
  (needs a live daemon via `--ignored`). **NOT RUN** — `--ignored` would drive the
  operator's real daemon/registry, breaking isolation. Its assertion was corrected
  (`dead`→`unref?`); the same behavior is proven by the lib unit tests + the live
  isolated capture.
- 247 workspace tests `#[ignore]` (daemon-dependent) — excluded by default
  `cargo test`, same isolation reason.
- Peripheral commands not output-audited — FLAGGED (per slice scope), not skipped.

---

## 6. Follow-up flags (out of scope here)

1. **Top-level `rmap --help` is STALE for `declare` (CODE bug, CONFIRMED).** It lists
   `declare` under "Declarations (resolve repo from cwd):" with cwd-style forms (no
   positionals), but the handlers require `<db_path> <repo_uid>` (verified:
   `rmap declare boundary` → `usage: … <db_path> <repo_uid> <module_path> …`). Fix =
   migrate `declare` to REG-1 OR correct the top-level help string
   (`commands/declare/*.rs` + top-level help). Docs document the handler truth + note
   the mismatch.
2. **Incomplete REG-1 migration.** `declare *`, `enrich`, `modules boundary` still
   `open_storage(<db_path>)` while the rest of the CLI is daemon-native.
3. **`rmap <cmd> --help` is an unreliable verification surface (CODE/doc-tooling).**
   Symbol-taking commands (`callers`/`callees`) treat `--help` as the symbol and print
   nothing; several print usage only via the unknown-flag path. The handler usage
   string is the reliable surface (used throughout this audit).
4. **Peripheral output-words audit** (churn/hotspots/risk/coverage/surfaces/docs/
   resource + deep policy/boundaries/contracts output) — not audited this slice.
5. **Minor `modules` wording:** `modules deps` "…exist" phrasing (vs honest
   "detected"); `modules list` cross-module count uses a rough `max(out,in)/2` dedup.

---

## 7. Decisions recorded

1. **Audit report delivered as a tracked `docs/testing/` file** (not only the
   gitignored `.agent-manager/build-N.md` or `docs/audits/`). The DoD requires the
   table delivered; the reviewer reviews `git diff`; the report channel is gitignored
   and failed to reach the reviewer across iterations 0–3. `docs/testing/` is the
   repo's committed validation-report home (`daemon-validation-report.md` precedent),
   not a "slice doc" or "audit" (both special-cased), and not in FILES_OUT_OF_SCOPE.
   Low blast radius (one markdown doc) → decide and record.
2. **Handler truth over the packet's original blanket-REG-1 enumeration** (ratified
   iteration-2). `policy`/`boundaries`/`contracts`/`assess`/`gate` are REG-1; only
   `declare`/`enrich`/`modules boundary` are positional.
3. **Honest label `unref?` (list) / `unreferenced` (show), each caveated** — states
   exactly what the extraction backs (no inbound reference in the modeled graph), no
   "safe to delete" connotation. Rejected `dead?` (keeps centering the overclaiming
   word).
4. **`modules show` fixed alongside `modules list`** — same `dead_symbol_count` class
   on a live core command; leaving it would produce contradictory certainty for the
   same number.
