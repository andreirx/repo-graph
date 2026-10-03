//! Explain use case — multi-section detail pipeline.
//!
//! `run_explain` resolves a target to a symbol, file, or path area
//! and emits typed explain sections covering identity, callers,
//! callees, imports, symbols, files, dead code, cycles, boundary
//! violations, gate, trust, and measurements.
//!
//! Focus resolution reuses `orient`'s resolution logic so the same
//! target string resolves to the same entity in both commands.
//!
//! EXPLAIN-LIVEGRAPH-IMPL: the [`coherent`] submodule wraps this use case's
//! bare [`OrientResult`] into a `CoherenceEnvelope<CoherentOrientResult>`
//! (per-leaf provenance/trust/freshness + the root MEET). The section logic
//! below is UNTOUCHED — the coherence layer wraps the answer, it does not
//! re-aggregate it.

mod call_ranking;
pub mod coherent;

pub use coherent::{explain_to_coherent, ExplainLgDecisions};

use repo_graph_gate::GateStorageRead;

use crate::confidence::derive_repo_confidence;
use crate::dto::budget::Budget;
use crate::dto::envelope::{
    Confidence, Focus, NextAction, NextKind, OrientResult, EXPLAIN_COMMAND, ORIENT_SCHEMA,
};
use crate::dto::signal::*;
use crate::errors::{AgentStorageError, ExplainError};
use crate::ordering;
use crate::ranking;
use crate::storage_port::{
    AgentCancelCheck, AgentFocusCandidate, AgentReliabilityLevel, AgentSnapshot, AgentStorageRead,
    AgentSymbolContext, AgentSymbolResolution,
};

/// Items cap per budget tier (medium minimum, large optional).
fn items_cap(budget: Budget) -> usize {
    match budget {
        Budget::Small | Budget::Medium => 15,
        Budget::Large => 50,
        // TRUNCATION-AUDIT-1: `--full` uncaps every per-section item list.
        Budget::Full => usize::MAX,
    }
}

/// Truncate a list to the items cap and return truncation metadata.
fn truncate_items<T>(items: &mut Vec<T>, cap: usize) -> (Option<bool>, Option<u64>) {
    if items.len() <= cap {
        (None, None)
    } else {
        let omitted = items.len() - cap;
        items.truncate(cap);
        (Some(true), Some(omitted as u64))
    }
}

/// Entry point for the explain use case.
pub fn run_explain<S: AgentStorageRead + GateStorageRead + ?Sized>(
    storage: &S,
    repo_uid: &str,
    target: &str,
    budget: Budget,
    now: &str,
) -> Result<OrientResult, ExplainError> {
    run_explain_cancellable(storage, repo_uid, target, budget, now, &mut || {
        std::ops::ControlFlow::Continue(())
    })
}

/// Cancellable entry point for the explain use case (DAEMON-CANCEL-3).
///
/// Identical to [`run_explain`] but threads a cooperative `cancel` checkpoint into
/// the module-cycle Tarjan it reaches on the path/symbol-focus pipelines
/// (`explain_path`/`explain_symbol`). The daemon's explain handler passes a
/// checkpoint built from its request emitter; [`run_explain`] passes a no-op,
/// preserving byte-identical behavior for every other caller. The file-focus
/// pipeline reaches no cycle Tarjan, so it is not threaded (NARROW scope).
pub fn run_explain_cancellable<S: AgentStorageRead + GateStorageRead + ?Sized>(
    storage: &S,
    repo_uid: &str,
    target: &str,
    budget: Budget,
    now: &str,
    cancel: AgentCancelCheck<'_>,
) -> Result<OrientResult, ExplainError> {
    // Budget: minimum medium.
    let budget = match budget {
        Budget::Small => Budget::Medium,
        other => other,
    };

    // ── 1. Resolve repo identity. ────────────────────────────
    let repo = storage
        .get_repo(repo_uid)?
        .ok_or_else(|| ExplainError::NoRepo {
            repo_uid: repo_uid.to_string(),
        })?;

    // ── 2. Resolve snapshot. ─────────────────────────────────
    let snapshot =
        storage
            .get_latest_snapshot(repo_uid)?
            .ok_or_else(|| ExplainError::NoSnapshot {
                repo_uid: repo_uid.to_string(),
            })?;

    let snapshot_uid = &snapshot.snapshot_uid;

    // ── 3. Resolve focus (reusing orient's resolution logic). ─
    let resolution = storage.resolve_path_focus(snapshot_uid, target)?;

    if resolution.has_exact_file {
        return explain_file(
            storage,
            &repo.name,
            &snapshot,
            target,
            resolution.file_stable_key.as_deref(),
            budget,
            now,
        );
    }

    if resolution.has_content_under_prefix || resolution.module_stable_key.is_some() {
        return explain_path(
            storage,
            &repo.name,
            &snapshot,
            target,
            resolution.module_stable_key.as_deref(),
            budget,
            now,
            cancel,
        );
    }

    // ── 4. Try stable-key resolution. ────────────────────────
    use crate::storage_port::AgentFocusKind;

    match storage.resolve_stable_key_focus(snapshot_uid, target)? {
        Some(candidate) if candidate.kind == AgentFocusKind::Symbol => {
            let context = storage.get_symbol_context(snapshot_uid, &candidate.stable_key)?;
            match context {
                Some(ctx) => explain_symbol(
                    storage,
                    &repo.name,
                    &snapshot,
                    &candidate.stable_key,
                    &ctx,
                    target,
                    budget,
                    now,
                    cancel,
                ),
                None => Ok(build_no_match(&repo.name, &snapshot, target, budget)),
            }
        }
        Some(candidate) if candidate.kind == AgentFocusKind::File => {
            let file_path = candidate.file.as_deref().unwrap_or(target);
            explain_file(
                storage,
                &repo.name,
                &snapshot,
                file_path,
                Some(&candidate.stable_key),
                budget,
                now,
            )
        }
        Some(candidate) => {
            // MODULE by stable key.
            let path = extract_path_from_candidate(&candidate, target);
            explain_path(
                storage,
                &repo.name,
                &snapshot,
                &path,
                Some(&candidate.stable_key),
                budget,
                now,
                cancel,
            )
        }
        None => {
            // ── 5. Symbol resolution through the SHARED resolver (SYMBOL-IDENTITY-1 §2.1,
            //    ruling EXPLAIN-RESOLVER-ROUTING = B). The SAME exact-then-suffix ladder
            //    `callers`/`callees` use (`storage.resolve_symbol`): exact stable_key →
            //    qualified_name → name, THEN the qualified-SUFFIX step, definition preferred over
            //    declaration. So a `find`-printed qualified name (`DBImpl::Recover`,
            //    `OwnerController.processCreationForm`) resolves in `explain` too. Ambiguity is
            //    LISTED with candidates (never "not found"); a miss is `NotFound` and NOT rendered
            //    as high confidence.
            match storage.resolve_symbol(snapshot_uid, target)? {
                AgentSymbolResolution::NotFound => {
                    // TEST-EDGE-SCOPE-1A (D-TESA-07): a tracked file with no FILE node
                    // (config / contract / unreadable) whose test status can't be
                    // determined is explained as a file carrying that status; any other
                    // miss keeps today's no-match.
                    match explain_tracked_only_undetermined_file(
                        storage, &repo.name, &snapshot, target,
                    )? {
                        Some(result) => Ok(result),
                        None => Ok(build_no_match(&repo.name, &snapshot, target, budget)),
                    }
                }
                AgentSymbolResolution::Resolved(candidate) => {
                    let context =
                        storage.get_symbol_context(snapshot_uid, &candidate.stable_key)?;
                    match context {
                        Some(ctx) => explain_symbol(
                            storage,
                            &repo.name,
                            &snapshot,
                            &candidate.stable_key,
                            &ctx,
                            target,
                            budget,
                            now,
                            cancel,
                        ),
                        None => Ok(build_no_match(&repo.name, &snapshot, target, budget)),
                    }
                }
                AgentSymbolResolution::Ambiguous(symbol_candidates) => {
                    // CPP-DECLARATORS-1 §2.3 (AMENDED 2026-09-07): when the surviving
                    // candidates are exactly ONE type (class/struct/enum/interface) plus
                    // constructor(s) of that type, a bare-name query resolves to the TYPE — a
                    // bare name means the type in every language's model, and constructors are
                    // named after it. The constructor cursor(s) are surfaced in `next` so a
                    // constructor query is never HIDDEN (operator ruling
                    // `cpp_decl_explain_constructor_collision`). Any other mixed candidate set
                    // stays honestly ambiguous.
                    if let Some((type_candidate, constructors)) =
                        classify_type_constructor_collision(
                            storage,
                            snapshot_uid,
                            target,
                            &symbol_candidates,
                        )?
                    {
                        if let Some(ctx) =
                            storage.get_symbol_context(snapshot_uid, &type_candidate.stable_key)?
                        {
                            let mut result = explain_symbol(
                                storage,
                                &repo.name,
                                &snapshot,
                                &type_candidate.stable_key,
                                &ctx,
                                target,
                                budget,
                                now,
                                cancel,
                            )?;
                            // Surface the constructor cursor(s) as `next` actions —
                            // `explain_symbol` leaves `next` empty, so this is the sole writer.
                            let cap = budget.max_next();
                            let total = constructors.len();
                            let mut actions: Vec<NextAction> = constructors
                                .into_iter()
                                .take(cap)
                                .map(|hint| NextAction {
                                    kind: NextKind::Explain,
                                    repo: repo.name.clone(),
                                    target: Some(hint.cursor),
                                    // The counted "N constructor(s) also match" framing is added
                                    // by the renderer; the per-cursor reason just names the owner.
                                    reason: format!("constructor of {}", hint.type_name),
                                })
                                .collect();
                            let omitted = total.saturating_sub(actions.len());
                            result.next.append(&mut actions);
                            result.next_truncated = Some(omitted > 0);
                            result.next_omitted_count = (omitted > 0).then_some(omitted);
                            return Ok(result);
                        }
                    }
                    // Ambiguous — return candidates.
                    let focus_candidates = symbol_candidates
                        .into_iter()
                        .map(|c| {
                            crate::dto::envelope::FocusCandidate::deterministic(
                                c.stable_key,
                                c.file,
                                // ANCHORS-EVERYWHERE-1: file+line share the SQLite row.
                                c.line,
                                crate::dto::envelope::ResolvedKind::Symbol,
                            )
                        })
                        .collect();
                    Ok(OrientResult {
                        schema: ORIENT_SCHEMA,
                        command: EXPLAIN_COMMAND,
                        repo: repo.name,
                        display_name: None, // Populated by daemon handler
                        snapshot: snapshot.snapshot_uid.clone(),
                        focus: Focus::ambiguous(target, focus_candidates),
                        // SYMBOL-IDENTITY-1 §2.4: an UNRESOLVED outcome (here, ambiguity — the
                        // target did not resolve to a single symbol) must not claim `high`
                        // confidence in a resolution it did not make. `low` is the honest floor:
                        // the candidate list is a fact, but "which one you meant" is unresolved.
                        confidence: Confidence::Low,
                        documentation: None,
                        signals: Vec::new(),
                        signals_truncated: None,
                        signals_omitted_count: None,
                        limits: Vec::new(),
                        limits_truncated: None,
                        limits_omitted_count: None,
                        next: Vec::new(),
                        next_truncated: None,
                        next_omitted_count: None,
                        truncated: false,
                    })
                }
            }
        }
    }
}

/// CPP-DECLARATORS-1 §2.3 (2026-09-07): a constructor cursor surfaced in `next` when a bare
/// name resolved to its type. `cursor` is the `explain` target (the constructor's qualified
/// name, or its stable key when unqualified); `type_name` names the owning type in the reason.
struct ConstructorHint {
    cursor: String,
    type_name: String,
}

/// Is this a subtype that a bare name denotes directly (a TYPE)? Constructors are named after
/// their type, so a bare-name query means the TYPE, not its constructor.
fn is_type_subtype(subtype: &str) -> bool {
    matches!(subtype, "CLASS" | "STRUCT" | "ENUM" | "INTERFACE" | "TRAIT")
}

/// CPP-DECLARATORS-1 §2.3 (AMENDED 2026-09-07): detect the "one type + its constructor(s)"
/// candidate set. Returns `Some((type_candidate, constructor_hints))` iff the surviving
/// candidates are EXACTLY one TYPE plus one-or-more CONSTRUCTORs of that type (nothing else),
/// so a bare-name query resolves to the type while the constructor cursors stay visible. Any
/// other mix (two types, a type + an unrelated function, a lone constructor) returns `None`
/// and stays honestly ambiguous — this is the ratified collapse, NOT a general heuristic.
///
/// Classification uses each candidate's stored `subtype`/`qualified_name` (via
/// `get_symbol_context`). If ANY candidate's context is missing, returns `None` (cannot
/// classify safely → stay ambiguous).
///
/// review-3 #4 — the collapse is now proof-only, never inference:
///   1. Completeness. `candidates` is the `Ambiguous` set from the shared resolver
///      (`storage.resolve_symbol`, SYMBOL-IDENTITY-1 §2.1) — for the real SQLite adapter that set
///      is the UNCAPPED name-tier definitions; for a name-only test double it is the `LIMIT 5`
///      window of `resolve_symbol_name`. Either way a truncated window could hide a 6th exact-name
///      definition that makes a `{type + constructors}` set look unambiguous, so we require the
///      UNCAPPED definition count for `name` (`count_symbol_definitions_by_name`) to equal the
///      number of candidates classified — a truncated window can NEVER satisfy the rule. Unknown
///      count (adapter without the method) ⇒ stay ambiguous.
///   2. Membership. Every constructor must carry a qualified name whose container equals the
///      type's qualified name. A constructor (or the type) with NO qualified name is not proof of
///      membership — it is an inference — so it now stays ambiguous instead of being accepted.
fn classify_type_constructor_collision<S: AgentStorageRead + ?Sized>(
    storage: &S,
    snapshot_uid: &str,
    name: &str,
    candidates: &[AgentFocusCandidate],
) -> Result<Option<(AgentFocusCandidate, Vec<ConstructorHint>)>, AgentStorageError> {
    let mut the_type: Option<(AgentFocusCandidate, AgentSymbolContext)> = None;
    let mut constructors: Vec<(AgentFocusCandidate, AgentSymbolContext)> = Vec::new();

    for cand in candidates {
        let Some(ctx) = storage.get_symbol_context(snapshot_uid, &cand.stable_key)? else {
            return Ok(None); // Cannot classify one candidate → stay ambiguous.
        };
        let Some(subtype) = ctx.subtype.as_deref() else {
            return Ok(None); // No subtype → not a type/constructor pattern.
        };
        if is_type_subtype(subtype) {
            if the_type.is_some() {
                return Ok(None); // More than one TYPE → genuinely ambiguous.
            }
            the_type = Some((cand.clone(), ctx));
        } else if subtype == "CONSTRUCTOR" {
            constructors.push((cand.clone(), ctx));
        } else {
            return Ok(None); // A non-type, non-constructor candidate → ambiguous.
        }
    }

    let (Some((type_candidate, type_ctx)), false) = (the_type, constructors.is_empty()) else {
        return Ok(None); // Need exactly one type AND at least one constructor.
    };

    // Completeness: the classified candidates must be the WHOLE definition universe for `name`.
    // `resolve_symbol_name` capped at 5 and dropped forward declarations; if the uncapped
    // definition count exceeds what we saw, the window hid a definition ⇒ cannot prove "exactly"
    // ⇒ stay ambiguous. Unknown count (default-`None` adapter) is also unproven ⇒ ambiguous.
    match storage.count_symbol_definitions_by_name(snapshot_uid, name)? {
        Some(total) if total == candidates.len() as u64 => {}
        _ => return Ok(None),
    }

    // Membership by PROOF: the type has a qualified name, and each constructor's container (its
    // qualified name minus the trailing `::<name>`) equals it. Any missing qualified name, or a
    // container that does not match, is not proof of membership → stay ambiguous.
    let Some(type_qn) = type_ctx.qualified_name.as_deref() else {
        return Ok(None); // No type qualified name → cannot prove any constructor belongs to it.
    };
    let mut hints = Vec::with_capacity(constructors.len());
    for (_ctor_cand, ctor_ctx) in constructors {
        let Some(ctor_qn) = ctor_ctx.qualified_name.as_deref() else {
            return Ok(None); // Constructor without a qualified name → membership unproven.
        };
        match ctor_qn.rsplit_once("::") {
            Some((container, _)) if container == type_qn => {}
            _ => return Ok(None), // Constructor of a DIFFERENT type (or unqualified) → ambiguous.
        }
        hints.push(ConstructorHint {
            cursor: ctor_qn.to_string(),
            type_name: type_ctx.name.clone(),
        });
    }

    Ok(Some((type_candidate, hints)))
}

fn extract_path_from_candidate(
    candidate: &crate::storage_port::AgentFocusCandidate,
    focus_str: &str,
) -> String {
    if let Some(ref f) = candidate.file {
        return f.clone();
    }
    let key = &candidate.stable_key;
    if let Some(stripped) = key.strip_suffix(":MODULE") {
        if let Some(colon) = stripped.find(':') {
            return stripped[colon + 1..].to_string();
        }
    }
    focus_str.to_string()
}

fn build_no_match(
    repo_name: &str,
    snapshot: &AgentSnapshot,
    target: &str,
    _budget: Budget,
) -> OrientResult {
    OrientResult {
        schema: ORIENT_SCHEMA,
        command: EXPLAIN_COMMAND,
        repo: repo_name.to_string(),
        display_name: None, // Populated by daemon handler
        snapshot: snapshot.snapshot_uid.clone(),
        focus: Focus::no_match(target),
        // SYMBOL-IDENTITY-1 §2.4 / STANDING HONESTY RULE 3: a MISS is never "Confidence: high".
        // Nothing resolved, so there is nothing to be confident about — `low` is the honest floor
        // (the previous static `High` was the "Confidence: high" printed beside `unresolved:
        // no_match` the root-cause audit §H-A flagged; nothing ever computed it).
        confidence: Confidence::Low,
        documentation: None,
        signals: Vec::new(),
        signals_truncated: None,
        signals_omitted_count: None,
        limits: Vec::new(),
        limits_truncated: None,
        limits_omitted_count: None,
        next: Vec::new(),
        next_truncated: None,
        next_omitted_count: None,
        truncated: false,
    }
}

// ── Symbol explain pipeline ─────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn explain_symbol<S: AgentStorageRead + GateStorageRead + ?Sized>(
    storage: &S,
    repo_name: &str,
    snapshot: &AgentSnapshot,
    symbol_stable_key: &str,
    context: &AgentSymbolContext,
    focus_input: &str,
    budget: Budget,
    now: &str,
    cancel: AgentCancelCheck<'_>,
) -> Result<OrientResult, ExplainError> {
    let snapshot_uid = &snapshot.snapshot_uid;
    let repo_uid = &snapshot.repo_uid;
    let cap = items_cap(budget);
    let mut signals: Vec<Signal> = Vec::new();

    // ── EXPLAIN_IDENTITY ────────────────────────────────────
    signals.push(Signal::explain_identity(ExplainIdentityEvidence {
        target_kind: "symbol".to_string(),
        path: context.file_path.clone(),
        stable_key: Some(symbol_stable_key.to_string()),
        name: Some(context.name.clone()),
        subtype: context.subtype.clone(),
        line_start: context.line_start,
        language: None,
        is_test: None,
        module_path: context.module_path.clone(),
        file_count: None,
        symbol_count: None,
        undetermined_test_status: None,
    }));

    // PYTHON-RECEIVER-BINDING-1 (RG-REQ-002-L11, RG-REQ-005-L09): what the certain rows leave out
    // — the inferred callers/callees and the unresolved calls naming or leaving the symbol. ONE
    // SQLite read, whatever engine serves the certain rows (D-PRB-SCOPE-1 amendment 4).
    let remainders =
        storage.find_symbol_call_remainders(snapshot_uid, symbol_stable_key, &context.name)?;

    // ── EXPLAIN_CALLERS ─────────────────────────────────────
    // Always emitted for symbol targets — "0 callers" is
    // meaningful positive information in a deep dive.
    let mut callers = storage.find_symbol_callers(snapshot_uid, symbol_stable_key)?;
    // COHERENCE-LEAF-SERVE-IMPL-2: rank the FULL caller set by relevance BEFORE truncation, so the
    // budget-truncated `items` are a pure function of the (callgraph-cert-proven-equal) caller SET —
    // byte-identical whether served from SQLite or the LiveGraph. See `call_ranking`.
    call_ranking::rank_caller_rows(&mut callers);
    {
        let count = callers.len() as u64;
        let top_modules = group_by_module(callers.iter().map(|c| c.module_path.as_deref()));
        let mut items: Vec<ExplainCallerItem> = callers
            .iter()
            .map(|c| ExplainCallerItem {
                stable_key: c.stable_key.clone(),
                name: c.name.clone(),
                module: c.module_path.clone(),
                // ANCHORS-EVERYWHERE-1: file+line are the SAME SQLite row (single source).
                file: c.file.clone(),
                line: c.line,
            })
            .collect();
        let (trunc, omitted) = truncate_items(&mut items, cap);
        let mut inferred_items = inferred_call_items(&remainders.inferred_callers);
        truncate_items(&mut inferred_items, cap);
        signals.push(Signal::explain_callers(ExplainCallersEvidence {
            count,
            top_modules,
            items,
            items_truncated: trunc,
            items_omitted_count: omitted,
            inferred_count: remainders.inferred_callers.len() as u64,
            inferred_items,
            inferred_by_basis: basis_counts(&remainders.inferred_callers),
            unresolved_naming: unresolved_calls_evidence(
                Some(context.name.clone()),
                &remainders.unresolved_naming,
            ),
        }));
    }

    // ── EXPLAIN_CALLEES ─────────────────────────────────────
    // Always emitted for symbol targets — same reasoning.
    let mut callees = storage.find_symbol_callees(snapshot_uid, symbol_stable_key)?;
    // Same relevance ranking as callers (the dual outgoing-edge set) — see `call_ranking`.
    call_ranking::rank_callee_rows(&mut callees);
    {
        let count = callees.len() as u64;
        let top_modules = group_by_module(callees.iter().map(|c| c.module_path.as_deref()));
        let mut items: Vec<ExplainCalleeItem> = callees
            .iter()
            .map(|c| ExplainCalleeItem {
                stable_key: c.stable_key.clone(),
                name: c.name.clone(),
                module: c.module_path.clone(),
                // ANCHORS-EVERYWHERE-1: file+line are the SAME SQLite row (single source).
                file: c.file.clone(),
                line: c.line,
            })
            .collect();
        let (trunc, omitted) = truncate_items(&mut items, cap);
        let mut inferred_items = inferred_call_items(&remainders.inferred_callees);
        truncate_items(&mut inferred_items, cap);
        signals.push(Signal::explain_callees(ExplainCalleesEvidence {
            count,
            top_modules,
            items,
            items_truncated: trunc,
            items_omitted_count: omitted,
            inferred_count: remainders.inferred_callees.len() as u64,
            inferred_items,
            inferred_by_basis: basis_counts(&remainders.inferred_callees),
            unresolved_from: unresolved_calls_evidence(None, &remainders.unresolved_from),
        }));
    }

    // ── EXPLAIN_MEMBERS + EXPLAIN_REFERENCED_BY (type focus only) ──
    // EXPLAIN-TYPE-SECTIONS-1 (RG-REQ-005-L04): a TYPE is described by its members and the files
    // that reference it — never "Callers (0) / Callees (0)". Emitted ONLY when the focus subtype is
    // a type (a function/method focus is byte-identical to before). Both counts are the PRE-truncation
    // totals; `items` are budget-capped exactly like every other explain section (RG-REQ-012-L04).
    if context.subtype.as_deref().is_some_and(is_type_subtype) {
        if let Some(ref qualified_name) = context.qualified_name {
            let members = storage.list_members_of_type(snapshot_uid, qualified_name)?;
            let count = members.len() as u64;
            let mut items: Vec<ExplainMemberItem> = members
                .into_iter()
                .map(|m| ExplainMemberItem {
                    name: m.name,
                    subtype: m.subtype,
                    file: m.file,
                    // ANCHORS-EVERYWHERE-1: the member's stored start line (already 0→None normalised
                    // by the adapter), for the `path:line` anchor.
                    line: m.line_start,
                    forward_decl: m.forward_decl,
                    // CPP-ATTRIBUTE-MACRO-1A (RG-REQ-002-L11): the all-or-nothing marker, verbatim.
                    undetermined_identity: m.undetermined_identity.map(|u| {
                        ExplainUndeterminedIdentity {
                            identity: MemberIdentityState::Undetermined,
                            identity_candidates: u.candidates,
                            identity_basis: u.basis,
                        }
                    }),
                })
                .collect();
            let (trunc, omitted) = truncate_items(&mut items, cap);
            signals.push(Signal::explain_members(ExplainMembersEvidence {
                count,
                items,
                items_truncated: trunc,
                items_omitted_count: omitted,
            }));
        }
        if let Some(ref file_path) = context.file_path {
            let importers = storage.find_file_importers(snapshot_uid, file_path)?;
            // PYTHON-SUBMODULE-IMPORT-1 (RG-REQ-002-L11): partition by file. A file with at least
            // one certain (non-`inferred`) edge to the target is a certain importer — counted once,
            // exactly as before; a file whose every edge is `inferred` is the inferred remainder,
            // stated beside the certain facts, never counted among them.
            // The certainty discriminator is closed: `static`/`dynamic` are certain, `inferred` is
            // the remainder, anything else is a broken read — never counted as certain.
            let mut certain_files: std::collections::BTreeSet<&str> = Default::default();
            for i in &importers {
                // TEST-EDGE-SCOPE-1B (D-TESB-14): the classification crate's one vocabulary.
                match repo_graph_classification::import_partition::ImportClass::from_resolution(
                    &i.resolution,
                ) {
                    Ok(repo_graph_classification::import_partition::ImportClass::Certain) => {
                        certain_files.insert(i.file.as_str());
                    }
                    Ok(repo_graph_classification::import_partition::ImportClass::Inferred) => {}
                    Err(_) => {
                        let other = &i.resolution;
                        return Err(AgentStorageError::new(
                            "find_file_importers",
                            format!(
                                "importer row for {} has resolution {other:?}, not one of \
                                 static | dynamic | inferred",
                                i.file
                            ),
                        )
                        .into());
                    }
                }
            }
            let mut certain: Vec<(String, Option<String>)> = Vec::new();
            // Distinct (file, module) rows, exactly the pre-slice `SELECT DISTINCT` shape.
            let mut certain_seen: std::collections::BTreeSet<(&str, Option<&str>)> =
                Default::default();
            let mut inferred_items: Vec<ExplainReferencedByInferredItem> = Vec::new();
            let mut inferred_files: std::collections::BTreeSet<String> = Default::default();
            for i in &importers {
                if certain_files.contains(i.file.as_str()) {
                    if i.resolution != "inferred"
                        && certain_seen.insert((i.file.as_str(), i.module_path.as_deref()))
                    {
                        certain.push((i.file.clone(), i.module_path.clone()));
                    }
                } else {
                    // The storage adapter guarantees a basis on every inferred row; a row without
                    // one is a broken read, never silently dropped.
                    let basis = i.basis.clone().ok_or_else(|| {
                        AgentStorageError::new(
                            "find_file_importers",
                            format!("inferred importer row for {} carries no basis", i.file),
                        )
                    })?;
                    inferred_files.insert(i.file.clone());
                    inferred_items.push(ExplainReferencedByInferredItem {
                        file: i.file.clone(),
                        module: i.module_path.clone(),
                        basis,
                        alternate: i.alternate_path.clone(),
                    });
                }
            }
            let count = certain.len() as u64;
            let top_modules = group_by_module(certain.iter().map(|(_, m)| m.as_deref()));
            let mut items: Vec<ExplainReferencedByItem> = certain
                .into_iter()
                .map(|(file, module)| ExplainReferencedByItem { file, module })
                .collect();
            let (trunc, omitted) = truncate_items(&mut items, cap);
            let inferred_count = inferred_files.len() as u64;
            let (_, inferred_omitted) = truncate_items(&mut inferred_items, cap);
            signals.push(Signal::explain_referenced_by(ExplainReferencedByEvidence {
                count,
                top_modules,
                items,
                items_truncated: trunc,
                items_omitted_count: omitted,
                inferred_items,
                inferred_count,
                inferred_items_omitted_count: inferred_omitted,
            }));
        }
    }

    // ── Inherited module-context signals ────────────────────
    let trust = storage.get_trust_summary(repo_uid, snapshot_uid)?;
    if let Some(ref module_path) = context.module_path {
        // EXPLAIN_CYCLES (DAEMON-CANCEL-3: cancellable Tarjan + filter). TEST-EDGE-SCOPE-1B: the
        // cycles of the DEFAULT view involving the focus module, and the excluded cycles involving
        // it — the block renders when either exists.
        let cycles = storage.find_cycles_involving_module_cancellable(
            snapshot_uid,
            module_path,
            &mut *cancel,
        )?;
        let partition = storage.import_cycle_partition(
            snapshot_uid,
            repo_graph_classification::import_partition::ImportView::DEFAULT,
            &mut *cancel,
        )?;
        let excluded: Vec<&crate::storage_port::AgentExcludedCycle> = partition
            .excluded_cycles
            .iter()
            .filter(|e| e.members.iter().any(|m| m == module_path))
            .collect();
        if let Some(signal) = explain_cycles_signal(cycles, &partition, &excluded, cap) {
            signals.push(signal.with_module_context());
        }

        // EXPLAIN_BOUNDARY — TEST-EDGE-SCOPE-1B (D-TESB-11): certain imports judged, inferred
        // ones counted with their files.
        let declarations = storage.get_active_boundary_declarations(repo_uid)?;
        let matching: Vec<(String, String)> = declarations
            .into_iter()
            .filter(|d| d.source_module == *module_path)
            .map(|d| (d.source_module, d.forbidden_target))
            .collect();
        if let Some(signal) = explain_boundary_signal(storage, snapshot_uid, matching, cap)? {
            signals.push(signal.with_module_context());
        }

        // EXPLAIN_GATE
        build_gate_signal(
            storage,
            repo_uid,
            snapshot_uid,
            now,
            Some(module_path),
            cap,
            &mut signals,
            true,
        )?;
    }

    // ── EXPLAIN_TRUST ───────────────────────────────────────
    signals.push(build_trust_signal(&trust));

    // ── EXPLAIN_MEASUREMENTS ────────────────────────────────
    // Omit when no measurement items exist. The Rust indexer
    // does not currently produce measurements; this section
    // activates when coverage or complexity data is present.
    let measurement_items: Vec<ExplainMeasurementItem> = Vec::new();
    if !measurement_items.is_empty() {
        signals.push(Signal::explain_measurements(ExplainMeasurementsEvidence {
            items: measurement_items,
            items_truncated: None,
            items_omitted_count: None,
        }));
    }

    // ── ranking + truncation ────────────────────────────────
    ranking::sort_and_rank(&mut signals);
    let sig_tx = ranking::truncate_signals(&mut signals, budget);

    let stale = !storage.get_stale_files(snapshot_uid)?.is_empty();
    let confidence = derive_repo_confidence(&trust, stale);

    let focus = Focus::symbol(focus_input, symbol_stable_key, context.file_path.as_deref());

    Ok(OrientResult {
        schema: ORIENT_SCHEMA,
        command: EXPLAIN_COMMAND,
        repo: repo_name.to_string(),
        display_name: None, // Populated by daemon handler
        snapshot: snapshot_uid.clone(),
        focus,
        confidence,
        documentation: None,
        signals,
        signals_truncated: sig_tx.truncated.then_some(true),
        signals_omitted_count: sig_tx.truncated.then_some(sig_tx.omitted),
        limits: Vec::new(),
        limits_truncated: None,
        limits_omitted_count: None,
        next: Vec::new(),
        next_truncated: None,
        next_omitted_count: None,
        truncated: sig_tx.truncated,
    })
}

// ── File explain pipeline ───────────────────────────────────────────

fn explain_file<S: AgentStorageRead + GateStorageRead + ?Sized>(
    storage: &S,
    repo_name: &str,
    snapshot: &AgentSnapshot,
    file_path: &str,
    file_stable_key: Option<&str>,
    budget: Budget,
    now: &str,
) -> Result<OrientResult, ExplainError> {
    let _ = now;
    let snapshot_uid = &snapshot.snapshot_uid;
    let repo_uid = &snapshot.repo_uid;
    let cap = items_cap(budget);
    let mut signals: Vec<Signal> = Vec::new();

    // ── EXPLAIN_IDENTITY ────────────────────────────────────
    let file_summary = storage.compute_file_summary(snapshot_uid, file_path)?;
    // TEST-EDGE-SCOPE-1A (RG-REQ-001-L07, D-TESA-13 S1): the focused file's stored test
    // flag through the shared UNDETERMINED function. A FILE-bearing focus is a tracked
    // file of its snapshot, so no tracked row is an inconsistency — an error, never
    // read as "determined".
    let undetermined_test_status = match storage
        .query_tracked_file_test_flags(snapshot_uid, Some(file_path))?
        .as_slice()
    {
        [row] => crate::dto::test_status::ExplainUndeterminedTestStatus::of(&row.path, row.is_test),
        rows => {
            return Err(ExplainError::Storage(AgentStorageError::new(
                "explain_file",
                format!(
                    "{file_path} has a FILE node but {} tracked rows on snapshot {snapshot_uid}; \
                     its test status is unknown",
                    rows.len()
                ),
            )))
        }
    };
    signals.push(Signal::explain_identity(ExplainIdentityEvidence {
        target_kind: "file".to_string(),
        path: Some(file_path.to_string()),
        stable_key: file_stable_key.map(|k| k.to_string()),
        name: None,
        subtype: None,
        line_start: None,
        language: file_summary.languages.first().cloned(),
        is_test: None,
        module_path: None,
        file_count: None,
        symbol_count: Some(file_summary.symbol_count),
        undetermined_test_status,
    }));

    // ── EXPLAIN_IMPORTS ─────────────────────────────────────
    // TEST-EDGE-SCOPE-1B (RG-REQ-002-L11): certain imports listed; the inferred ones are counted
    // (`import_remainder.inferred.imports`) with `rmap imports <file> --include-inferred` to show
    // them. The file is the subject, so its imports stay whatever its test status.
    //
    // IMPORTS-UNRESOLVED-REMAINDER-1 (RG-REQ-006-L12, RG-REQ-002-L11): the file's imports without a
    // confirmed target are counted too (`unresolved_count`), and a file whose only imports are those
    // still gets the section. A failed read is the explain's error, never a zero.
    let mut imports = storage.find_file_imports(snapshot_uid, file_path)?;
    let inferred_imports = storage.find_inferred_file_imports(snapshot_uid, file_path)?;
    let unresolved_imports = storage.find_unresolved_file_imports(snapshot_uid, file_path)?;
    if !imports.is_empty() || !inferred_imports.is_empty() || !unresolved_imports.is_empty() {
        // TRUNCATION-AUDIT-1: order by target_file ASC BEFORE the cut (deterministic,
        // source-independent). Previously truncated in storage order.
        ordering::sort_explain_imports(&mut imports);
        let count = imports.len() as u64;
        let mut items: Vec<ExplainImportItem> = imports
            .into_iter()
            .map(|i| ExplainImportItem {
                target_file: i.target_file,
            })
            .collect();
        let (trunc, omitted) = truncate_items(&mut items, cap);
        let additions = crate::dto::signal::EvidenceAdditions {
            import_remainder: (!inferred_imports.is_empty()).then(|| {
                let mut remainder =
                    repo_graph_classification::import_partition::ImportRemainder::default();
                remainder.inferred.imports = inferred_imports.len() as u64;
                remainder
            }),
            ..Default::default()
        };
        signals.push(
            Signal::explain_imports(ExplainImportsEvidence {
                count,
                items,
                items_truncated: trunc,
                items_omitted_count: omitted,
                unresolved_count: Some(unresolved_imports.len() as u64),
            })
            .with_evidence_additions(additions),
        );
    }

    // ── EXPLAIN_SYMBOLS ─────────────────────────────────────
    let trust = storage.get_trust_summary(repo_uid, snapshot_uid)?;
    let mut symbols = storage.list_symbols_in_file(snapshot_uid, file_path)?;

    if !symbols.is_empty() {
        // TRUNCATION-AUDIT-1: file reading order (line_start ASC, then name, then stable_key)
        // BEFORE the cut, so a truncated file view keeps the earliest symbols deterministically.
        ordering::sort_explain_symbols(&mut symbols);
        let count = symbols.len() as u64;
        let mut items: Vec<ExplainSymbolItem> = symbols
            .iter()
            .map(|s| ExplainSymbolItem {
                name: s.name.clone(),
                subtype: s.subtype.clone(),
                line_start: s.line_start,
            })
            .collect();
        let (trunc, omitted) = truncate_items(&mut items, cap);
        signals.push(Signal::explain_symbols(ExplainSymbolsEvidence {
            count,
            items,
            items_truncated: trunc,
            items_omitted_count: omitted,
        }));
    }

    // ── EXPLAIN_TRUST ───────────────────────────────────────
    signals.push(build_trust_signal(&trust));

    // ── EXPLAIN_MEASUREMENTS ────────────────────────────────
    // Omit when no measurement items exist. The Rust indexer
    // does not currently produce measurements; this section
    // activates when coverage or complexity data is present.
    let measurement_items: Vec<ExplainMeasurementItem> = Vec::new();
    if !measurement_items.is_empty() {
        signals.push(Signal::explain_measurements(ExplainMeasurementsEvidence {
            items: measurement_items,
            items_truncated: None,
            items_omitted_count: None,
        }));
    }

    // ── ranking + truncation ────────────────────────────────
    ranking::sort_and_rank(&mut signals);
    let sig_tx = ranking::truncate_signals(&mut signals, budget);

    let stale = !storage.get_stale_files(snapshot_uid)?.is_empty();
    let confidence = derive_repo_confidence(&trust, stale);

    let focus = Focus::file(file_path, file_stable_key, file_path);

    Ok(OrientResult {
        schema: ORIENT_SCHEMA,
        command: EXPLAIN_COMMAND,
        repo: repo_name.to_string(),
        display_name: None, // Populated by daemon handler
        snapshot: snapshot_uid.clone(),
        focus,
        confidence,
        documentation: None,
        signals,
        signals_truncated: sig_tx.truncated.then_some(true),
        signals_omitted_count: sig_tx.truncated.then_some(sig_tx.omitted),
        limits: Vec::new(),
        limits_truncated: None,
        limits_omitted_count: None,
        next: Vec::new(),
        next_truncated: None,
        next_omitted_count: None,
        truncated: sig_tx.truncated,
    })
}

/// TEST-EDGE-SCOPE-1A (D-TESA-07): `explain <path>` of a tracked file with NO FILE node
/// (a config, contract or unreadable file — nothing was extracted from it) whose test
/// status can't be determined: a file identity (`target_kind` `file`, `path`, the test
/// status; no language or symbol count — none is stored) instead of no-match. `None` for
/// any other target, which keeps today's no-match unchanged.
fn explain_tracked_only_undetermined_file<S: AgentStorageRead + ?Sized>(
    storage: &S,
    repo_name: &str,
    snapshot: &AgentSnapshot,
    target: &str,
) -> Result<Option<OrientResult>, ExplainError> {
    let snapshot_uid = &snapshot.snapshot_uid;
    let rows = storage.query_tracked_file_test_flags(snapshot_uid, Some(target))?;
    let status = match rows.as_slice() {
        [row] if !row.has_file_node => {
            crate::dto::test_status::ExplainUndeterminedTestStatus::of(&row.path, row.is_test)
        }
        _ => None,
    };
    let Some(status) = status else {
        return Ok(None);
    };
    let mut signals = vec![Signal::explain_identity(ExplainIdentityEvidence {
        target_kind: "file".to_string(),
        path: Some(target.to_string()),
        stable_key: None,
        name: None,
        subtype: None,
        line_start: None,
        language: None,
        is_test: None,
        module_path: None,
        file_count: None,
        symbol_count: None,
        undetermined_test_status: Some(status),
    })];
    let trust = storage.get_trust_summary(&snapshot.repo_uid, snapshot_uid)?;
    signals.push(build_trust_signal(&trust));
    ranking::sort_and_rank(&mut signals);
    let stale = !storage.get_stale_files(snapshot_uid)?.is_empty();
    Ok(Some(OrientResult {
        schema: ORIENT_SCHEMA,
        command: EXPLAIN_COMMAND,
        repo: repo_name.to_string(),
        display_name: None, // Populated by daemon handler
        snapshot: snapshot_uid.clone(),
        focus: Focus::file(target, None, target),
        confidence: derive_repo_confidence(&trust, stale),
        documentation: None,
        signals,
        signals_truncated: None,
        signals_omitted_count: None,
        limits: Vec::new(),
        limits_truncated: None,
        limits_omitted_count: None,
        next: Vec::new(),
        next_truncated: None,
        next_omitted_count: None,
        truncated: false,
    }))
}

// ── Path explain pipeline ───────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn explain_path<S: AgentStorageRead + GateStorageRead + ?Sized>(
    storage: &S,
    repo_name: &str,
    snapshot: &AgentSnapshot,
    path_prefix: &str,
    module_stable_key: Option<&str>,
    budget: Budget,
    now: &str,
    cancel: AgentCancelCheck<'_>,
) -> Result<OrientResult, ExplainError> {
    let snapshot_uid = &snapshot.snapshot_uid;
    let repo_uid = &snapshot.repo_uid;
    let cap = items_cap(budget);
    let mut signals: Vec<Signal> = Vec::new();

    // ── EXPLAIN_IDENTITY ────────────────────────────────────
    let path_summary = storage.compute_path_summary(snapshot_uid, path_prefix)?;
    signals.push(Signal::explain_identity(ExplainIdentityEvidence {
        target_kind: "path".to_string(),
        path: Some(path_prefix.to_string()),
        stable_key: module_stable_key.map(|k| k.to_string()),
        name: None,
        subtype: None,
        line_start: None,
        language: None,
        is_test: None,
        module_path: Some(path_prefix.to_string()),
        file_count: Some(path_summary.file_count),
        symbol_count: Some(path_summary.symbol_count),
        undetermined_test_status: None,
    }));

    // ── EXPLAIN_FILES ───────────────────────────────────────
    let mut files = storage.list_files_in_path(snapshot_uid, path_prefix)?;
    if !files.is_empty() {
        // TRUNCATION-AUDIT-1: rank by symbol_count DESC (then path ASC) BEFORE the cut, so a
        // truncated path view keeps the densest files — not the alphabetically-first ones.
        ordering::sort_explain_files(&mut files);
        let count = files.len() as u64;
        let mut items: Vec<ExplainFileItem> = files
            .into_iter()
            .map(|f| ExplainFileItem {
                path: f.path,
                symbol_count: f.symbol_count,
                is_test: f.is_test,
            })
            .collect();
        let (trunc, omitted) = truncate_items(&mut items, cap);
        signals.push(Signal::explain_files(ExplainFilesEvidence {
            count,
            items,
            items_truncated: trunc,
            items_omitted_count: omitted,
        }));
    }

    // ── EXPLAIN_CYCLES ──────────────────────────────────────
    let trust = storage.get_trust_summary(repo_uid, snapshot_uid)?;
    // DAEMON-CANCEL-3: cancellable Tarjan + filter. TEST-EDGE-SCOPE-1B: the DEFAULT view's
    // cycles involving the path and the excluded cycles involving it.
    let cycles =
        storage.find_cycles_involving_path_cancellable(snapshot_uid, path_prefix, &mut *cancel)?;
    let partition = storage.import_cycle_partition(
        snapshot_uid,
        repo_graph_classification::import_partition::ImportView::DEFAULT,
        &mut *cancel,
    )?;
    let excluded: Vec<&crate::storage_port::AgentExcludedCycle> = partition
        .excluded_cycles
        .iter()
        .filter(|e| crate::aggregators::cycles::involves_path(e, path_prefix))
        .collect();
    if let Some(signal) = explain_cycles_signal(cycles, &partition, &excluded, cap) {
        signals.push(signal);
    }

    // ── EXPLAIN_BOUNDARY ────────────────────────────────────
    // TEST-EDGE-SCOPE-1B (D-TESB-11): certain imports judged, inferred ones counted.
    let declarations = storage.find_boundary_declarations_in_path(repo_uid, path_prefix)?;
    let rules: Vec<(String, String)> = declarations
        .into_iter()
        .map(|d| (d.source_module, d.forbidden_target))
        .collect();
    if let Some(signal) = explain_boundary_signal(storage, snapshot_uid, rules, cap)? {
        signals.push(signal);
    }

    // ── EXPLAIN_GATE ────────────────────────────────────────
    build_gate_signal(
        storage,
        repo_uid,
        snapshot_uid,
        now,
        Some(path_prefix),
        cap,
        &mut signals,
        false,
    )?;

    // ── EXPLAIN_TRUST ───────────────────────────────────────
    signals.push(build_trust_signal(&trust));

    // ── EXPLAIN_MEASUREMENTS ────────────────────────────────
    // Omit when no measurement items exist. The Rust indexer
    // does not currently produce measurements; this section
    // activates when coverage or complexity data is present.
    let measurement_items: Vec<ExplainMeasurementItem> = Vec::new();
    if !measurement_items.is_empty() {
        signals.push(Signal::explain_measurements(ExplainMeasurementsEvidence {
            items: measurement_items,
            items_truncated: None,
            items_omitted_count: None,
        }));
    }

    // ── ranking + truncation ────────────────────────────────
    ranking::sort_and_rank(&mut signals);
    let sig_tx = ranking::truncate_signals(&mut signals, budget);

    let stale = !storage.get_stale_files(snapshot_uid)?.is_empty();
    let confidence = derive_repo_confidence(&trust, stale);

    let focus = Focus::path_area(path_prefix, module_stable_key, path_prefix);

    Ok(OrientResult {
        schema: ORIENT_SCHEMA,
        command: EXPLAIN_COMMAND,
        repo: repo_name.to_string(),
        display_name: None, // Populated by daemon handler
        snapshot: snapshot_uid.clone(),
        focus,
        confidence,
        documentation: None,
        signals,
        signals_truncated: sig_tx.truncated.then_some(true),
        signals_omitted_count: sig_tx.truncated.then_some(sig_tx.omitted),
        limits: Vec::new(),
        limits_truncated: None,
        limits_omitted_count: None,
        next: Vec::new(),
        next_truncated: None,
        next_omitted_count: None,
        truncated: sig_tx.truncated,
    })
}

// ── Shared helpers ──────────────────────────────────────────────────

const TOP_MODULES_N: usize = 3;

fn group_by_module<'a>(
    module_paths: impl Iterator<Item = Option<&'a str>>,
) -> Vec<ModuleCountEvidence> {
    // Shared with `call_ranking`'s concentration so `top_modules` and the caller/callee ranking count
    // identically (same per-row basis + `(unknown)` sentinel).
    let counts = call_ranking::module_counts(module_paths);
    let mut entries: Vec<ModuleCountEvidence> = counts
        .into_iter()
        .map(|(module, count)| ModuleCountEvidence { module, count })
        .collect();
    entries.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.module.cmp(&b.module)));
    entries.truncate(TOP_MODULES_N);
    entries
}

/// PYTHON-RECEIVER-BINDING-1: the explain rows of an inferred caller/callee remainder (call-site
/// anchored, with basis and — for an unrecorded basis — the writer).
fn inferred_call_items(
    rows: &[crate::storage_port::AgentInferredCallRow],
) -> Vec<ExplainInferredCallItem> {
    rows.iter()
        .map(|r| ExplainInferredCallItem {
            stable_key: r.stable_key.clone(),
            name: r.name.clone(),
            module: r.module_path.clone(),
            file: r.file.clone(),
            line: r.line,
            basis: r.basis.clone(),
            receiver: r.receiver.clone(),
            extractor: r.extractor.clone(),
        })
        .collect()
}

/// PYTHON-RECEIVER-BINDING-1: the inferred rows counted per basis (uncapped).
fn basis_counts(
    rows: &[crate::storage_port::AgentInferredCallRow],
) -> std::collections::BTreeMap<String, u64> {
    let mut counts = std::collections::BTreeMap::new();
    for r in rows {
        *counts.entry(r.basis.clone()).or_insert(0) += 1;
    }
    counts
}

/// PYTHON-RECEIVER-BINDING-1 (RG-REQ-005-L09): the unresolved-calls evidence, `None` when no
/// unresolved call bears on the symbol (the section is then byte-identical to before).
fn unresolved_calls_evidence(
    name: Option<String>,
    by_basis: &[crate::storage_port::AgentBasisCount],
) -> Option<ExplainUnresolvedCalls> {
    let count: u64 = by_basis.iter().map(|b| b.count).sum();
    (count > 0).then(|| ExplainUnresolvedCalls {
        name,
        count,
        by_basis: by_basis
            .iter()
            .map(|b| (b.basis_code.clone(), b.count))
            .collect(),
    })
}

fn build_trust_signal(trust: &crate::storage_port::AgentTrustSummary) -> Signal {
    use crate::storage_port::EnrichmentState;
    Signal::explain_trust(ExplainTrustEvidence {
        call_resolution_rate: trust.call_resolution_rate,
        call_graph_reliability: match trust.call_graph_reliability.level {
            AgentReliabilityLevel::High => "high".to_string(),
            AgentReliabilityLevel::Medium => "medium".to_string(),
            AgentReliabilityLevel::Low => "low".to_string(),
        },
        // dead_code_reliability — removed. Surface withdrawn.
        enrichment_state: match trust.enrichment_state {
            EnrichmentState::Ran => "ran".to_string(),
            EnrichmentState::NotApplicable => "not_applicable".to_string(),
            EnrichmentState::NotRun => "not_run".to_string(),
            // ORIENT-FACT-COHERENCE-1: explain is not a targeted surface (the daemon never injects the
            // in-flight state into explain's storage read), so this arm is defensive exhaustiveness —
            // it mirrors the shared machine token so a value that ever reaches here reads honestly.
            EnrichmentState::InFlight => "in_flight".to_string(),
        },
        // RELIABILITY-REFRAME-1 (review-1 §1): carry the in-scope COUNTS so the reader surface can
        // render the honest "no in-scope calls measured" for a 0-of-0 repo instead of the
        // `call_resolution_rate` 1.0 sentinel's fabricated 100%. The denominator excludes
        // known-external calls but KEEPS unclassified ones (`resolved_calls +
        // unresolved_calls_internal_like`) — the same denominator the band is scored on — so the
        // field is named "in-scope OR unclassified", not purely in-scope (review-5 §1).
        resolved_in_scope: trust.resolved_calls,
        // PYTHON-RECEIVER-BINDING-1 (D-PRB-RATE-1 = A): the universe keeps the inferred calls.
        in_scope_or_unclassified_total: trust.resolved_calls
            + trust.inferred_calls
            + trust.unresolved_calls_internal_like,
        inferred_calls: trust.inferred_calls,
    })
}

#[allow(clippy::too_many_arguments)]
fn build_gate_signal<S: GateStorageRead + ?Sized>(
    storage: &S,
    repo_uid: &str,
    snapshot_uid: &str,
    now: &str,
    target_filter: Option<&str>,
    cap: usize,
    signals: &mut Vec<Signal>,
    module_context: bool,
) -> Result<(), ExplainError> {
    use repo_graph_gate::{assemble_from_requirements, GateMode, GateRequirement};

    let requirements = storage
        .get_active_requirements(repo_uid)
        .map_err(|e| AgentStorageError::new("get_active_requirements", e.message))?;

    if requirements.is_empty() {
        return Ok(());
    }

    // Filter obligations by target.
    let filtered: Vec<GateRequirement> = if let Some(target) = target_filter {
        requirements
            .into_iter()
            .filter_map(|req| {
                let matching: Vec<_> = req
                    .obligations
                    .into_iter()
                    .filter(|o| match &o.target {
                        Some(t) => {
                            if module_context {
                                t == target
                            } else {
                                t == target || t.starts_with(&format!("{}/", target))
                            }
                        }
                        None => false,
                    })
                    .collect();
                if matching.is_empty() {
                    None
                } else {
                    Some(GateRequirement {
                        req_id: req.req_id,
                        version: req.version,
                        obligations: matching,
                    })
                }
            })
            .collect()
    } else {
        requirements
    };

    if filtered.is_empty() {
        return Ok(());
    }

    let report = match assemble_from_requirements(
        storage,
        repo_uid,
        snapshot_uid,
        GateMode::Default,
        now,
        filtered,
    ) {
        Ok(r) => r,
        Err(_) => return Ok(()),
    };

    if report.outcome.counts.total == 0 {
        return Ok(());
    }

    let obligation_count = report.outcome.counts.total as u64;
    let outcome = report.outcome.outcome.clone();
    let mut items: Vec<ExplainGateItem> = report
        .obligations
        .iter()
        .map(|o| ExplainGateItem {
            req_id: o.req_id.clone(),
            obligation_id: o.obligation_id.clone(),
            method: o.method.clone(),
            effective_verdict: format!("{:?}", o.effective_verdict),
        })
        .collect();
    // TRUNCATION-AUDIT-1: worst verdict first (FAIL→…→PASS), then obligation identity, BEFORE
    // the cut, so a truncated gate view keeps the most urgent obligations deterministically.
    ordering::sort_explain_gate_items(&mut items);
    let (trunc, omitted) = truncate_items(&mut items, cap);

    let sig = Signal::explain_gate(ExplainGateEvidence {
        outcome,
        obligation_count,
        items,
        items_truncated: trunc,
        items_omitted_count: omitted,
    });

    if module_context {
        signals.push(sig.with_module_context());
    } else {
        signals.push(sig);
    }

    Ok(())
}

/// TEST-EDGE-SCOPE-1B (RG-REQ-004-L12): the `EXPLAIN_CYCLES` signal over the focus's cycles of the
/// DEFAULT view, with the excluded cycles involving the focus, the remainder and the importer
/// UNDETERMINED block. `None` when there is neither a cycle nor an excluded cycle.
fn explain_cycles_signal(
    mut cycles: Vec<crate::storage_port::AgentCycle>,
    partition: &crate::storage_port::AgentImportCyclePartition,
    excluded: &[&crate::storage_port::AgentExcludedCycle],
    cap: usize,
) -> Option<Signal> {
    if cycles.is_empty() && excluded.is_empty() {
        return None;
    }
    // TRUNCATION-AUDIT-1: rank the FULL cycle set (length DESC, then ring members) BEFORE the
    // cut so the surviving top-N are the biggest cycles, deterministically.
    ordering::canonicalize_cycles(&mut cycles);
    let count = cycles.len() as u64;
    let mut items: Vec<CycleEvidence> = cycles
        .into_iter()
        .map(|c| CycleEvidence {
            length: c.length,
            modules: c.modules,
            type_only: c.type_only,
            // EXPLAIN-CYCLES-HONEST-1 (§2.1): the focus-scoped SQLite read carries the REAL directed
            // walk (via `label_focus_cycles`); the renderer draws the verified ring from it.
            walk: c.walk,
        })
        .collect();
    let (trunc, omitted) = truncate_items(&mut items, cap);
    let additions = crate::aggregators::cycles::cycle_partition_additions(
        partition,
        excluded,
        items.iter().map(|c| c.modules.as_slice()),
    );
    Some(
        Signal::explain_cycles(ExplainCyclesEvidence {
            count,
            items,
            items_truncated: trunc,
            items_omitted_count: omitted,
        })
        .with_evidence_additions(additions),
    )
}

/// TEST-EDGE-SCOPE-1B (D-TESB-11): the `EXPLAIN_BOUNDARY` signal over `(source, forbidden)`
/// rules — CERTAIN imports judged (edge counts, as before); the INFERRED imports across each rule
/// counted with their sorted source files (`inferred_imports_not_judged`), never judged. `None`
/// when nothing crosses any rule.
fn explain_boundary_signal<S: AgentStorageRead + ?Sized>(
    storage: &S,
    snapshot_uid: &str,
    rules: Vec<(String, String)>,
    cap: usize,
) -> Result<Option<Signal>, AgentStorageError> {
    let mut per_rule: Vec<BoundaryViolationEvidence> = Vec::new();
    let mut total = 0u64;
    let mut not_judged: Vec<crate::dto::signal::InferredImportsNotJudgedRule> = Vec::new();
    for (source, forbidden) in rules {
        let edges = storage.find_imports_between_paths(snapshot_uid, &source, &forbidden)?;
        let inferred =
            storage.find_inferred_imports_between_paths(snapshot_uid, &source, &forbidden)?;
        if !inferred.is_empty() {
            let files: std::collections::BTreeSet<&str> =
                inferred.iter().map(|e| e.source_file.as_str()).collect();
            not_judged.push(crate::dto::signal::InferredImportsNotJudgedRule {
                source_module: source.clone(),
                target_module: forbidden.clone(),
                count: inferred.len() as u64,
                files: files.into_iter().map(str::to_string).collect(),
            });
        }
        if edges.is_empty() {
            continue;
        }
        let edge_count = edges.len() as u64;
        total += edge_count;
        per_rule.push(BoundaryViolationEvidence {
            source_module: source,
            target_module: forbidden,
            edge_count,
        });
    }
    if total == 0 && not_judged.is_empty() {
        return Ok(None);
    }
    // TRUNCATION-AUDIT-1: rank by edge_count DESC (then source/target) BEFORE the cut.
    ordering::sort_boundary_violations(&mut per_rule);
    let (trunc, omitted) = truncate_items(&mut per_rule, cap);
    let additions = crate::dto::signal::EvidenceAdditions {
        inferred_imports_not_judged: (!not_judged.is_empty())
            .then_some(crate::dto::signal::InferredImportsNotJudgedEvidence::PerRule(not_judged)),
        ..Default::default()
    };
    Ok(Some(
        Signal::explain_boundary(ExplainBoundaryEvidence {
            violation_count: total,
            items: per_rule,
            items_truncated: trunc,
            items_omitted_count: omitted,
        })
        .with_evidence_additions(additions),
    ))
}
