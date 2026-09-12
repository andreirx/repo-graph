# EXPLAIN-TYPE-SECTIONS-1 — `explain <Type>` describes the type: members, bases, derived, referenced by

Status: SPECIFIED (2026-09-12) · Track: audit round six, Q6 (HIGH; never worked, language-wide). THREE increments, each independently shippable and reviewed: (1) RENDER — members + referenced-by from existing rows, every language, no reindex; (2) C++ ANCHOR — base-clause edges on the type node; (3) C++ MACRO RECOVERY — base clauses on macro-decorated declarations. This packet is increment 1; increments 2–3 are packeted after 1 ships. Builder: Codex gpt-5.6-sol; reviewer: Codex gpt-5.6-terra.

## 0. Requirements allocation

**Implements (inc 1):** RG-REQ-005-L04 (members and referenced-by halves), RG-REQ-005-L09 (the type zero-state says "a type is not called"), RG-REQ-002-L07 (degradation with next action — "bases: not indexed for C++ on this build" until inc 2).
**Implements (inc 2–3, later):** RG-REQ-005-L04 bases/derived halves; unblocks CALL-BINDING-RECEIVER-1's base walk.

**Changes:** every `explain <Type>` answer in every language gains a Members section (count, anchored rows, capped with `+N more`) and a Referenced-by section (files importing the type's file, grouped by module, capped); the explain output-contract tests move; `Callers (0) / Callees (0)` for a type is accompanied by "a type is not called — see Members / Referenced by".

**Preserves (§3):** RG-REQ-005-L05/L06 (resolution, confidence), RG-REQ-003-L01 (cycles block after Q4), RG-REQ-010-L01 (seed block), RG-REQ-012-L07 (anchors), RG-REQ-001-L04 (stable keys), the nodes-free-on-green routing (ruling B), byte-identical `explain` for FUNCTION/METHOD focus.

## 1. Problem (ROOT-CAUSED — RC-3)

`agent/src/explain/mod.rs:467-697 explain_symbol` emits exactly IDENTITY/CALLERS/CALLEES/CYCLES/TRUST/MEASUREMENTS for every subtype; a CLASS is never called → 0/0 by construction; `explain_sections.rs:84-97` has no members/bases/derived/referenced-by section. The only member listing is `EXPLAIN_SYMBOLS` inside `explain_file` (a FILE listing reachable only by exact path — SYMBOL-IDENTITY-1's FRAKTAG proof happened to hit it). The data exists: vcmi `CGHeroInstance` has 133 METHOD + 1 CONSTRUCTOR + 1 DESTRUCTOR nodes with `qualified_name LIKE 'CGHeroInstance::%'` and 178 IMPORTS edges into its header's FILE node. C++ bases are additionally anchored on the FILE node (`cpp-extractor/src/extractor.rs:1288`, since e0fa892) and dropped on macro-decorated declarations (`:1276-1278`; 451 `class DLL_LINKAGE` in vcmi/lib).

## 2. Contract (increment 1)

1. When the focus subtype is a type (`is_type_subtype`, `mod.rs:324`), `explain_symbol` pushes `EXPLAIN_MEMBERS` from a new `AgentStorageRead::list_members_of_type(snapshot_uid, qualified_name)` — nodes whose `qualified_name` is `<qn>::<name>` or `<qn>.<name>` with no further separator, ordered by `line_start`, with kind/subtype and anchor — and `EXPLAIN_REFERENCED_BY` from IMPORTS edges targeting the focus symbol's FILE node, grouped by owning module with counts. Both are additive signals; JSON additive.
2. The renderer adds Members (cap per budget with `+N more — --full`) and Referenced by (top modules + file count; cap) sections; for a type focus the Callers/Callees zero lines carry "a type is not called — see Members / Referenced by".
3. Bases/derived: inc 1 renders "Base classes: not recorded from a type focus on this build (C++ inheritance edges are file-anchored — EXPLAIN-TYPE-SECTIONS-1 increment 2)" for C++ and omits the section where no inheritance edges exist; never a fabricated list.
4. Outward proof: vcmi `explain CGHeroInstance` → Members 135 anchored at `lib/mapObjects/CGHeroInstance.h:<line>`, Referenced by 178 files with top modules lib/, client/, server/, AI/; django `explain BaseHandler` lists its methods; FRAKTAG `explain ConversationManager` BY NAME lists its methods (proves the fix is not path-focus).

## 3. Regression watch

| Preserved L | What would regress | Proof |
|---|---|---|
| RG-REQ-005-L05 | resolution routing (ruling B: `resolve_symbol` delegated; `resolve_symbol_name` name-only) | `explain_serve_tests` spy expectations; `explain_symbol.rs::explain_resolves_qualified_suffix_through_shared_resolver` |
| nodes-free on green | the new members read must be a decorator-served or explicitly SQLite-delegated port method like `count_symbol_definitions_by_name` — state which | `explain_serve_tests/spy.rs` green; record the choice |
| RG-REQ-005-L06 | confidence | `no_high_confidence_beside_no_match` |
| RG-REQ-003-L01 / RG-REQ-004-L07 | cycles block | after Q4: unordered form unchanged |
| RG-REQ-010-L01 | seed candidates block | `semantic_no_match_renders_labeled_candidates_in_human_mode` |
| RG-REQ-012-L07 | anchors from one store | `tier0_*`/`tier1_*` tests; the member rows' lines come from `nodes.line_start` only |
| FUNCTION/METHOD focus | byte-identical | `explain leveldb::DBImpl::Recover` diff shows no change |
| RG-REQ-012-L04 | budgets change length, not facts | member count identical across `--budget` tiers; cap stated |
| RG-REQ-001-L04 | no identity change | no stable-key or schema change |

## 4. Stop conditions

Frozen: wire (additive), schema (no new column — the member query uses `qualified_name`), exit codes, the resolution routing, the LiveGraph parity cert, `explain_file`'s existing sections. If `list_members_of_type` cannot be served honestly on green without violating the spy, delegate to SQLite (the ruling-B precedent) and record it. Inc 2 (re-anchoring ~850 C++ IMPLEMENTS edges) is NOT part of this packet. STANDING HONESTY RULES. Do NOT commit.

## 5. Validation (ORDERED)

1. Failing tests FIRST: `list_members_of_type` on a fixture with `A::m1`, `A::m2`, `A::Inner::m3` (returns m1, m2; not m3); renderer tests for Members/Referenced-by/type zero-line; a FUNCTION focus renders no Members section.
2. `cargo test -p repo-graph-storage`, `-p repo-graph-agent`, `-p repo-graph-rgr --lib presentation::explain`, `-p repo-graph-daemon-runtime --lib explain`.
3. Live proof: isolated FRAKTAG index (`ConversationManager` by name) and isolated leveldb (`explain leveldb::DBImpl` the class; `explain leveldb::DBImpl::Recover` byte-identical); vcmi/django from retained copies are OPERATOR-RUN.
4. `build-N.md`.

## 6. Definition of done

§2.4 holds on FRAKTAG and leveldb (vcmi/django operator-run); §3 green; gates green; inc 2/3 packets filed with the measured IMPLEMENTS shape histogram as their before.

CORPUS PATHS: FRAKTAG at ../FRAKTAG; leveldb at ../legacy-codebases/leveldb; vcmi/django retained copies under ~/repo-graph-retained/audit-v0.18.0.
