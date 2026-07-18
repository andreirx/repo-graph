# RECON-M-R2-UNION-SERVING-1 — union serving for callers/callees in W-BOTH, flag-gated (reconciliation IMPL milestone M-R2)

Status: IMPLEMENTED (2026-07-18; working tree, uncommitted — reviewer gate pending; iteration 1
applied the post-escalate operator resolutions, §6.5; iteration 2 applied the review-1 §3.6
per-symbol-unanswerability fix, §6.6; iteration 3 applied the review-2 partition-level
eligibility fix for `Unavailable` anchors, §6.7) · Track:
Reconciliation IMPL (recon-design-1 §6.1, ratified §8)
Depends: M-R1 (done, c0e1dad — the ledger + amendment bad69da). Ordering: M-R2 ∥ M-R3a after M-R1.
Implementation record: §6 below.

## 1. Contract — the recon-design-1 §6.1 **M-R2 row IS the binding contract**, verbatim

Union serving for callers/callees in W-BOTH: the CAPTURE-CONTRACT flip
(ledger-validity-gated, verdict-independent — §4.2/§5.1; the named movement
`fallback_reason`; the flip RIDES THE SAME FLAG as union serving — the default path's
capture stays GREEN-gated byte-exact until the recorded default flip), the LG kind filter
(§3.4-3), union rows + `witness` fields (dual-measured only; `mixed` +
`occurrences: {confirmed, total}` on P-excess delta pairs; S-excess instances MINT
`semantic` rows — §3.3, iteration 6) + `witness_counts` incl. `unmeasured` (1:1 with rows,
§5.2), MAX multiplicity = row count (the preserved `count == rows.len()` contract), null-
not-zero locations (§3.7-4; §3.3a definition-location semantics), presentation accepts
unknown; replace the §3.7-3 row builder; ADD the pipeline-only test fixture (informed by
the amodx boundary + uncorroborated shapes).

**SHIPS FLAG-GATED, NON-DEFAULT, until S-1..S-3 (§6.2 — the monorepo field gates). The
default flip is its own recorded step, NOT this slice.** With the flag OFF (the default),
every served byte everywhere is byte-identical to today.

## 2. Gate — the M-R2 row's gate column, verbatim (highlights)

union ⊇ P verbatim (named test); R-0 + R-1 byte-parity (nginx/petclinic + zap-engine
mixed); count/MAX + ROW/COUNT INVARIANT (`count == rows.len()` across every fixture
class); DIVERGENT-CAPTURE (divergent fixture CAPTURES a fingerprint at M-R2 under the
flag and serves union in W-BOTH — the twin of M-R1's opposite test); EPOCH-MOVED
(fingerprint moved between capture and read → pipeline bytes at the pinned snapshot, NO
witness fields, the movement `fallback_reason`); CAPTURE-FAILED (ledger build error →
pipeline serve + doctor-reportable reason); DELTA-PAIR row tests (P=2/S=1 → both rows
`mixed` + {confirmed:1, total:2}, never `both`; P=1/S=2 → count 2, two rows: one P `both`
+ one S-minted `semantic`/`multiplicity`, closure + row multiset 1:1); STALE-serving
(pipeline bytes, no union fields — W-ONE); collision-withheld pairs NEVER serve (M-R1's
guard fixture through serving); W-B epoch tests (pin + eviction unchanged).

## 3. Stop conditions

Frozen: W-B epoch/coordinator invariants, activity-registry, enrich_pass, postpass/
extractor walks, the M-3a/M-3b persisted pipeline accounting, trust ratio denominator
(remains the pipeline-only floor — never inflated by union counts). The DEFAULT FLIP is
out of scope — flag-off byte-parity everywhere is part of the definition of done. A
baseline/parity mismatch is a FINDING. Do NOT commit. Consolidation witness green;
manifest edits explicit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

The §2 gate column in full; flag-OFF byte-parity proven on R-0 dogfood (nginx +
spring-petclinic) AND the self-index; chunked cargo gates; witness 15/15; isolated
dogfood.

## 5. Definition of done

Behind the flag: callers/callees in W-BOTH serve the union with instance-granular witness
provenance, honest unknowns, and the capture-contract flip; flag off: byte-identical
serving everywhere. All gate tests green. The default flip remains a separate recorded
step gated on S-1..S-3.

## 6. Implementation record (M-R2 build, 2026-07-18)

### 6.1 Shape (least-new-surface, recorded per the packet)

- **The flag**: `RMAP_RECON_UNION=1` (env; exactly `"1"` is ON — the smallest unambiguous
  contract; the `RMAP_CALLGRAPH_DIFF` gating pattern). Read per request in the two dispatch
  arms; it selects BOTH the capture function and the serve arm, so capture and serve cannot
  disagree within a request. Daemon-process-scoped (env), NON-DEFAULT.
- **The serve module**: NEW `daemon-runtime/src/union_serve/{mod,tests}.rs` beside
  `orient_serve`. Placement decision (architecture-boundary, recorded): `livegraph_feed` is the
  documented LOWEST serve module (depends on neither `callgraph_cert` nor `orient_serve`) and
  the union path must consume `callgraph_cert::ledger` types — extending `livegraph_feed`
  would invert that documented layering; `union_serve` depends downward on both, exactly as
  `orient_serve` does. Listed in the EC-M1 reader-set witness manifest ([production]
  `union_serve/mod.rs = callers, callees`; [test-scaffolding] `union_serve/tests.rs`) — the
  explicit manifest edit; the sanctioned-surface set is UNCHANGED (callers/callees were
  already sanctioned).
- **The capture flip**: `callgraph_cert::callgraph_union_eligibility` — warm via the SAME
  `callgraph_is_green` build (one store event writes cert + ledger), then peek a MEASURED
  ledger (`classification.is_some()` — a degenerate ledger licenses no union serve) at exactly
  the current fingerprint under ONE read guard (build-then-peek verbatim). Verdict-independent.
  The GREEN-gated `callgraph_cert_eligibility` is untouched — the default path and the
  bounded-orient cert keep their GREEN gate byte-exact (§5.1 "untouched" list).
- **The union projection source**: the WITNESS LEDGER's `CallClassification::pairs` at the
  pinned fingerprint — exact per-pair `(p, s_calls)`, dual-measurability, collision-withholding.
  The §3.4-3 kind filter is thereby satisfied AT the union call projection: the ledger's
  `s_calls` admits only strict-`Calls` IR edges from W-BOTH-eligible partitions, and withheld
  pairs are structurally absent. **Recorded decision: NO new kind-filter API on
  `repo-graph-livegraph`** — it would have zero consumers (the kind-blind traversal remains the
  byte-frozen cert-compare substrate; §3.7-1's fix is that union serving no longer flows
  through it).
- **Union assembly** (W-BOTH activated): P rows = the request's `find_direct_callers/callees`
  bytes VERBATIM (union ⊇ P; their `line`/`column` stay the endpoint DEFINITION locations,
  §3.3a), tagged per pair (`both` / `syntactic` / `mixed`+`occurrences{confirmed,total}` /
  no-field on unmeasured); S-minted rows (ONE mechanism for `new_pair` + S-excess
  `multiplicity`): `s−p` rows per pair, `symbol_context`-enriched name/file, NULL `line`/
  `column` (§3.7-4 retired on this path), `edge_type: "CALLS"` (honest under the
  kind-partitioned projection — the §3.7-3 row-builder replacement), `witness: "semantic"`.
  `count == rows.len()` = Σ max(p, s) per pair. Answer adds `witness_counts {both,
  semantic_only, syntactic_only, unmeasured}` (instances, 1:1 with rows) +
  `backend_used: "union"` (additive vocabulary value, flag-ON only — recorded).
- **The fallback ladder** (flag-ON, non-W-BOTH answers): mirrors today's `Auto` reduction in
  today's ORDER through the SAME `callers_auto_or_sqlite`/`callees_auto_or_sqlite` builders
  (made `pub(crate)`) — byte-identical fallback answers by construction. ONE renamed case:
  pin-moved-mid-request serves `fallback_reason: "LiveGraphEpochMoved"` (the §4.2 transient-1
  name — the old fold into `LiveGraphUnavailable` was false for a resident graph whose pin
  moved). **The movement reason is LOCAL to `union_serve`** (`UnionFallback::EpochMoved` +
  the module-owned string; iteration 1, COHERENCE-SCOPE resolution 2026-07-18): the shared
  `FallbackReason` enum and its cross-crate `CoherenceFallbackReason` mirror stay at today's
  variant set — the epoch-moved answer serves through the shared builder with the reason name
  written into the builder's always-present `fallback_reason` key (same key, same shape,
  byte-identical to a builder-carried reason). Capture-failed keeps `LiveGraphUnavailable`
  (the ledger genuinely is not available).
  **Iterations 2–3 (review-1 + review-2 fixes): only REGIME facts fall back — per-symbol
  unanswerability inside W-BOTH serves, BOTH classes (§3.6, the design's two-axes rule §4.2).**
  The `¬Exact → LiveGraphPartial` arm is NARROWED to regime causes (`missing_partitions`
  non-empty — W-ONE `not_resident` — or a non-TS language mix, the D4 scope; both carried
  today's arm-3 `Partial` reason, preserved). A `Fresh`, TS-only, fully-resident `Partial`
  projection (identity/answerability degradation only: structural file-scope nodes,
  fallback-identity endpoints, unresolved callees — the amodx-dominant unanswerable classes)
  SERVES the union. An `Unavailable`-class anchor (S cannot ground the symbol — not in the
  xref / no identity basis) carries NO regime evidence in its own envelope
  (`FreshnessState::Unavailable`, empty languages), so the regime is decided at the regime's
  OWN granularity: the anchor FILE's partition state via the existing
  `LiveGraph::file_partition_status` (iteration 3, review-2's discriminator — no new boundary).
  A file in a resident ∧ `Fresh` ∧ TS partition ⇒ the anchor lives INSIDE W-BOTH (a pipeline
  symbol S's producer did not emit — amodx measures 128 such projections on a fully-covered TS
  corpus) and the union SERVES; a file with no eligible partition (uncovered language /
  non-resident / stale / non-TS / no pipeline file coordinate) is a genuine W-ONE/W-NONE answer
  and keeps today's `LiveGraphUnavailable` fallback — the R-1 uncovered-answer shape (the
  ratified pipeline-only fixture's `rustFn` gate: `src/r.rs` is in NO resident partition;
  zap-engine's uncovered `.rs`/`.py` answers likewise, byte-identical). Served rows are labeled
  per the LEDGER's per-pair `dual_measured` at the pinned fingerprint — a pair measured from
  the OTHER endpoint's projection keeps its class; a pair neither projection measured carries
  NO witness field and counts `witness_counts.unmeasured` (unmeasured rows exist exactly where
  an anchor-touching projection is unanswerable: an `Exact` anchor's own projection
  dual-measures all its pairs). Every still-falling-back case keeps today's exact reason
  (arm-order parity; today's ladder maps every `Unavailable` anchor to `LiveGraphUnavailable`
  before its freshness check, so the file-level split changes serve-vs-fallback only, never a
  reason).
- **Transient-2 retention**: `RepoState.witness_ledger_build_failure`
  (`ledger::LedgerBuildFailure {fingerprint, reason}`) — set when the ledger build returns
  `None` (M-R1 contract: SQLite-error-only, so the reason is that class), cleared on every
  successful store. The doctor RENDERING lands with M-R3a (the M-R1 collision-rendering
  amendment's ownership pattern); this is the substance that makes it reportable.
- **Presentation accepts unknown** (`rgr` `graph_edges.rs`): `EdgeSymbol.file/line/column` →
  `Option` (null/absent parses); unknown renders honestly (`file` alone when the line is
  unknown — never an invented `:0`; `-` when the file is unknown); the legacy `""`/`0`
  placeholder rows keep rendering `:0` byte-identically (parity-pinned by test). Witness
  rendering is data-driven on `witness_counts` presence: ONE section line ("N confirmed by
  both analyses · … — call sites are syntax-detected") + compact per-row markers
  (`[both]` / `[compiler-only]` / `[syntax-only]` / `[both N/M]`); absent → byte-identical
  legacy render (R-0/R-1).
- **The ADDED pipeline-only fixture**: `test_fixture::build_pipeline_only_fixture()` — two S
  partitions, no S call edges; P CALLS shaped by the amodx evidence: one `boundary` pair
  (endpoints in p1/p2), two `uncorroborated` pairs (same-partition; endpoint-absent-from-S),
  one `unmeasured` uncovered `.rs` pair (zap's coverage-not-divergence lesson at fixture
  scale). Ledger-classified in `ledger_tests.rs`; driven through serving in
  `union_serve/tests.rs`.

### 6.2 Decide-and-record (one line each)

- Flag name/value: `RMAP_RECON_UNION` = exactly `"1"`; env-var precedent (`RMAP_CALLGRAPH_DIFF`).
- `backend_used: "union"` on W-BOTH-activated answers (both witnesses consulted); fallback
  answers keep `"sqlite"` + today's reasons — additive, flag-ON-only vocabulary.
- S-minted row `resolution: "livegraph"` (the shipped LG-row value — compat) and `kind: ""`
  (today's LG-row placeholder; the §3.7-4 defect named locations, not kind).
- Row labels use the SERVED row count (`served_p`) with the ledger's `s` — at a matched pin
  they equal the ledger's `p` (same `edges` CALLS multiset, same snapshot; basis: per-snapshot
  stable-key uniqueness via `:dupN`); served rows govern, so a claim can never outrun the
  served bytes (belt-and-suspenders).
- Ledger-pair scan per answer is O(union pairs) for callers (full map scan) — acceptable for
  the flag-gated non-default path; S-1's monorepo measurement prices any index before the
  default flip.
- Iteration 3: the `Unavailable`-anchor regime lookup reuses the EXISTING
  `LiveGraph::file_partition_status` (review-2's named discriminator — no new API/boundary);
  it is an O(resident IR nodes) scan, at most once per flag-ON `Auto` request and only on
  `Unavailable`-class anchors — the same cost class as the ledger-pair scan above, priced by
  the same S-1 gate before any default flip.
- The engine parse moved ABOVE the epoch capture in the two dispatch arms (pure param parsing,
  no side effects) so the capture fn is chosen per arm; flag-ON + `--engine sqlite/livegraph/
  compare` keep today's exact behavior (escape hatches; union rides `Auto` only).
- ~~`CoherenceFallbackReason::LiveGraphEpochMoved` added to the pure mirror crate~~ **REVERTED
  (iteration 1)** — COHERENCE-SCOPE operator resolution (2026-07-18, post-escalate): a
  union-only enum value with zero coherence producers is dormant capability on a frozen
  cross-crate shape. Both coherence edits (`orient_coherence.rs`,
  `repo-graph-coherence/src/lib.rs`) reverted to HEAD; the movement reason lives in
  `union_serve`'s own types (`UnionFallback::EpochMoved`). Orient/explain posture rendering of
  the transient remains M-R3a's, now WITHOUT a pre-widened mirror.
- `union_serve` module boundary **RATIFIED** by the operator (2026-07-18, post-escalate
  UNION-MODULE-BOUNDARY): two concrete current users (callers + callees arms), mirrors the
  `orient_serve` module pattern, preserves the `livegraph_feed` dependency direction, keeps
  policy out of `dispatch.rs`. Abstraction ledger satisfied; manifest edit stands.
- ~~The collision fixture's answers take today's fallback (envelope `Partial`); the withheld
  pair is barred by TWO reinforcing mechanisms.~~ **Superseded (iteration 2, review-1 fix):**
  a per-symbol `Partial` now SERVES the union (§3.6), so the STRUCTURAL barrier carries alone —
  the ledger's collision-excluded `s_calls` is the assembly's ONLY S source (M-R1-proven), and
  the withheld pair's P row serves UNMEASURED (every projection touching the fallback endpoint
  is unanswerable → `dual_measured: false` → no witness claim). Still asserted through serving
  in both directions: no `both`, no `semantic`, no minted row, `witness_counts {0,0,0,1}`.
- CAPTURE-FAILED validation is composed from its two halves (capture: corrupted-db build →
  `None` + retained failure; serve: `fingerprint: None` → pipeline + today's reason) — full
  fault injection (a storage error with a healthy concurrent pipeline read) is not
  constructible in the unit harness; the composition covers both observable behaviors.
- Witness manifest edit (explicit): `union_serve/mod.rs = callers, callees` [production] +
  `union_serve/tests.rs` [test-scaffolding]; sanctioned-surface set unchanged; witness 15/15.

### 6.3 Live end-to-end record (EXECUTED 2026-07-18, isolated)

Isolated long-lived daemons (unix socket under `/private/tmp/m-r2-live`, throwaway state
root, dev-pinned `scip-typescript-node18` 0.4.0), TS fixture with a real call graph, LG fed
via `rmap dev livegraph-refresh --all-discovered` (partitions `Refreshed`, producer ran):

- **Flag OFF** (daemon 1): `callers computeScore --json` → `backend_used: "sqlite"`,
  `fallback_reason: "LiveGraphUnavailable"` — the GREEN gate refuses the divergent capture;
  today's exact bytes (M-R1 behavior preserved with a RESIDENT enriched LG).
- **Flag ON** (daemon 2, same state root): `backend_used: "union"`, the P row VERBATIM
  (file/line/resolution identical) + `witness: "both"`, `witness_counts {both:1,…}`,
  `count == rows.len()`; callees symmetric (`both` ×2); human render shows the §5.2 section
  line + `[both]` markers.

### 6.4 Validation summary (details in the relay TEST REPORT)

Workspace cargo gates green (fmt --check, clippy --workspace --all-targets, test --workspace:
240 suites, 0 failures — incl. the 12 new union_serve gate tests, the pipeline-only ledger
test, 11 graph_edges presentation tests, all M-R1 suites); consolidation witness 15/15 (the
manifest edit accepted; test-gating verified); isolated dogfood PASS; flag-off byte-parity
PROVEN old-binary-vs-new (HEAD worktree build) AND flag-ON R-0 inertness: 16/16 byte-identical
comparisons across nginx (C), spring-petclinic (Java), the TS fixture, and the repo-graph
self-index (callers/callees/orient, JSON + human, isolated state roots, operator registry
untouched). NOT RUN: the amodx/zap-engine retained-corpus serving replays (the M-R1 ledger
baselines those corpora pin are unchanged by this slice — serving reads the ledger; the
fixture classes cover every serving path) and the deployment-monorepo run (S-1..S-3 — the
default-flip gate, explicitly out of scope).

### 6.5 Iteration 1 (2026-07-18, post-escalate resolutions applied)

Review-0 verdict was `escalate`; the operator resolved both DECISIONs and mandated canonical
smoke validation (selection-packet OPERATOR_NOTE, 2026-07-18). Applied:

1. **UNION-MODULE-BOUNDARY — RATIFIED, no code change.** `daemon-runtime::union_serve` stands
   (recorded in §6.1/§6.2 and the module header).
2. **COHERENCE-SCOPE — REJECTED → localized.** Both coherence edits REVERTED to HEAD
   (`orient_coherence.rs`, `repo-graph-coherence/src/lib.rs`); `FallbackReason::
   LiveGraphEpochMoved` REMOVED from the shared daemon enum; the movement reason now lives in
   `union_serve`'s own types (`UnionFallback::EpochMoved` + the module-owned
   `EPOCH_MOVED_REASON` string, written into the shared builder's always-present
   `fallback_reason` key — same served bytes, no cross-crate shape change). All 12 union gate
   tests (incl. EPOCH-MOVED pinning the served string) pass unchanged.
3. **Canonical smoke protocol EXECUTED** (scripts only; all artifacts script-generated):
   - New-tree runs (this repo's `smoke-runs/`): `2026-07-18T17-30-36Z` (self, 6 cmds) ·
     `2026-07-18T17-31-23Z` (nginx) · `2026-07-18T17-31-40Z` (spring-petclinic) ·
     `2026-07-18T17-32-06Z` (validation-repos sweep ×3 repos, trust/orient/check, 3/3 pass) ·
     `2026-07-18T17-32-38Z` / `…T17-32-55Z` / `…T17-33-16Z` (the three `RMAP_RECON_UNION=1`
     flag-ON legs).
   - Baseline runs (worktree at HEAD `9fd2130` — pre-M-R2 code; retained at
     `../m-r2-baseline-worktree/smoke-runs/`): `2026-07-18T17-25-51Z` (nginx) ·
     `…T17-27-35Z` (petclinic) · `…T17-28-53Z` (sweep) · `…T17-29-14Z` (self, indexing THIS
     working tree by absolute path — identical source bytes on both sides).
   - **Flag-OFF byte-parity: 24/24 captures MATCH** (old-binary vs new-binary; callers/callees
     human+JSON + orient + trust on nginx / spring-petclinic / self-index, plus
     trust/orient/check on nginx + petclinic via the sweep pair).
     **Flag-ON R-0 inertness: 16/16 captures MATCH** (flag-ON vs flag-OFF, same new binaries).
     Comparison masks exactly the run-varying token classes (the §4.2 end-of-slice convention):
     cargo `Running/Finished` wrapper lines, per-run repo ULIDs, snapshot ids/timestamps,
     state-root task paths, latency decorations; observed raw variance was only the wrapper
     line + the ULIDs. Zero parity findings.
4. **Re-validated after the localization:** fmt --check clean; clippy `-D warnings` clean;
   daemon-runtime 377 lib + all integration targets green; rgr all targets green; FULL
   workspace suite 240 targets, 0 failures; consolidation witness 15/15; isolated dogfood
   PASS (operator registry proven untouched).

### 6.6 Iteration 2 (2026-07-18, review-1 fix: per-symbol unanswerability inside W-BOTH)

Review-1 verdict was `revise`: the `union_outcome` ladder treated every non-`Exact` projection
as a regime fallback, conflating §4.2's two axes — so a per-symbol unanswerable projection
inside an eligible W-BOTH regime could never serve, and `witness_counts.unmeasured` was
structurally unreachable on union answers (an `Exact` anchor's own projection dual-measures all
its pairs via the ledger's `measured_pair` `cm`/`em` disjunction — unmeasured rows exist exactly
where the anchor's projection is unanswerable). Applied:

1. **The ladder split (`union_serve::union_outcome`).** The `¬Exact → LiveGraphPartial` arm is
   narrowed to REGIME causes (`missing_partitions` non-empty ∨ ¬TS-only); a `Fresh` ∧ TS-only ∧
   fully-resident `Partial` (per-symbol identity/answerability degradation only) serves the
   union with ledger-driven per-pair labels — §3.6-i/ii verbatim: measurable-side facts serve,
   no witness claim where no projection measured, `unmeasured` counted so the composition never
   hides. ~~`class Unavailable` (anchor unknown to S's world) KEEPS today's fallback — the R-1
   uncovered-answer shape, pinned by the ratified pipeline-only fixture's `rustFn` gate and the
   zap-engine R-1 byte-identity requirement (a blanket serve would break both; recorded
   interpretation: "per-symbol unanswerability INSIDE W-BOTH" requires an anchor the S witness
   knows — `Partial`-class projections; `Unavailable`-class anchors have no LG-derivable
   regime).~~ **Superseded (iteration 3, review-2 fix — §6.7):** the interpretation conflated
   "no LG-derivable regime FROM THE ENVELOPE" with "no regime at all". An `Unavailable` anchor
   in a Fresh/resident/TS FILE belongs to W-BOTH (the regime lives at partition granularity —
   `file_partition_status` supplies it) and SERVES; only a file with no eligible partition is
   the R-1 uncovered shape (which is what actually pins `rustFn` and zap's `.rs` answers —
   their FILES, not their class). Reason parity for every still-falling-back case is preserved
   by keeping today's arm order (`Unavailable` → `Stale` → `Partial` → `UnsupportedLanguage`).
2. **The required serving test** (`partial_projection_inside_w_both_serves_union_with_unmeasured_counts`)
   + fixture (`build_partial_unanswerable_fixture`): a Fresh/resident eligible TS partition
   whose anchor projection is `Partial` (fallback-identity endpoint), serving a MIXED answer —
   one pair measured from the OTHER endpoint's `Exact` projection (`syntactic`), one measured by
   neither (unmeasured, no witness field) — asserting the Partial precondition, union ⊇ P
   verbatim, nonzero `unmeasured`, no false row witness, and the four counts summing to
   `rows.len()`; both directions. The collision gate test updated to the new barrier structure:
   the withheld pair serves NOTHING through the union path itself (the ledger's
   collision-excluded `s_calls` is the assembly's only S source — the sole, structural barrier).
3. **Presentation fixture made contract-valid with nonzero `unmeasured`**
   (`witness_counts_render_section_line_and_row_markers`): counts {1, 0, 1, 1} 1:1 with a
   3-row multiset (a `mixed` pair's two rows + one witness-less unmeasured row), asserting the
   section line renders "1 not measured by the compiler" and the unmeasured row carries no
   marker. (The prior fixture declared `unmeasured: 0` beside a witness-less row — the §5.2
   1:1 contract violation review-1 named.)
4. **The zap-engine mixed-repo R-1 serving replay** executed (review-1 requirement 3), isolated
   two-daemon pattern (§6.3's), throwaway state root, dev-pinned producer: uncovered-partition
   answers (`world_to_tile`, `.rs`) BYTE-IDENTICAL flag-ON vs flag-OFF (3/3 normalized captures
   MATCH) and witness-free; the covered TS answer (`computeProjection`) activates union under
   the flag (`backend_used: "union"`, 3 `both` P rows + 3 S-minted `semantic` rows with null
   locations, counts 1:1, §5.2 section line + markers in the human render) while flag-OFF keeps
   today's GREEN-gate refusal (`sqlite` + `LiveGraphUnavailable`). Driver + captures:
   `/private/tmp/m-r2-i2-eval/{zap-replay.sh, zap-out/}`.
5. **Canonical smoke re-run (iteration 2)**: new-tree flag-OFF `2026-07-18T18-{46-28,46-47,47-03}Z`
   (self/nginx/petclinic) + flag-ON `…T18-47-{16,34,49}Z` + sweep `…T18-48-30Z` (3/3);
   baseline-worktree self re-capture `…T18-42-15Z` (the self-index subject bytes changed this
   iteration, so the old-binary side was re-captured against the current tree — iteration-1
   methodology; nginx/petclinic compare against the retained `…T17-{25-51,27-35}Z` baselines).
   **Flag-OFF byte-parity 18/18 MATCH; flag-ON R-0 inertness 18/18 MATCH** (the §4.2 masked
   token classes; zero findings). Workspace cargo gates green (fmt/clippy/240 targets ×2 runs,
   0 failures; daemon-runtime 378 lib incl. the 13 union gates; witness 15/15); isolated
   dogfood PASS.

### 6.7 Iteration 3 (2026-07-18, review-2 fix: partition-level eligibility for `Unavailable` anchors)

Review-2 verdict was `revise`: the iteration-2 ladder still short-circuited EVERY
`Unavailable`-class anchor to fallback, conflating per-symbol answerability with the
partition-level regime (§4.2 defines W-BOTH eligibility from PARTITION state; §3.6 names
`Partial` AND `Unavailable` as per-symbol unanswerable classes whose measurable-side facts
still serve). A pipeline symbol absent from the S xref but located in a Fresh, resident TS
partition was incorrectly served as SQLite without union provenance. Applied:

1. **The ladder split completed (`union_serve::union_outcome`).** An `Unavailable`-class
   anchor's envelope carries no regime evidence (`FreshnessState::Unavailable`, empty
   languages — the xref-absence construction site), so the regime is now read at its OWN
   granularity: the anchor FILE's partition state via the existing
   `LiveGraph::file_partition_status` (review-2's named discriminator — an existing API, no
   new boundary; same read guard as the envelope + ledger reads, EV-A discipline unchanged).
   Eligible file (resident ∧ `Fresh` ∧ TS) ⇒ serve the union, rows labeled by the ledger's
   per-pair `dual_measured` (such an anchor's pairs are unmeasured unless the OTHER endpoint's
   projection measured them). No eligible partition (uncovered language / non-resident /
   stale / non-TS / no pipeline file coordinate) ⇒ genuine W-ONE/W-NONE: today's exact bytes +
   today's `LiveGraphUnavailable` reason (R-1 parity — today's ladder maps every `Unavailable`
   anchor to that reason before its freshness check, so the split changes serve-vs-fallback
   only, never a reason string). Grounded anchors keep the iteration-2 ladder unchanged.
2. **The required serving test** (`unavailable_anchor_in_eligible_partition_serves_union_with_
   unmeasured_counts`) + fixture (`build_unavailable_in_w_both_fixture`): a P-only anchor
   (`ghostFn`) in a Fresh resident TS file, absent from the S xref, with per direction one
   dual-measured pair (measured from the OTHER endpoint's Exact projection → `syntactic`) and
   one pair no projection measured (unmeasured, no witness field) — asserting the Unavailable
   precondition + file eligibility, union ⊇ P verbatim, dual-measured rows retain their class,
   nonzero `witness_counts.unmeasured`, and the four counts summing to `rows.len()`; BOTH
   directions on the same anchor. The uncovered-Rust fallback test retained separately
   (`rustFn`'s file is in NO resident partition — the genuine W-NONE shape). Fixture ledger
   mechanics documented on the builder; distinct builder rather than extending the ratified
   pipeline-only fixture (any ghost edge would shift its amodx-informed gate assertions).
3. **The conflicting interpretation removed** from the `union_serve` module docs + ladder docs
   and §§6.1/6.6 (superseded-with-record, this doc's supersession pattern): "per-symbol
   unanswerability INSIDE W-BOTH requires an anchor the S witness knows" was wrong — what pins
   `rustFn` and zap's uncovered `.rs` answers to fallback is their FILES' partition state, not
   their answer class.
4. **Full validation re-run** (iteration 3, all EXECUTED): union gate suite **14/14** (13 prior
   unchanged + the new both-classes gate); zap-engine mixed R-1 replay **PASS** (retained driver
   `/private/tmp/m-r2-i2-eval/zap-replay.sh`: uncovered `.rs` 3/3 normalized captures MATCH
   flag-ON vs flag-OFF and witness-free — `world_to_tile` now traverses the NEW file-level
   discriminator and produces today's exact `sqlite`+`LiveGraphUnavailable` bytes; covered TS
   `computeProjection` reproduces iteration-2's exact union shape, 6 rows = 3 `both` + 3
   `semantic`, counts 1:1). Canonical smoke (iteration 3): new-tree flag-OFF
   `2026-07-18T19-44-{11,30,45}Z` (self/nginx/petclinic) + flag-ON `…T19-{44-57,45-14,45-30}Z`
   + sweep `…T19-45-43Z` (3/3); baseline-worktree self re-capture `…T19-43-50Z` (subject bytes
   changed this iteration; nginx/petclinic/sweep against the retained
   `…T17-{25-51,27-35,28-53}Z` baselines). **Flag-OFF byte-parity 24/24 MATCH (18 pair-runs +
   6 sweep); flag-ON R-0 inertness 18/18 MATCH** (the §4.2 masked token classes; zero
   findings). fmt/clippy clean; daemon-runtime 379 lib + all integration targets; rgr 57
   targets; full workspace 240 targets, 0 failed; consolidation witness 15/15 (no manifest
   edit this iteration); isolated dogfood PASS (operator registry proven untouched).
