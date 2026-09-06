# SYMBOL-IDENTITY-1 — what `find` prints, `explain` and `callers` accept

Status: SPECIFIED (2026-09-06) · Track: surfaced by the codegraph comparison, root-caused
(`docs/audits/2026-09-06-root-causes-v0.17.0.md` §H-A). NEW SCOPE — proposed position: after
CPP-DECLARATORS-1 (it consumes that slice's decl flag for the C++ decl/def tie-break; the
suffix step itself needs no reindex). CODE slice, storage resolve_symbol + agent explain +
daemon dispatch. Maturity: MATURE (every drill-down starts here).

## 1. Problem (ROOT-CAUSED)

Outward surface: the hand-off from `find` to `explain` / `callers` / `callees`.
`rmap find DBImpl::Recover` → `leveldb::DBImpl::Recover — db/db_impl.cc:292 [METHOD]`;
`rmap explain DBImpl::Recover` → "Target: DBImpl::Recover (unresolved: no_match) / Confidence:
high"; `rmap callers OwnerController.processCreationForm` → "symbol not found" while the error's
own hint lists `org.springframework.samples.petclinic.owner.OwnerController.processCreationForm`.
An agent that reads a `find` row cannot drill into it without hand-assembling the stable key.

Cause: one identity, three resolvers. `find` matches a case-insensitive SUBSTRING over
`name` + `qualified_name` (`storage/src/find_facts_reads.rs:157-166`); `explain` matches the exact
SHORT NAME only (`agent/src/explain/mod.rs:185` → `storage/src/agent_impl.rs:974`, never
`qualified_name`); `callers` matches exact stable_key → exact qualified_name → exact name
(`storage/src/queries.rs:653-691`). `qualified_name` means different things per extractor
(C++ namespace-qualified `leveldb::DBImpl::Recover`; Java package-qualified
`org….OwnerController.processCreationForm`; Python/Rust container-relative
`BaseHandler.get_response`), so `DBImpl::Recover` and `OwnerController.processCreationForm` are
SUFFIXES of the stored value that no resolver tries. The hint that lists the FQN is the
embedding fallback (`dispatch_seed.rs:177-208`), not lexical matching. "Confidence: high" on a
miss is a static literal (`explain/mod.rs:276`, preserved verbatim by `coherent.rs:334`) —
nothing computes it. Never worked (e01386f, fda7adc).

## 2. Contract

1. **One resolver.** `explain` resolves through `storage.resolve_symbol` (the `callers` path),
   mapping `Ambiguous(keys)` to its existing up-to-5-candidate focus; `resolve_symbol_name`
   (name-only) is retired or reduced to a thin call. The livegraph focus resolver's parity
   comment (`focus_resolver/mod.rs:257-260`) is updated to the new contract.
2. **A qualified suffix resolves.** `resolve_symbol` gains ONE step after the exact-name miss:
   `qualified_name` equals the query OR ends with `<sep><query>` where `<sep>` is the language's
   separator (`::` or `.`); exactly one hit resolves; more than one → `Ambiguous` listing the
   candidates with their files (honest); zero → not found. `DBImpl::Recover`, `BaseHandler.
   get_response`, `OwnerController.processCreationForm`, `ServiceDispatcher.dispatch` all
   resolve; a bare `Recover` on leveldb stays ambiguous (4 candidates → 2 after
   CPP-DECLARATORS-1's decl filter → still 2: DBImpl vs VersionSet).
3. **Decl/def tie-break.** When the candidates differ only by declaration-vs-definition
   (the CPP-DECLARATORS-1 flag), the definition wins; and when the exact-name candidates are exactly one TYPE plus its constructors, the type wins with a rendered pointer to the constructor (rule ratified in CPP-DECLARATORS-1 §2.3, 2026-09-07 — this slice inherits it in the shared resolver, it does not re-decide it); `callers leveldb::DBImpl::Recover` no
   longer says "ambiguous … hint: use qualified name" for a qualified name.
4. **A miss is not "Confidence: high".** `build_no_match` and the ambiguous arm derive their
   confidence from the same source the resolved path uses, or state `low`; the render never
   prints `high` beside `no_match`.
5. **Movement measured (isolated; "before" = a COPY of the retained root served with auto
   passes off, or a `git worktree` before-binary on a fresh isolated index — NEVER the retained
   root itself (it is not read-only under a serving daemon, bitten 2026-09-07); items 1, 2, 4
   need no reindex; item 3 needs the C++ reindex from CPP-DECLARATORS-1):**
   the four hand-off commands above before/after, verbatim; `explain get_response` (bare)
   still ambiguous with 8 candidates; `callers` on the resolved symbols returns what the
   resolved edges hold (the under-report itself is CPP-DECLARATORS-1's / the no-resolver gap's,
   not this slice's — state the counts, do not claim them).

## 3. Stop conditions

Frozen: wire protocol (additive), storage schema, exit codes, `find`'s substring semantics
(unchanged), the classification of unresolved calls. If the suffix step needs an index to stay
fast on django-size stores (78k symbols; `qualified_name` LIKE with a leading wildcard is a
scan), measure it and, if > 50 ms, STOP + DECISION_REQUIRED with the index option. STANDING
HONESTY RULES. Unmet DoD → STOP + DECISION_REQUIRED. Never touch the operator's real state
root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

Failing tests FIRST: `storage/src/queries.rs:3751-3840` family gains
`resolve_symbol_accepts_qualified_suffix` (C++ `::`, Java `.`, Python `.`),
`resolve_symbol_suffix_ambiguous_lists_candidates`, `resolve_symbol_prefers_definition`;
`rgr/src/presentation/explain.rs:473-477` fixture asserts no `high` beside `no_match`; an
explain test through the shared resolver. Live proof (isolated): the §2.5 commands on leveldb,
django, spring-petclinic, repo-graph. Gates recorded FIRST; chunked cargo; witness;
dogfood-isolated.

## 5. Definition of done

Every row `find` prints resolves in `explain`/`callers`/`callees` by its printed qualified name;
ambiguity is listed, never "not found"; a miss never claims high confidence; one resolver;
gates green.

CORPUS PATHS: leveldb, django, spring-petclinic at ../legacy-codebases/<name>; repo-graph is
THIS repo.
