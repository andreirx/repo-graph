# SCIP-UNRESOLVED-CALL-PROBE-1: does SCIP carry unresolved calls, and can it reach parity with `unresolved_edges`?

Slice ID: SCIP-UNRESOLVED-CALL-PROBE-1
Type: **INVESTIGATIVE SPIKE** (execution + empirical evidence required; NOT a spec). Produces exactly one
tracked deliverable: this report. NO production code change, NO migration, NO deletion of tracked files.
Track: Stage D / SQLite-raw decommission — Option B producer program. The S1 feasibility PROBE gating
TRUST-SUMMARY-LIVEGRAPH-1 (operator-ratified DR-TS-0 → S1, 2026-06-13).
Date: 2026-06-13.
Revision: **rev-1 (2026-06-13)** — addresses `review-0` (verdict `revise`). The blocking objection was that Q2
compared TWO corpora (a `/tmp` sample for the SCIP side, the committed `.repo-graph.db` for the SQLite side),
so the "parity" claim was cross-corpus, not paired. **This revision runs BOTH producers on the SAME scratch
sample and reports the paired counts** (§4.2–§4.5); the large `.repo-graph.db` breakdown is retained only as
explicitly-labelled SUPPORTING context on a DIFFERENT corpus (§4.6). Evidence labels corrected: paired counts
are `OBSERVED`; the irreconcilability JUDGMENT is `INFERRED over OBSERVED` (§4.5, §5).
Baseline: `docs/slices/trust-summary-livegraph-1.md` (VERDICT `NEEDS-EXTENSION`; the §4 MISSING-1/MISSING-2
payload; the DR-TS-0 → S1 sequencing decision routing the load-bearing unknown to THIS probe).

Resolution: **DR-TS-0-POST-PROBE ratified by operator (2026-06-13): Option A (accept the honest hybrid).** The
NO-GO is accepted. The trust summary's unresolved-call fields stay homegrown-`unresolved_edges`-sourced, served
SQLite-LABELLED (the TRUST-LIVEGRAPH-1 Half-B shape); the `edges`/`unresolved_edges` deletion gate for those
fields stays **RED BY DESIGN** (no current-state SCIP source — VISION Fact-Certainty Model). The DR-TS-1 A
"extension populated from SCIP" producer line is **CLOSED**; orient DR-1 / explain DR-E1 (the shared producer
they were gated on) are **refuted**. S4 (a redefined SCIP-native metric) was NOT taken (no parity; would require
a new metric contract + consumer threshold migration).

---

## 0. HEADLINE VERDICT — `NO-GO` (decisive; both questions empirical; Q2 now PAIRED on one corpus)

> **Q1 (MISSING-1) — Does scip-typescript EMIT unresolved call occurrences? → NO. [OBSERVED, first-hand.]**
> When a TS call target cannot be bound to a definition (method on an `any`-typed / dynamic / untyped
> receiver), scip-typescript@0.4.0 emits **NO occurrence for that call target** — no occurrence referencing any
> symbol, no `local`/synthetic symbol, no undefined-definition symbol, no symbol role, no marker. The
> unresolved call is simply **ABSENT** from the SCIP graph. The only occurrences near such a call site are for
> the parts the typechecker CAN bind (the receiver variable/parameter). There is therefore **no current-state
> SCIP source from which to COUNT unresolved calls.**
>
> **Q2 (MISSING-2) — Does an SCIP-derived unresolved-call count reach PARITY with SQLite `unresolved_edges`?
> → NO.**
> **Paired on the IDENTICAL corpus** (one scratch TS sample, both producers run THIS revision):
> SCIP-recoverable unresolved-call signal = **0**, homegrown SQLite `unresolved_edges` (CALLS) = **3**.
> **`0 ≠ 3` → no count-parity on the same corpus. [OBSERVED — arithmetic on directly observed counts; §4.5.]**
> The divergence is **structural, not a classification gap**: at the 3 sites the homegrown extractor calls
> "unresolved," SCIP either emits *nothing* (`p.mysteryMethod`, `externalThing.run` — uncountable) or emits a
> *resolved reference* to a bound parameter (`fn` — the OPPOSITE disposition). "Unresolved" denotes inverted
> facts under the two producers (**RISK-T-D, demonstrated in-sample**). No classification pass reconciles a
> present count of 3 against a structural absence/inversion. **[Irreconcilability: INFERRED over OBSERVED.]**
>
> **GO/NO-GO = `NO-GO`.** GO required Q1 YES *and* Q2 parity-achievable. Both fail. The IR extension framed as
> DR-TS-1 A (a `CallObservation` analogue **populated from SCIP**, yielding a no-loss current-state
> unresolved-call count) is **not feasible**: SCIP does not carry the fact (Q1), and the recoverable notion
> measures something different and cannot reach the homegrown count on the same corpus (Q2).
> **Recommendation: reconsider Option A (DR-TS-0 S3) — keep the homegrown `unresolved_edges` as the
> SQLite-labelled trust input — or the hybrid S4 (a REDEFINED current-state metric, explicitly not byte-parity
> with the old count).** The Option-A-vs-S4 choice is a governance call surfaced as `DECISION_REQUIRED` (§7).

This probe RESOLVES the single unproven fact the whole TRUST-SUMMARY-LIVEGRAPH-1 → producer → fastpath path
hinged on (`trust-summary-livegraph-1.md` §0 headline; §4 MISSING-1 "an OPEN PROBE … not assumable"). It is
resolved against the producer (Q1) and against paired parity (Q2).

---

## 1. Evidence labels (repo Evidence Law; `agent_docs/validation.md`)

- **EXECUTED** — a command I ran THIS probe with output observed directly (cited with the command + a real
  output excerpt).
- **OBSERVED** — an artifact (file / decoded index / DB row) inspected first-hand THIS probe. **Includes direct
  arithmetic on observed counts** (e.g. `0 ≠ 3`).
- **INFERRED** — my judgment over EXECUTED/OBSERVED facts. **`INFERRED over OBSERVED`** is used where an
  interpretation (the class taxonomy, the *irreconcilability* conclusion, the verdict, the recommendation) is
  built on top of observed facts but does not itself follow as raw arithmetic.
- **NOT RUN** — skipped, with the reason stated.

Per `review-0` required-change #3: semantic reconciliation / irreconcilability is **never** labelled `OBSERVED`.
Only commands, artifacts, and direct count arithmetic are `OBSERVED`; the parity *judgment* is
`INFERRED over OBSERVED`.

---

## 2. Method, tooling, and evidence basis

### 2.1 Tool availability (the gating infra check) — EXECUTED / OBSERVED

```text
$ which rmap rmapd scip-typescript scip node sqlite3
/Users/apple/.local/bin/rmap                               # rmap 0.2.1
/Users/apple/.local/bin/rmapd                              # rmapd 0.2.1 (daemon)
scip-typescript not found                                  # not on PATH; provisioned in /tmp (below)
scip not found
/Users/apple/.nvm/versions/node/v22.21.1/bin/node          # Node v22.21.1
/usr/bin/sqlite3
```

`scip-typescript` and the `scip` CLI are NOT pre-installed on PATH. The producer is provisioned in scratch
(`/tmp/scip-probe/node_modules/@sourcegraph/scip-typescript@0.4.0`) — the SAME pinned producer the repo's
LIVEGRAPH-INTEGRATION-1C uses (`CURRENT_SLICE.md`: "pinned `scip-typescript@0.4.0`"). It ran on Node v22.21.1
for these small samples (exit 0); **PRODUCER-COMPAT-1 (`0.4.0 ⊥ Node22`) did NOT reproduce at this scale** — I
do NOT generalize that to large/multi-tsconfig repos (§8).

**Daemon:** `rmap index` requires the daemon. `rmap doctor` showed the launchd service `loaded but not running`
(socket absent), so I started `rmapd` directly; `rmap doctor` then reported `socket_ping: pong received`. The
daemon is daemon-runtime state outside the tracked tree (state root
`~/Library/Application Support/repo-graph/`), so starting it touches NO tracked repo file (§8, §10).

### 2.2 The decode path — same crate as the repo's authoritative decoder — OBSERVED

The repo decodes SCIP via `decode_index(bytes) = Index::parse_from_bytes(bytes)` using the `scip` crate 0.7.1
(`rust/crates/repo-graph-scip-ingest/src/lib.rs:30-37`; `Cargo.toml:11`). My scratch decoder
(`/tmp/scip-probe/decoder`) uses the **identical** `scip = "0.7.1"` + `protobuf = "3.7"` and the identical
`Index::parse_from_bytes` entrypoint. I cross-checked it against the committed real producer output before
trusting it on the new sample (§3.5).

### 2.3 What was EXECUTED (full inventory, THIS revision)

| # | Command (abridged) | Purpose | Result |
|---|---|---|---|
| E1 | `scip-typescript --version` | producer runnable on Node 22? | `0.4.0`, exit 0 |
| E2 | `scip-typescript index --output index.scip` (sample) | produce real SCIP of the known-disposition sample | exit 0, 4387 B |
| E3 | `cargo run … decoder index.scip` | dump every occurrence (range/roles/local-global/symbol) | occ=29 (§3.2, §4.2) |
| E4 | `cargo run … decoder synthetic/index.scip` (committed fixture) | decoder correctness + resolved contrast | occ=34 (§3.5) |
| E5 | `cargo run -p repo-graph-scip-ingest --example edge_probe -- …` | repo's REAL ingest on the sample SCIP | `syntax_confirmed_calls=2`, `ts_extractor_call_sites=5` (§3.6) |
| E6 | `scip-typescript index` (sample, `strict:true`) + decode | rule out a strict-mode difference | identical (§3.4) |
| **E7** | **`rmap index /tmp/scip-probe/sample --alias scip-probe-sample`** | **homegrown index of the SAME sample (paired Q2)** | **`2 files, 11 nodes, 5 edges (3 unresolved)`** (§4.3) |
| **E8** | **`sqlite3 <sample-db> "… edges / unresolved_edges …"`** | **homegrown CALLS rows for the SAME sample** | **resolved CALLS=2, unresolved CALLS=3** (§4.3) |
| E9 | `sqlite3 .repo-graph.db "… unresolved_edges …"` | SUPPORTING (different corpus) breakdown at scale | §4.6 |

Scratch lives entirely under `/tmp/scip-probe/`, `rust/target/` (gitignored), and the daemon state root
(`~/Library/Application Support/repo-graph/databases/`, outside the tracked tree). The tracked tree was clean
before and after (`git status --short` → only this report). See §10.

---

## 3. Q1 (MISSING-1) — does scip-typescript emit unresolved call occurrences? — ANSWER: NO

### 3.1 The probe sample (KNOWN unresolved + resolved calls) — OBSERVED

`/tmp/scip-probe/sample/src/main.ts` (with `src/resolved.ts` exporting `helper`). Each call site is labelled
with its EXPECTED disposition:

```ts
import { helper } from "./resolved";
function inner(): number { return 1; }
export function resolvedLocal(): number { return inner(); }            // L8  RESOLVED same-file
export function resolvedCrossFile(): number { return helper(2); }      // L13 RESOLVED cross-file
export function resolvedExternal(): number {
  console.log("x");                                                    // L18 RESOLVED external (stdlib)
  return Math.floor(3.7);                                              // L19 RESOLVED external (stdlib)
}
export function unresolvedAnyMethod(p: any): number {
  return p.mysteryMethod();                                            // L25 UNRESOLVED (method on any)
}
declare const externalThing: any;
export function unresolvedDynamic(): void { externalThing.run(); }     // L31 UNRESOLVED (dynamic dispatch)
export function unresolvedBareCall(fn: any): void { fn(); }            // L36 UNRESOLVED (any-typed callee)
```

### 3.2 The decoded occurrences — EXECUTED (the decisive Q1 evidence)

Decoder output for `index.scip`, `src/main.ts` (verbatim excerpt; `roles=(none)` = a non-definition reference;
`L`/`C` are 1-based line / 0-based col):

```text
--- DOCUMENT src/main.ts (symbols=11, occurrences=25) ---
  L 8:C9   roles=(none)       GLOBAL sym=… src/`main.ts`/inner().                       <- L8  inner()        RESOLVED in-partition
  L13:C9   roles=(none)       GLOBAL sym=… src/`resolved.ts`/helper().                  <- L13 helper(2)      RESOLVED cross-file
  L18:C2   roles=(none)       GLOBAL sym=… npm typescript 5.9.3 lib/`lib.dom.d.ts`/console.        <- L18 console
  L18:C10  roles=(none)       GLOBAL sym=… npm typescript 5.9.3 lib/`lib.dom.d.ts`/Console#log().  <- L18 .log()  RESOLVED external
  L19:C9   roles=(none)       GLOBAL sym=… npm typescript 5.9.3 lib/`lib.es5.d.ts`/Math#           <- L19 Math
  L19:C14  roles=(none)       GLOBAL sym=… npm typescript 5.9.3 lib/`lib.es5.d.ts`/Math#floor().   <- L19 .floor() RESOLVED external
  L24:C16  roles=Definition   GLOBAL sym=… src/`main.ts`/unresolvedAnyMethod().
  L24:C36  roles=Definition   GLOBAL sym=… src/`main.ts`/unresolvedAnyMethod().(p)
  L25:C9   roles=(none)       GLOBAL sym=… src/`main.ts`/unresolvedAnyMethod().(p)      <- L25 the RECEIVER `p` only
  ## (no occurrence at L25:C11 for `.mysteryMethod`)                                      <- UNRESOLVED TARGET: ABSENT
  L30:C16  roles=Definition   GLOBAL sym=… src/`main.ts`/unresolvedDynamic().
  L31:C2   roles=(none)       GLOBAL sym=… src/`main.ts`/externalThing.                 <- L31 the RECEIVER `externalThing` only
  ## (no occurrence at L31:C16 for `.run`)                                                <- UNRESOLVED TARGET: ABSENT
  L35:C16  roles=Definition   GLOBAL sym=… src/`main.ts`/unresolvedBareCall().
  L35:C35  roles=Definition   GLOBAL sym=… src/`main.ts`/unresolvedBareCall().(fn)
  L36:C2   roles=(none)       GLOBAL sym=… src/`main.ts`/unresolvedBareCall().(fn)      <- L36 a RESOLVED reference to the `fn` PARAM (no call marker)
== TOTALS occ=29 definitions=14 references=15 ==
```

**OBSERVED, decisive:** at the `any`-member call sites (L25, L31) the ONLY occurrence is for the bindable
receiver (`p`, `externalThing`); there is **no occurrence whatsoever for the unresolved call target**
(`mysteryMethod`, `run`). At the bare-call site (L36) the only occurrence is a **resolved reference to the `fn`
parameter** — SCIP binds `fn` to its parameter symbol; it carries no call semantics and no unresolved marker.
scip-typescript emits an occurrence only when the TS typechecker returns a symbol for the node; an `any`-typed
member access has no symbol, so no occurrence, no symbol string, no role bit. There is no `local N`, no
synthetic, no undefined-definition placeholder for the dropped target.

### 3.3 The three call-target classes SCIP induces — INFERRED over §3.2 OBSERVED

| Class | Example (sample) | SCIP behavior | Recoverable as an "unresolved call"? |
|---|---|---|---|
| **C1 in-partition resolved** | `inner()`, `helper(2)` | occurrence → in-repo GLOBAL symbol → becomes an `IrEdge` | N/A (resolved; it is the numerator) |
| **C2 external resolved** | `console.log()`, `Math.floor()` | occurrence → EXTERNAL GLOBAL symbol (`…npm typescript 5.9.3 lib/…`) | NO — SCIP marks it **RESOLVED-external**, the *opposite* of "unresolved" |
| **C3 truly unresolvable** | `p.mysteryMethod()`, `externalThing.run()` | **NO occurrence at all**; or (bare `fn()`) a resolved *reference* to the bound param | NO — nothing to count, or an inverted (resolved) disposition |

The only class that semantically corresponds to the homegrown "unresolved call" (C3) is exactly the class SCIP
**drops** (or *resolves to a parameter*, for bare calls). C2 carries a SCIP signal but it is a *resolved-external*
signal, not an unresolved one.

### 3.4 Strict-mode cross-check — EXECUTED (residual closed)

Re-ran E2 with `compilerOptions.strict=true, noImplicitAny=true` (explicit `: any` is permitted even in strict
mode, so the member access should still produce no symbol). Decoder output for `index-strict.scip`,
`src/main.ts`:

```text
--- DOCUMENT src/main.ts (symbols=11, occurrences=25) ---
  L25:C9   roles=(none)  GLOBAL sym=… unresolvedAnyMethod().(p)     # receiver `p` only — `.mysteryMethod` still ABSENT
  L31:C2   roles=(none)  GLOBAL sym=… externalThing.               # receiver only — `.run` still ABSENT
  L36:C2   roles=(none)  GLOBAL sym=… unresolvedBareCall().(fn)     # resolved param reference — no call marker
== TOTALS occ=29 definitions=14 references=15 ==
```

**OBSERVED: identical to `strict:false`.** The absence of the unresolved-target occurrence is not a
loose-config artifact; it is intrinsic to scip-typescript's symbol-driven occurrence model.

### 3.5 Decoder correctness + resolved contrast on the committed REAL fixture — EXECUTED

Decoding the committed `rust/crates/repo-graph-scip-ingest/tests/fixtures/synthetic/index.scip` (a REAL
scip-typescript output, per LIVEGRAPH-INTEGRATION-1A "real producer output, NOT hand-built"):

```text
== INDEX …/synthetic/index.scip ==   documents=2 external_symbols=0
  … L10:C8  roles=Definition  LOCAL  sym=local 2          # `const circle` — a RESOLVED local binding
  … L11:C16 roles=(none)      GLOBAL sym=… src/`shapes.ts`/Circle#describe().   # report -> Circle.describe RESOLVED
== TOTALS occ=34 definitions=18 references=16 ==
```

This confirms (a) the decoder matches the authoritative path on real producer bytes, and (b) `local N` symbols
are emitted for **resolved** function-local bindings (e.g. `const circle`), **not** for unresolved call
targets. The fixture is all-resolved by design, which is why it cannot itself answer Q1 — the purpose-built
sample (§3.1) supplies the unresolved cases.

### 3.6 How the repo's own ingest treats it — OBSERVED (code) + EXECUTED (edge_probe)

`derive_edges` (`repo-graph-scip-ingest/src/lib.rs:708-726`) iterates non-definition occurrences; for each it
looks the occurrence symbol up in the partition-wide `symbol_to_key` (in-partition definitions only); if the
symbol is absent (C2/C3) it `continue`s — **dropped, with no unresolved-call record created**:

```rust
let callee = symbol_to_key.get(&occ.symbol);
…
let (Some(caller), Some((callee_key, is_fb, callee_name))) = (caller, callee) else {
    continue;   // <- C2/C3 dropped here; no IrEdge, no observation
};
```

This is the IR's "resolved-only `IrEdge`" property the gating slice OBSERVED (`trust-summary-livegraph-1.md`
§2b). Running the repo's REAL ingest on the sample SCIP (`edge_probe`, THIS revision):

```text
EDGES (strict-default, semantic split):
  total_ref_occurrences      = 15
  callee_resolved(in-part)   = 5
  emitted_edges              = 6
  syntax_confirmed_calls     = 2   [SyntaxConfirmedCall]   # only inner() + helper(2) become strict calls
RMAP BOUND:
  ts_extractor_call_sites    = 5   (raw; strict calls 2 <= this)
```

The ingest emits 2 strict in-partition calls (C1) and **0 unresolved-call observations** (the IR has no
`CallObservation` analogue). `ts_extractor_call_sites = 5` is the **homegrown** tree-sitter extractor's raw
call-site count (`lib.rs:1156`) — the only syntactic call-site denominator in the pipeline, and NOT from SCIP.
This `5` independently matches the homegrown DB's `2 resolved + 3 unresolved` CALLS (§4.3) — two independent
homegrown code paths agree the sample has 5 call sites and that `console.log`/`Math.floor` are not among them.

### 3.7 Q1 ANSWER — NO [OBSERVED]

scip-typescript does **not** emit any recoverable signal for an unresolved call target. The unresolved call is
absent (no occurrence/symbol/role/marker) or, for a bare `any` call, present only as a *resolved* reference to
the bound parameter. The trust summary must COUNT unresolved calls; **SCIP provides no current-state source for
that count.**

---

## 4. Q2 (MISSING-2) — does an SCIP-derived count reach parity with `unresolved_edges`? — ANSWER: NO

### 4.1 The paired protocol (one corpus, two producers) — fixes `review-0`

`review-0` rejected the prior cross-corpus comparison. This section runs **both** producers on the **identical
corpus** — the scratch sample of §3.1 (`/tmp/scip-probe/sample`, exactly `src/main.ts` + `src/resolved.ts`) —
and reports the paired counts. (This is `review-0`'s offered option: "run both SCIP and the homegrown SQLite
unresolved-edge path on the same scratch TypeScript sample.")

- **SCIP side:** decode `index.scip` (E3) → count the recoverable unresolved-call signal (§4.2).
- **Homegrown side:** `rmap index /tmp/scip-probe/sample` (E7) → query the produced SQLite `unresolved_edges`
  (E8) (§4.3).

### 4.2 SCIP side on the sample — recoverable unresolved-call signal = 0 — OBSERVED

From the §3.2 decode of `index.scip`, at the three sites the homegrown extractor will call "unresolved":

| Site | SCIP occurrence at the call target | Recoverable unresolved-call signal |
|---|---|---|
| L25 `p.mysteryMethod()` | none (only receiver `p`) | 0 |
| L31 `externalThing.run()` | none (only receiver `externalThing`) | 0 |
| L36 `fn()` | a *resolved* reference to param `…unresolvedBareCall().(fn)` (no call marker) | 0 (it is resolved, not unresolved) |

**SCIP-recoverable unresolved-call count on the sample = 0.** [OBSERVED — decode output, §3.2/§3.4.]

### 4.3 Homegrown side on the SAME sample — `unresolved_edges` (CALLS) = 3 — EXECUTED / OBSERVED

```text
$ rmap index /tmp/scip-probe/sample --alias scip-probe-sample
indexed 2 files, 11 nodes, 5 edges (3 unresolved)
  repo: repo_01kv0y4xvbbh35762cekx32s3x
```

`rmap index` extracted exactly the 2 source files (`node_modules` auto-excluded — same document set as SCIP).
Querying the produced per-repo SQLite DB (daemon state root; NOT a tracked file):

```text
$ sqlite3 <sample-db> "SELECT path FROM files;"
package.json
src/main.ts
src/resolved.ts
tsconfig.json

$ sqlite3 <sample-db> "SELECT type, COUNT(*) FROM edges GROUP BY type;"          # resolved edges
OWNS|2   CALLS|2   IMPORTS|1

$ sqlite3 <sample-db> \
  "SELECT target_key, line_start, category, classification, basis_code FROM unresolved_edges ORDER BY line_start;"
p.mysteryMethod   |25|calls_obj_method_needs_type_info    |unknown|no_supporting_signal
externalThing.run |31|calls_obj_method_needs_type_info    |unknown|no_supporting_signal
fn                |36|calls_function_ambiguous_or_missing |unknown|no_supporting_signal
```

**Homegrown `unresolved_edges` (CALLS) on the sample = 3** (resolved CALLS = 2). [OBSERVED — DB rows.] The 3
unresolved rows are EXACTLY the dynamic/`any` sites L25/L31/L36. Note `console.log`/`Math.floor` appear in
**neither** table — the homegrown tree-sitter extractor does not treat global-builtin member calls as call
sites (corroborated by `ts_extractor_call_sites = 5`, §3.6). [INFERRED over OBSERVED: the *reason* is a
builtin-receiver exclusion; the *absence* is OBSERVED.]

### 4.4 The paired per-site disposition table (same corpus) — OBSERVED

| Call site | line | Homegrown (`rmap index` → SQLite) | SCIP (decode `index.scip`) | Agree? |
|---|---|---|---|---|
| `inner()` | L8 | resolved CALLS edge → `inner` | resolved occ → `main.ts/inner().` | ✓ both resolved |
| `helper(2)` | L13 | resolved CALLS edge → `helper` | resolved occ → `resolved.ts/helper().` | ✓ both resolved |
| `console.log()` | L18 | **not a call site** (absent) | resolved-external → `typescript 5.9.3 …/Console#log().` | ✗ disjoint |
| `Math.floor()` | L19 | **not a call site** (absent) | resolved-external → `typescript 5.9.3 …/Math#floor().` | ✗ disjoint |
| `p.mysteryMethod()` | L25 | **unresolved** (`unknown`) | **absent** (no target occurrence) | ✗ homegrown=unresolved, SCIP=absent |
| `externalThing.run()` | L31 | **unresolved** (`unknown`) | **absent** (no target occurrence) | ✗ homegrown=unresolved, SCIP=absent |
| `fn()` | L36 | **unresolved** (`unknown`) | **resolved** reference to param `fn` | ✗ INVERTED disposition |

**Paired counts (same corpus):** homegrown resolved CALLS = 2, unresolved CALLS = 3 (universe = 5);
SCIP in-partition resolved = 2, resolved-external = 2 sites, recoverable-unresolved = 0. The two producers
agree only on the 2 trivially in-partition-resolved calls; **all 5 "interesting" sites are treated
incomparably.** [OBSERVED.]

### 4.5 Q2 ANSWER — NO; no count-parity, irreconcilable

1. **Count-parity fails on the identical corpus:** SCIP-recoverable unresolved-call count = **0**, homegrown
   `unresolved_edges` (CALLS) = **3**. **`0 ≠ 3`.** [OBSERVED — direct arithmetic on §4.2/§4.3 counts.]
2. **The divergence is structural, not a classification gap** — so no classification pass can reconcile it
   [INFERRED over OBSERVED, on §4.4]:
   - L25/L31: SCIP emits *nothing* for the target → the homegrown "unresolved" is **uncountable** from the
     index (you cannot classify an absent row).
   - L36: SCIP *resolves* `fn` to a bound parameter while the homegrown marks it **unresolved** → the SAME site
     has **inverted** dispositions. "Unresolved" denotes different facts under the two producers
     (**RISK-T-D, demonstrated in-sample**).
3. To recover ANY unresolved-call count from SCIP you must re-introduce a syntactic call-site enumerator (the
   homegrown `ts_call_sites`) and define "unresolved" as `AST_call_sites − SCIP_resolved` — a HYBRID metric,
   not a SCIP-sourced one, and it would still NOT equal the homegrown 3 (it would, e.g., also need to decide
   how to treat the SCIP-resolved `fn` param reference and the C2 externals). [INFERRED over OBSERVED.]

Q2 = NO. The OBSERVED fact is the paired count mismatch (`0 ≠ 3`); the irreconcilability is INFERRED over those
observed counts and the §4.4 per-site dispositions.

### 4.6 SUPPORTING context — the divergence WIDENS at scale (DIFFERENT corpus; NOT the parity comparison) — OBSERVED + INFERRED

The toy sample's homegrown-unresolved set happens to be pure C3 (3/3 dynamic). A larger corpus shows the
divergence does not shrink — it **inverts and grows**. The committed homegrown self-index `.repo-graph.db`
(repo `repo`, extractor `ts-core:0.2.0`; `.repo-graph.db` is gitignored — read-only diagnostic), latest
snapshot:

```text
$ sqlite3 .repo-graph.db "SELECT … resolved_CALLS, unresolved_CALLS …"        # latest snapshot
resolved_CALLS = 899   unresolved_CALLS = 4375          # call-resolution rate = 899/5274 = 17.0%

$ sqlite3 .repo-graph.db "SELECT classification, COUNT(*) FROM unresolved_edges WHERE type='CALLS' AND snapshot_uid=<latest> GROUP BY classification ORDER BY 2 DESC;"
unknown                     2843
external_library_candidate  1201      # SCIP RESOLVES these to external package symbols (C2)
internal_candidate           331      # SCIP RESOLVES these cross-file/`this` (C1)
```

[OBSERVED — counts.] **This is NOT the Q2 parity comparison** (it is a different, larger corpus; the SCIP side
was not run over the full multi-tsconfig repo-graph — §8). It is corroboration only: at scale, ≥ 35% of the
homegrown "unresolved" set (1201 external + 331 internal, of 4375) is precisely what SCIP **resolves**
(C2/C1 per §3.3), so a SCIP-derived count would be far smaller AND classified by inverted criteria.
[INFERRED over OBSERVED.] Both corpora point the same way: paired count mismatch (sample) and inverted-criteria
divergence (at scale).

> Honesty note (INFERRED, load-bearing): the homegrown 17% resolution rate is largely a *syntax-only-extractor
> artifact* — most of the 4375 are resolvable calls tree-sitter+import-bindings could not bind. Porting that
> count forward as "current-state truth" would propagate the outgoing extractor's blindness, not a Layer-0/1
> fact. This weakens the no-loss-PARITY premise itself: the SQLite number is an artifact to retire, not ground
> truth to match.

---

## 5. GO / NO-GO criterion — stated, then judged

```text
CRITERION (from the selection packet):
  GO    = Q1 YES (SCIP carries a recoverable unresolved-call signal) AND Q2 parity-achievable
          (counts match or reconcile to value-equality, possibly after a classification pass).
  NO-GO = Q1 NO (SCIP drops unresolved calls) OR Q2 irreconcilable divergence.
  PARTIAL/CONDITIONAL allowed if the evidence supports it.

JUDGMENT against the evidence:
  Q1 = NO   [§3.7, OBSERVED]            — SCIP emits no occurrence/symbol/role/marker for an unresolvable
                                          call target (absent), or only a resolved param reference (inverted).
  Q2 = NO   — paired same-corpus counts: SCIP-recoverable = 0 vs SQLite unresolved_edges = 3; `0 ≠ 3`
              [§4.5, OBSERVED — count arithmetic]. Irreconcilable because the divergence is structural
              (absence/inversion), not a classification gap [§4.5, INFERRED over OBSERVED].
  => BOTH failure conditions are independently met. Result: NO-GO.

Why NOT PARTIAL: review-0 directed PARTIAL only "if paired Q2 execution is impossible." It was POSSIBLE and was
EXECUTED (E7/E8 on the same sample as E2/E3). The paired count is decisive (0 vs 3), so the PARTIAL fallback
does not apply.
```

---

## 6. VERDICT — `NO-GO`

The IR/LiveGraph extension as framed by `trust-summary-livegraph-1.md` DR-TS-1 A — *"a `CallObservation`
analogue of `ImportObservation` … REQUIRES the SCIP-ingest to EMIT such observations"* — **cannot be built to
yield a no-loss current-state unresolved-call count**, because:

- **MISSING-1 is answered NO:** scip-typescript does not emit the unresolved call, so the ingest has nothing to
  turn into a `CallObservation` (it would be populated from an empty source for C3 and an inverted-meaning
  source for C2). [§3]
- **MISSING-2 is answered NO:** on the IDENTICAL corpus the SCIP-recoverable count (0) does not match the
  homegrown `unresolved_edges` count (3), and the divergence is structural/inverted, not a reconcilable
  classification gap. [§4]

This matches the gating slice's anticipated failure mode (`trust-summary-livegraph-1.md` §6c RED-EVEN-AFTER-
EXTENSION (i) + §4 MISSING-2 RISK-T-D) — now CONFIRMED with paired empirical evidence rather than hypothesized.

What is NOT claimed: this does not refute SCIP as the L0/L1 substrate (it resolves *more* than the homegrown
extractor — that is the point). It refutes only the narrow hypothesis that SCIP can SOURCE a
parity-with-`unresolved_edges` unresolved-call count for the trust summary.

---

## 7. Recommended next step

Per the packet ("NO-GO → recommend reconsidering Option A (DR-TS-0 S3) or the hybrid (S4)"):

**Primary recommendation: Option A (DR-TS-0 S3) — do NOT source unresolved-call disposition from SCIP.** Keep
the homegrown extractor's `unresolved_edges` (+ `extraction_diagnostics_json`) as the trust summary's
unresolved-call input, served **SQLite-labelled** (exactly the TRUST-LIVEGRAPH-1 hybrid Half-B shape). The
trust summary's current-state half then covers only the LG-DERIVABLE fields the gating slice's §4 CLASS T1/T2
already identified (`resolved_calls`, module rollups, registry/alias/framework downgrades); the unresolved-call
fields stay labelled-outgoing. This is honest under VISION's Fact Certainty Model: a Layer-1 value with no
current-state source is reported as such, not synthesized.

**Alternative: hybrid S4 — a REDEFINED current-state unresolved-call notion.** If a current-state
unresolved-call signal is desired, derive it as `homegrown_AST_call_sites − SCIP_resolved_occurrences` (the
ingest already computes both — `ts_call_sites` and the strict `calls`). This is FEASIBLE but it is **not
no-loss parity**: it counts C2 external calls differently, dedupes/classifies differently, and will not equal
the old `unresolved_edges` number (the sample alone shows `5 − 2 = 3` only by coincidence of the toy's shape;
at scale the hybrid and the homegrown counts diverge). It must ship as a NEW metric with its own contract, with
the old count retained + labelled — never presented as the same fact.

Both paths terminate the DR-TS-1 A "extension populated from SCIP" line. DR-TS-1 / DR-TS-2 / DR-TS-CRATE-HOME
in the gating slice should be re-scoped accordingly (DR-TS-1 A is refuted as a no-loss producer).

```text
DECISION_REQUIRED:
- ID: DR-TS-0-POST-PROBE
  QUESTION: Given NO-GO, which path replaces DR-TS-1 A for the trust summary's unresolved-call fields?
  OPTIONS:
  - Option A (DR-TS-0 S3): keep homegrown `unresolved_edges`/diagnostics as the trust input, served
    SQLite-LABELLED. Consequence: the trust summary's current-state half covers only the LG-DERIVABLE
    CLASS T1/T2 fields; `edges`/`unresolved_edges` stay load-bearing for the unresolved-call fields (the
    SQLITE-RAW-DECOMMISSION-1 deletion gate for these fields stays RED — by design, honestly). Lowest risk;
    no new metric; aligns with TRUST-LIVEGRAPH-1's existing hybrid.
  - Option S4 (hybrid REDEFINED metric): compute a current-state unresolved-call count as
    `AST_call_sites − SCIP_resolved`, ship it as a NEW contract, retain+label the old count. Consequence:
    enables a current-state unresolved-call SIGNAL (not parity), re-introduces dependence on the homegrown
    AST call-site enumerator (so the trust summary is hybrid, not pure-SCIP), and requires a metric-contract +
    consumer migration (confidence/TRUST_LOW_RESOLUTION/EXPLAIN_TRUST thresholds were tuned to the OLD number).
  RECOMMENDED: Option A. It is the only path that does not redefine a shipped Layer-1 metric, and the probe
    shows the SQLite unresolved count is an extractor artifact rather than a fact worth reproducing.
  BLOCKING_REASON: This is an architecture-boundary + Layer-1 trust-semantics decision (it changes what the
    trust summary CLAIMS as current-state fact and whether the unresolved-call fields ever leave SQLite). It
    is the operator's governance call; the probe supplies the evidence (NO-GO) but does not settle the
    replacement. It blocks any TRUST-SUMMARY-LIVEGRAPH-IMPL-1 scoping.
```

---

## 8. Residuals, limitations, and honesty

- **Q2 paired comparison WAS run on the scratch sample (the controllable corpus)** — E7/E8 index the SAME
  `/tmp/scip-probe/sample` that E2/E3 produced the SCIP index for. This is `review-0`'s offered option 1 and is
  the basis of the §4.5 verdict. [Corrects the prior rev's cross-corpus Q2.]
- **The large `.repo-graph.db` figures are SUPPORTING context on a DIFFERENT corpus (§4.6), explicitly NOT the
  parity comparison.** I did NOT additionally run scip-typescript over the full multi-tsconfig repo-graph to
  produce a paired numeric SCIP-vs-SQLite count at that scale, because (a) the sample already yields a clean
  paired count, and (b) a full repo-graph SCIP run risks the PRODUCER-COMPAT-1 crash. [NOT RUN at scale, with
  reason; the in-scope paired evidence is the sample.]
- **PRODUCER-COMPAT-1 not generally refuted.** scip-typescript@0.4.0 ran on Node v22.21.1 for the small samples
  here (E2/E6 exit 0). I did NOT test it on a large or multi-tsconfig repo, where the documented crash was
  observed. This probe only establishes the producer was runnable enough to answer Q1/Q2 on the sample; it does
  not reopen PRODUCER-COMPAT-1. [Honest scope.]
- **Daemon was started manually for E7.** The launchd service was `loaded but not running`; I ran `rmapd`
  directly to obtain a live socket for `rmap index`. This is daemon-runtime state outside the tracked tree; it
  changes no tracked repo file. The sample repo was registered under alias `scip-probe-sample` in the daemon
  registry (also outside the tracked tree). [Honest scope; §10.]
- **Symbol-model generality.** Q1's finding (no occurrence for an `any`/dynamic call target) follows from
  scip-typescript's symbol-driven occurrence model and was confirmed in two compiler configs (§3.4). Exotic
  forms not individually enumerated (optional-chaining `a?.b()`, index-signature `a["b"]()`, call through a
  union with one `any` arm) are expected to behave identically (no checker symbol → no occurrence) but were not
  each tested. This does not affect the verdict, which rests on the dominant `any`/dynamic case. [Honest scope.]
- **`fn` param-reference disposition (L36)** is read from the decode (a `roles=(none)` reference to
  `…unresolvedBareCall().(fn)`). I classify it as "resolved reference, no call marker" — that `fn` binds to the
  parameter is OBSERVED from the symbol; that this is the *opposite* of the homegrown "unresolved call" is the
  INFERRED-over-OBSERVED inversion point. [Honest scope.]

---

## 9. Reproduction (validation) commands

```bash
# Producer (scratch, /tmp):
cd /tmp/scip-probe && npm install @sourcegraph/scip-typescript@latest    # -> 0.4.0
node node_modules/@sourcegraph/scip-typescript/dist/src/main.js --version
cd /tmp/scip-probe/sample && node /tmp/scip-probe/node_modules/@sourcegraph/scip-typescript/dist/src/main.js index --output index.scip

# Decode (scratch decoder, same scip 0.7.1 as the repo):
cd /tmp/scip-probe/decoder && cargo run --release -- /tmp/scip-probe/sample/index.scip

# Repo's real ingest on the sample SCIP:
cd rust && cargo run -p repo-graph-scip-ingest --example edge_probe -- /tmp/scip-probe/sample/index.scip /tmp/scip-probe/sample sample

# PAIRED Q2 — homegrown index of the SAME sample, then query its unresolved_edges (daemon required):
rmapd &                                                                  # if the socket is absent
rmap index /tmp/scip-probe/sample --alias scip-probe-sample              # -> "2 files, 11 nodes, 5 edges (3 unresolved)"
#   locate the sample DB (most recently written) under the daemon state root, then:
DB="$HOME/Library/Application Support/repo-graph/databases/<sample-db>.db"
sqlite3 "$DB" "SELECT type, COUNT(*) FROM edges GROUP BY type;"          # resolved: OWNS|2 CALLS|2 IMPORTS|1
sqlite3 "$DB" "SELECT target_key, line_start, category, classification FROM unresolved_edges ORDER BY line_start;"  # 3 CALLS

# SUPPORTING (different corpus) breakdown at scale:
sqlite3 .repo-graph.db "SELECT classification, COUNT(*) FROM unresolved_edges WHERE type='CALLS' \
  AND snapshot_uid=(SELECT snapshot_uid FROM unresolved_edges ORDER BY observed_at DESC LIMIT 1) \
  GROUP BY classification ORDER BY 2 DESC;"
```

---

## 10. Scratch artifact inventory (UNCOMMITTED — /tmp, gitignored, or daemon state root)

| Path | What | Tracked? |
|---|---|---|
| `/tmp/scip-probe/node_modules/@sourcegraph/scip-typescript` | the producer (0.4.0) | no (/tmp) |
| `/tmp/scip-probe/sample/` | the TS sample + `index.scip` + `index-strict.scip` | no (/tmp) |
| `/tmp/scip-probe/decoder/` | the scratch `scip 0.7.1` occurrence dumper | no (/tmp) |
| `rust/target/**` | cargo build output of `edge_probe` / decoder | no (gitignored) |
| `~/Library/Application Support/repo-graph/databases/<sample-db>.db` | homegrown index of the sample (E7) | no (daemon state root, outside tracked tree) |
| daemon registry entry `scip-probe-sample` | sample repo registration (E7) | no (daemon state root) |
| `.repo-graph.db` | committed homegrown self-index (read-only diagnostic, §4.6) | no (gitignored) |
| `docs/slices/scip-unresolved-call-probe-1.md` | **this report — the only tracked deliverable** | YES |

`git status --short` shows only this report. No production code, migration, or tracked-file deletion was made.
