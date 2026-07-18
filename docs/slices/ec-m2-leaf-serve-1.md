# EC-M2-LEAF-SERVE-1 — finish the (b)-leaves: MODULE_SUMMARY + cycle VALUES LG-serve on GREEN (EC-1 milestone M-2)

Status: SPECIFIED (2026-07-17) · Track: Consolidation milestones (EC-1 §5.2 M-2)
Depends: M-0 (done). Supersedes the parked `*-SQLITE-FREE-1` spec-first plans (D-EC-4 ratified) —
those docs stay valid as ANALYSIS (leaf inventories, cert designs), retired as plans.

## 1. Problem

orient/explain still serve MODULE_SUMMARY (+ trust + cycle values) from eager SQLite even on
GREEN with LG (b)-leaves live (EC-1 §3.3 orient row). The deferred P1 remainder: DR-2/DR-E3 —
the `module_stats` identity reconciliation for MODULE_SUMMARY structural counts (LG-derivable,
was ratified-SQLite pending exactly this cert) — and CYCLES-B (cycle VALUES LG-serve). The
ratified M-2 row decides the direction: Cat-2(ii) cache serves over SQLite-owned classes,
cert-witnessed, on GREEN only.

## 2. Contract (EC-1 §5.2 M-2 row, as ratified)

1. **orient + explain MODULE_SUMMARY LG-serve on GREEN:** structural counts (file/symbol/
   languages) serve from the LiveGraph cache when the coherence envelope is GREEN, with the
   DR-2/DR-E3 `module_stats` identity reconciliation cert proving LG-derived counts ==
   SQLite-derived counts per module (the stats `module_stats` compare pattern —
   orient-sqlite-free-1 §cert-4). Identity divergence = cert RED = SQLite serve (no silent
   drift; the RISK-E divergence concern is answered by the cert, not assumed away).
2. **Cycle VALUES LG-serve on GREEN (CYCLES-B):** same Cat-2(ii) posture, cert-witnessed.
3. **Envelope discipline:** GREEN-only; RED/YELLOW serve SQLite exactly as today. The
   OrientServeDecorator/CoherenceEnvelope mechanism is the existing seam — extend it; do not
   invent a parallel one.
4. **Explicitly NOT here (ratified):** a `resolved_calls` LG-serve — that leaf's terminal
   source is the M-3b persisted aggregate (ae6e7f8); the trust leaf stays SQLite-labeled.
   No FC ownership changes; this is serving-path only (Cat-2(ii) cache over SQLite-owned
   classes) — the ownership table (C-1) is untouched.
5. **The M-1 witness stays green**; any manifest edits explicit + reviewed.

## 3. Stop conditions

Frozen areas: W-B epoch/coordinator invariants, activity-registry semantics, enrich_pass
semantics, postpass/extractor walks. If the identity-reconciliation cert finds a REAL
divergence between LG and SQLite module counts on fixtures, that is a FINDING (evidence +
DECISION_REQUIRED) — do not paper over, do not "fix" the divergence inside this slice. Do NOT
commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

Per-leaf no-loss certs GREEN + byte-compare on the smoke fixtures (GREEN serve == SQLite serve
byte-identical; RED path unchanged) + `dogfood-isolated.sh` (RMAP_BIN override sanctioned);
chunked cargo gates (standing pattern); witness 15/15.

## 5. Definition of done

On GREEN, orient/explain MODULE_SUMMARY and cycle VALUES serve from the LG cache,
cert-witnessed byte-identical to the SQLite serve; on non-GREEN the SQLite path is untouched;
certs + byte-compare + dogfood + gates green. The EC-1 §3.3 orient/explain rows' "always
SQLite" caveat shrinks to the trust leaf (M-3b-persisted) alone.

## 6. DELIVERY (builder, 2026-07-18 — uncommitted working tree; reviewer inspects via `git diff`)

Mechanism (extends the existing `OrientServeDecorator`/`CoherenceEnvelope` seam — no parallel one):

- **The MODULE_SUMMARY identity-reconciliation cert** (`daemon-runtime/src/module_summary_cert/`,
  new sibling of `focus_resolution_cert`/`callgraph_cert`; slot on `RepoState`): GREEN iff the
  LiveGraph per-file structural inventory (`LiveGraph::structural_file_inventory`, new read-side
  leaf accessor) reconciles with SQLite at THREE granularities — per-file (path/AST-symbol-count/
  language), per-module dirname rollup (the ratified "LG counts == SQLite counts per module"),
  and the EXACT `compute_repo_summary` totals. SQLite half: `file_structural_rows` (queries.rs,
  cert-build read, once per fingerprint). ANY divergence ⇒ RED ⇒ SQLite serve (RISK-E answered
  by the cert; the known structural bound — tracked config/contract files have no LG presence —
  reconciles RED honestly rather than silently).
- **Cycle VALUES (CYCLES-B):** the cycles cert gains a `values_verdict` beside the shipped
  set-based `verdict` — GREEN iff the CANONICAL served agent shapes (repo SHORT-name + qualified
  lists, both via the agent's `canonicalize_cycles`) are byte-equal across stores. CYCLES-A's
  order-sensitivity blocker is dissolved by canonicalizing the agent's cycle output (members
  sorted within each cycle; list length-DESC then members — a pure function of the cycle set on
  BOTH engines) — the M-2-sanctioned wire change to the shipped cycle `modules` order.
  Name-vs-semantics correction landed with it: `ordering::sort_cycles` → `canonicalize_cycles`
  (the old doc claimed storage rotated cycles to smallest member — VERIFIED FALSE first-hand,
  queries.rs `find_cycles_cancellable` emits Tarjan traversal order).
- **Serving path:** `orient_serve_witness` (bounded fr∧cg eligibility + per-leaf M-2 peeks at ONE
  fingerprint, warm-then-peek) → `OrientServeDecorator::with_leaf_serves` serves
  `compute_{repo,path,file}_summary` from the inventory and `find_module_cycles*`/
  `find_cycles_involving_*` from the LG SCC (cancellable variants keep the DAEMON-CANCEL
  checkpoint); each leaf degrades independently; RED/absent ⇒ delegate byte-identically.
- **Envelope labels follow the ACTUAL serve:** orient MODULE_SUMMARY leaf + explain FILE/PATH
  identity leaf label `{livegraph, sqlite}` (multi-source — the discovery half stays SQLite) only
  when served this request, revalidated post-use-case by `epoch_still_resident` (a mid-request LG
  swap under-claims to sqlite, never over-claims — the callgraph-label conservative asymmetry).
  IMPORT_CYCLES leaf label unchanged (shipped cert-gated semantics).
- **NOT here (ratified):** `resolved_calls` stays M-3b-persisted, trust leaf SQLite-labeled; no
  FC ownership changes; witness manifest edit explicit (one line:
  `module_summary_cert/mod.rs = orient, explain`).

Validation (all EXECUTED, synchronous — full ledger in the relay report): shared faithful fixture
extended (file_versions + a REAL `src`↔`lib` cycle, both stores); certs GREEN on it, RED on an
injected divergence (named path); byte-parity GREEN-serve == SQLite-serve (repo + path focus,
non-empty cycle); no-eager-read spies; leaves-off byte-identity; label tests; workspace gates
3773/413/1038 pass 0 fail; witness 15/15; fmt/clippy clean; `dogfood-isolated.sh` PASS (no-LG
path unchanged, operator registry untouched). STOP-condition check: the only fixture divergence
observed was the DELIBERATE injected one (config file absent from LG — the documented structural
bound, cert RED ⇒ SQLite serve); NO real reconciliation divergence on the faithful fixture — no
FINDING to escalate.

## 7. REVISION 1 (review-0 verdict `revise`, builder 2026-07-18 — same uncommitted tree)

1. **Per-leaf decoupling (review-0 #1):** `orient_serve_witness` now returns a named
   `OrientServeWitness { fingerprint, bounded, m2 }` — the EV-A pin plus THREE independent leaf
   decisions peeked under one guard at the same fingerprint. The M-2 certs warm whenever a
   fingerprint is computable (previously skipped on a RED fr∧cg fold — the silent coupling); the
   decorator gained a `bounded` flag gating ONLY the six (b) focus/callgraph methods
   (`bounded_epoch_resident`); dispatch constructs the decorator on ANY green leaf and pins
   `serve_from_lg` (the callgraph-label gate) to `witness.bounded` — labelling it from
   `fingerprint.is_some()` would mint false `livegraph` callgraph provenance on an M-2-only serve.
   Proof: `m2_leaves_serve_independently_when_unrelated_bounded_cert_red` (seeded focus-resolution
   RED, callgraph + both M-2 certs GREEN): M-2 leaves serve over the panicking `M2Spy`
   byte-identical to SQLite; after a SQLite-only CALLS delete the (b) callers read demonstrably
   DELEGATES (no LiveGraph leak). Doc-truth sweep landed with it (7 stale "bounded-GREEN ∧" /
   "RED ⇒ bare SQLite" claims across dispatch/coherence/cert/test docs).
2. **SQLite-exact `LIKE` (review-0 #2):** `sqlite_like_match` now walks CHARACTERS (code points —
   SQLite's `Utf8Read` unit): `_` consumes exactly one char (`'aéb/x.ts' LIKE 'a_b/%'` = 1);
   `char::eq_ignore_ascii_case` is exactly SQLite's both-ASCII-only fold. Pinned two ways:
   the pure Unicode regression (`like_underscore_matches_one_unicode_character_not_one_byte`) and
   a GROUND-TRUTH pin against the REAL engine through the exact shipped `compute_path_summary`
   query on a 9-path × 5-prefix matrix with hand-derived expected counts
   (`like_prefix_matches_real_sqlite_like`) — parity AND absolute counts, so an identical bug on
   both sides cannot hide.
3. **Changed-surface GREEN coverage (review-0 #3):** orient FILE-focus parity + no-eager-read
   (`m2_parity_full_serve_equals_sqlite_file_focus`, over the M2Spy); explain FILE- and PATH-focus
   decorator-vs-SQLite parity through `run_explain` (`m2_parity_explain_{file,path}_focus_*`);
   NON-EMPTY explain cycle (the real `src`↔`lib` qualified ring asserted); explain no-eager-read
   for the newly served methods (`m2_no_eager_read_explain_file_and_path_serve_from_livegraph`,
   recording-spy flags FALSE for `compute_{file,path}_summary` + `find_cycles_involving_path`,
   TRUE for the listings — the DR-E3 honest bound re-asserted). The explain spy gained two cycle
   recorders; `M2Spy`/`PartialSpy` gained the three empty-default (c)-class delegates
   (`list_{module_sizes,directory_groups,manifest_roots}`) their "delegates everything else"
   contract claimed but lacked — surfaced by the new independence parity test (spy-only defect;
   the production decorator always forwarded them).

Trust posture unchanged in kind: no new certainty class; the newly-serving mixed states
(unrelated-cert-RED ∧ leaf-GREEN) are cert-witnessed byte-identical and their labels still follow
the ACTUAL serve with post-use-case `epoch_still_resident` revalidation (under-claim asymmetry
preserved). `resolved_calls`/trust leaf untouched; witness manifest unchanged (no new LiveGraph
reader file).
