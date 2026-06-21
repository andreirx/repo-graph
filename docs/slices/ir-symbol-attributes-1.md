# IR-SYMBOL-ATTRIBUTES-1: Structural per-symbol attributes on the Layer-0 IR node

Slice ID: IR-SYMBOL-ATTRIBUTES-1
Status: **SPEC-FIRST — DESIGN, NOT IMPLEMENTED. No source code, no deletion, no schema/data migration
executed.** This document ratifies the PLAN for extending `IrNode` (the Layer-0 canonical IR node) with
the structural per-symbol attributes that `stats` needs and the IR currently lacks: symbol **visibility**,
**top-level/parent** status, and **symbol-kind** (interface / type-alias / class / enum / …) classification.
The IMPLEMENTATION is a follow-up build slice; the consuming `stats` served path is a SEPARATE follow-up
(STATS-LIVEGRAPH-IMPL-1) and is OUT OF SCOPE here.

Track: Stage D / IR data model — prerequisite for LiveGraph `stats`.
Depends (precedent, reused):
- INGEST-CORE-1 (`PartitionIr` / `IrNode` / the AST-adopt-then-SCIP-fallback identity model; the
  "subtype kept as a string to avoid coupling the IR to an extractor enum" precedent).
- PARTITIONED-WARM-CACHE-ARCH-1 + WARM-CACHE-1 (the cache-side mirror-DTO boundary, the `SCHEMA_VERSION`
  self-validation gate, the non-authoritative-rebuildable-cache framing — the format-bump precedent).
- VALUE-JOIN-1 (the per-symbol value-facts channel — the "structural attributes do NOT belong on the
  measurement channel" boundary that makes IrNode the correct home).
Consumer (the slice this unblocks): STATS-LIVEGRAPH-1 §D0 / DECISION_REQUIRED `STATS-IR-ATTRS`.

## Ratified decision this slice implements (do NOT re-open)

```text
STATS-IR-ATTRS = Option A + Option D.
  RATIFICATION SOURCE (authoritative): the operator slice-selection packet for THIS slice
    (IR-SYMBOL-ATTRIBUTES-1): "RATIFIED DECISION (do not re-open): STATS-IR-ATTRS = Option 1 — extend
    IrNode with structural Layer-0 fields (clean data ownership), as a prerequisite slice before the stats
    served-path implementation." Ratification is OPERATOR-supplied (selection packet + WHY_THIS_SLICE_NOW
    "Operator-ratified"); it is NOT derivable from the stats slice. The operator's "Option 1" maps onto the
    stats slice's own option labels as A (data home) + D (own prerequisite slice).
  OPTIONS + RATIONALE EVIDENCE [OBSERVED: docs/slices/stats-livegraph-1.md §D0 RECOMMENDATION :213-219 +
    DECISION_REQUIRED :374-377]: the stats slice ENUMERATES the options and RECORDS a recommendation plus an
    OPEN question (its DECISION_REQUIRED). That is evidence for WHAT the options and their rationale are —
    NOT operator ratification (the stats slice itself left STATS-IR-ATTRS OPEN; the operator ratified it via
    the selection packet for this slice).
  A (data home): extend IrNode (Layer-0) with the structural attributes — clean data ownership; structural
    facts belong on the IR node, NOT on the measurement (value-facts) channel.
  D (sequencing): the Layer-0 extraction change is its OWN prerequisite slice (THIS slice), ratified and
    validated independently BEFORE the query-serving fastpath (STATS-LIVEGRAPH-IMPL-1) consumes it.
This slice SPECS Option A's data shape + extraction + format/propagation. It does NOT re-litigate A vs B
(value-fact channel) vs C (partial-stats); those were rejected upstream. [Option enumeration OBSERVED:
stats-livegraph-1.md §D0 :182-219; ratification per the operator selection packet above.]
```

## Spec-first note (read first)

```text
This is a SPECIFICATION. It produces NO source code, NO table deletion, NO schema/data migration execution,
NO default flip, NO CLI change. It RATIFIES: (1) the exact new IrNode fields (names/types/semantics, each
justified by a stats consumer need — §New IrNode fields); (2) the extraction sourcing + fallback
(§Extraction); (3) the IR→LiveGraph→warm-cache propagation + the warm-cache FORMAT bump + compat plan
(§Propagation, §Format + versioning); (4) the validation plan the eventual IMPL must satisfy (§Validation).
Per the repo evidence law every claim is labelled OBSERVED (inspected first-hand: code file:line or doc) or
INFERRED (my judgment from those OBSERVED facts). All cited code/doc lines were read first-hand in this
authoring: repo-graph-ir/src/lib.rs, repo-graph-scip-ingest/src/lib.rs, repo-graph-warm-cache/src/lib.rs,
indexer/src/types.rs, storage/src/queries.rs, repo-graph-livegraph{,-feed}/src/lib.rs, and the two
referenced slice docs.
```

## Why now (priority path)

```text
[OBSERVED: CURRENT_SLICE.md banner :5 + Stage D order :191-198; stats-livegraph-1.md §Why-now :30-39.]
stats is the LAST SQLite-only drilldown default with no LiveGraph served path. The cert-fastpath model
(imports/cycles precedent) requires the LiveGraph to compute the FULL stats answer so a repo-wide compare
can gate the served path GREEN. The STRUCTURAL/degree half of stats (fan_in/fan_out/file_count/module set)
is computable from existing IR edges; the SYMBOL-CLASSIFICATION half (symbol_count / abstract_count /
type_count) is STRUCTURALLY uncomputable today because IrNode carries no visibility, no top-level/parent
attribute, and the value-facts channel is complexity-only [OBSERVED: stats-livegraph-1.md §Data dependency
:98-123]. This slice closes exactly that Layer-0 gap, and ONLY that gap. It is the first decommission
migration that hits a hard Layer-0 EXTRACTION-SUBSTRATE gap (imports/cycles reused existing edges and added
NO node attribute) [OBSERVED: stats-livegraph-1.md :120-123].
```

## Current state — what the IR carries and what it drops (OBSERVED, first-hand)

```text
THE IR NODE [OBSERVED: rust/crates/repo-graph-ir/src/lib.rs:290-308]:
  IrNode { key: CanonicalKey, subtype: String, name: String, range: Option<SourceRange>,
           partition_id: PartitionId, identity_source: IdentitySource, provenance: Provenance }
  *** NO `visibility`. NO `parent` / `is_top_level`. ***
  `subtype` is a free String, deliberately "kept as a string to avoid coupling the IR to an extractor
  enum" [OBSERVED: lib.rs:296]. Its doc-comment claims values like "FUNCTION","CLASS" [OBSERVED:
  lib.rs:295] — but see the DISCREPANCY below.

WHAT `subtype` ACTUALLY HOLDS — a load-bearing discrepancy [OBSERVED, first-hand]:
  scip-ingest sets `subtype: d.kind` where `d.kind` is the SCIP terminal-DESCRIPTOR suffix label
  [OBSERVED: repo-graph-scip-ingest/src/lib.rs:351-353 (matched), :374-383 (fallback synth)]. The decl
  filter admits only the four SCIP descriptor suffixes ["Namespace","Type","Method","Term"] [OBSERVED:
  lib.rs:332], and `d.kind` comes from `descriptors_info` → the SCIP `Descriptor.suffix` enum formatted
  to a string [OBSERVED: lib.rs:128-157]. So IrNode.subtype is one of {Namespace, Type, Method, Term},
  NOT {FUNCTION, CLASS, INTERFACE, TYPE_ALIAS, ENUM}. A SCIP `Type` descriptor covers class AND interface
  AND type-alias AND enum ALL as the single string "Type" — strictly COARSER than the {INTERFACE,
  TYPE_ALIAS,CLASS,ENUM} distinction stats needs [OBSERVED: stats-livegraph-1.md §Data dependency
  :112-113 "subtype PARTIAL"]. => IrNode.subtype CANNOT supply the symbol-kind classification; a NEW,
  granular attribute is required. (The granular subtype IS embedded inside the AST canonical KEY string
  `repo:file#name:SYMBOL:subtype[:dupN]` [OBSERVED: ingest-core-1.md :75,:81], but only for AST-adopted
  keys, and string-parsing a key for a structural field is fragile — REJECTED as a source; see
  §Extraction.)

WHERE THE ATTRIBUTES ALREADY EXIST AT THE PRODUCER BOUNDARY [OBSERVED: rust/crates/indexer/src/types.rs]:
  ExtractedNode (the ts-extractor's emitted node, mirror of the TS GraphNode) ALREADY carries all three
  [OBSERVED: types.rs:255-271]:
    - subtype: Option<NodeSubtype>           [:261]  — granular; NodeSubtype includes Interface[:139],
      Class[:137], Enum[:144], TypeAlias[:141], Function[:136], Method[:138], … [:134-197]. serde =
      SCREAMING_SNAKE_CASE [:133] → "INTERFACE","TYPE_ALIAS","CLASS","ENUM","FUNCTION",… (the EXACT spelling
      the SQLite stats compare uses).
    - parent_node_uid: Option<String>        [:265]  — None ⇔ top-level.
    - visibility: Option<Visibility>         [:268]  — Visibility = {Public,Private,Protected,Internal,
      Export} [:201-209]; serde = lowercase [:202] → "export" (the spelling the SQLite compare uses).

WHERE THE IR DROPS THEM [OBSERVED: repo-graph-scip-ingest/src/lib.rs:160-212]:
  scip-ingest reduces each producer node to `AstNodeLite { stable_key, name, cyclomatic, is_file_scope,
  line/col span }` [OBSERVED: struct :160-180; built :197-211, :498-512]. This reduction KEEPS only
  identity + span + complexity + file-scope flag and DISCARDS subtype, visibility, and parent_node_uid.
  => The data is NOT missing at the producer; it is THROWN AWAY in the AST→AstNodeLite reduction. This is
  the entire technical content of the slice: thread three already-emitted producer fields through a
  reduction that currently discards them, and land them on IrNode.

WHAT THE SQLite CONSUMER ACTUALLY COMPARES [OBSERVED: rust/crates/storage/src/queries.rs:1142-1146,
compute_module_stats]:
  export_count   = SUM(CASE WHEN n.visibility = 'export' THEN 1 ELSE 0 END)                         [:1142]
  abstract_count = SUM(CASE WHEN n.subtype IN ('INTERFACE','TYPE_ALIAS')
                            AND n.parent_node_uid IS NULL THEN 1 ELSE 0 END)                         [:1143-1144]
  type_count     = SUM(CASE WHEN n.subtype IN ('INTERFACE','TYPE_ALIAS','CLASS','ENUM')
                            AND n.parent_node_uid IS NULL THEN 1 ELSE 0 END)                         [:1145-1146]
  These three columns (visibility, subtype, parent_node_uid) on the SQLite `nodes` table are persisted
  FROM the SAME ExtractedNode fields above. => If the IR carries the SAME producer fields, IR-side stats
  equal SQLite-side stats BY CONSTRUCTION (same producer → same values), which is exactly what the
  STATS-LIVEGRAPH-IMPL-1 cert needs to gate GREEN. [INFERRED from the two OBSERVED producer/consumer reads.]

CONSEQUENCE FOR RISK [INFERRED, OBSERVED-backed]: the consumer slice flagged Option A as needing "a spike
  confirming scip-typescript/the ts-extractor reliably emit export-visibility + nesting + the 4 subtypes"
  [OBSERVED: stats-livegraph-1.md §D0-A :194-196]. That capability ALREADY EXISTS and ALREADY SHIPS: the
  SQLite stats path consumes these three producer fields in production (live ledger symbols=3699)
  [OBSERVED: stats-livegraph-1.md §Current-state live corroboration :71]. The spike therefore SHRINKS from
  "does the producer emit these?" (answered: yes) to "what is the per-subtype AST-adopt JOIN coverage for
  Type-descriptor defs, i.e. how many interface/type-alias/class/enum defs adopt an AST node vs fall back?"
  (§Validation, §Risks). This is a de-risking, not a new capability.
```

## New IrNode fields (the data shape — every field justified by a stats consumer need)

```text
Add ONE cohesive optional struct to IrNode, present iff the node is an AST-adopted SYMBOL node (i.e. it
has a matched producer ExtractedNode). Absent (None) for ScipSynthesizedFallback and AstFileScope nodes,
which have NO producer ExtractedNode and therefore NO honest structural attributes [OBSERVED: the fallback
+ FILE node build paths attach no AST node — repo-graph-scip-ingest/src/lib.rs:371-414]. This honours the
architecture's explicit-degradation rule: null = unknown, never conflated with known-zero [OBSERVED:
agent_docs/architecture.md §Mandatory Rules 6 :10].

  // repo-graph-ir/src/lib.rs — NEW, pure-domain (no new crate dependency; see §Format D8).
  /// Visibility classification of a symbol, mirrored from the producer. IR-owned (no extractor-enum
  /// coupling). Lowercase string spellings match the producer serde + the SQLite `visibility` column.
  pub enum IrVisibility { Public, Private, Protected, Internal, Export }

  /// Structural per-symbol attributes sourced from the AST-adopted producer node (IR-SYMBOL-ATTRIBUTES-1).
  /// Present ONLY for AST-adopted SYMBOL nodes; None on fallback/FILE nodes (unknown, not zero).
  pub struct SymbolAttributes {
      /// Producer visibility. `None` when the producer emitted no visibility for the symbol.
      pub visibility: Option<IrVisibility>,
      /// True iff the producer's `parent_node_uid` is None (top-level). Mirrors the SQLite
      /// `parent_node_uid IS NULL` predicate exactly (parity by construction).
      pub is_top_level: bool,
      /// Granular symbol kind: the producer `NodeSubtype` SCREAMING_SNAKE_CASE spelling
      /// ("INTERFACE","TYPE_ALIAS","CLASS","ENUM","FUNCTION",…). `None` when the producer emitted no
      /// subtype. Distinct from IrNode.subtype (which holds the COARSE SCIP descriptor suffix).
      pub symbol_kind: Option<String>,
  }

  // On IrNode:
  /// Structural per-symbol attributes (IR-SYMBOL-ATTRIBUTES-1). `Some` for AST-adopted SYMBOL nodes,
  /// `None` for SCIP-synthesized-fallback + FILE-scope nodes (which have no producer AST node).
  pub attributes: Option<SymbolAttributes>,

PER-FIELD JUSTIFICATION (each tied to a stats consumer need):
  | field                 | type                  | stats consumer need (OBSERVED: queries.rs)              |
  |-----------------------|-----------------------|--------------------------------------------------------|
  | attributes.visibility | Option<IrVisibility>  | symbol_count = SUM(visibility='export') [:1142]. Match  |
  |                       |                       | `== Export` for the export count.                       |
  | attributes.is_top_level| bool                 | abstract_count + type_count both require                |
  |                       |                       | `parent_node_uid IS NULL` [:1144,:1146]. is_top_level   |
  |                       |                       | := producer parent_node_uid.is_none() — exact mirror.   |
  | attributes.symbol_kind| Option<String>        | abstract_count: kind∈{INTERFACE,TYPE_ALIAS} [:1143];    |
  |                       |                       | type_count: kind∈{INTERFACE,TYPE_ALIAS,CLASS,ENUM}      |
  |                       |                       | [:1145]. The granular kind IrNode.subtype lacks.        |

  Note: instability/abstractness/distance are Rust-side ARITHMETIC over the above + the degree graph
  [OBSERVED: stats-livegraph-1.md §Current-state :64-67] — NOT new fields. No `measurements` table is
  involved (stats reads none) [OBSERVED: stats-livegraph-1.md §Current-state discrepancy :73-80].
```

### Forced decisions on the data shape (every cell filled — recorded; non-blocking)

```text
These are LOCAL data-shape refinements WITHIN the ratified Option A. Each is reversible (pure-domain struct
+ mechanical cache-mirror in one crate, no built downstream consumer yet — STATS-LIVEGRAPH-IMPL-1 is
sequenced AFTER). Per CLAUDE.md Decision Autonomy (match ceremony to blast radius) these are DECIDED AND
RECORDED here, not stopped on. The one genuinely load-bearing semantic question (the fate of the EXISTING
`subtype` field) is surfaced as DECISION_REQUIRED below.

DS1 — grouping: one Option<SymbolAttributes> vs three flat Option fields on IrNode.
  | option                          | trade-off                                                          |
  | (a) grouped Option<SymbolAttributes> [CHOSEN] | cohesive: the BLOCK is Some iff the node has an AST producer ExtractedNode, None otherwise — ONE marker for "AST-sourced symbol attrs present as a unit." A fallback/FILE node is a single None, not three separate Nones to keep in sync; one optional to reason about at the block level; single From/Into block. (Subfields stay independently Option INSIDE the block — see narrowing below.) |
  | (b) three flat Option fields    | flatter access; but loses the BLOCK-presence semantic: a fallback/FILE node becomes three independent Nones with no single "no AST producer" marker, and nothing structurally ties block-presence to AST-adoption (a future edit could half-populate it). The presence/absence-AS-A-UNIT fact is exactly what (a) encodes. |
  CHOSEN: (a). Rationale: the CONTAINER Some/None reflects AST-producer-node presence as ONE fact (the block
  is co-present or co-absent AS A UNIT) — one reason to change (SRP), one marker for "no producer node."
  EXPLICIT NARROWING: this couples ONLY the block's presence to AST-adoption; it does NOT couple the
  subfields to each other. `visibility` and `symbol_kind` remain INDEPENDENTLY Option<…> inside the block —
  the producer emits each as an independent Option [OBSERVED: indexer/src/types.rs:261 subtype, :268
  visibility] — so within an AST-adopted node, a Some-visibility + None-symbol_kind state is LEGAL and
  expected, NOT impossible. [INFERRED, with OBSERVED subfield-optionality.]

DS2 — symbol_kind representation: producer subtype STRING vs IR-owned SymbolKind enum vs full NodeSubtype mirror.
  | option                          | trade-off                                                          |
  | (a) Option<String> (producer SCREAMING_SNAKE_CASE spelling) [CHOSEN] | follows the EXISTING IR precedent ("subtype kept as a string to avoid coupling the IR to an extractor enum" [OBSERVED: lib.rs:296]); byte-parity with the SQLite string compare `subtype IN ('INTERFACE',…)` FOR FREE; forward-compatible (a new producer subtype passes through as a string, no IR enum edit). |
  | (b) IR-owned SymbolKind enum (symbol subtypes only) | type-safe; but duplicates ~15 NodeSubtype variants, must be edited whenever the producer adds a symbol subtype, and needs an explicit String spelling map to keep SQLite parity — more surface, no parity gain. |
  | (c) full NodeSubtype mirror     | couples the pure IR to a 30+-variant vocabulary incl. FILE/MODULE/STATE subtypes meaningless for SYMBOLs — over-broad; REJECT. |
  CHOSEN: (a). Consistent with the field already on IrNode, zero parity-mapping code, forward-compatible. [INFERRED.]

DS3 — visibility representation: IR-owned IrVisibility enum vs producer STRING vs is_export bool.
  | option                          | trade-off                                                          |
  | (a) Option<IrVisibility> enum [CHOSEN] | Visibility is a CLOSED 5-variant set [OBSERVED: types.rs:201-209] unlikely to grow — an enum is honest + type-safe here without the open-vocabulary problem of DS2; preserves the FULL fact (private/protected/internal), not just export, for future consumers (dead-code, API-surface). is_export derives as `== Export`. |
  | (b) Option<String> (lowercase)  | symmetric with DS2; but loses type-safety on a closed set with no forward-compat benefit (the set is closed). |
  | (c) is_export: bool             | smallest; but LOSSY — collapses {public,private,protected,internal} to "not export", discarding a real Layer-0 fact. Violates the honest-fact posture; a later consumer needing private/internal would re-extract. REJECT. |
  CHOSEN: (a). A closed enum is the faithful Layer-0 representation; carrying the full classification (not a
  lossy bool) is the rock-solid choice. The DS2/DS3 asymmetry (string kind, enum visibility) is deliberate:
  symbol-kind is an OPEN, growing vocabulary (string, decoupled); visibility is CLOSED (enum, type-safe). [INFERRED.]

DS4 — top-level: is_top_level bool vs parent identity vs a parent CONTAINS edge.
  | option                          | trade-off                                                          |
  | (a) is_top_level: bool [CHOSEN] | exactly the SQLite predicate `parent_node_uid IS NULL` [OBSERVED: queries.rs:1144,:1146]; stats needs ONLY the boolean; avoids importing the producer's node_uid identity (a DIFFERENT identity model from the IR's CanonicalKey/stable_key) into the IR. |
  | (b) parent: Option<CanonicalKey>| richer (enables nesting queries); but the producer gives parent_node_uid (node_uid, NOT stable_key), so it needs a node_uid→CanonicalKey parent map the ingest does not build today — new machinery for no stats benefit. |
  | (c) a parent/CONTAINS IrEdge    | most general; but a new edge TYPE is a larger architecture change, out of this slice's scope (stats needs a node attribute, not an edge). |
  CHOSEN: (a). Minimal, exact-parity, no second identity model. The richer (b)/(c) are noted as future
  options if a nesting consumer appears; NOT built here (no stated need — do not get ahead of scope). [INFERRED.]
```

## Extraction — where each field is sourced, and the fallback (OBSERVED + INFERRED)

```text
SOURCE (the precise plumbing the IMPL adds):
  1. Widen the AST reduction `AstNodeLite` [OBSERVED: repo-graph-scip-ingest/src/lib.rs:160-180] to CARRY
     the three producer fields it currently drops: from the `ExtractedNode` [OBSERVED: types.rs:255-271]
     keep `subtype` (NodeSubtype), `visibility` (Visibility), and `parent_node_uid.is_none()` (the
     top-level bool). This is the ONLY new data crossing the producer→reduction boundary; it is a STRICT
     WIDENING of an existing reduction (no new producer call, no new SCIP decode).
  2. In `build_partition_nodes`, on the AST-ADOPTED branch [OBSERVED: lib.rs:346-366], populate
     `IrNode.attributes = Some(SymbolAttributes { visibility: map(ast.visibility), is_top_level:
     ast.parent_node_uid.is_none(), symbol_kind: ast.subtype.map(serde_screaming_snake) })` from the
     matched AST node. The serde spelling is the producer's existing SCREAMING_SNAKE_CASE for NodeSubtype
     [OBSERVED: types.rs:133] and lowercase for Visibility [OBSERVED: types.rs:202] — the EXACT strings the
     SQLite path stores + compares (parity by construction).

THE ATTRIBUTE SOURCE IS THE AST, NOT SCIP (a deliberate authority statement):
  These are AST/producer structural facts. SCIP's role in this slice is UNCHANGED — it supplies the
  definition OCCURRENCE that triggers the (file, range, name) AST-adopt join [OBSERVED: lib.rs:235-244,
  identity model ingest-core-1.md :80-90]; SCIP contributes resolution/linkage, NOT identity and NOT these
  structural attributes. The attributes ride on the AST node the def ADOPTS. This keeps the IR's
  "SCIP is a producer, not the domain model" invariant intact [OBSERVED: repo-graph-ir/src/lib.rs:5-14].

FALLBACK — when there is no AST node (the honest-degradation cases) [OBSERVED + per architecture rule 6]:
  - ScipSynthesizedFallback nodes (a SCIP def with NO AST match) [OBSERVED: lib.rs:371-385]: there is no
    ExtractedNode → `attributes = None` (unknown). These are counted + surfaced as fallback already
    [OBSERVED: fallback_node_count lib.rs:369-376] — the symbol-attribute unknown rate is bounded by that
    same fallback rate.
  - AstFileScope (FILE) nodes [OBSERVED: lib.rs:392-414]: a FILE is not a SYMBOL; it has no visibility /
    kind / top-level meaning → `attributes = None`.
  Consequence for stats parity: a fallback type-def the SQLite path DID persist (with a real subtype) but
  the IR carries as `attributes=None` would UNDERCOUNT vs SQLite → the STATS-LIVEGRAPH-IMPL-1 cert compare
  sees a field mismatch → RED → SQLite fallback (NO wrong answer; a smaller GREEN set) [OBSERVED: the cert
  is a field-exact no-loss gate — stats-livegraph-1.md §D1 :223-233, RISK-4 :311-314]. This slice does NOT
  need correspondence to be perfect; it needs the attributes sourced FAITHFULLY and the unknowns HONEST.

NO SILENT INFERENCE: a node's attributes are EITHER the producer's real values (AST-adopted) OR None
  (no producer node). The IR never guesses visibility/kind/top-level from the SCIP descriptor, the name,
  or the key string. [INFERRED — the explicit-degradation rule applied to this field.]
```

## Propagation — IR → LiveGraph → warm-cache (OBSERVED end-to-end)

```text
The new `attributes` field rides on IrNode, so it flows through every existing carrier of a whole
PartitionIr WITHOUT a signature change on the propagation path. Verified first-hand:

  INGEST → IR: build_partition_nodes assembles IrNode into the PartitionIr [OBSERVED:
    repo-graph-scip-ingest/src/lib.rs:1114 `ir.nodes.extend(b.nodes)`].
  IR → LIVEGRAPH (live feed): feed_partition moves the WHOLE outcome.ir into the runtime —
    `lg.load_partition(id, outcome.ir, language)` [OBSERVED: repo-graph-livegraph-feed/src/lib.rs:74].
  IR → LIVEGRAPH (warm-cache feed): feed_partition_ir loads a DECODED PartitionIr the same way
    [OBSERVED: repo-graph-livegraph-feed/src/lib.rs:90-91].
  LIVEGRAPH residency: load_partition stores the IR resident; ResidentPartition holds `ir:
    Option<PartitionIr>` [OBSERVED: repo-graph-livegraph/src/lib.rs:318, :102]. The new fields are present
    on every resident node automatically.

=> PROPAGATION needs NO change to feed or livegraph signatures. The only edit OUTSIDE repo-graph-ir +
   repo-graph-scip-ingest is the warm-cache MIRROR DTO (next section), because bincode is a positional,
   non-self-describing format and the DTO must mirror the new fields to round-trip them. The eventual
   livegraph CONSUMPTION of these attributes (computing per-module symbol_count/abstract_count/type_count)
   is the IMPL slice's job (STATS-LIVEGRAPH-IMPL-1), NOT this one. [INFERRED from the OBSERVED carriers.]
```

## Format + versioning — the warm-cache binary impact and compat plan (OBSERVED-grounded)

```text
THE FORMAT IMPACT [OBSERVED: rust/crates/repo-graph-warm-cache/src/lib.rs]:
  The warm cache persists PartitionIr via a cache-side MIRROR DTO (D8: no serde on repo-graph-ir)
  [OBSERVED: lib.rs:19-22, CacheIrNodeDto :313-330, From/From conversions :597-622]. To round-trip the new
  IrNode.attributes the IMPL must:
    1. Add a `CacheSymbolAttributesDto` (+ `CacheIrVisibilityDto`) mirror, serde-deriving, in
       repo-graph-warm-cache (NOT in repo-graph-ir — the D8 boundary holds: structural serialization is
       infrastructure, the IR stays zero-dep) [OBSERVED: PARTITIONED-WARM-CACHE-ARCH-1 §D8 :126-146;
       group10 zero-dep invariant preserved].
    2. Add `attributes: Option<CacheSymbolAttributesDto>` to CacheIrNodeDto [OBSERVED: the struct to extend
       :313-330] + the two-way From conversions [OBSERVED: the conversion sites :597-622].
    3. Extend the round-trip test fixture (sample_partition_ir) so a node carries `Some(attributes)` and a
       fallback node carries `None`, proving the D8 semantic round-trip
       `PartitionIr → DTO → PartitionIr == ` [OBSERVED: the existing round-trip test + the v6
       external_node_modules precedent :1073, :1156-1175].

THE VERSIONING PLAN — BUMP SCHEMA_VERSION 6 → 7 (discard-and-reindex; the ESTABLISHED ratified pattern):
  [OBSERVED: SCHEMA_VERSION const :54; the v2..v6 bump-log precedent :45-53; the self-validation gate
  validate_manifest :819-824 → CacheError::SchemaMismatch on mismatch :81-88.]
  bincode is positional + non-self-describing, so adding a field changes the payload LAYOUT. A v6 cache
  read by v7 code would mis-deserialize. The ratified mechanism: `SCHEMA_VERSION` is a crate-owned constant
  self-validated BEFORE any decode; a mismatch is `SchemaMismatch` → the entry is DISCARDED and the
  partition is treated as needing a re-index [OBSERVED: PARTITIONED-WARM-CACHE-ARCH-1 §D3 :65-72, §D4
  :74-82]. This is exactly how v2 (import field), v3 (import_observations), v5 (source_file), and v6
  (external_node_modules / package fields) were handled [OBSERVED: lib.rs:45-53]. Applying it (→ v7) is
  DECIDE-AND-RECORD under that ratified precedent, NOT a new architecture decision. [INFERRED, precedent-backed.]

WHY A HARD BUMP, NOT serde(default) BACKWARD-DECODE [the rock-solid posture, explained]:
  Some prior fields (package_name, tsconfig_aliases) used `#[serde(default)]` for backward-compatible
  decode AND still forced a clean re-ingest via the bump [OBSERVED: lib.rs:51-53, :375-386]. For THIS
  field, serde(default) backward-decode of a v6 cache would yield `attributes = None` for EVERY node —
  i.e. a graph whose every symbol's visibility/kind/top-level is silently "unknown". stats over that graph
  would compute all-zero symbol/abstract/type counts. The cert compare would catch it (RED → SQLite
  fallback, no WRONG answer) — but serving a warm graph with silently-all-unknown structural attributes is
  a latent trust trap (it would also degrade any FUTURE consumer of these attributes). The honest choice,
  matching how prior bumps "force a clean re-ingest for the new evidence" [OBSERVED: lib.rs:52-53], is the
  HARD bump: a v6 cache is DISCARDED, the partition re-ingests under v7 code, and the warm graph ALWAYS
  carries real attributes. No data migration (caches are non-authoritative + rebuildable) [OBSERVED:
  PARTITIONED-WARM-CACHE-ARCH-1 framing :12-17]. [INFERRED, OBSERVED-backed.]

FINGERPRINT / CERT IMPLICATIONS [OBSERVED + INFERRED]:
  - The warm-cache CACHE KEY already includes `repo_graph_version` (runtime identity) [OBSERVED:
    CacheKey.repo_graph_version :141; KeyMismatch on change :825-827]. A runtime carrying the new field is
    a new repo_graph_version → any cache written by an older runtime is KeyMismatch-rejected too. The
    schema bump (SchemaMismatch) and the version key (KeyMismatch) are BELT-AND-SUSPENDERS: either alone
    invalidates a stale cache.
  - The IN-MEMORY (non-cached) IR is ALWAYS built by the running code, so it always carries the new
    attributes; the ONLY stale-attribute vector is the warm cache, closed by the bump above. [INFERRED.]
  - The STATS-LIVEGRAPH-IMPL-1 cert fingerprint is the SHARED import_cert_fingerprint (partition
    epoch/hash/producer + snapshot_uid + policy version) [OBSERVED: stats-livegraph-1.md §D2 :237-246].
    This slice adds NO new fingerprint input: the attributes are sourced from the SAME partition build,
    under the SAME build_inputs_hash. A partition re-ingested under v7 keeps its build_inputs_hash (same
    sources) but is a fresh in-memory IR with attributes — so the stats cert, when STATS-LIVEGRAPH-IMPL-1
    builds it, compares a fully-attributed LiveGraph against SQLite. No cert-model change is owed by THIS
    slice; it only makes the symbol half COMPUTABLE so that the IMPL's cert CAN be built. [INFERRED,
    OBSERVED-backed.]
```

## Back / forward compatibility + rebuild / refresh implications

```text
BACKWARD (old artifacts under new code):
  - Old warm caches (schema ≤ 6): rejected at the manifest schema gate → discarded → partition re-ingests
    [OBSERVED: the SchemaMismatch path :819-824]. No corruption, no partial trust.
  - SQLite raw graph: UNTOUCHED. This slice adds nothing to and removes nothing from SQLite. The SQLite
    stats path (compute_module_stats) continues to serve unchanged; it remains the fallback + the cert
    oracle for STATS-LIVEGRAPH-IMPL-1. [OBSERVED: no queries.rs edit in scope.]

FORWARD (new attribute, future producers / subtypes):
  - symbol_kind as a STRING (DS2) accepts any future NodeSubtype spelling with no IR edit; a brand-new
    producer subtype simply appears as its string and is matched by whatever consumer cares.
  - A new visibility variant WOULD require an IrVisibility enum edit (DS3) — accepted trade-off for the
    closed set; if the producer's Visibility grows, IrVisibility grows in lockstep (a one-line mirror).
  - Non-TS partitions: the producer ExtractedNode model is the shared extractor contract; for any extractor
    that does not emit subtype/visibility/parent, the fields arrive as None (honest unknown), and stats
    (TS-only by §Scope) falls back to SQLite for non-TS regardless [OBSERVED: stats-livegraph-1.md §D5
    :280-281]. No non-TS behavior change here.

REBUILD / REFRESH [OBSERVED: PARTITIONED-WARM-CACHE-ARCH-1 §D6 :94-104]:
  - Daemon start: a valid v7 cache loads warm WITH attributes; a stale (≤ v6) cache is discarded → the
    partition is ProducerUnavailable until a SCIP refresh re-ingests it under v7 [OBSERVED: D6 load/refresh
    :95-104]. This is the normal cold-after-format-bump path, identical to every prior bump.
  - Successful refresh (1C path): writes the v7 cache AFTER the PartitionIr (now with attributes) is
    produced; a cache-write failure must NOT block serving the fresh in-memory state [OBSERVED: D6
    :100-104]. Attributes are present on the fresh in-memory IR regardless of cache outcome.
  - Delta refresh: a partition re-ingested by delta re-runs build_partition_nodes → attributes are
    repopulated; copy-forward of an UNCHANGED partition copies the whole cached PartitionIr (attributes
    included). [INFERRED — attributes are a pure function of the partition build, so they follow the
    partition's refresh unit.]

PERSISTENCE-COMPLETENESS CHECKLIST [agent_docs/architecture.md :57-69], for the eventual IMPL:
  [✓ to-build] write path (build_partition_nodes populates attributes; warm-cache DTO encodes them).
  [✓ to-build] read path (decode reconstructs; livegraph holds them — CONSUMPTION is STATS-LIVEGRAPH-IMPL-1).
  [✓ to-build] refresh / copy-forward / invalidation (schema bump + repo_graph_version key, above).
  [✓ to-build] trust / maturity impact (Layer-0 fact; None = honest unknown; fallback rate bounds unknowns).
  [n/a here]  CLI visibility — NO CLI change in THIS slice; the stats CLI served path is STATS-LIVEGRAPH-IMPL-1.
  [✓ to-build] validation covers fresh index AND refresh (§Validation).
```

## Validation plan (how the eventual IMPLEMENTATION would be proven — NOT run here)

```text
SUPPORT / OFF-TARGET UNIT (pure, headless — the architecture's off-target-testability mandate):
  - AstNodeLite widening: a fixture ExtractedNode with known {subtype=Interface, visibility=Export,
    parent_node_uid=None} → assert the reduced AstNodeLite carries them.
  - build_partition_nodes population: an AST-adopted def → assert IrNode.attributes == Some({Export,
    is_top_level=true, "INTERFACE"}); a fallback def → assert attributes == None; a FILE node → None.
  - serde-spelling parity: assert the symbol_kind string == the producer NodeSubtype SCREAMING_SNAKE_CASE
    AND the visibility maps to the lowercase producer spelling — i.e. the EXACT strings queries.rs:1142-1146
    compares ("export","INTERFACE","TYPE_ALIAS","CLASS","ENUM"). This is the parity-by-construction proof.
  - warm-cache D8 round-trip: PartitionIr with Some(attributes) + a None-attributes node →
    DTO → PartitionIr == (the existing round-trip test extended); a v6 cache → SchemaMismatch → discard.

PARITY (the central correctness claim — TO EXECUTE in the eventual IMPL on the real corpus via
dev-install-local; NOT run in this spec-only slice):
  - Ingest a real TS partition; for every AST-adopted SYMBOL node, assert IrNode.attributes equals the
    producer ExtractedNode's {visibility, parent.is_none(), subtype} — which equals the SQLite `nodes`
    row's {visibility, parent_node_uid IS NULL, subtype} (same producer → same values). This proves the
    IR can reproduce the SQLite symbol-classification inputs.
  - Per-subtype JOIN-coverage spike (the de-risked residual): MEASURE, per {INTERFACE, TYPE_ALIAS, CLASS,
    ENUM}, the AST-adopt rate vs fallback rate on real repos (e.g. xpart, amodx). Report the fallback
    (attributes=None) fraction for type-like defs — the bound on stats undercount vs SQLite. Any gap →
    those modules cert RED → SQLite fallback (no wrong answer, smaller GREEN set). Honest number, surfaced.

INTEGRATION (the propagation proof):
  - feed_partition a real IngestOutcome → assert the resident LiveGraph nodes carry attributes; warm-cache
    encode → decode → feed_partition_ir → assert attributes survived the cache round-trip.

GATE: cargo test --workspace 0 failures; clippy -D warnings clean; fmt clean.
EVIDENCE LABELS: each result EXECUTED / OBSERVED per agent_docs/validation.md; no INFERRED presented as
  OBSERVED. THIS SPEC owed no code/test (spec-only deliverable); the validation above is what the IMPL must
  satisfy.
```

## Scope boundary (hard guardrails)

```text
IN SCOPE (this slice, when built): the Layer-0 EXTRACTION + IR DATA-SHAPE + warm-cache FORMAT prerequisite
  ONLY — add IrNode.attributes (SymbolAttributes), thread the three producer fields through AstNodeLite +
  build_partition_nodes, mirror them in the warm-cache DTO, bump SCHEMA_VERSION 6→7, and prove parity +
  round-trip. This SPEC produces NONE of that code (spec-first).

OUT OF SCOPE (hard):
  - The stats SERVED PATH — the cert, the fastpath ladder, the livegraph per-module stats COMPUTATION, the
    `--engine` surface, the default flip — is STATS-LIVEGRAPH-IMPL-1 [OBSERVED: stats-livegraph-1.md §Target
    :161-178, §D3 :248-260]. This slice only makes the symbol half COMPUTABLE; it computes nothing.
  - NO source code, NO table deletion, NO schema/data migration EXECUTED, NO default flip, NO CLI change.
  - NO SQLite raw decommission (SQLite stays the fallback + cert oracle).
  - NO non-TS support, NO resolver / module-identity change, NO value-facts (measurement-channel) change —
    structural attributes go on IrNode, NOT the value channel (the whole point of A over B).
  - NO edit to docs/slices/stats-livegraph-1.md, ROADMAP.md, or CURRENT_SLICE.md.
  - NO repurpose/rename/deletion of the EXISTING IrNode.subtype field (see DECISION_REQUIRED) — this slice
    is strictly ADDITIVE.
```

## DECISION_REQUIRED

```text
DECISION_REQUIRED:
- ID: IR-SUBTYPE-FIELD-FATE
  QUESTION: The EXISTING `IrNode.subtype: String` field documents itself as holding "FUNCTION"/"CLASS"
    [OBSERVED: repo-graph-ir/src/lib.rs:295] but is ACTUALLY populated with the COARSE SCIP terminal-
    descriptor suffix {Namespace, Type, Method, Term} [OBSERVED: repo-graph-scip-ingest/src/lib.rs:351-353,
    :332]. This slice ADDS a separate, granular `attributes.symbol_kind` (the AST subtype) and LEAVES
    `subtype` untouched. Should a FOLLOW-UP slice (a) leave `subtype` as-is and only CORRECT its
    doc-comment to say "SCIP descriptor suffix"; (b) repurpose `subtype` to hold the granular AST subtype
    (making the field match its current doc-comment), retiring the SCIP-suffix value; or (c) rename it to
    `scip_descriptor_kind` for clarity and keep the granular kind solely on `attributes.symbol_kind`?
  OPTIONS:
  - (a) leave + fix the doc-comment only: zero data-shape change; smallest; the misleading comment is
    corrected; `subtype` (SCIP suffix) remains available as provenance. Two kind-ish fields coexist
    (subtype=coarse SCIP, attributes.symbol_kind=granular AST) — documented, not ambiguous.
  - (b) repurpose `subtype` to the granular AST subtype: one authoritative kind field; BUT changes the
    SEMANTICS of an existing Layer-0 field — any current/future reader of subtype-as-SCIP-suffix breaks,
    and it forces a warm-cache value change (another bump). (No livegraph/feed consumer reads IrNode.subtype
    today [OBSERVED: grep of repo-graph-livegraph{,-feed} for `.subtype` returned none], which LOWERS but
    does not erase the risk — provenance/diagnostic readers may exist elsewhere.)
  - (c) rename to `scip_descriptor_kind` + keep granular kind only on attributes.symbol_kind: clearest
    final names; BUT a rename is a wider mechanical churn across the cache DTO + any reader, for a naming
    gain.
  RECOMMENDED: (a) for THIS prerequisite (additive, zero risk: add attributes.symbol_kind, correct the
    subtype doc-comment in the same edit), and DEFER (b)/(c) to a separate IR-field-cleanup slice if the
    operator wants a single authoritative kind field. This keeps IR-SYMBOL-ATTRIBUTES-1 strictly additive
    and free of a false-continuity risk on an existing field.
  BLOCKING_REASON: NOT blocking this slice — the recommended additive path (add `attributes`, leave
    `subtype`, fix only its doc-comment) lets the build proceed with zero ambiguity and zero false-trust
    risk. This DECISION_REQUIRED records a PRE-EXISTING doc-vs-code discrepancy (not introduced here) and
    asks for a ruling on the EXISTING field's longer-term fate, since repurposing/renaming an existing
    Layer-0 data shape is an architecture-boundary call per CLAUDE.md Decision Autonomy — to be ratified
    before any cleanup slice, NOT silently chosen here.
```

## References

- `rust/crates/repo-graph-ir/src/lib.rs` — `IrNode` :290-308 (NO visibility/parent; `subtype` String :295-297, "string to avoid extractor-enum coupling" :296); `PartitionIr` :330-341; SCIP-is-a-producer invariant :5-14.
- `rust/crates/repo-graph-scip-ingest/src/lib.rs` — `AstNodeLite` reduction (drops subtype/visibility/parent) :160-180, :197-211, :498-512; `build_partition_nodes` (`subtype: d.kind`) :346-385; decl set + `descriptors_info` (SCIP suffix vocabulary) :128-157, :332; fallback/FILE node build :371-414; `ir.nodes.extend` :1114.
- `rust/crates/indexer/src/types.rs` — `ExtractedNode` (subtype/parent_node_uid/visibility) :255-271; `NodeSubtype` (Interface/Class/Enum/TypeAlias) :134-197, SCREAMING_SNAKE_CASE serde :133; `Visibility` (Export) :201-209, lowercase serde :202.
- `rust/crates/storage/src/queries.rs` — `compute_module_stats` symbol-classification CASE WHENs (visibility='export'; subtype IN ('INTERFACE','TYPE_ALIAS'[,'CLASS','ENUM']); parent_node_uid IS NULL) :1142-1146.
- `rust/crates/repo-graph-warm-cache/src/lib.rs` — `SCHEMA_VERSION` const + v2..v6 bump-log :44-54; `CacheIrNodeDto` :313-330 + From conversions :597-622; `validate_manifest` schema gate :808-835; `SchemaMismatch` :81-88; D8 mirror-DTO / IR-purity note :19-22; round-trip test + v6 field precedent :1073, :1156-1175.
- `rust/crates/repo-graph-livegraph-feed/src/lib.rs` — `feed_partition` → `load_partition(outcome.ir)` :74; `feed_partition_ir` (warm-cache path) :90-91.
- `rust/crates/repo-graph-livegraph/src/lib.rs` — `load_partition(ir: PartitionIr)` :318; `ResidentPartition.ir: Option<PartitionIr>` :102.
- `docs/slices/stats-livegraph-1.md` — §Data dependency (per-field verdict; IrNode has no visibility/parent) :83-123; §D0 (Option A+D, the ratified decision) :182-219; §D1 cert no-loss predicate :223-233; §D2 fingerprint :237-246; RISK-3/4 (extraction gap, subtype coverage) :309-314; DECISION_REQUIRED STATS-IR-ATTRS :353-384.
- `docs/slices/partitioned-warm-cache-arch-1.md` — framing (non-authoritative/rebuildable/validated) :12-17; D3 cache key :55-72; D4 validate-before-load :74-82; D6 refresh interaction :94-104; D8 cache-side mirror DTO, NO serde in repo-graph-ir :126-146.
- `docs/slices/ingest-core-1.md` — canonical IR subset + identity model (AST-adopt then SCIP-fallback; key encodes `:SYMBOL:subtype`) :46-97.
- `agent_docs/architecture.md` — Layer 0 = extraction substrate (symbols, stable keys) :18; explicit-degradation rule (null=unknown) :10; persistence-completeness checklist :57-69; build order :49-55.
