# Per-command usefulness audit — rmap v0.21.0 (audit round nine), 2026-10-05

Artifact (house format): see the ROADMAP link. Root causes: `docs/audits/2026-10-05-root-causes-v0.21.0.md` (RC-1 seam-read and reproduced live; RC-3/RC-5 operator-verified; the rest grader-observed with the cited mechanism). Grader reports, the adjudication prompt and verdict, and the 155 captures are kept locally under `~/repo-graph-retained/audit-v0.21.0/reports/` (not committed; the retained root is the witness).

## Corpus and method
- Release: v0.21.0 (`751aeda9`, tag pushed; installed locally with `scripts/dev-install-local.sh`; daemon restarted under launchd). Nine slices shipped since v0.20.0: IMPORTS-UNRESOLVED-REMAINDER-1, TS-ALIAS-RESOLUTION-1, DEPS-GRADLE-CATALOG-1B, MODULES-DEPS-SUMMARY-SCOPE-1, DGC-ATTRIBUTION-PRECISE-1, RUST-SELF-RESOLUTION-1, HELP-SURFACE-PARITY-1, DOCTOR-FALLBACK-STATE-ROOT-1, JAVA-SYMBOL-AMBIGUITY-HINT-1.
- Smoke: `SMOKE_SKIP=linux ./scripts/smoke-validation-repos.sh audit-v0.21.0 --retain …26 commands` → run `smoke-runs/2026-10-05T08-25-30Z`. The harness ran `cargo run --release` from HEAD `751aeda9` (the release commit, the same source as the installed binary). Operator launch error: `--retain` was placed after the task name, so it ran as a bogus command on every repo and the harness marked all 29 repos failed. Those `<repo>---retain.txt` captures are ignored. Real outcome: 25 repositories indexed in the batch and ran all 26 commands. One command failed: repo-graph `assess`, a transient `Busy` while an enrich pass was writing; its retest exited 0. Four repositories hit the client's 300 s read timeout while the daemon carried several detached indexes (gstreamer, hadoop, kafka, vscode; RC-6). gstreamer, hadoop and kafka completed detached (hadoop after about 1 h 42 m). vscode was cut by the smoke's daemon shutdown and re-indexed alone over stdio on the retained root: 992 s, exit 0, 11,878 files. Their 26 commands were captured afterwards as `retest-*`. linux was skipped (env). Retained root relocated to `~/repo-graph-retained/audit-v0.21.0` (29 `db_path` rewritten).
- Supplementals: `agent-manager/scripts/audit21-supplemental.sh` (35 probes; prefixes iur/tsa/dgc/mds/dap/rsr/hsp/dfsr/jsah/readme) re-checking every shipped slice's recorded AFTER literal; `validate-captures.py` confirms every capture's command equals the script's probe. Retests and most supplementals ran over an isolated rmapd on a COPY of the root, because stdio costs about 3 min per command on the large repos. That copy sat at a different directory depth, and that exposed RC-1. The affected vscode commands were re-captured over stdio on the retained root. Operator relocation proofs: `relocated-root-vscode-*`, `relocated-depth-leveldb-*`.
- Gate: matrix grader (rubric HIT/EVIDENCE/HONESTY/ECONOMY, ground-truthed against the checkouts) + judge (no rubric, per-capture verdicts) + Codex **gpt-6.1-sol** standalone adjudication (read-only sandbox, both reports inlined; human 2026-10-05 reviewer assignment). Operator verification: RC-1 live, RC-2 (kafka `ZstdCompression.java:27-30`), RC-3 (25/25 trust captures), RC-5 (20 direct includers of `slice.h`).

## Dimension movement (v0.20.0 → v0.21.0)
Adjudicated (published): **HIT B+ → A-** · EVIDENCE A- → A- (flat) · HONESTY B+ → B+ (flat) · ECONOMY B- → B- (flat). The matrix grader proposed HONESTY A-. The adjudicator held B+ on four counts: false absence on an unreachable root (RC-1), false Java zeros (RC-2), a tautological resolution line (RC-3), and an unsupported registry/factory diagnosis (RC-4).

## Grade matrix (HIT/EVIDENCE/HONESTY/ECONOMY)
| Command | Rust/TS-int | Java | C/C++ | Python | TS-legacy |
|---|---|---|---|---|---|
| orient small/med/large | A-/A-/A/A | A-/A-/A/B+ | B+/A-/A/A- | B+/A-/A-/A- | B+/A-/A-/B+ |
| orient --full | C+/B+/B-/D+ | C+/B+/B-/D+ | C+/B+/B-/D | C+/B+/B-/D+ | C+/B+/B-/D+ |
| check --full | B/A-/A-/A | B/A-/B+/A | B/A-/A-/A | B/A-/A-/A | B/A-/B+/A |
| trust | **A**/A/A-/B+ | **A**/A/A/B+ | B+/A/A-/B+ | A-/A/A-/B+ | A-/A/A-/B+ |
| modules list | A-/B+/A-/B | **A**/B+/A/B- | B+/A-/A/B | C+/B+/A-/A- | C+/B+/B+/B |
| stats | B/B-/B+/C+ | B-/B-/B+/C | B/B-/A-/C+ | B/B-/B+/C+ | B/B-/B+/C |
| cycles | **B+**/A-/**B+**/A | A-/A-/A/A- | A-/A-/A/A | A/A-/A-/A | A/A-/A-/B |
| churn / hotspots | B/B+/A-/A | D/B+/A-/A | D/B+/A-/A | D/B+/A-/A | C/B+/A-/A |
| risk | F/A/**A**/A | F/A/**A**/A | F/A/**A**/A | F/A/**A**/A | F/A/**A**/A |
| dead (refuses) | —/A/A+/A | —/A/A+/A | —/A/A+/A | —/A/A+/A | —/A/A+/A |
| surfaces list | A/A/A-/B- | A/A/A-/A- | F/—/A/— | F/—/A/— | A-/A/A/A- |
| boundaries list/summary | C+/A-/A-/B | C+/A-/A-/B | C+/A-/A-/B | —/—/A/— | C+/A-/A-/B |
| resource list | D/A/A/A | D/A/A/A | D/A/B+/A | D/A/A/A | D/A/A/A |
| docs list | B+/B+/B+/A | A-/A-/A-/A | B+/B+/A-/A | B+/A-/A-/A | B+/B+/B+/A |
| inferences list | B+/A-/A-/A | B/A-/A-/A | —/—/A/— | —/—/A/— | A/A-/A-/A |
| deps list | A-/A-/A-/A | **B+**/A-/**B+**/**B+** | B-/A-/A/A | B+/A-/A-/A | A-/A-/A-/A- |
| map --dry-run | C+/A/A/C | C+/A/A/C | C+/A/A/C | C+/A/A/C | C+/A/A/C |
| doctor | **B+**/A-/**A**/C+ | **B+**/A-/**A**/C+ | **B+**/A-/**A**/C+ | **B+**/A-/**A**/C+ | **B+**/A-/**A**/C+ |
| gate / assess / violations | F/—/**A-**/A | F/—/**A-**/A | F/—/**A-**/A | F/—/**A-**/A | F/—/**A-**/A |
| find exact-symbol | n/p [A-/B+/A-/A-] | n/p [B+/A-/A-/A-] | n/p [B+/A-/A-/A-] | n/p [B/B+/A-/A-] | n/p [A-/A-/A-/A-] |
| find concept-seeds | n/p [B-/B+/B+/A-] | n/p | n/p [B/B+/B+/A-] | n/p [B/B+/B+/A-] | n/p |
| find --text | not probed | — | — | — | — |
| explain / callers / callees | **B+**/B+/**A-**/B | **B+/A-/A-**/B | B/A-/A-/B | n/p [B+/A-/A-/B] | n/p [B/B+/B+/B] |
| *imports <file> (new row, supplementals)* | A-/A-/A-/B | n/p | A/A/A/B+ | n/p | n/p |

Bold = moved since v0.20.0; `n/p [x]` = not probed, last round's grade carried.

## Shipped slices — in the field (code under analysis)
All nine deliver their recorded literal; none regressed or dormant (adjudicator: CONFIRM ×8, REFINE ×1).
- IMPORTS-UNRESOLVED-REMAINDER-1 — nginx `imports src/core/ngx_core.h`: 32 static + `+2 inferred` + `13 imports without a confirmed target:` = the file's 47 includes; `ngx_time.h  line 52  several indexed files matched (same file name in 2 places) — 2 candidates: src/os/unix/ngx_time.h, src/os/win32/ngx_time.h` — never a pick. amodx `Toolbar.tsx` 4 bound + 4 unconfirmed.
- TS-ALIAS-RESOLUTION-1 — amodx `admin/tsconfig.json:9-10 "@/*": ["./src/*"]` → four Toolbar rows `static (resolved through tsconfig paths)`, JSON `basis: tsconfig_paths`; hexmanos follows `tsconfig.app.json:11-12`; amodx/hexmanos no longer `alias resolution suspected` in trust.
- DEPS-GRADLE-CATALOG-1B (REFINE: literal FIXED, absorbed RC-8 PARTIAL) — kafka clients `used 26`, grpc core `used 4` hold; but `com.github.luben.zstd (0 import sites, 2 call sites)` against four imports at `ZstdCompression.java:27-30` (RC-2).
- MODULES-DEPS-SUMMARY-SCOPE-1 — leveldb `Summary (module table, all directions):` 46 = 5+30+1+10; `--outbound` 41; JSON `diagnostics_scope: "module"`; poco Foundation 1653. (The ROADMAP's `--direction outbound` is a record defect, RC-8.)
- DGC-ATTRIBUTION-PRECISE-1 — grpc-java root `declared_manifest_paths: []`; `examples/example-tls` on its own row citing its own `build.gradle`; `modules list` `67 modules`, `66 Gradle projects from 21 settings.gradle files` (21 = `find -name 'settings.gradle*'`).
- RUST-SELF-RESOLUTION-1 — `imports rust/crates/agent/src/aggregators/trust.rs` binds 10 items; `std` imports stay unbound. Downstream: directory cycles and a trust downgrade (RC-4).
- HELP-SURFACE-PARITY-1 — 44 top-level heads match the dispatch arms; flags and arming cursors remain (HELP-SURFACE-PARITY-2).
- DOCTOR-FALLBACK-STATE-ROOT-1 — forced stdio: `[note] daemon_service: launchd service (global state root): running … this service is not used by a stdio client`; healthy exit 0 (macOS).
- JAVA-SYMBOL-AMBIGUITY-HINT-1 — kafka `callers KafkaProducer.send` exit 2, `Candidates (2)` at `:942`/`:1061` with two `rmap callers '<key>'` cursors; leveldb `explain Slice` constructor cursors. The JSON form still prints human text on stderr (ERROR-JSON-1).

## Where the scope-misattribution class went
Dead on every targeted surface; it survives in dependency attribution (same-module and JDK packages `undeclared`, an in-repo workspace subpath `external library?`). Two new classes, both against the VISION's central distinction:
1. **Unreachable root rendered as absence** (RC-1): on a moved or copied state root, orient and `modules list` say `No README or architecture doc found` beside a real `README.md`, and churn/hotspots/risk blame git.
2. **Count–predicate mismatch** (RC-2/3/4/5, N4): `0 import sites` beside real imports; `Edges: 100% resolved` (resolved ÷ resolved); "module-level cycles" that are directories inside one crate; `Referenced by (20 files)` = direct includers; Rust items vs files.
Adjudicator: "a measured absence is not an unavailable measurement, and a count is not honest until its predicate is named."

## README grade — C (held)
No command shown in the README is missing from the binary. CONFIRMED defects (file:line in `README.md`): `:80` describes `repo remove` as registry-only while `--help` says it deletes the database and `.rgr/` too (destructive understatement); `:110-111` calls `rmap dev` a hidden diagnostic surface while `--help` lists it; stale status lines `:182` (Rust MODULE nodes only — Gradle modules are declared), `:184` (resources not populated — hadoop/gstreamer have resource rows), `:200` (enrichment not ported); `:295-301` absolute author-machine paths; Java "Operational" omits the Maven limitation (`:146-152`); `--engine`, `gate --strict/--advisory` absent from `--help`. CHALLENGED as unproved: `:185` contracts not populated, `:248` LiveGraph serving and `scip-clang` (no capture exercised them; absence of evidence is not a broken promise). No `CHANGELOG.md` (release-document gap).

## Fix queue — cut along root causes (adjudicator's order; the human orders)
1. **STATE-ROOT-RELATIVE-REPO-ROOT-1** (RC-1) — one source for the repo root (the registry's absolute path); an unreachable root is named on every surface, never rendered as an absence or a git failure.
2. **DEPS-JAVA-IMPORT-SITES-1** (RC-2) with DEPS-JAVA-SELF-1 and DEPS-JDK-BUILTINS-1.
3. **TRUST-EDGES-TAUTOLOGY-1** (RC-3) with the Rust registry/factory false positive and CYCLES-RUST-POPULATION-1 (RC-4).
4. ERROR-JSON-1. 5. FOREGROUND-PATIENCE-FLAKE-1 (the batch `assess` Busy). 6. EXPLAIN-CALLSITE-ANCHOR-1 + EXPLAIN-REFERENCED-BY-PREDICATE-1 (RC-5). 7. HELP-SURFACE-PARITY-2 (arming cursors). 8. TS-WORKSPACE-SUBPATH-EXPORTS-1 + the Maven warning scope. 9. RUST-IMPORT-UNIT-1, MODULES-DEPS-WORDING-1, the static JSON `line` contract. 10. TSCONFIG-JSONC-PARSER-PARITY-1. 11. DOCS-UNREADABLE-PATH-1, TRUST-CATEGORIES-NAME-1. 12. ALIAS-SUSPICION-1. 13. CYCLE-WALK-DETERMINISM-1, STATS-TEST-TOTAL-1.
Evidence gate (no field manifestation this round): LOCK-TEST-FLAKE-1, TS-TEST-RACE-1, TESB-GATE-UNMEASURED-1, PARTITION-DECODE-HARDENING-1, DFSR-LINUX-1. Operator note: LOCK-TEST-FLAKE-1 has no product-visible effect, but it failed the v0.21.0 release cut twice (local validation and CI attempt 1). Its placement is a process-cost question for the human. Harness fixes (RC-7) and the README corrections go alongside the relevant work.

## Keep and imitate
`13 imports without a confirmed target:` with candidates, never a pick · `static (resolved through tsconfig paths)` with the basis on the row · `listing limit: this listing omits type-only imports and re-exports … — open the file for those` · `Summary (module table, all directions):` with the universe in the header · `Candidates (2) — each matches 'KafkaProducer.send'; pick one by its stable key:` + runnable cursors · the long-op `STILL RUNNING` note with exit 3 · `Modules: 66 Gradle projects from 21 settings.gradle files · 1 inferred …` · `declared set may be incomplete: 1 dependency block(s) not statically attributed (build.gradle:518) — investigate`.

## Verdict
The queue did what it was cut for: all nine slices deliver their literal in the field, nothing regressed, and HIT moved a step (alias binding, scoped summaries, Gradle ownership, runnable ambiguity cursors). HONESTY did not move. This round's defects show that a number is honest only when its predicate is named, and that an unavailable measurement must never print as an absence. The highest-impact item is not a precision defect: a state root moved to another directory depth makes orient state a false fact. That has been the case since the daemon took over storage in May; every earlier audit happened to relocate between folders at the same depth.

## Could not ground-truth
A daemon concurrency cap on indexes (text search is not proof of absence); whether a solo socket phase stays silent for 300 s; LiveGraph-served answers (every capture fell back to SQLite); `contracts` population; Linux behaviour (DFSR-LINUX-1); `find` (not probed this round).
