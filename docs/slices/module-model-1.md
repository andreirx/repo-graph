# MODULE-MODEL-1: a true, coherent module model on the orientation surface — SPEC

Slice: MODULE-MODEL-1
Status: **DELIVERED** — IMPL shipped 2026-06-23 as MODULE-MODEL-IMPL-1 (`17dbe93`,
relay-approved iteration 1): D1/D2/D4 core (shared `package_groups` fold behind
orient + stats, two self-labelled notions, JVM main/test merge + prefix collapse)
and the D3 umbrella descent. Delivery record added retroactively 2026-07-11 — the
record was missed at close-out. **§13 (added 2026-07-11) is a FOLLOW-UP contract
(MODULE-MODEL-2), not part of the delivered slice:** D7 bounded-at-scale and D4
per-toolchain grouping are NOT yet implemented (verified 2026-07-11: the
package-groups renderer iterates unbounded; grouping has no crate/workspace
awareness); the §13 D2 population clarification IS already satisfied by the
delivered shared fold.
Track: Product-surface honesty (`docs/ROADMAP.md` → Current priority, P1 "orient module
under-segmentation" ↔ P2 "module model unification" — "decide the notion once").
Pairs: this slice designs the fix for **TECH-DEBT #3** (orient under-segments nested layouts)
and **TECH-DEBT #4** (the word "module" denotes different things across commands) as ONE
coherent slice.
Grounded in: the first End-to-End Usefulness Protocol run on spring-petclinic
(`smoke-runs/2026-06-21T15-55-40Z/`), confirmed against the actual heuristics in code (cited).
Model: this doc follows `docs/slices/orient-density-1.md` (principle → load-bearing behavior →
target output → root-cause design → coherence design → per-choice VISION defense →
decisions-to-surface → validation).
Prior art: `docs/slices/orient-bug-1-module-count.md` (ORIENT-BUG-1) fixed the orient/trust
module **count** mismatch by unifying both on `module_candidates`. THIS slice is the distinct
**under-segmentation + cross-command-notion** problem, and it finishes the unification
ORIENT-BUG-1 started (it left `stats` on the old source).

---

## 0. Evidence law (how to read the claims below)

- `OBSERVED` — read from the smoke capture artifact or from source, cited by path/line.
- `INFERRED` — concluded from cited code, not directly executed.
- The IMPL slice re-validates everything labelled INFERRED via live `rmap` capture (§9).

All source line numbers are against the working tree at spec time; the IMPL confirms before
editing (lines drift).

---

## 1. The problem (OBSERVED, real output)

On spring-petclinic (`smoke-runs/2026-06-21T15-55-40Z/`), three commands answer the question
"what is the module structure?" with three different, unlabelled answers:

`orient --full` (`spring-petclinic-orient---full.txt`):
```
spring-petclinic · 49 files, 290 symbols · 1 module: .
...
Modules (by size)
  - . — 47 files
```

`modules list` (`spring-petclinic-modules-list.txt`):
```
1 module
  spring-petclinic    30 files (17 test)   83 unref?   0 violations  declared
```

`stats` (`spring-petclinic-stats.txt`):
```
Summary
  modules: 11
  total_files: 47
By size
  src/main/java/org/springframework/samples/petclinic/owner   files=12 ...
  src/main/java/org/springframework/samples/petclinic/vet     files=6  ...
  src/main/java/org/springframework/samples/petclinic/system  files=5  ...
  ... (11 rows; main+test package directories)
```

Two distinct failures, both forbidden by the VISION:

1. **orient delivers a structurally WRONG model (TECH-DEBT #3, P1).** The repo plainly has
   `owner` / `vet` / `system` / `model` / `service` package separation. `orient` — THE primary
   orientation surface — flattens it to "1 module: ." and "Modules (by size): . — 47 files". An
   agent reading `orient` learns something *false* about where the code lives, and carries that
   into every downstream decision. Per the Protocol-Surface Standard this fails the Layer-2 test
   in the worst way: "can an agent learn the truth from the output alone?" → it learns a falsehood.
2. **The surface is incoherent (TECH-DEBT #4, P2).** `orient`/`modules` say **1**; `stats` says
   **11**; and even `orient` (`. — 47 files`) and `modules list` (`spring-petclinic — 30 files`)
   disagree on the single module's name and size. Nothing labels which notion each count means,
   so the consumer must reverse-engineer it. A machine protocol cannot answer "modules?" with
   1 vs 11 vs (`.`/47) vs (`spring-petclinic`/30), all unlabelled.

---

## 2. Root cause — confirmed against code (corrects the TECH-DEBT hypothesis)

TECH-DEBT #3 recorded a HYPOTHESIS: the inferred-module umbrella heuristic fails to descend
`src/main/java/…` single-child chains. **That hypothesis is real but does NOT explain
spring-petclinic** — confirmed by reading the code. The true root cause is the unmigrated
**dual-path** model plus a single-manifest declared root. There are three mechanisms:

### 2a. orient + modules read `module_candidates`; spring-petclinic has ONE declared root

- spring-petclinic ships a root `settings.gradle` + `pom.xml` (OBSERVED: repo `ls`). The Rust
  indexer's `settings.gradle` reader produces **one declared module** rooted at `.`, display
  name `spring-petclinic`.
  - OBSERVED: `modules list` labels it `declared`; `compose.rs:507-508` (`extract_gradle_modules`).
- Because a declared root exists, inferred detection runs in **gap-fill mode** — only on files
  NOT under a declared root — so it produces nothing extra here (the Gradle module covers the
  JVM files).
  - OBSERVED: `rust/crates/repo-index/src/compose.rs:519-525`
    (`if declared_roots.is_empty() { … } else { detect_inferred_modules_gap_fill(…) }`).
- `orient`'s "Modules (by size)" reads `module_candidates ⋈ module_file_ownership`.
  - OBSERVED: `rust/crates/storage/src/agent_orient_reads.rs:103-135` (`module_sizes`).
- `orient`'s headline count + kinds read the same Layer-1/2 model via `get_module_summary`.
  - OBSERVED: `rust/crates/agent/src/storage_port.rs:735`; signal `dto/signal.rs::module_summary`.

⇒ orient and modules both see the **one declared Gradle module** → "1 module".
**The inferred umbrella heuristic never runs on this repo** (gap-fill-suppressed). So the
TECH-DEBT #3 fix-as-written ("descend umbrella chains in the inferred heuristic") would change
**nothing** on spring-petclinic. This is the load-bearing correction.

### 2b. stats reads `nodes` kind='MODULE' (per-directory) — the unmigrated path

- The Rust indexer creates a MODULE node for **every directory** (all ancestors), qualified_name
  = the directory path.
  - OBSERVED: `rust/crates/indexer/src/orchestrator.rs:1036-1075` (`create_module_nodes`).
- `stats` computes from `nodes` WHERE kind='MODULE' joined to `OWNS` edges, **filtered to
  `files.cnt > 0`**. OWNS edges link a file only to its **immediate (deepest) directory**, so only
  leaf directories that directly contain files survive the filter.
  - OBSERVED: `rust/crates/storage/src/queries.rs:1159-1294` (`compute_module_stats`),
    filter at L1247-1248 (`WHERE m.kind='MODULE' AND COALESCE(files.cnt,0) > 0`).
- spring-petclinic's leaf package directories = the 11 rows stats prints (main: owner/vet/system/
  model/petclinic; test: owner/petclinic/system/service/vet/model).

⇒ stats reports **physical leaf-directory topology** (11), a *different notion* from the
declared/inferred `module_candidates` model orient/modules use (1).

This is exactly the dual-truth ORIENT-BUG-1 diagnosed ("two competing definitions of module:
`module_candidates` vs `nodes` kind=MODULE"). ORIENT-BUG-1 migrated **trust** onto
`module_candidates` (OBSERVED: `rust/crates/storage/src/trust_impl.rs:313` reads
`module_candidates`) but **left `stats` on `nodes` kind=MODULE**. `stats` is the last command on
the old source.

### 2c. The inferred umbrella heuristic IS limited (the TECH-DEBT hypothesis, for manifest-less repos)

Separately true, and relevant to manifest-less nested repos (not spring-petclinic): the umbrella
splitter computes children at a **fixed depth of exactly 2** and requires **≥2** children, so a
single-child chain (`src/main/java/…`) never splits.

- OBSERVED: `rust/crates/indexer/src/inferred_modules.rs`
  - umbrella child key is always `parts[0]/parts[1]` (depth-2): L229-234, L278-294
    (`let child_path = format!("{}/{}", parts[0], parts[1]);`).
  - split needs `qualifying_children.len() >= UMBRELLA_MIN_CHILDREN` (=2): L56, L366-367.
  - a single child ⇒ no split ⇒ falls through to one top-level `src` module: L398-413; a
    flat/uncovered repo falls back to a single root `.` module: L440-454.
  - Tested-in: `detect_nginx_structure` (L834-850) asserts `src/core, src/http, src/event` →
    **1 module `src`** today (single child `src/...` at depth 2 from nginx's own layout would
    not split; the umbrella test that DOES split, `umbrella_split_when_thresholds_met` L1490,
    uses 3 siblings directly under `src/`).

So for a **manifest-less** repo whose real packages sit below a single-child chain (e.g.
`src/main/java/org/app/{a,b,c}` with no manifest), the inferred model also collapses. The fix for
*that* class is umbrella-chain descent. It is a **secondary** improvement, not the spring-petclinic
fix.

### Root-cause summary

| Symptom (OBSERVED) | Mechanism (cited) | Notion |
|---|---|---|
| orient/modules → **1** | `module_candidates`: one declared Gradle root; inferred gap-fill-suppressed | declared/inferred module (Layer 1/2) |
| stats → **11** | `nodes` kind=MODULE leaf dirs, `files.cnt>0` filter | physical directory topology (Layer 0/1) |
| orient `.`/47 vs modules `spring-petclinic`/30 | two reads over the candidate model with different file-count joins (`module_sizes` COUNT(ownership) vs modules-list `owned_file_count`/`owned_test_file_count`) | same notion, divergent counting — RESIDUAL, IMPL to confirm exact join |
| manifest-less nested repos collapse | umbrella splitter fixed-depth-2 + ≥2-children | inferred module (Layer 2) |

**The two findings are one problem:** the surface carries **two legitimate but unlabelled
notions** — *physical directory/package topology* and *declared/inferred module boundaries* —
and `orient` leads with the less useful one ("1 module") while never naming the more useful one
(the packages). #4 is the labelling half; #3 is the orientation-density half on top of it.

---

## 3. Principle (what "true and coherent" means here)

- **A single-module Gradle/Maven project genuinely IS one build module that contains many
  packages.** "1 declared module" is not false — it is the *wrong altitude* for orientation. The
  agent needs "the code is organized into owner / vet / system / model / service", which is
  **directory/package topology**, a Layer-0/1 extracted fact the indexer already computes.
- **The bug is not that two notions differ — it is that neither is labelled, and orient leads
  with the wrong one.** On a single-module repo the topology (many packages) SHOULD differ from
  the build-module count (1). Coherence = each notion **named and self-labelled**, not collapsed.
- **orient must lead with the load-bearing structure** (ORIENT-DENSITY-1): the named packages,
  not "1 module: .".
- **Honest layering (Product Layer Model):** directory/package topology is Layer 0/1 (extracted
  fact); inferred modules are Layer 2 (interpretation with basis, confidence 0.7) and must stay
  labelled inferred; declared modules are Layer 1. No count may be rendered as a certainty class
  above its layer.

---

## 4. Desired output (the true model) — before / after

The IMPL is "done" when `orient` on spring-petclinic NAMES the real packages instead of
"1 module: .". Exact label tokens are decision-dependent (§7 D1/D4); the **invariant** is fixed:
orient names owner/vet/system/model/service and every count is self-labelled.

**BEFORE** (OBSERVED):
```
spring-petclinic · 49 files, 290 symbols · 1 module: .
...
Modules (by size)
  - . — 47 files
```

**AFTER** (TARGET; the package set + counts are checkable, label tokens per §7):
```
spring-petclinic · 49 files, 290 symbols · 6 package groups · 1 declared module (gradle)
...
Structure (directory/package groups — Layer 0/1 topology; distinct from the 1 declared gradle module)
  - owner      — 17 files (5 test)
  - vet        — 8 files  (2 test)
  - system     — 8 files  (3 test)
  - petclinic  — 7 files  (4 test)
  - model      — 5 files  (1 test)
  - service    — 2 files  (2 test)
```
(Counts derive from the stats capture, merging `src/main/java/…/<pkg>` with `src/test/java/…/<pkg>`
by logical package — §7 D4: 12+5, 6+2, 5+3, 3+4, 4+1, 0+2; total 47.)

**Coherence (AFTER) across commands** — each count self-labelled, no two unlabelled answers:
- `orient`: leads with the named package groups; reports "1 declared module (gradle)" as a
  labelled secondary fact.
- `stats`: same package/directory groups, header relabelled from "modules" to the ratified unit
  (§7 D1) — e.g. `directory groups: 11` (or `package groups`), never bare "modules: 11".
- `modules list` / `trust`: continue to report the **declared/inferred module** notion
  (`module_candidates`), labelled "modules" — these answer the build-module question, which is a
  different, legitimately-labelled question.

A reader can now tell *which question each number answers*. That is the Protocol-Surface bar.

---

## 5. Root-cause design (the smallest change that delivers the true model)

The data for the true model **already exists** — no new extraction, no new subsystem:
- physical leaf-directory topology: `nodes` kind=MODULE + OWNS (stats already reads it), and the
  file→directory mapping the indexer computes (`create_module_nodes` / OWNS edges);
- the declared/inferred model: `module_candidates ⋈ module_file_ownership` (orient/modules read it).

The fix is **wiring + labelling + presentation**, with one **optional, contained heuristic tweak**
(umbrella-chain descent) for the manifest-less variant:

1. **Name the two notions distinctly and self-label every count** (closes #4). Decide the unit
   word for the topology notion (§7 D1) and apply it to `stats` and to `orient`'s structure line;
   keep "module" for the `module_candidates` notion (modules/trust).
2. **orient surfaces the package topology as its STRUCTURE headline** (closes #3). orient already
   reads `module_file_ownership`; the package/directory grouping is a roll-up of owned-file paths
   to the meaningful directory level. orient names those groups (the load-bearing structure) and
   reports the declared-module count as a labelled secondary fact. (Mechanism choice = §7 D2.)
3. **(Optional, secondary) extend the inferred umbrella splitter to descend single-child chains**
   to the first directory with ≥2 qualifying sibling children (closes the manifest-less variant
   in 2c). Threshold reuse from `inferred_modules.rs` (≥2 children, 5+ files/child). This does
   **not** change spring-petclinic; it earns its place only if D3 ratifies it.

**No new module subsystem / registry / adapter is introduced** by the recommended path (§7
recommendations). It reuses the per-directory topology the indexer already produces and the
`module_candidates`/`module_file_ownership` reads that already exist; the work is a directory
roll-up read + relabelled renderers + (optionally) a bounded heuristic edit. The larger
alternative that WOULD touch the module-identity contract (segmenting a declared module into
sub-package modules) is surfaced as a decision and recommended **against** for this slice (§7 D2,
and STOP-CONDITION note §10).

---

## 6. Cross-command coherence design (closing #4)

Canonical rule for the discovery surface, to be ratified (§7 D1):

- **Two named notions, each self-labelled, never collapsed:**
  - *directory/package groups* (Layer 0/1 physical topology) — what `stats` enumerates and what
    `orient` should lead with. Unit word decided in D1.
  - *declared/inferred modules* (Layer 1/2 `module_candidates`) — what `modules`/`trust` report,
    and what `orient` reports as a labelled secondary count.
- **No command emits a bare "modules: N" that means directory groups.** `stats`'s header changes
  to the ratified unit; `orient` labels both ("N package groups · 1 declared module (gradle)").
- **One source per notion.** The topology notion comes from one computation shared by `stats` and
  `orient` (no third divergent count); the module notion stays on `module_candidates` (finishing
  ORIENT-BUG-1's unification — and, if D1 = "align stats to module_candidates", migrating stats
  too, with the consequence noted in §7 D1).

This also resolves the **orient `.`/47 vs modules `spring-petclinic`/30 residual** (§2 table):
once orient leads with the topology groups and the declared-module count is a single labelled
read, the two commands stop reporting different sizes for "the module". The IMPL confirms the
exact file-count join and states it (Persistence-Completeness: read path + CLI visibility).

---

## 7. Decisions to surface (DECISION_REQUIRED — operator ratifies; the IMPL does NOT re-decide)

Each is presented as an exhaustive matrix with a defensible recommendation. The IMPL executes the
ratified cells without re-opening them.

DECISION_REQUIRED:
- ID: D1-CANONICAL-NOTION
  QUESTION: What is the canonical "module" notion for the discovery surface, and what does `stats`
    do about its unit word?
  OPTIONS:
  - A1 Unify on `module_candidates` (migrate stats to it, finishing ORIENT-BUG-1): ONE notion
    everywhere; consequence — stats then ALSO shows "1" for spring-petclinic, so the package
    topology DISAPPEARS from the surface unless §D2 also makes the candidate model segment into
    packages. Coherent but loses the only true segmentation currently present, unless paired with
    a larger D2.
  - A2 Unify on directory-groups (migrate orient/modules to leaf-dir grouping): regresses
    ORIENT-BUG-1's declared/inferred semantics and ecosystem-scoped ownership; rejected.
  - A3 Two distinct notions, each self-labelled (RECOMMENDED): "package/directory groups"
    (topology) vs "declared/inferred modules" (`module_candidates`). Cheapest; preserves both
    truths; satisfies the Protocol-Surface bar by labelling, exactly the "(or self-label each)"
    resolution TECH-DEBT #4 allows. `stats` header → "directory groups" (or "package groups").
  RECOMMENDED: A3.
  BLOCKING_REASON: Changes `stats`'s output contract (header unit word; possibly JSON key) and
    sets what `orient` labels — an output-contract + cross-command-notion decision (architecture
    boundary per CLAUDE.md). The IMPL cannot pick the canonical notion without this.

- ID: D2-ORIENT-SEGMENTATION-MECHANISM
  QUESTION: How does `orient` surface the true package topology so it NAMES owner/vet/system/…?
  OPTIONS:
  - (i) orient reads the directory/package topology roll-up (from owned-file paths it already
    reads, or the per-directory MODULE nodes stats reads) and names the groups (RECOMMENDED):
    smallest; no module-identity change; reuses existing data; pairs with A3.
  - (ii) Segment declared single-module JVM roots into sub-package modules inside
    `module_candidates`: makes the candidate count itself meaningful and enables A1, BUT changes
    what a "declared module" IS (one Gradle project → N package modules), churning module UIDs/keys
    and the declared-vs-inferred semantics. Larger blast radius; edges toward a module-identity
    contract change (see §8, §10).
  - (iii) Inferred-umbrella-descent only: does NOT fix spring-petclinic (declared, gap-fill-
    suppressed — §2a); insufficient for the named acceptance. Rejected as the primary mechanism.
  RECOMMENDED: (i).
  BLOCKING_REASON: (ii) crosses the module-identity contract and approaches "new module
    subsystem" (packet STOP-CONDITION). The mechanism determines blast radius and must be chosen
    before IMPL.

- ID: D3-UMBRELLA-DESCENT
  QUESTION: Include the inferred umbrella-chain descent (2c) in THIS slice (helps manifest-less
    nested repos), or defer it?
  OPTIONS:
  - Include as a bounded second commit (RECOMMENDED): small, contained edit to
    `inferred_modules.rs` (descend single-child chains to first ≥2-sibling fan-out, reusing the
    existing thresholds); addresses the variant the TECH-DEBT named; carries identity churn for
    inferred modules on manifest-less nested repos (handle per §8).
  - Defer to a follow-up: keeps this slice strictly orient/stats; the manifest-less variant stays
    broken until then.
  RECOMMENDED: Include — but the spec MUST state plainly that it does not change spring-petclinic;
    spring-petclinic is fixed by D2(i)+D1.
  BLOCKING_REASON: Changes inferred-module identity for affected repos (heuristic upgrade —
    §8). Operator should ratify the identity-churn vs scope trade-off.

- ID: D4-PACKAGE-GROUP-SHAPE
  QUESTION: How are package/directory groups shaped for the topology view?
  OPTIONS:
  - Merge `src/main/java/…/<pkg>` with `src/test/java/…/<pkg>` by logical package, show test count
    (RECOMMENDED): the agent sees "owner" once (17 files, 5 test), matching how humans name the
    package; collapses the meaningless `src/main/java/org/springframework/samples/petclinic`
    prefix.
  - Keep raw leaf-directory paths (as stats does today): no prefix logic, but ugly and splits
    main/test — lower orientation value.
  RECOMMENDED: Merge by logical package + show test count; collapse the common source-root prefix.
  BLOCKING_REASON: Affects the exact AFTER counts (§4) and the shared grouping computation; both
    stats and orient must use the SAME shape to stay coherent.

- ID: D5-JSON-CONTRACT
  QUESTION: Does the JSON/`CoherenceEnvelope` shape change to carry the topology groups + labelled
    counts, or only the human output?
  OPTIONS:
  - Human-only first, JSON additive (RECOMMENDED): add the named-groups + labelled-count fields
    without removing existing keys; do not regress the envelope (ORIENT-DENSITY-1 §6 discipline).
  - Breaking JSON change (rename "modules" count keys to disambiguate notions): cleaner long-term;
    requires contract-doc updates (`v1-cli.txt` / normative specs) and a version note.
  RECOMMENDED: Additive (human-first), with the disambiguating fields self-labelled.
  BLOCKING_REASON: Any breaking JSON change is an output-contract change (Decision Rules:
    "new CLI surface changes JSON → update normative contract docs").

- ID: D6-SLICE-SPLIT
  QUESTION: One IMPL slice or split (orient under-seg vs stats vocabulary)?
  OPTIONS:
  - ONE slice for notion + labels + orient topology (RECOMMENDED): per the roadmap mandate
    "decide the notion once" — the stats relabel and the orient topology view are the SAME notion
    decision; splitting them risks relabelling stats while orient still leads with "1 module".
    The D3 umbrella descent MAY be a second commit within the slice.
  - Split: stats-vocabulary slice first, orient-topology slice second. More steps; risks an
    intermediate state where the surface is half-coherent.
  RECOMMENDED: ONE slice (D1+D2+D4+D5 together), D3 as an optional second commit.
  BLOCKING_REASON: Sequencing/scope decision that the roadmap couples ("decide the notion once").

---

## 8. Honest layering & identity evolution (VISION defense per choice)

Per-choice defense against the cited VISION sections (every choice defended; none contradicts):

- **Primary orientation surface must be TRUE (Primary Use Case; Orientation Not Oracle).** D2(i)
  makes orient name the packages an agent needs first. The named groups are Layer-0/1 directory
  facts (where files physically sit) — the strongest certainty class, safe to lead with.
- **Honest layering (Product Layer Model).**
  - Directory/package groups = Layer 0/1 extracted fact → may be stated plainly.
  - Declared modules = Layer 1; inferred modules = Layer 2 (confidence 0.7) → stay labelled, never
    rendered as Layer-0 truth. The AFTER labels "1 declared module (gradle)" — declared, not bare.
  - No count is promoted above its layer; no notion is collapsed into another's certainty class.
- **Protocol-Surface coherence (Layer 2 output contract).** After this slice, no two commands
  answer "modules?" with unlabelled 1 vs 11. Each count is self-describing; an agent learns which
  question each answers from the output alone.
- **Inferred-module identity evolution (TECH-DEBT "Inferred Module Identity Evolution").** If D3
  ships, inferred-module UIDs/keys change on affected manifest-less nested repos — this is
  *intentional heuristic evolution*, not a breaking change; inferred modules are orientation-grade
  (0.7), not declared truth, and identities are recomputed per snapshot/refresh (no cross-snapshot
  identity guarantee is claimed). The IMPL records the heuristic-version bump and states "no
  migration needed; refresh recomputes" per the existing contract. If D2(ii) were chosen instead
  (NOT recommended), DECLARED-module identity would change — a stronger, contract-level change
  requiring an explicit identity/versioning decision (§10).
- **Directory-group identity.** Path-anchored (the directory/package path), deterministic, Layer-0
  — same identity discipline as inferred modules' path anchoring, but a *physical* fact, not an
  inference.

---

## 9. Validation plan (for the IMPL)

EXECUTED-class evidence the IMPL must produce (per `docs/testing/end-of-slice-procedure.md` and
the isolated dogfood — never index into the operator's real registry):

1. **Named packages on spring-petclinic.** Isolated `rmap orient --full` capture shows the
   structure line NAMING owner/vet/system/model/service (and the root package), NOT "1 module: .".
   Compare against §4 AFTER (the package set + merged counts are checkable: 17/8/8/7/5/2 = 47).
2. **Coherence across commands.** On the same snapshot, `orient`, `stats`, `modules list`, `trust`
   either share one notion or each count is self-labelled; no bare "modules: 11" vs "1 module".
   Capture all four; assert the labels.
3. **No regression on the cases ORIENT-BUG-1 fixed.** repo-graph (declared crates), OpenXcom,
   Django module counts unchanged in the `module_candidates` (modules/trust) notion.
4. **(If D3) manifest-less nested descent.** A fixture or known manifest-less repo with
   `src/.../{a,b,c}` packages shows the descended groups, not a single collapsed module; assert the
   inferred unit tests in `inferred_modules.rs` (e.g. a new `descend_single_child_chain` test).
5. **Honesty preserved.** No count rendered above its layer; "declared"/"inferred" labels intact;
   reliability/certainty footer unchanged. No overclaim re-introduced.
6. **Contracts.** `cargo build/fmt/clippy/test` green in `rust/`; smoke protocol
   (`docs/testing/rmap-test-protocol.md`); JSON envelope not regressed (or the change ratified per
   D5 and normative docs updated). `total_symbols: 0` is OUT OF SCOPE (TECH-DEBT #5, separate
   slice) — note it in the capture but do not fix it here.

---

## 10. Smallest-design statement & STOP-condition assessment

- **Smallest design.** The recommended path (D1=A3, D2=(i), D4=merge, D5=additive, D6=one slice,
  D3 optional) introduces **no new module subsystem, registry, adapter, DTO layer, or config
  surface**. It reuses: the per-directory topology the indexer already emits
  (`orchestrator.rs::create_module_nodes`), the `module_file_ownership` read `orient` already
  performs, and the `module_candidates` model `modules`/`trust` already use. The new work is one
  shared directory-roll-up read (so `stats` and `orient` cannot diverge), relabelled renderers, and
  an optional bounded heuristic edit to `inferred_modules.rs`. The single new shared computation is
  justified by **two concrete current callers** (`stats` and `orient`'s structure headline) needing
  the *same* directory-group numbers — without it they re-diverge (the very bug). Simpler
  alternative rejected: letting each command compute its own grouping — that is the current
  incoherence.
- **STOP-condition assessment (packet).**
  - "If the canonical notion / stats-alignment changes a command's OUTPUT CONTRACT → record as
    DECISION_REQUIRED." → DONE: D1 (stats unit word), D5 (JSON) are DECISION_REQUIRED.
  - "If delivering a true model would require a NEW module subsystem (not just heuristic
    descent) → STOP + DECISION_REQUIRED." → The **recommended** path does NOT require one
    (assessed above). The only option that would approach a module-identity-contract change /
    subsystem is **D2(ii)** (segmenting declared modules), which is surfaced as a DECISION_REQUIRED
    and recommended **against** for this slice. So no hard global stop is warranted; the boundary
    is recorded here and gated behind D2 so the operator chooses with the trade-off explicit.

---

## 11. Out of scope

- `total_symbols: 0` false-zero (TECH-DEBT #5) — separate slice; note in capture, do not fix.
- stats fan-in/out reliability marker (TECH-DEBT #6) — separate slice.
- Any production code in THIS slice (this is design only; a later IMPL executes it).
- ROADMAP.md / TECH-DEBT.md / CURRENT_SLICE.md edits (out of scope per the selection packet).

---

## 12. Ratification (operator — 2026-06-23)

D1–D6 ratified. The §7 matrices are preserved as the audit trail; this section is the
binding outcome the IMPL executes (it does NOT re-open these).

- **D1 = A3** — two self-labelled notions (package/directory groups vs declared/inferred
  modules). As recommended.
- **D2 = (i)** — orient reads the directory roll-up; no module-identity change. As recommended.
- **D3 = include** — the umbrella-chain descent ships as a bounded second commit. As recommended.
- **D4 = merge** — main+test by logical package, show test count. As recommended.
- **D5 — RATIFIED WITH CORRECTION (the §7 D5 recommendation is SUPERSEDED).** The
  agent-facing CLI/JSON output is **USEFUL INFORMATION — the product — not a frozen API.**
  Shape it for maximum usefulness, density, and honesty, and **break from the past output
  shape when the past was not ideal**; backward-compat does not veto a better output. The
  only "contract" that constrains is an **internal seam between modules that work together**
  — keep that coherent as ordinary internal design. (The one genuine *output* contract is
  the governance/gate surface — verdict states + exit codes, which CI depends on and the
  VISION freezes — and it is not touched by this discovery-output change.) The IMPL picks
  the BEST orient/stats output shape and updates our own consumers (host hook shims,
  AGENTS.md/CLAUDE.md, any doc that describes the shape) to match — it is NOT constrained
  to additive-only.
- **D6 = one slice** (D1+D2+D4; D3 = optional second commit) — RATIFIED, rationale
  CORRECTED. Not "don't ship a half-coherent state" (a done slice is not shipped — gates +
  the release script sit between slice-done and a release). The reason is **minimize
  leftover tech debt + keep the surface coherent at each slice boundary**: relabelling
  stats without fixing orient would leave the tree half-migrated.

---

## 13. Amendment — follow-up contract for MODULE-MODEL-2 (operator + standalone reviewer, 2026-07-11)

**DELIVERED 2026-07-12 (`170be30`, relay-approved iteration 6 — 7 cycles, 2 ratified
escalations, operator close-out).** D4 per-toolchain fold (Rust crate / TS workspace
package; one shared rollup behind orient+stats) + D7 bounded tables with true omission
lines everywhere (JSON complete on both surfaces). Two decisions ratified during the run:
ROOT-MANIFEST-POLYGLOT — conservative rule (a root manifest owns descendants only when no
nested manifest root exists; never fabricated ownership) with a VISIBLE reader-frame
limitation marker on orient/stats; CARGO-WORKSPACE-INHERITANCE — option A, workspace-
inheriting crates degrade honestly to directory groups pending the upstream
`cargo-workspace-inheritance-1` slice. Self-dogfood: repo-graph's own orient names crates
(`storage, agent, repo-index, indexer, … · 281 package groups`); omission arithmetic true
at every tier (20+261, 50+231=281, 50+267=317). Review notes: the review-3 demand to cap
`--full`'s package-group section was UPHELD against the operator's initial challenge — the
ORIENT-DENSITY-1 enum contract ("Full = large with the complexity table uncapped, the only
uncapped tier") pre-existed this slice; the stale orient usage line was corrected instead.

The ratified package was put to a standalone adversarial review (gpt-5.6-sol, self-contained
prompt per `prompts/standalone-review.md`; the operator independently flagged the same scale
gap) ahead of deploying against a 160k-file polyglot monorepo. §12 STANDS — nothing below
re-opens D1–D6, and the 2026-06-23 delivery (`17dbe93`) already satisfies the D2
clarification below (one shared `package_groups` fold behind orient + stats). What is NOT
yet implemented — D7 and the D4 per-toolchain definitions — binds the follow-up slice
**MODULE-MODEL-2**.

### D7 — bounded presentation at scale (RATIFIED 2026-07-11: bounded human, complete JSON)

The near-term deployment target is a 160k-file polyglot monorepo; the §4 desired output
would print thousands of package groups. Ratified:

- **Human output is bounded:** orient (and stats' human table) shows the top-N groups by
  file count, deterministic tie-break (lexicographic path), followed by an explicit
  omission line naming the count and the drill-down ("… and N more groups — see
  `stats --json` / `modules`"). N integrates with orient's existing progressive budget
  ladder (C5 / HONEST-DEGRADATION-1) rather than adding a new knob.
- **Headline counts count ALL groups**, never only the displayed ones.
- **JSON output carries the complete group set** (machine consumers get the full topology).
- Display names must be collision-safe after prefix collapse (if two groups collapse to the
  same display name, disambiguate with the shortest distinguishing path suffix).
- Validation adds a scale acceptance: on a synthetic or real deep tree with >100 groups,
  human output stays within the budget ladder and the omission line is present and true.

### D2 clarification — authoritative input population (binding)

The package-group computation reads ONE authoritative population: the indexed-file set
behind the per-directory MODULE nodes / OWNS edges (what stats reads today). orient's
group counts and stats' group counts MUST derive from that same read; the §1 discrepancy
(49 repo / 47 stats / 30 module-owned files) is resolved by naming this source, and the
IMPL's acceptance counts are computed against it. Exclusions (untracked, ignored,
non-indexed) are whatever that population already excludes — no new filtering logic.

### D4 clarification — "logical package" per toolchain (binding)

"Merge by logical package" is defined per detected toolchain, not by basename:

- **Rust:** the crate (nearest `Cargo.toml`); groups below crate level only via D3 descent.
- **TS/JS:** the workspace package (nearest `package.json`); source-root collapse applies
  to `src/` inside it.
- **JVM:** logical package with `src/main|test/<lang>` source-root collapse merged, test
  counts shown (the §4 spring-petclinic shape — unchanged).
- **C/C++ and manifest-less trees:** directory groups (leaf-dir grouping as today),
  eligible for D3 umbrella descent.
- Basename merging across UNRELATED roots is forbidden — merge only within one
  source-root family; collision-safe display names per D7.
- Vendored/generated trees group like any other directory tree (no special casing in this
  slice; hotspot-pollution handling stays TECH-DEBT).

### D3 note — determinism + fixtures (validation-plan addition)

The umbrella descent (second commit, ratified scope — the §12 D6 word "optional" is
corrected: it is IN scope, merely sequenced second) must be deterministic (single-pass,
depth-bounded, documented stopping rule: descend single-child chains to the first
≥2-sibling fan-out, thresholds unchanged) and validated with fixtures for: a deep
single-child chain, multiple fan-outs at different depths, and a vendored-style
manifest-less layout.
