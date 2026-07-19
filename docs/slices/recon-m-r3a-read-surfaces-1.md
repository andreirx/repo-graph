# RECON-M-R3A-READ-SURFACES-1 — divergence posture + union accounting read surfaces (reconciliation IMPL milestone M-R3a)

Status: IMPLEMENTED, REVISION 3 (2026-07-19; working tree, uncommitted — reviewer gate pending;
implementation record §6; post-fuse-kill resume record §7; iteration-1 revision record §8;
iteration-2 revision record §9 — its five closures stand, its §9.2 validation was cut short by
a second fuse-kill; **iteration-3 revision record §10 — review-2's renderer labeling gate
closed + the FULL binding validation re-executed against the final tree; §10 supersedes ALL
prior binary-level evidence (§6.4/§7.2/§8.3/§8.6/§9.2)**) · Track: Reconciliation IMPL
(recon-design-1 §6.1, ratified §8)
Depends: M-R1 (c0e1dad), M-R2 (c202279). M-R2 ∥ M-R3a — both consume the M-R1 ledger.

## 1. Contract — the recon-design-1 §6.1 **M-R3a row IS the binding contract**, verbatim

Divergence posture + the union accounting's read surfaces: the trust `witnesses` block, the
doctor operational block, orient/stats g1u lines, g2u liveness/degree overlays, g3u sketch
pairs (§5.3.2-4, §5.4) — all through ONE shared projection. This INCLUDES the
escalate-deferred `identity_collision` rendering (trust block + doctor; the recorded M-R1
gate amendment bad69da moved it here). The union accounting NEVER touches the M-3a/M-3b
persisted pipeline accounting or its write path (§5.3 — no new coupling); the trust ratio's
denominator remains the pipeline-only floor.

## 2. Gate — the M-R3a row's gate column, verbatim

§5.3.1 invariance + accounting-label tests; zero-SCIP absence (R-0: the blocks/overlays
absent or explicitly n/a, never zeros) + mixed-repo scoping (R-1) tests; W-ONE
REASON-RENDERING tests (three reasons → three distinct posture lines + next actions; stale
≠ "available but not loaded"; the stale∧producer-absent compound renders its blocker);
doctor's ledger-ABSENT rendering (last capture outcome + build-failure reason; trust
renders unknown, never a stale number); deterministic ordering; RECORDS the measured g3u
pair delta (§5.3.4); smoke.

## 3. Stop conditions

Frozen: W-B epoch/coordinator invariants, activity-registry, enrich_pass, postpass/
extractor walks, the M-3a/M-3b persisted families + their write paths, the trust
denominator, union serving's row semantics (M-R2 — consume, don't change). Honesty rules
absolute: unknown renders as unknown, never zero, never stale numbers. Any
baseline/invariance mismatch is a FINDING. Do NOT commit. Consolidation witness green;
manifest edits explicit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

The §2 gate column in full; flag semantics consistent with M-R2 (surfaces appear per the
ratified visibility rules — the union accounting blocks are read surfaces of ledger state,
their rendering follows the design's §5.3/§5.4 visibility, recorded explicitly in the
report); chunked cargo gates; witness 15/15; canonical smoke per
docs/testing/end-of-slice-procedure.md with provenance; isolated dogfood.

## 5. Definition of done

trust/doctor/orient/stats render the witness-ledger accounting through one shared
projection with honest unknowns and deterministic ordering; collision rendering lands
(closing the M-R1 amendment); R-0 repos show no phantom zeros; pipeline accounting
untouched; all gates green.

## 6. Implementation record (M-R3a build, 2026-07-18)

### 6.1 Shape (least-new-surface, recorded per the packet)

- **The shared projection**: NEW `daemon-runtime/src/witness_projection/{mod,tests}.rs` —
  ONE computation feeding every witness read surface (trust `witnesses` block, doctor
  operational block, orient/stats g1u, modules g2u-a, explain g2u-b, map g3u). PEEK-only:
  it renders `RepoState.witness_ledger` / `witness_ledger_build_failure` + LiveGraph
  partition state + producer discovery; it NEVER builds the ledger (D-R8-A: absent →
  unknown). NEVER-STALE rule enforced at one place: ledger figures render ONLY when the
  stored fingerprint equals the CURRENT resident fingerprint (computed under the same
  read guard as the inventory); any witness movement ⇒ `measured: null` + a reason.
  Placement beside `union_serve` (consumes `callgraph_cert::ledger` types downward, same
  layering); the M-R1 regime classifier `classify_state`/`WOneReason`/`StateClass` gains
  its named production consumer (the `cfg(test)` allows came off).
- **Visibility (the packet's "flag semantics" record)**: the read surfaces are
  DATA-DRIVEN on ledger/partition-state evidence, NOT gated on `RMAP_RECON_UNION` — per
  the packet's own definition ("the union accounting blocks are read surfaces of ledger
  state, their rendering follows the design's §5.3/§5.4 visibility"). The M-R2 flag keeps
  gating what M-R2 gates (union serving + capture flip); the blocks truthfully describe
  ledger state under either flag value (the promotion-funnel precedent: measurement
  visible without a serving flip). Practical blast radius today: a ledger exists only on
  LG-fed repos (dev flows — the producer is dev-pinned), so every current default
  install renders byte-identically; the §5.3.1 invariance test pins exactly this.
- **Regime evidence scope (recorded)**: W-BOTH/W-ONE rows render for partitions the
  LiveGraph KNOWS (resident or summary-retained slots). A repo the daemon never
  SCIP-touched has no partition-level fact to state — unknown stays unstated (no
  phantom W-ONE rows on plain TS repos; §4.2's coverage-is-evidence rule at its
  read-time floor). W-NONE renders NO discovery-output line (R-0); producer capability
  truth lands on doctor (D-R1 carve-out). The three W-ONE reasons render three DISTINCT
  posture lines + next actions from the §4.2 ladder verbatim; the stale∧producer-absent
  compound names its blocker on the next action (one reason, never a fourth state).
  Producer presence = read-time discovery (`discover_scip_typescript`); the slot-level
  warm-cache `ProducerUnavailable` flag is NOT re-rendered (current provisioning governs
  the next action; recorded).
- **Trust**: `CoherentTrustReport.witnesses: Option<Value>` (skip-if-None), attached
  post-fold in `trust_coherence.rs` — deliberately OUTSIDE the coherence MEET (an absent
  second witness is coverage truth, not v1-report degradation; folding an Unavailable
  witnesses leaf would downgrade every zero-SCIP repo's root posture). Ratio inputs
  untouched (`service.rs`/`rules.rs` untouched; §5.3.1 invariance test in
  `trust_coherence` pins byte-identity EXCEPT the block). Human render: the shared
  `rgr presentation/witnesses.rs` section (§5.4 reader-frame lines).
- **Doctor**: the operational block attaches in `handle_storage_health` ONLY (idle path,
  ready-snapshot-gated; `repo_info` deliberately untouched — least-new-surface). Client:
  `witness_probe_from_facts` (storage_probe.rs) → the `witness_ledger` probe, registered
  in the Storage section filter (human label "call-graph witnesses"); always `passed`
  (a self-healing measurement state is not installation ill-health; the FACT renders).
  Ledger-absent → last build outcome + failure reason; superseded → named; measured →
  adoption per partition, colliding keys + reader line, occurrence-delta enumeration
  (capped WITH stated counts — no silent caps), regime next-action lines.
- **Orient/stats g1u**: `CoherentOrientResult.witnesses: Option<Value>` set in
  `orient_coherence` (check constructs it `None` — §5.3.1 check byte-invariance);
  stats via `inject_stats_summary_fields` (ONE injector, all four engine paths — the
  stats cert's byte-freeze covers only the `stats` rows). One additive human line via
  the shared `witnesses::g1u_line` renderer on both surfaces.
- **Modules g2u-a**: additive `unref_reduction` block (`{fewer_flagged, accounting,
  coverage, basis}`) beside `dead_symbol_count` on modules_show rollups + per
  modules_list row — REDUCTION-ONLY (flagged ∩ the ledger's `s_incoming_witnessed`),
  absent when unmeasured or zero. The ledger gained ONE additive field
  (`s_incoming_witnessed`: dst of non-withheld S Calls ∪ non-colliding S References dst,
  self-edges excluded — conservative; collision keys excluded per guard 2). Human:
  one line under modules_show Symbols; one aggregate footnote on modules_list.
  g2u-b on modules_show: N/A — modules_show has no per-symbol fan-in/out figure to
  attach to (module-level counts are IMPORT-graph figures; a call overlay there would
  be a category error; §5.3.3b's "may" + nothing-to-attach → recorded skip).
- **Explain g2u-b**: additive `union` object on EXPLAIN_CALLERS/EXPLAIN_CALLEES
  `evidence` (count + pipeline_count + accounting/coverage), attached post-serialization
  in `handle_explain` via the projection (`attach_explain_union_degrees`) — ONLY on a
  SYMBOL focus AND where the union degree differs (§5.3.3b "where it differs"); the
  pipeline `count` never changes. Human: heading suffix "· reconciled N —
  syntax+compiler analyses".
- **Map g3u**: `witnesses` block on the map response (`dependency_call_pairs_added` from
  `semantic`/`new_pair` ledger pairs, path-scoped, minus existing pipeline file pairs;
  `pair_delta` recorded in-band) + client sketch merge — witness-only targets labeled
  inline "(compiler-witnessed call — reconciled, syntax+compiler)"; the count line
  includes them. Union ⊇ pipeline: no pair is ever removed.
- **EC-M1 reader-set amendment (explicit; the slice's architecture item)**:
  `SANCTIONED_SURFACES` + `witness/livegraph_reader_set.txt` gain FOUR surfaces —
  `modules_show`, `modules_list`, `map`, `storage_health` — all reading through the ONE
  projection file (`witness_projection/mod.rs = trust, orient, stats, explain,
  modules_show, modules_list, map, storage_health`; tests.rs in [test-scaffolding]).
  Basis: the ratified M-R3a row mandates these read surfaces, and rendering a ledger
  FIGURE honestly requires the resident-fingerprint currency check (a `.livegraph`
  read; "never a stale number") — no honest alternative exists (stored-fingerprint
  labeling was rejected: a superseded ledger's numbers would serve as current). Flagged
  as DECISION_REQUIRED for the reviewer in the relay report; witness 15/15 green with
  the amendment. `dispatch_fact_classes.txt` unchanged (the ledger is not a persisted
  fact class).

### 6.2 The measured g3u pair delta (the §5.3.4 RECORD; **amended by §8.4 — now MEASURED**)

- **amodx (250 files, 8 partitions): the measured delta is `pair_delta: 11`** [EXECUTED
  2026-07-19, iteration 1 — the §8.4 end-to-end measurement through the implemented map
  semantics: fresh isolated pipeline index of the UNCHANGED source (HEAD `4230541`
  predates the 2026-07-17 capture; tree clean) + the 8 RETAINED capture-time SCIP
  partitions preloaded + ledger warmed + the daemon map response's in-band record].
  The 11 added dependency pairs: 6× `→ admin/src/lib/utils.ts` (the `cn(...)` class) +
  5× `→ renderer/src/lib/api-client.ts` (the route class), enumerated in the response.
  *History of the estimate:* the killed run's artifact-only derivation gave **12** cross-file
  candidate pairs as an UPPER BOUND (pipeline IMPORTS pairs were not in the retained set,
  so import coverage could not be subtracted); the full-sketch subtraction (IMPORTS +
  CALLS, the shipped semantics) absorbs exactly ONE (the backend pair) → **11**. §5.3.4's
  prediction that import coverage absorbs MOST of these did NOT hold at this scale — the
  `cn`/route pairs ride tsconfig-alias imports the pipeline leaves unresolved (exactly why
  their calls are `semantic`/`new_pair`), so they are genuinely NEW sketch information.
  Same-file dominance (22/37 identities add no pair) confirmed as before.
- **Committed synthetic fixture**: delta **0** (semantic new_pair = 0 per the M-R1
  fixture record — the union call graph is exactly the corroborated calls).
- **Fixture-proven nonzero**: the hand-built S-only pair (`src/a.ts → src/b.ts`,
  `build_multiplicity_fixture(0, 1)`) yields exactly one added pair
  (`g3u_semantic_new_pair_yields_the_cross_file_pair`).
- The monorepo-scale delta remains the S-1..S-3 field measurement (per §5.3.4/§6.2 of
  the design; the code path is the same union projection either way).

### 6.3 Decide-and-record (one line each)

- Trust `witnesses` is `Option<serde_json::Value>` (the `trust_briefing` convention),
  never a coherence leaf; one shape owner (the projection).
- The client renders through ONE shared `presentation/witnesses.rs` (no per-surface
  phrasing drift; defensive `Value` readers — a malformed block renders absence, never
  a panic or an invented zero).
- Reason ids `stale`/`not_resident`/`producer_unavailable` are the §4.2 ladder's
  machine-readable contract identifiers; posture/next-action carry the reader language.
- `agreement_pct` serializes as the raw quotient (deterministic given the pair); human
  renders `{:.1}` (97.4 — the ratified display form).
- Stale sub-detail (pending vs in-flight vs failed) renders via the slot freshness
  (`PrecisionPending`/`RefreshFailed` variants of the ONE `stale` reason's posture).
- `LiveGraph::partition_states()` added (one consumer: the projection; simpler
  alternative rejected: diffing `live_partitions()` against `resident_irs()`, which
  exposes neither status detail nor residency directly).
- Doctor probe caps (8 colliding keys / 8 delta pairs per enumeration) truncate WITH
  stated remainder counts — no silent caps.
- `storage_summary_probe` → `storage_summary_probes` (plural contract: storage +
  optional witness probe from ONE `storage_health` round-trip; local rename).
- Cost class recorded: one classification clone + O(pairs) scans per request on
  ledger-bearing repos only (zero-SCIP repos exit before even producer discovery);
  S-1 prices monorepo scale before any default flip.

### 6.4 Validation summary (details in the relay TEST REPORT)

fmt --check clean; clippy --workspace --all-targets -D warnings clean; FULL workspace
suite 240 targets / 5,321 tests, 0 failed (incl. 20 new witness_projection gate tests,
the §5.3.1 trust invariance test, 3 doctor witness-probe tests, 7 shared-renderer
tests); consolidation witness 15/15 (the reader-set amendment accepted; test-gating
verified); isolated dogfood PASS (operator registry proven untouched).

**R-0 byte-parity, old-binary-vs-new** [EXECUTED]: baseline worktree rebuilt at HEAD
`a981d70` (pre-M-R3a code) vs the working-tree build, on the no-SCIP fixture in
isolated state roots — `byte-compare-five-surfaces.sh` **20/20 PASS**
(trust/check/orient/explain/stats × human+JSON, A-vs-B write+read AND A-vs-C same-DB
raw) + `byte-compare-map-modules-surfaces.sh` **12/12 PASS** (modules-list/
modules-show/map). Canonical smoke sweep (`smoke-validation-repos.sh m-r3a trust
orient check`, isolated state root): **Passed 24** (nginx C, spring-petclinic Java,
zap-engine mixed, amodx TS, legacy set + internal — all commands, all metas zero
failures); **0 of 72 captures contain any witness line** — the R-0/R-1 live absence
proof (no LG fed → no block, no phantom W-ONE rows, on covered and uncovered repos
alike). One harness-budget artifact, stated: `linux:index` — the kernel-scale index
exceeded the sweep client's 300s read budget and continued detached (the ratified
INDEX-DISCONNECT-1 semantics; the script then stopped its own throwaway daemon), so
linux produced no captures. Not an M-R3a surface: this slice touches no index/write
path (diff inventory), and the read-surface byte-compares above cover the R-0 claim.

**Live E2E** [EXECUTED 2026-07-18, isolated long-lived daemon + socket under /private/tmp,
committed synthetic fixture indexed + LG-preloaded via `rmap dev livegraph-preload`]:
BEFORE any call-graph read, trust rendered `measured: null` + "not yet measured"
(unknown, never a number) with the W-BOTH regime row; a real `rmap callers` warmed the
ledger; THEN trust/orient/stats rendered the labeled union block/lines (union 2 =
pipeline 2, 100.0% corroborated, coverage TypeScript/synthetic/fingerprint; reference
tier 9; projections 4-of-18 — matching the M-R1 fixture record exactly), doctor
rendered the operational probe (adoption 9/4/2, fingerprint, producer truth), explain
correctly rendered NO union suffix (degrees agree), map correctly rendered NO added
pairs (semantic 0), and modules_list rendered the live g2u reduction ("reconciled: 2
fewer flagged across 1 module") beside the UNTOUCHED pipeline `4 unref?` column.
Operator registry proven untouched; scratch state cleaned. The g3u delta record: §6.2.

## 7. Resume validation record (2026-07-19 — post-fuse-kill builder resume; INCREMENTAL, binding)

The 2026-07-18 builder run was fuse-killed DURING validation (operator note in the resume
selection packet). This section is the resumed run's incremental evidence record; it is
written as each gate lands so a further kill leaves a resumable artifact.

### 7.1 Tree-state verification + the timeline finding (OBSERVED)

- Working tree matches the §6 record: `witness_projection/{mod,tests}.rs` +
  `presentation/witnesses.rs` (untracked) + 26 modified files, 1,029 insertions; full diff
  read and checked against §6.1–§6.3 (shared projection both halves, additive-only injection,
  R-0 absence at every site, frozen areas untouched, doctor caps stated, manifest edits
  explicit). No other untracked/modified files. [OBSERVED: `git status`, `git diff`.]
- **Timeline finding (governs what re-runs):** file mtimes show
  `callgraph_cert/ledger.rs` (PRODUCT code) last edited **00:21:41 local** and
  `witness_projection/tests.rs` **00:22:04**, AFTER the working-tree release binaries were
  built (00:01) and after the baseline-worktree binaries (00:17). The §6.4 binary-level
  evidence (R-0 byte-parity 20/20 + 12/12, the `2026-07-18T21-01-24Z` sweep, live E2E) was
  produced by binaries that PREDATE the final `ledger.rs` state, and the §6.4 cargo-suite
  claim may predate the final test state. [OBSERVED: `stat` mtimes; `smoke-runs/` listing.]
- Disposition: §6.4 claims are downgraded to OBSERVED-with-caveat (artifacts exist; they
  validated a tree differing from final only in the late `ledger.rs`/`tests.rs` edits). The
  resumed run re-executes the full binding §4 validation against a fresh build of the FINAL
  tree. Prior scratch that survives and is reused: the baseline worktree
  `../m-r2-baseline-worktree` at `a981d70` (pre-M-R3a HEAD) with built release binaries —
  correct and current for the baseline side (that commit's tree is unchanged).
- Orphan check: the only running `rmapd` is the operator's launchd daemon
  (`~/.local/bin/rmapd`) — untouched; no orphaned throwaway daemon survived the kill.
  [EXECUTED: `pgrep -lf rmapd`.]

### 7.2 Re-executed gates (2026-07-19)

Evidence labels per `agent_docs/validation.md`. Each entry appended when its command
completes.

- **fmt** [EXECUTED]: `cargo fmt --all --manifest-path rust/Cargo.toml -- --check` → clean
  (exit 0, no output).
- **clippy** [EXECUTED]: `cargo clippy --workspace --all-targets --manifest-path
  rust/Cargo.toml -- -D warnings` → clean (`Finished dev … in 24.19s`, zero warnings; the
  final `ledger.rs`/`tests.rs` state compiled — the dead-code-allow removals hold).
- **daemon-runtime suite** [EXECUTED]: `cargo test -p repo-graph-daemon-runtime
  --manifest-path rust/Cargo.toml` → **400 lib tests, 0 failed** (incl. the 20
  `witness_projection::tests::*` gate tests, the §5.3.1
  `witnesses_block_is_the_only_delta_between_ledger_absent_and_present` invariance test,
  collision rendering, three-distinct-W-ONE-postures, deterministic ordering — names
  observed in output) + every integration target green, incl. **consolidation witness
  15/15** (`tests/consolidation_witness.rs` — the reader-set amendment accepted with
  test-gating verified) + concurrency_dispatch 18, daemon_visibility 6, enrich_lifecycle
  10, snapshot_retention 13, others; 2 ignored (pre-existing engine regressions).
- **rgr suite** [EXECUTED]: `cargo test -p repo-graph-rgr --manifest-path rust/Cargo.toml`
  → **57 targets all ok, 0 failed** (629 lib tests incl. the 7 `presentation::witnesses`
  renderer tests + 3 doctor `witness_probe` tests — 10 M-R3a client tests observed by
  name).
- **full workspace** [EXECUTED]: `cargo test --workspace --manifest-path rust/Cargo.toml`
  → **240 targets, 5,321 tests passed, 0 failed** (grep over the captured output found no
  `FAILED` and no nonzero `failed` count) — reproduces the §6.4 figure, this time against
  the FINAL tree (post-00:22 state).
- **release build (final tree)** [EXECUTED]: `cargo build --release --manifest-path
  rust/Cargo.toml --bin rmap --bin rmapd` → `Finished release … in 46.67s`
  (daemon-runtime + rgr + rmapd recompiled — the binaries now INCLUDE the 00:21
  `ledger.rs` state, closing the §7.1 gap).
- **R-0 byte-parity, old-binary-vs-new (RE-RUN, final binaries)** [EXECUTED]: baseline =
  `../m-r2-baseline-worktree/rust/target/release/rmap` (worktree at `a981d70`, `git
  status --short` clean, its binaries current for that commit); candidate = the fresh
  build above.
  `BASELINE_RMAP=… CANDIDATE_RMAP=… ./scripts/byte-compare-five-surfaces.sh` →
  **20/20 PASS** (trust/check/orient/explain/stats × human+JSON; A-vs-B normalized
  write+read AND A-vs-C same-DB raw; outputs
  `/private/tmp/rg-m3b-bytecompare/20260718T215015Z-88258/`).
  `… ./scripts/byte-compare-map-modules-surfaces.sh` → **12/12 PASS**
  (modules-list/modules-show/map × human+JSON, same two comparisons; outputs
  `/private/tmp/rg-m3a-bytecompare/20260718T215026Z-88422/`). No-SCIP fixtures = the R-0
  live case: zero witness bytes anywhere.
- **isolated dogfood** [EXECUTED]: `./scripts/dogfood-isolated.sh` (final release
  binaries, stdio transport, throwaway SandboxLocal state root) → **OK**, all
  TRUNCATION-AUDIT assertions PASS, non-pollution check both halves PASS (operator
  registry proven untouched), state root cleaned. orient/explain/check outputs carry NO
  witness lines (R-0 on the final binaries, observed directly in the captures).
- **prior-sweep absence verification** [EXECUTED grep over OBSERVED artifacts]:
  `grep -rliE "witness|reconciled|combined-analyses" smoke-runs/2026-07-18T21-01-24Z` →
  no matches: **0 of the 72 captures** (24 repos × trust/orient/check; meta:
  `00-meta.json` passed=24, failed=`linux:index` only) contain any witness token —
  the §6.4 R-0/R-1 live-absence claim verified against the retained script-generated
  artifacts (old binaries; superseded for the final tree by the re-run below).
- **canonical smoke re-run (final binaries)** [EXECUTED — chunked via the script's
  `SMOKE_ONLY` lever (script-supported), same canonical repo inventory as the prior
  sweep, each chunk a full script invocation with its own script-generated
  `smoke-runs/<ts>/` provenance; task `m-r3a-r2`, isolated state root
  `/private/tmp/repo-graph-tests/m-r3a-r2`, script-deleted on success]:
  `SMOKE_ONLY="…" ./scripts/smoke-validation-repos.sh m-r3a-r2 trust orient check` ×4 →
  **Passed 24 / Failed 0** across the chunks — `2026-07-18T21-52-03Z` (15: all 7 internal
  + buildroot leveldb mempalace nginx OpenXcom rabbitmq-tutorials spring-petclinic
  swupdate) · `…T21-54-23Z` (6: django duckdb grpc-java langchain4j poco sqlite) ·
  `…T21-56-54Z` (2: gstreamer kafka) · `…T22-00-29Z` (1: hadoop). Same 24-repo coverage
  as the prior sweep. `check` FAIL@Fresh verdicts on low-resolution corpora are the
  commands' own verdicts (`NOTE: check exit 1 (command verdict/status — not a harness
  error)`) and MATCH the prior sweep's verdicts on the spot-compared repos (repo-graph /
  nginx / spring-petclinic / amodx / zap-engine) — no delta.
  **R-0/R-1 live absence on final binaries**: `grep -rliE
  "witness|reconciled|combined-analyses"` over all four new run dirs → no matches: **0 of
  72 captures** (24 repos × trust/orient/check) contain any witness token — no LG fed ⇒
  no block, no phantom W-ONE rows, on covered (TS) and uncovered repos alike.
  **linux** [NOT RUN — deliberate]: the kernel-scale index exceeded the sweep client's
  read budget in the prior run and produced no captures (ratified INDEX-DISCONNECT-1
  semantics; harness-budget artifact, stated then, unchanged now). Not an M-R3a surface:
  the diff touches no index/write path; the R-0 read-surface byte-parity above carries
  the claim.
- **live E2E (final binaries; full replay of the §6.4 recipe)** [EXECUTED]: driver
  `/private/tmp/m-r3a-resume-live/driver3.sh` (retained with outputs, run
  `20260718T221124Z-95511/out/`) — isolated long-lived `rmapd` + socket under
  `/private/tmp`, throwaway state root, committed synthetic fixture indexed +
  LG-preloaded (`rmap dev livegraph-preload`, partition `synthetic`, no producer), daemon
  killed by trap. **All assertions PASS**:
  - BEFORE any call-graph read: trust `witnesses.measured: null` + "corroboration:
    unknown — not yet measured" + the W-BOTH regime row (unknown, never a number).
  - A real `rmap callers describe` warmed the ledger; THEN trust rendered the labeled
    block verbatim: "TypeScript (1 partition): 2 syntax-resolved calls; the compiler
    could measure 2 — 2 corroborated (100.0%)" + "beyond the syntax graph: 9
    compiler-verified references" + "4 of 18 symbol-direction lookups had no
    compiler-side answer" — the M-R1 fixture record exactly (union 2 = pipeline 2,
    reference tier 9, projections 4-of-18).
  - orient + stats: the one additive g1u line ("reconciled: 2 combined-analyses calls
    (TypeScript (1 partition)) …") via the shared renderer; check: NO witness line
    (§5.3.1 check invariance, asserted).
  - doctor: "[ok] call-graph witnesses: ledger current — union accounting measured" +
    "adoption synthetic: 9 adopted / 4 fallback / 2 file-scope" + the fingerprint line +
    "producer scip-typescript: not provisioned" (capability truth).
  - explain (SYMBOL focus): NO union suffix (degrees agree); modules list: "reconciled:
    2 fewer flagged across 1 module — compiler-verified references found" BESIDE the
    untouched pipeline "4 unref?" column.
  - map: the DAEMON response carries the g3u block — `"witnesses":{"pair_delta":0,
    "dependency_call_pairs_added":[],"accounting":"union","coverage":{…}}` (in-band
    delta record, §5.3.4) — observed via a raw NDJSON socket probe; the rendered sketch
    carries NO phantom labels at delta 0.
  - Operator registry proven untouched (asserted); no orphan daemon (trap-killed).
  **Correction recorded (my probe, not the product):** the first driver run FAILED its
  map assertion because it grepped `rmap map --json` for `pair_delta` — that CLI output
  is the RENDERED MAP.md documents, not the daemon facts; the block rides the daemon
  response and surfaces in the render only as inline labels when `pair_delta > 0`. A v2
  discrimination probe (retained, `…220855Z-95361/out/`) confirmed every other surface
  measured on the same daemon; the corrected v3 observable (raw socket probe) passed.
  No product defect. A live NONZERO delta is unreachable on the DEFAULT path by ratified
  design (default ledger capture is GREEN-gated per M-R1 ⇒ measured ⇒ semantic
  new_pair = 0); the nonzero path is pinned by the EXECUTED unit gate
  (`g3u_semantic_new_pair_yields_the_cross_file_pair`).
- **g3u pair delta record (§6.2) disposition**: the record stands. Fixture legs
  RE-EXECUTED this run (committed fixture delta **0** live — the `pair_delta: 0` probe
  above; nonzero fixture-proven in the suite). The amodx leg is OBSERVED: the retained
  corpus exists UNCHANGED (`.agent-manager/slices/RECON-DESIGN-1/runs/amodx/*`, mtimes
  2026-07-17 — predating both builder runs, read-only discipline held), and §6.2's
  numbers were EXECUTED by the 2026-07-18 run over exactly these artifacts; this resume
  did not re-derive them (the derivation script was not retained; re-writing it would
  re-measure an unchanged corpus).
- **prior-run scratch disposal** (test-protocol lifecycle): the killed run's stale state
  root `/private/tmp/repo-graph-tests/m-r3a` (25 DBs indexed by the superseded 00:01
  binaries) DELETED after verification — superseded by the `m-r3a-r2` re-run; its
  `smoke-runs/2026-07-18T21-01-24Z` provenance artifacts RETAINED (durable audit trail).
  Byte-compare outputs + live-E2E drivers/outputs RETAINED under `/private/tmp/` for
  reviewer re-inspection (paths above). Baseline worktree `../m-r2-baseline-worktree`
  RETAINED at `a981d70` (the reviewer's byte-parity re-run needs it).
- **Install/deploy** [NOT RUN — by contract]: `dev-install-local.sh` is Phase 2, only
  after reviewer approval. **Cleanup** (`clean-build.sh --all`) [NOT RUN — deliberate]:
  the reviewer gate is pending and re-running any gate above needs the build tree;
  run at slice end after review.

### 7.3 Resume verdict

Every §4 binding gate is now EXECUTED against the FINAL tree state (the post-00:22
`ledger.rs`/`tests.rs`): fmt/clippy clean; 5,321 tests / 240 targets / 0 failed (incl.
all named M-R3a gate tests); witness 15/15 with the explicit reader-set amendment;
R-0 byte-parity 32/32 old-binary-vs-new; canonical smoke 24/24 repos with 0/72 witness
tokens (R-0/R-1 live); isolated dogfood PASS; live E2E full-surface PASS incl. the
in-band g3u delta record. Zero product findings; one probe-side observation-shape error
found and corrected in the harness driver (recorded above). The §6.4 timeline gap
(binaries predating the last product edit) is CLOSED. Reviewer gate remains pending;
nothing committed; nothing installed.

## 8. Iteration-1 revision record (2026-07-19 — post-escalate; INCREMENTAL, binding)

Review-0 escalated with two DECISION_REQUIREDs (both operator-RATIFIED, recorded in the
resume selection packet) and five defects. This section records the closure of each and the
re-executed binding validation, appended incrementally as evidence lands.

### 8.1 The two ratified decisions, implemented

- **M-R3A-TRUST-POSTURE (ratified: amend the posture representation).** The review-0
  CONTRADICTION — `current_state_posture.value.resident: false` ("LiveGraph not loaded")
  beside a W-BOTH witnesses block on a resident-but-cert-gated state — is structurally
  removed. `LiveGraphPosture` gains TWO ADDITIVE fields (`Option<bool>`, skip-if-none +
  serde-default): `livegraph_resident` (ACTUAL residency — ≥1 resident partition observed
  under the posture build's read guard) and `coherent_serve_eligible` (the EV-A/no-loss-cert
  serve gate). The EV-A-failed path now returns the new `resident_withheld_leaf()` — legacy
  `resident: false` (the SERVE fact — epoch invariant untouched), class `Unavailable`
  unchanged, partitions still withheld, but the two facts stated: resident yes / eligible no.
  The genuinely-cold path omits both fields (absent = the legacy field is the complete truth
  → the zero-SCIP wire stays BYTE-IDENTICAL — R-0 preserved, proven by the byte-parity gate
  below and the named wire test). Human render: three states — "Resident: no (LiveGraph not
  loaded…)" ONLY on cold; "Resident: yes (compiler analysis is loaded) — current-state detail
  withheld: not verified coherent with this report's snapshot for this request" on the
  withheld state; the served rendering unchanged. Trust ratio inputs untouched
  (`service.rs`/`rules.rs` not in the diff; the §5.3.1 invariance test still pins it).
  NON-CONTRADICTION is a named daemon test
  (`resident_cert_gated_state_renders_two_labeled_facts_never_not_loaded`) reproducing the
  reviewer's exact state (resident LG + measured ledger + no GREEN cert witness): the
  witnesses block renders W-BOTH ∧ the posture states `livegraph_resident: true`.
  **Name-vs-semantics finding, surfaced not renamed:** the legacy wire field `resident` IS
  the serve fact, not the residency fact — its doc comment now says so; renaming it (e.g. to
  `posture_served`) is a breaking JSON change on the trust surface, DEFERRED to its own
  ratification per the Change Doctrine.
- **M-R3A-READER-SET-AMENDMENT (ratified: the exact four additions).** The amendment note is
  recorded in the `witness/livegraph_reader_set.txt` HEADER (sanctioned set now 16; the four
  additions named with their basis and the ratification date) and mirrored at the
  `SANCTIONED_SURFACES` constant — the reviewed two-site change the witness demands.
  Consolidation witness re-run green (§8.3).

### 8.2 The five defects, closed

- **(a) Superseded ledger masked a current build failure.** The projection's `Superseded`
  arm now READS the retained `witness_ledger_build_failure` (the store clears it only on
  success, so its presence is always the LATEST attempt's outcome) and renders BOTH facts:
  doctor's ledger object gains `last_build_outcome: "failed"` + `failed_fingerprint` +
  `failure_reason` beside `present: true / current: false`; trust's `unknown_reason` reads
  "superseded by witness movement, and the latest re-measurement attempt failed (<reason>)".
  Client doctor probe renders both on the superseded arm. Named tests daemon+client
  (`superseded_ledger_does_not_mask_the_latest_build_failure`,
  `witness_probe_superseded_renders_the_latest_build_failure_beside_it`).
- **(b) Collision unit truth.** The ledger's `identity_collision` counts WITHHELD S
  strict-`Calls` INSTANCES (ledger.rs doc — correct all along); trust/doctor rendered it as
  colliding "identities". Now: the measured block serializes `identity_collision` in the
  sibling `{instances, identities}` convention (identities = distinct withheld call pairs,
  the block's uniform identity unit); the trust human line renders the INSTANCE unit
  ("identity collisions …: N compiler-witnessed call instances (M call pairs) withheld —
  shown separately, never merged"); doctor's collision line carries BOTH populations with
  their own units ("K symbol identities collide … — N compiler-witnessed call instances
  withheld"), K = distinct colliding KEYS (the §5.4 KEY population, whose keys render
  beside it). The env-gated debug diff artifact (`RMAP_CALLGRAPH_DIFF`) still emits the raw
  ledger field on the explicit debug surface — not a reader-facing rendering; untouched.
- **(c) Mixed-repo scoping fixture.** The M-R2 covered+uncovered fixture
  (`build_pipeline_only_fixture`: two resident Fresh TS partitions in S; Rust symbols
  pipeline-only) is now driven through the M-R3a PROJECTION surfaces —
  `mixed_repo_projection_scopes_to_covered_partitions_never_the_uncovered_language`: regime
  rows = exactly the two TS partitions (no phantom row for the uncovered language); coverage
  basis names TypeScript/p1/p2 only; the uncovered-pair instance lands `unmeasured`
  (coverage, never divergence) while the three TS-caller pairs are the dual-measured
  `syntactic` split; g2u claims no compiler witness for uncovered-language symbols; g3u adds
  no phantom pairs; doctor adoption rows = covered partitions only.
- **(d) The g3u pair delta — properly measured.** See §8.4: the amodx delta is now an
  EXECUTED end-to-end measurement through the implemented map semantics (isolated daemon,
  fresh pipeline index, the RETAINED capture-time SCIP partitions preloaded, ledger warmed,
  `pair_delta` read in-band), replacing §6.2's upper bound. §6.2 is amended in place with
  the measured record.
- **(e) EOF blank line** in this document removed (`git diff --check` clean).

### 8.3 Re-executed gates (appended as they land)

- **fmt** [EXECUTED]: `cargo fmt --all --manifest-path rust/Cargo.toml -- --check` → clean.
- **clippy** [EXECUTED]: `cargo clippy --workspace --all-targets --manifest-path
  rust/Cargo.toml -- -D warnings` → clean (`Finished dev … 26.73s`, zero warnings).
- **trust crate** [EXECUTED]: `cargo test -p repo-graph-trust` → **110 passed, 0 failed**
  (incl. the two new wire tests: cold omits the amendment fields; withheld states both).
- **daemon-runtime** [EXECUTED]: `cargo test -p repo-graph-daemon-runtime` → **403 lib
  tests, 0 failed** (+3 over §7: the superseded-not-masked, mixed-repo-scoping, and
  posture-non-contradiction gate tests — all observed by name) + every integration target
  green incl. **consolidation witness 15/15** with the ratified amendment note.
- **rgr** [EXECUTED]: `cargo test -p repo-graph-rgr` → **57 targets, 0 failed** (new tests
  observed by name: `witness_probe_superseded_renders_the_latest_build_failure_beside_it`,
  `resident_withheld_posture_renders_loaded_with_detail_withheld_never_not_loaded`,
  `collision_line_renders_when_the_guard_fired` with the unit-truth assertions).
- **full workspace** [EXECUTED]: `cargo test --workspace --manifest-path rust/Cargo.toml` →
  **240 targets, 5,328 tests passed, 0 failed** (+7 new gate tests over §7's 5,321; zero
  `FAILED` lines in the captured output).
- **release build (revised tree)** [EXECUTED]: `cargo build --release … --bin rmap --bin
  rmapd` → `Finished release … 50.27s` — the binary-level gates below run on THESE binaries.
- **R-0 byte-parity, old-binary-vs-new (revised binaries)** [EXECUTED]: baseline =
  `../m-r2-baseline-worktree` @ `a981d70` (clean; binaries current for that commit);
  candidate = the fresh revised-tree release build.
  `byte-compare-five-surfaces.sh` → **20/20 PASS**
  (`/private/tmp/rg-m3b-bytecompare/20260718T231428Z-14086/`);
  `byte-compare-map-modules-surfaces.sh` → **12/12 PASS**
  (`/private/tmp/rg-m3a-bytecompare/20260718T231435Z-14248/`). This ALSO proves the posture
  amendment's R-0 half at the binary level: the cold-path trust JSON carries NO new fields —
  zero-SCIP wire byte-identical to the pre-M-R3a binary.

### 8.4 The PROPER g3u measurement (defect d — EXECUTED, replacing the §6.2 upper bound)

Method (driver retained: `/private/tmp/m-r3a-amodx-g3u/driver-amodx.sh`, run
`20260718T231512Z-14398/out/` incl. the raw daemon response): an ISOLATED throwaway daemon
(own state root + socket; operator registry untouched — non-pollution asserted, the only
surviving `rmapd` after the run is the operator's launchd one); `rmap index` of amodx fresh
(pipeline witness; source UNCHANGED since the capture — amodx HEAD `4230541`, 2026-06-29,
predates the 2026-07-17 capture; `git status` clean, so the same-state condition for pairing
the two witnesses holds); the EIGHT retained capture-time `.scip` partitions preloaded
(`/private/tmp/recon-design-1-amodx/scip/` — the SAME S witness the §6.2/§3.0b analyses
used); the witness ledger warmed by a real `rmap callers` read; then the RAW daemon `map`
response read over the NDJSON socket.

**Result: `pair_delta: 11`** — `accounting: union`, coverage TypeScript × all 8 partitions,
the 11 `dependency_call_pairs_added` enumerated in-band (6× admin components →
`admin/src/lib/utils.ts`; 5× renderer API routes → `renderer/src/lib/api-client.ts`). The
shipped subtraction runs against the FULL pipeline sketch (`map_resolved_dep_edges_in_path`
= IMPORTS + CALLS file pairs), which the killed run's artifact-only derivation could not do
(no retained pipeline-IMPORTS pairs → its 12 was labeled an upper bound). Upper bound 12 →
measured 11: exactly one candidate (the backend pair) was import-absorbed. The remaining 11
ride alias imports the pipeline leaves unresolved — the same cause that makes their calls
`semantic`/`new_pair` — so g3u at this scale adds 11 genuinely new dependency-sketch pairs,
not noise. The monorepo-scale delta remains S-1..S-3's field measurement (unchanged).
- **isolated dogfood** [EXECUTED]: `./scripts/dogfood-isolated.sh` (revised release
  binaries) → **OK**, all assertions PASS, non-pollution BOTH halves PASS (operator
  registry proven untouched), state root cleaned.
- **live E2E (revised binaries; driver4 = §7's driver3 + the iteration-1 assertions)**
  [EXECUTED — retained: `/private/tmp/m-r3a-resume-live/driver4.sh`, run
  `20260718T231714Z-14596/out/`]: the FULL §7 assertion set re-passed (unknown-before →
  labeled-after, doctor probe, check invariance, explain no-suffix, modules g2u beside the
  untouched pipeline column, daemon map g3u block `pair_delta: 0` in-band, registry
  untouched) PLUS the iteration-1 gates, all PASS:
  - **posture amendment LIVE in the review-0 contradiction state** (resident LG + measured
    ledger + no GREEN cert): `livegraph_resident: true` ∧ `coherent_serve_eligible: false`
    ∧ legacy `resident: false` ∧ the W-BOTH regime row in ONE response — the two blocks
    AGREE; human render "Resident: yes (compiler analysis is loaded) — current-state detail
    withheld…"; the string "not loaded" absent from the whole trust output.
  - **collision unit-truth wire shape**: `identity_collision` serves the unit-labeled
    `{instances, identities}` object.
- **canonical smoke (revised binaries; §7's chunk pattern, task `m-r3a-i1`)** [EXECUTED —
  each chunk a full script invocation with script-generated provenance]:
  `SMOKE_ONLY="…" ./scripts/smoke-validation-repos.sh m-r3a-i1 trust orient check` →
  **24/24 repos passed** across `2026-07-18T23-18-01Z` (15: all 7 internal + buildroot
  leveldb mempalace nginx OpenXcom rabbitmq-tutorials spring-petclinic swupdate) ·
  `…T23-20-02Z` (5 of 6; duckdb FINDING below) · `…T23-27-00Z` (duckdb PASS) ·
  `…T23-27-45Z` (gstreamer kafka) · `…T23-31-17Z` (hadoop). Check verdicts spot-compared
  vs the §7 sweep (repo-graph / nginx / spring-petclinic / amodx / zap-engine): identical.
  **R-0/R-1 live absence**: `grep -rliE "witness|reconciled|combined-analyses"` over all
  five run dirs (75 capture files) → **zero matches** — no LG fed ⇒ no block, no phantom
  rows, covered and uncovered repos alike, on the revised binaries.
  **FINDING (transient, discriminated — not a product regression):** duckdb failed its
  chunk run and two re-runs with `error: InternalError: failed to open storage connection:
  database is locked` on 1–2 of the 3 commands, the failing command MOVING run-to-run
  (trust→orient/check) — a race, not determinism. Discrimination: (1) an A/B harness
  (fresh isolated state roots, same index→trust/orient/check sequence — drivers retained,
  `/private/tmp/m-r3a-duckdb-ab/`) passed IDENTICALLY under BOTH the pre-M-R3a baseline
  binaries (`a981d70`) and the revised ones (trust 0 / orient 0 / check 1-own-verdict);
  (2) the M-R3a diff adds no SQLite open/locking path (the projection is PEEK-only and
  exits before any work on LG-less repos — duckdb's exact class — and the R-0 byte-parity
  gates pin the read behavior); (3) the shared smoke root had accumulated 4.4 GB across
  chunks + duckdb re-runs, and duckdb's DB carried live `-shm`/`-wal` sidecars from the
  script's daemon kills — after the project's ratified derived-cache remediation
  (operator reset: delete the throwaway DB, reindex), the SAME harness + binaries passed
  (`…T23-27-00Z`). Verdict: pre-existing lock-wait-at-open behavior under harness-state
  churn; failing-run artifacts RETAINED (`smoke-runs/2026-07-18T23-{20-02,22-48,25-35}Z`).
  **linux** [NOT RUN — deliberate, unchanged from §7]: kernel-scale index exceeds the
  sweep client's read budget (ratified INDEX-DISCONNECT-1 semantics); not an M-R3a
  surface; the R-0 byte-parity gates carry the claim.
- **Install/deploy** [NOT RUN — by contract]: `dev-install-local.sh` only after reviewer
  approval. **Cleanup** (`clean-build.sh --all`) [NOT RUN — deliberate]: reviewer gate
  pending; re-running any gate above needs the build tree.

### 8.5 Iteration-1 verdict

Both ratified decisions implemented and validated (the posture contradiction is
structurally unmintable — daemon test + pure-crate wire tests + renderer test + LIVE
reproduction of the reviewer's state, all green; the reader-set amendment recorded at both
sites, witness 15/15). All five defects closed — (a) and (b) with daemon+client tests and
live wire proof, (c) with the named mixed-repo projection gate test, (d) with an EXECUTED
end-to-end measurement (`pair_delta: 11` at amodx scale, §8.4, replacing the upper bound),
(e) verified by `git diff --check`. Full re-validation on the revised tree: fmt/clippy
clean; 240 targets / 5,328 tests / 0 failed; witness 15/15; R-0 byte-parity 32/32
old-binary-vs-new; smoke 24/24 with 0/75 witness tokens and one discriminated
environmental FINDING (above); isolated dogfood PASS; live E2E full-surface PASS incl.
the iteration-1 assertions. Zero product findings against the M-R3a diff. Reviewer gate
pending; nothing committed; nothing installed.

### 8.6 Post-record simplification + FINAL-binary re-run (supersedes §8.3's binary evidence)

The pre-report self-review removed one unearned structure: the projection's private
`LastBuildFailure` mirror struct — the ledger's own `LedgerBuildFailure` is `Clone`, so the
`Superseded` arm now carries it directly (same data, one type fewer; suite/clippy/fmt
re-verified green: 403 daemon lib tests, 0 failed). Because that edit touched daemon
product code AFTER §8.3's binary gates ran (the §7.1 timeline-gap class), release was
REBUILT from the final tree and EVERY binary-level gate re-executed on it — all PASS
[EXECUTED]:

- R-0 byte-parity: five-surfaces **20/20** (`/private/tmp/rg-m3b-bytecompare/
  20260718T233808Z-20667/`) + map/modules **12/12** (`…rg-m3a-bytecompare/
  20260718T233818Z-20829/`).
- amodx g3u measurement: **`pair_delta: 11`** reproduced exactly (all 8 partitions).
- live E2E driver4: all assertions PASS (run `20260718T233833Z-21010/out/`).
- isolated dogfood: PASS, non-pollution both halves.
- canonical smoke, all four chunks on a FRESH state root (task `m-r3a-i1f`):
  **24/24 passed, zero failures** — `2026-07-18T23-38-45Z` (15) · `…T23-40-40Z` (6,
  duckdb PASSING first-try on the fresh root — reconfirming the §8.3 FINDING's
  environmental diagnosis) · `…T23-43-08Z` (2) · `…T23-46-36Z` (1); **0/72 captures**
  contain any witness token (R-0/R-1 live absence on the final binaries).

Every §4 binding gate has now been EXECUTED against the exact final tree state.

## 9. Iteration-2 revision record (2026-07-19 — post-review-1 "revise"; INCREMENTAL, binding)

Review-1 (verdict: revise) required five changes. This section records each closure and the
re-executed binding validation, appended incrementally as evidence lands.

### 9.1 The five required changes, closed

- **(1) The witness-currency race under W-B refresh concurrency — CLOSED by single-guard
  atomicity.** Iteration 1 captured the inventory + current fingerprint under `livegraph.read()`,
  RELEASED the guard, then read `witness_ledger` — a refresh swap (`livegraph.write()`,
  `livegraph_refresh.rs`) between the two reads could pair the pre-swap fingerprint with the
  retained pre-swap ledger → false `Measured` (a stale number rendered as current). Fix:
  `compute_with_seam_probe` holds ONE LiveGraph read guard across the inventory/fingerprint
  capture AND the ledger-currency selection — the swap's write acquisition cannot interleave, so
  `Measured` only ever pairs a ledger with the fingerprint current at the same consistent
  instant. Precedent: `callgraph_cert::callgraph_union_eligibility` peeks under the same one-guard
  discipline. **Deadlock analysis (deterministic grep over all 11 `witness_ledger` files, both
  crates):** every dual-lock site orders `livegraph` → `witness_ledger`
  (`callgraph_union_eligibility`, `union_serve::…`, the builder holds LG only inside
  `build_witness_ledger_outcome` and stores guard-free); no site acquires them in reverse; the
  `witness_ledger_build_failure` lock nests strictly innermost. Two DETERMINISTIC regression
  tests at the exact seam (a private seam-probe parameter; the child test module injects, no
  cfg(test) surface): `livegraph_swap_cannot_interleave_fingerprint_capture_and_ledger_selection`
  (the swap's `try_write()` must FAIL at the seam) and
  `ledger_movement_at_the_seam_renders_superseded_never_measured` (a concurrent ledger store
  landing mid-projection classifies against the PINNED fingerprint → Superseded, never Measured
  — the seam half that remains movable, forced deterministically). Cost note: the guard is now
  held across the one measured-classification clone (the recorded per-request cost class,
  ledger-bearing repos only); the refresh swap waits that clone out — micro-scale beside the
  seconds-scale producer run it follows.
- **(2) The g3u coverage label on the rendered map — CLOSED.** `presentation/witnesses.rs::
  coverage_phrase` is now `pub` (second concrete caller — the shared phrasing, no new
  abstraction) and `presentation/map.rs` renders the §5.3.0 human frame beside the additions:
  "Compiler-witnessed additions below are reconciled — combined analyses (coverage: TypeScript
  (N partitions))." — rendered ONLY when witness pairs actually fold (R-0: zero-witness maps
  byte-identical). The §5.3.0 labeling rule is now ENFORCED at the fold: pairs fold only when
  the block carries `accounting: "union"` AND a derivable coverage basis — a union value never
  renders unlabeled (malformed/label-less block → exactly the pipeline sketch, tested).
- **(3) Nonzero final-surface tests — ADDED.** (i) map: witness-only pair through FINAL
  MAP.md rendering — labeled inline + the coverage line + the merged count, WITH a
  pipeline-reachable pair proven subtracted (plain render, no double count)
  (`witness_pairs_render_labeled_with_coverage_and_subtract_known_targets`), plus the
  labeling-gate negative (`witness_pairs_without_coverage_basis_never_fold`); (ii) modules
  list + show: nonzero reduction through final human rendering beside the untouched pipeline
  figures (`list_render_nonzero_reduction_renders_the_reconciled_footnote`,
  `show_render_nonzero_unref_reduction_renders_beside_pipeline_count`) + the zero/malformed
  absence guard (`show_render_zero_or_malformed_reduction_renders_nothing`; modules_show
  gained the `n > 0` filter its sibling already had — a present-zero renders nothing);
  (iii) explain: differing union degree through final human rendering on callers AND callees
  (`render_shows_union_degree_suffix_where_it_differs`) + the no-union-object exact-heading
  negative.
- **(4) `PartitionState::producer_unavailable` — REMOVED.** The field was populated but read
  nowhere (deterministic grep: zero `.producer_unavailable` field accesses; `PartitionState`
  has exactly one constructor and one consumer), and its doc claimed it was the projection's
  compound-state evidence while the implementation's recorded decision is that CURRENT
  provisioning governs. Field + population removed; `partition_states()` doc now states the
  truthful contract (producer availability deliberately not exposed — the read-time discovery
  probe governs the §4.2 compound; a slot loaded producer-absent may sit beside a producer
  provisioned since).
- **(5) Malformed witness counts never coerce to zero — CLOSED.** Every aggregate now REQUIRES
  its component fields: `measurement_lines`' syntax-only causes (all four) and semantic sum
  (both), doctor's adoption line (all three) and delta pairs (both counts; the "… and N more"
  remainder now counts RENDERABLE pairs so the stated cap stays truthful under skips);
  single-field reads (`references`) are explicit `Option`s. A malformed/additive payload
  renders ABSENCE for the broken aggregate while intact fields still render (partial payloads
  degrade honestly, never invent). Malformed-block tests on both surfaces
  (`malformed_syntactic_aggregate_renders_absence_never_a_partial_sum`,
  `malformed_semantic_aggregate_omits_its_part_but_keeps_references`,
  `missing_references_field_renders_absence_not_zero`,
  `witness_probe_malformed_counts_render_absence_never_invented_zeros`).

### 9.2 Re-executed gates (appended as they land)

- **witness_projection suite (early gate)** [EXECUTED]: `cargo test -p
  repo-graph-daemon-runtime witness_projection` → **24 passed, 0 failed** (the 22 prior gate
  tests + the 2 new seam-race regression tests, all observed by name).
- **fmt** [EXECUTED]: `cargo fmt --all --manifest-path rust/Cargo.toml -- --check` → clean
  (after one mechanical reflow of the new code).
- **clippy** [EXECUTED]: `cargo clippy --workspace --all-targets --manifest-path
  rust/Cargo.toml -- -D warnings` → clean (exit 0; the `producer_unavailable` removal and the
  seam-probe threading introduce no warnings).
- **trust crate** [EXECUTED]: `cargo test -p repo-graph-trust` → **110 passed, 0 failed**
  (posture-amendment wire tests unchanged — no trust-crate code touched this iteration).
- **daemon-runtime** [EXECUTED]: `cargo test -p repo-graph-daemon-runtime` → **480 passed,
  0 failed across lib + every integration target** (lib now 405: +2 seam-race tests over §8's
  403), incl. **consolidation witness 15/15** (each of the 15 observed by name; the reader
  set is UNCHANGED this iteration — R1–R5 added no new `.livegraph` reader, so no manifest
  edit was needed or made).
- **rgr** [EXECUTED]: `cargo test -p repo-graph-rgr` → **57 targets, all ok, 0 failed**; the
  10 new client tests observed by name (map ×2, modules list/show ×3, explain ×1,
  witnesses malformed ×3, doctor malformed ×1).
- **Record cut short here**: the iteration-2 run was fuse-killed during validation; the
  remaining §4 gates (workspace suite, release rebuild, byte-parity, smoke, dogfood, live
  E2E, g3u confirmation) never re-ran on the iteration-2 tree. Review-2 flagged exactly
  this. Iteration 3 (§10) changes product code again, so §10's full re-run against ITS
  final tree supersedes what §9.2 would have recorded.

## 10. Iteration-3 revision record (2026-07-19 — post-review-2 "revise"; INCREMENTAL, binding)

Review-2 (verdict: revise) required two changes. Relay report: `.agent-manager/slices/
RECON-M-R3A-READ-SURFACES-1/build-3.md` (same content, written incrementally as evidence
lands).

### 10.1 Required change 1 — the §5.3.0 labeling gate on the g2u human renderers, CLOSED

Review-2's finding: `explain_sections.rs::union_degree_suffix`, `modules_list.rs`, and
`modules_show.rs` consumed union counts WITHOUT requiring `accounting: "union"` + a valid
coverage basis, and without rendering the coverage beside the reconciled value — presenting
a union value without the mandated human frame "reconciled — combined analyses
(coverage: …)", unlike the corrected map renderer.

Closure — ONE shared gate, six consumers (no per-surface drift possible):

- `presentation/witnesses.rs` gains `pub fn union_coverage_phrase(block)` — the §5.3.0 gate
  in ONE place: `Some(coverage phrase)` ONLY when the block carries `accounting: "union"`
  AND a derivable coverage basis; `None` → the consumer renders NO union value (suppression,
  never an unlabeled figure). `coverage_phrase` PRIVATIZED (the phrase alone is not the
  gate; its sole caller is the gate). `g1u_line` + `measurement_lines` switched to the gate
  (they required coverage but not the marker — the same latent class, closed uniformly; the
  daemon always sends the marker, so well-formed payloads render identically).
- `explain_sections.rs::union_degree_suffix`: gated; the suffix now renders
  " · reconciled N — combined analyses (coverage: <phrase>)"; gate-fail → exactly the
  pipeline heading.
- `modules_show.rs`: gated; "(reconciled — combined analyses; coverage: <phrase>)" beside
  the untouched pipeline count; gate-fail → line absent.
- `modules_list.rs`: PER-ROW gate; the footnote renders "— combined analyses (coverage:
  <phrase>)."; a failing row contributes nothing; distinct phrases collected as a set
  (one projection today ⇒ one phrase; the join stays honest if that ever diverges).
- `map.rs`: its inline accounting check replaced by the SAME shared gate (behavior
  identical — its own tests pin it).

Negative tests (missing/malformed accounting OR coverage suppresses the union value):
`union_values_without_the_accounting_marker_never_render` (witnesses),
`union_degree_without_accounting_or_coverage_never_renders` (explain, both cases),
`show_render_zero_or_malformed_reduction_renders_nothing` (extended ×2 cases),
`list_render_unlabeled_reduction_rows_render_no_footnote` (×2 cases + the MIXED
labeled/unlabeled per-row proof), `witness_pairs_without_the_accounting_marker_never_fold`
(map — the accounting half; the coverage half existed). Positive tests updated to assert
the coverage phrase beside every reconciled value.

Decide-and-record: `union_coverage_phrase` — six concrete callers, axis = the §5.3.0 gate
must be uniform across renderers (review-2's exact defect class); rejected simpler
alternative: per-site checks (the drift that caused this revision). JSON wire UNCHANGED
(daemon blocks already carry both labels — `unref_reduction_block` / `union_degree_label` /
`g3u_label` / `witnesses_block`). Client-render-only ⇒ no new `.livegraph` reader ⇒ no
manifest edit.

### 10.2 Required change 2 — the FULL binding validation against the exact final tree
(appended as each gate lands)

- **rgr presentation (early gate)** [EXECUTED]: `cargo test -p repo-graph-rgr --lib
  presentation` → **412 passed, 0 failed** (one test-side assertion reshape during
  bring-up — trailing-newline strictness; product behavior correct throughout).
- **fmt** [EXECUTED]: clean. **clippy** [EXECUTED]: `--workspace --all-targets
  -D warnings` → clean, zero warnings.
- **rgr** [EXECUTED]: `cargo test -p repo-graph-rgr` → **57 targets, 1,075 passed, 0
  failed**; the 9 iteration-3 gate tests observed by name (5 negatives incl. the map
  accounting-half + the modules_list mixed per-row proof; 4 updated positives asserting
  the coverage phrase beside every reconciled value).
- **daemon-runtime** [EXECUTED]: **11 targets, 480 passed, 0 failed**, incl.
  **consolidation witness 15/15** (reader set UNCHANGED — client-render-only diff; no
  manifest edit needed or made).
- **trust** [EXECUTED]: **3 targets, 111 passed, 0 failed** (crate untouched).
- **full workspace** [EXECUTED]: `cargo test --workspace` → **240 targets, 5,344 passed,
  0 failed** (249 ignored — the standing pre-existing shape; no new `#[ignore]`).
- **release build (final tree)** [EXECUTED]: `Finished release … 45.97s`; all binary gates
  below on these binaries.
- **R-0 byte-parity old-vs-new** [EXECUTED]: five-surfaces **20/20 PASS**
  (`/private/tmp/rg-m3b-bytecompare/20260719T005832Z-46448/`); map/modules **12/12 PASS**
  after ONE FINDING, discriminated + closed as a HARNESS defect (details in build-3.md):
  the script's `<MODUID>` mask required exactly 16 hex chars while `generate_module_uid`
  renders an UNPADDED `{:x}` u64 (1..16 chars; ~1/16 of per-run repo uids yield 15) — this
  run's baseline uid was 15 chars, escaped the mask, spurious AvsB diff. A-vs-C passed
  byte-identical RAW (read path unchanged); uid derivation is in the untouched `indexer`
  crate. Harness mask widened to `{1,16}` + its rule-7 comment corrected
  (`scripts/byte-compare-map-modules-surfaces.sh`); re-run **12/12 PASS**
  (`…/20260719T010004Z-46818/`). The product's unpadded uid format is surfaced as a
  reviewer observation (pre-existing, uniform across the five `*-mod-{:x}` sites,
  persisted identity — out of scope to change here). Failing artifacts retained
  (`…/20260719T005838Z-46611/diffs/`).
- **isolated dogfood** [EXECUTED]: OK; non-pollution BOTH halves PASS; state root cleaned.
- **live E2E (driver5 = driver4 full set + iteration-3 gates)** [EXECUTED — retained:
  `/private/tmp/m-r3a-resume-live/driver5.sh`, run `20260719T010200Z-47098/out/`]: ALL
  PASS — the complete §7/§8 regression set (unknown-before → labeled-after, doctor probe,
  check §5.3.1 invariance, explain no-suffix, map `pair_delta: 0` in-band + phantom-free
  render, posture non-contradiction, collision unit truth, registry untouched) PLUS the
  iteration-3 positives LIVE: modules list footnote "…— combined analyses (coverage:
  TypeScript (1 partition))." beside the untouched "4 unref?" column, and modules show
  "(reconciled — combined analyses; coverage: TypeScript (1 partition))" (module root
  ".", block labels JSON-verified before the render assertions).
- **g3u delta confirmation** [EXECUTED — `/private/tmp/m-r3a-amodx-g3u/`, run
  `20260719T010335Z-47294/out/`]: **`pair_delta: 11` reproduced exactly** (same 11 pairs,
  coverage TypeScript × 8 partitions; amodx HEAD `4230541` clean — the same-state pairing
  condition re-verified). One probe-race correction recorded (my probe, not the product):
  the first re-run raced the queued background enrichment — in-flight enrich = witness
  movement, so the daemon honestly omitted the block (never-stale, as ratified); the
  driver now waits for "op enrich completed" before warming. Failing artifacts retained
  (`…/20260719T010223Z-47191/out/`).
- **canonical smoke (final binaries; the standing 4-chunk pattern, task `m-r3a-i3`)**
  [EXECUTED]: `SMOKE_ONLY="…" ./scripts/smoke-validation-repos.sh m-r3a-i3 trust orient
  check` ×4 → **24/24 distinct repos passed, 0 failed** — `2026-07-19T01-04-27Z` (15:
  all 7 internal + buildroot leveldb mempalace nginx OpenXcom rabbitmq-tutorials
  spring-petclinic swupdate) · `…T01-06-23Z` (6: django duckdb grpc-java langchain4j poco
  sqlite — duckdb first-try on the fresh root, reconfirming §8.3's environmental
  diagnosis) · `…T01-08-51Z` (gstreamer kafka) · `…T01-12-29Z` (hadoop).
  **R-0/R-1 live absence**: `grep -rliE "witness|reconciled|combined-analyses|combined
  analyses"` over all four run dirs (100 capture files) → **zero matches** — no LG fed ⇒
  no block, no phantom rows, covered and uncovered repos alike, on the final binaries.
  **linux** [NOT RUN — deliberate, unchanged from §7/§8]: kernel-scale index exceeds the
  sweep client's read budget (ratified INDEX-DISCONNECT-1 semantics); not an M-R3a
  surface; the R-0 byte-parity gates carry the claim.
- **deterministic ordering** [EXECUTED via suites]: the daemon-side ordering gate tests
  ran within the 480 (the §7 witness_projection set, unchanged); iteration-3's only new
  aggregation (`modules_list` coverage phrases) is a `BTreeSet` join — deterministic by
  construction, pinned by the exact-string footnote assertions.
- **`git diff --check`** [EXECUTED]: clean (exit 0).
- **Install/deploy** [NOT RUN — by contract]: `dev-install-local.sh` only after reviewer
  approval. **Cleanup** (`clean-build.sh --all`) [NOT RUN — deliberate]: reviewer gate
  pending; re-running any gate above needs the build tree.

### 10.3 Iteration-3 verdict

Review-2's two required changes are CLOSED: (1) the §5.3.0 labeling gate is now ONE
shared function enforced at every union-value renderer — explain and both modules
renderers require `accounting: "union"` + a valid coverage basis and render the coverage
beside every reconciled value, with negative tests proving suppression (never an
unlabeled figure) and live E2E proving the positive frame on the changed surfaces;
(2) every §4 binding gate is EXECUTED against the exact final tree: fmt/clippy clean;
240 targets / 5,344 tests / 0 failed; witness 15/15; R-0 byte-parity 32/32; smoke 24/24
with 0-match witness grep (R-0/R-1 live); isolated dogfood PASS; live full-surface E2E
PASS; `pair_delta: 11` reconfirmed. Two FINDINGS this round, both discriminated to
harness/probe causes with artifacts retained and fixes recorded (the `<MODUID>` mask
width; the enrichment race in the g3u driver) — zero product findings against the M-R3a
diff. Reviewer gate pending; nothing committed; nothing installed.

## 6. DELIVERY (2026-07-19)

Delivered across 4 relay cycles (fable-5 builder, gpt-5.6-sol reviewer; 1 fuse-kill resume;
1 escalate) + operator close-out. Escalate resolutions (recorded in-packet): the trust posture
DTO amendment (ACTUAL RESIDENCY distinguished from COHERENT-SERVE ELIGIBILITY — the false
"not loaded" claim on a resident-but-cert-gated state eliminated) and the reader-set amendment
(modules_show, modules_list, map, storage_health added to the sanctioned list via the witness's
two-site reviewed flow). Review arc forced: the fingerprint/ledger atomicity fix (the W-B swap
race that could pair a pre-swap fingerprint with a pre-swap ledger), superseded-ledger failure
visibility, collision unit truth (instances vs identities), nonzero final-surface tests, no
unwrap_or(0) on witness counts, and the §5.3.0 coverage-label gate shared by every renderer.
Operator close-out (review-3 residue): `coverage_phrase` now requires the COMPLETE basis —
nonempty fingerprint + nonempty all-valid-string language/partition arrays — with a
six-mutation suppression test; 13/13 renderer tests, rgr 56 suites green, witness 15/15.

One shared projection (daemon `witness_projection/` + client `presentation/witnesses.rs`)
feeds trust witnesses block, doctor operational block, orient/stats g1u, modules g2u, map g3u
(measured pair_delta: 11 recorded), explain union-degree suffix. Three W-ONE reasons render
three distinct posture lines with next actions; ledger-absent renders last capture outcome +
failure reason; unknown never zero. M-R3a complete.
