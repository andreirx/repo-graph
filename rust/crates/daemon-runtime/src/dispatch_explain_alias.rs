//! FIND-EVIDENCE-1 (§2.3) — the ADDITIVE short-cursor alias for `explain`.
//!
//! `find`'s symbol rows print a RELATIVE cursor `explain <suffix>` where `<suffix>` is
//! the symbol's stable_key with its `<repo_uid>:` prefix stripped (the cursor diet: the
//! 26-char uid is printed ONCE in the header, not restated per row). For that printed
//! cursor to RUN VERBATIM, `explain` must accept the suffix — but focus resolution
//! matches `stable_key` EXACTLY, so a bare suffix would not resolve. This reattaches the
//! repo's uid prefix so the short form resolves to the SAME node the full key does.
//!
//! **Purely SYNTACTIC — no storage read (review-0 fix, operator ruling 2026-09-03 #2).**
//! An earlier draft PROBED SQLite (`resolve_stable_key_focus`, a `nodes` read) to confirm
//! the prefixed form is a real node before rewriting. That probe ran on EVERY ready-epoch
//! `explain` — including a plain SYMBOL-name focus (`CompactRange`) — and so REGRESSED the
//! ratified green-path property "explain SYMBOL-focus is `nodes`-free" (COHERENCE-LEAF-
//! SERVE-IMPL-2 `9e6077c`): the whole point of that arc is that focus resolution serves
//! from the LiveGraph with ZERO eager `nodes` reads. Reattaching a prefix is a pure STRING
//! operation, so the probe is unnecessary: this function reattaches on SYNTAX alone and
//! lets the UNCHANGED downstream resolution (the decorator, `nodes`-free on green) do the
//! actual lookup. Taking no `StorageConnection` argument, it CANNOT read `nodes` — the
//! property is restored by construction.
//!
//! **Strictly additive** (the property the slice pre-ratifies, and the bar the precedent
//! chain sets — `FindNext.cwd` additive omit-with-reason; the semantic-fallback additive
//! `data` on the unchanged `symbol not found` error; the additive serving-fact `value`
//! enrichment): it rewrites a target ONLY when the raw target has the printed short-cursor
//! SYNTAX — a symbol stable_key SUFFIX (carries the `:SYMBOL` fact-class discriminant) that
//! does NOT already carry this repo's `<repo_uid>:` prefix. So:
//!   - a full `<repo_uid>:…` key → has the prefix → NO rewrite → byte-identical to today
//!     (the canonical full cursor takes the existing resolution path untouched);
//!   - a path focus (`src/x.ts`) or a plain SYMBOL-name focus (`CompactRange`) → no
//!     `:SYMBOL` discriminant → NO rewrite → those tiers resolve EXACTLY as before (and
//!     stay `nodes`-free on green — a plain name is never rerouted through a stable_key
//!     miss);
//!   - a `find` symbol suffix (`db/x.cc#Sym:SYMBOL:METHOD`) → matches → rewrite to
//!     `<repo_uid>:db/x.cc#Sym:SYMBOL:METHOD`, exactly reconstructing the original key, so
//!     the short cursor runs.
//! No currently-resolving target changes: the only inputs it touches are bare stable_key
//! suffixes, which today never resolve (keys are uid-prefixed) and so already report
//! not-found; this converts some of those not-founds into the correct node, never the
//! reverse. It never widens `explain` output, exit codes, or resolution semantics for any
//! input that already worked.
//!
//! Abstraction record — module: `dispatch::explain_alias`; concrete current users (as of
//! CURSOR-ROUNDTRIP-1 §2.1): `dispatch::ServiceDispatcher::handle_explain` (inline, before
//! its focus-resolution pipeline) AND `dispatch::ServiceDispatcher::resolve_symbol_cursor`
//! (the shared `resolve_symbol` normalizer for `callers`/`callees`/`path`) — one aliasing
//! function, several call sites, never a per-handler copy (STANDING HONESTY RULE 2). Axis:
//! the ≤500-line / "do not append responsibilities to oversized files" guardrail — this
//! target-normalization responsibility gets its own file + unit seam rather than growing
//! the ~5k-line `dispatch.rs`; rejected simpler alternative: inlining the check in each
//! handler (no unit seam, more mass on the god-file, drift risk across sites).
//!
//! NAME NOTE (surfaced, not renamed): the module/file name `explain_alias` /
//! `dispatch_explain_alias.rs` predates this slice, when `explain` was the sole caller.
//! The function now serves every symbol-cursor command, so the `explain` in the name
//! under-describes the contract. The name is RETAINED here because the ratified slice doc
//! (`docs/slices/cursor-roundtrip-1.md` §2.1) references it by this exact path and a rename
//! is a boundary-touching change (module path + `#[path]` + call sites); flagged for the
//! reviewer rather than applied unilaterally.

/// The fact-class discriminant every SYMBOL stable_key carries (`…#name:SYMBOL[:KIND]` —
/// see `storage::types` fixtures, e.g. `r1:src/foo.ts#bar:SYMBOL`). Its presence in a
/// prefix-less target is the printed short-cursor SYNTAX: it distinguishes a symbol
/// stable_key SUFFIX from a plain symbol NAME (`CompactRange`) or a PATH (`src/x.ts`),
/// neither of which contains a `:` fact-class token. Gating on it is what keeps a plain
/// symbol-name focus untouched (and thus `nodes`-free on green).
const SYMBOL_KEY_MARKER: &str = ":SYMBOL";

/// If `target` is the printed short-cursor form — a symbol stable_key SUFFIX (carries
/// [`SYMBOL_KEY_MARKER`]) lacking this repo's `<repo_uid>:` prefix — return the full key
/// (`<repo_uid>:<target>`); otherwise `None` (leave the target untouched, so every
/// currently-resolving form is byte-identical). Pure: no I/O, no storage read — the
/// downstream focus resolution (decorator-served, `nodes`-free on green) does the lookup.
pub(super) fn reattach_repo_uid_prefix(repo_uid: &str, target: &str) -> Option<String> {
    // A missing repo uid (degraded no-snapshot state) has nothing to reattach; `find`
    // also omits the header + prints full cursors then, so there is no short form to accept.
    if repo_uid.is_empty() {
        return None;
    }
    // Already a full key for THIS repo → nothing to reattach (the canonical full cursor
    // takes the existing resolution path untouched).
    if target.starts_with(&format!("{repo_uid}:")) {
        return None;
    }
    // Not the printed short-cursor SYNTAX → untouched. A path or a plain symbol name has
    // no fact-class discriminant, so it resolves exactly as before (and stays nodes-free).
    if !target.contains(SYMBOL_KEY_MARKER) {
        return None;
    }
    Some(format!("{repo_uid}:{target}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const UID: &str = "leveldb-abc123";

    #[test]
    fn short_symbol_cursor_gets_the_uid_prefix_reattached() {
        // The printed short cursor — a symbol stable_key suffix — is reattached to the
        // EXACT original full key, so `explain <suffix>` resolves to the same node.
        let suffix = "db/db_impl.cc#leveldb::DBImpl::CompactRange:SYMBOL:METHOD";
        assert_eq!(
            reattach_repo_uid_prefix(UID, suffix),
            Some(format!("{UID}:{suffix}"))
        );
    }

    #[test]
    fn plain_symbol_name_focus_is_untouched_stays_nodes_free() {
        // A plain SYMBOL-name focus has no `:SYMBOL` discriminant → NOT rewritten. This is
        // the review-0 regression fix: were it reattached, `{uid}:CompactRange` would miss
        // stable_key resolution and never reach the symbol-name tier (a behavior change);
        // untouched, it flows to the decorator-served resolution — `nodes`-free on green.
        assert_eq!(reattach_repo_uid_prefix(UID, "CompactRange"), None);
    }

    #[test]
    fn canonical_full_cursor_is_untouched() {
        // A full `<uid>:…` key already carries the prefix → NO rewrite (no double prefix);
        // the canonical full cursor takes the existing path untouched.
        let full = format!("{UID}:db/db_impl.cc#CompactRange:SYMBOL:METHOD");
        assert_eq!(reattach_repo_uid_prefix(UID, &full), None);
    }

    #[test]
    fn path_focus_is_untouched() {
        // A path focus has no fact-class discriminant → untouched (resolves via path focus).
        assert_eq!(reattach_repo_uid_prefix(UID, "db/db_impl.cc"), None);
        assert_eq!(reattach_repo_uid_prefix(UID, "src"), None);
    }

    #[test]
    fn empty_uid_never_fabricates_a_prefix() {
        // Degraded no-uid state: nothing to reattach, never a `:suffix` fabrication.
        assert_eq!(
            reattach_repo_uid_prefix("", "db/x.cc#Sym:SYMBOL:METHOD"),
            None
        );
    }
}
