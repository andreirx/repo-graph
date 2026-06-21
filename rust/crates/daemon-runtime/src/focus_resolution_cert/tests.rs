//! FOCUS-RESOLUTION-LIVEGRAPH-IMPL: cert tests (split from the impl + fixture per the 500-line
//! guardrail, review-1 pt5). Covers the GREEN parity case, the RED/fallback classes (SQLite
//! divergence, non-resident, no-LiveGraph, non-TS), ambiguity parity vs SQLite, the serve-ladder
//! (cached-verdict reuse + fingerprint invalidation), and the GREEN-path no-`nodes`-read proof.

use super::test_fixture::*;
use super::{
    build_and_store_focus_resolution_cert, candidates_eq, context_eq, focus_resolution_is_green,
    opt_candidate_eq, path_eq,
};
use crate::state::RepoState;
use repo_graph_agent::AgentStorageRead;
use repo_graph_storage::StorageConnection;
use repo_graph_trust_model::LanguageSupport;
use tempfile::tempdir;

/// RESOLUTION-PARITY (the GREEN case): a faithful SQLite mirror -> the cert is GREEN, AND every
/// function resolves the SAME identity on both sides (asserted directly for a representative focus
/// of each kind).
#[test]
fn focus_cert_green_and_parity_on_faithful_mirror() {
    let dir = tempdir().unwrap();
    let (db_path, snapshot_uid) = build_default_mirror(dir.path(), None);
    let state = RepoState::open(&db_path, REPO).expect("open repo state");
    *state.livegraph.write() = Some(build_default_livegraph());

    // Direct per-function parity (path / stable-key / symbol-name / symbol-context).
    {
        let guard = state.livegraph.read();
        let lg = guard.as_ref().unwrap();

        let lp = lg.resolve_path("src").data().unwrap().clone();
        let sp = state
            .storage
            .resolve_path_focus(&snapshot_uid, "src")
            .unwrap();
        assert!(path_eq(&lp, &sp), "path parity for 'src': {lp:?} vs {sp:?}");

        let lk = lg
            .resolve_stable_key(&module_key("src"))
            .data()
            .unwrap()
            .clone();
        let sk = state
            .storage
            .resolve_stable_key_focus(&snapshot_uid, &module_key("src"))
            .unwrap();
        assert!(opt_candidate_eq(&lk, &sk), "module stable-key parity");

        let ln = lg.resolve_symbol_name("foo").data().unwrap().clone();
        let sn = state
            .storage
            .resolve_symbol_name(&snapshot_uid, "foo")
            .unwrap();
        assert!(
            candidates_eq(&ln, &sn),
            "symbol-name parity: {ln:?} vs {sn:?}"
        );

        let widget = format!("{REPO}:src/a.ts#Widget.render:SYMBOL:METHOD");
        let lc = lg.symbol_context(&widget).data().unwrap().clone();
        let sc = state
            .storage
            .get_symbol_context(&snapshot_uid, &widget)
            .unwrap();
        assert!(
            context_eq(&lc, &sc),
            "symbol-context parity: {lc:?} vs {sc:?}"
        );
    }

    let fp = live_fp(&state, &snapshot_uid);
    let green = build_and_store_focus_resolution_cert(&state, &snapshot_uid, Some(fp.clone()));
    assert_eq!(green, Some(true), "faithful mirror -> GREEN");
    let cert = state.focus_resolution_cert.read().clone().unwrap();
    assert_eq!(cert.verdict, "GREEN");
    assert_eq!(cert.fingerprint, fp);
}

/// FALLBACK (divergence): drop one symbol from the SQLite mirror -> the LiveGraph resolves it but
/// SQLite does not -> the cert is RED (the consumer must fall back to SQLite).
#[test]
fn focus_cert_red_when_sqlite_diverges() {
    let dropped = format!("{REPO}:src/a.ts#Widget:SYMBOL:CLASS");
    let dir = tempdir().unwrap();
    let (db_path, snapshot_uid) = build_default_mirror(dir.path(), Some(&dropped));
    let state = RepoState::open(&db_path, REPO).expect("open repo state");
    *state.livegraph.write() = Some(build_default_livegraph());

    let fp = live_fp(&state, &snapshot_uid);
    let green = build_and_store_focus_resolution_cert(&state, &snapshot_uid, Some(fp));
    assert_eq!(green, Some(false), "a dropped SQLite symbol -> RED");
    assert_eq!(
        state.focus_resolution_cert.read().as_ref().unwrap().verdict,
        "RED"
    );
}

/// SET-EQUALITY (SQLite-extra FILE, review-2 pt1): SQLite carries a FILE node the LiveGraph lacks.
/// The bidirectional corpus enumerates it from the SQLite side, so `resolve_path` misses it on the
/// LiveGraph (has_exact_file=false) but hits on SQLite -> the cert MUST go RED. A one-sided LG-only
/// corpus would have passed GREEN here — the false no-loss this fix removes.
#[test]
fn focus_cert_red_when_sqlite_has_extra_file() {
    let dir = tempdir().unwrap();
    let extras = MirrorExtras {
        extra_files: vec!["src/ghost.ts".into()],
        ..Default::default()
    };
    let (db_path, snapshot_uid) = build_sqlite_mirror_ex(
        dir.path(),
        &default_files(),
        &default_symbols(),
        None,
        &extras,
    );
    let state = RepoState::open(&db_path, REPO).expect("open repo state");
    *state.livegraph.write() = Some(build_default_livegraph());

    let fp = live_fp(&state, &snapshot_uid);
    assert_eq!(
        build_and_store_focus_resolution_cert(&state, &snapshot_uid, Some(fp)),
        Some(false),
        "a SQLite-only FILE the LiveGraph lacks -> RED"
    );
}

/// SET-EQUALITY (SQLite-extra MODULE, review-2 pt1): SQLite carries a directory-MODULE node the
/// LiveGraph cannot derive (no resident file under that dir). `resolve_path`/`resolve_stable_key`
/// miss it on the LiveGraph but hit on SQLite -> the cert MUST go RED.
#[test]
fn focus_cert_red_when_sqlite_has_extra_module() {
    let dir = tempdir().unwrap();
    let extras = MirrorExtras {
        extra_modules: vec!["ghostmod".into()],
        ..Default::default()
    };
    let (db_path, snapshot_uid) = build_sqlite_mirror_ex(
        dir.path(),
        &default_files(),
        &default_symbols(),
        None,
        &extras,
    );
    let state = RepoState::open(&db_path, REPO).expect("open repo state");
    *state.livegraph.write() = Some(build_default_livegraph());

    let fp = live_fp(&state, &snapshot_uid);
    assert_eq!(
        build_and_store_focus_resolution_cert(&state, &snapshot_uid, Some(fp)),
        Some(false),
        "a SQLite-only MODULE the LiveGraph cannot derive -> RED"
    );
}

/// SET-EQUALITY (SQLite-extra SYMBOL, review-2 pt1): SQLite carries a SYMBOL node (in an existing
/// file) the LiveGraph lacks. `resolve_stable_key`/`resolve_symbol_name` miss it on the LiveGraph but
/// hit on SQLite -> the cert MUST go RED.
#[test]
fn focus_cert_red_when_sqlite_has_extra_symbol() {
    let dir = tempdir().unwrap();
    let extras = MirrorExtras {
        extra_symbols: vec![sym("src/a.ts", "ghost", "ghost", "FUNCTION", 99)],
        ..Default::default()
    };
    let (db_path, snapshot_uid) = build_sqlite_mirror_ex(
        dir.path(),
        &default_files(),
        &default_symbols(),
        None,
        &extras,
    );
    let state = RepoState::open(&db_path, REPO).expect("open repo state");
    *state.livegraph.write() = Some(build_default_livegraph());

    let fp = live_fp(&state, &snapshot_uid);
    assert_eq!(
        build_and_store_focus_resolution_cert(&state, &snapshot_uid, Some(fp)),
        Some(false),
        "a SQLite-only SYMBOL the LiveGraph lacks -> RED"
    );
}

/// SET-EQUALITY (fallback-symbol, review-2 pt1 / spec §7c L2): a `ScipSynthesizedFallback` node is
/// resident in the LiveGraph (so the resolver SKIPS it — AST-adopted only) AND present in SQLite as a
/// normal SYMBOL. `resolve_stable_key` returns None on the LiveGraph but Some on SQLite -> RED. This
/// is the fallback-node divergence the spec's L2 limit names, now PROVEN by a test.
#[test]
fn focus_cert_red_when_fallback_symbol_present() {
    let dir = tempdir().unwrap();
    // SQLite holds the fallback as a NORMAL resolvable SYMBOL (default symbols + the fallback).
    let mut sqlite_symbols = default_symbols();
    sqlite_symbols.push(sym(
        "src/a.ts",
        "fallbackSym",
        "fallbackSym",
        "FUNCTION",
        50,
    ));
    let (db_path, snapshot_uid) =
        build_sqlite_mirror(dir.path(), &default_files(), &sqlite_symbols, None);
    let state = RepoState::open(&db_path, REPO).expect("open repo state");
    // The LiveGraph holds the SAME key as a ScipSynthesizedFallback node (unresolvable by the
    // resolver); the default symbols stay AST-adopted.
    *state.livegraph.write() = Some(build_livegraph_with_fallback(
        &default_files(),
        &default_symbols(),
        &[sym(
            "src/a.ts",
            "fallbackSym",
            "fallbackSym",
            "FUNCTION",
            50,
        )],
        LanguageSupport::TypeScriptPrimary,
    ));

    let fp = live_fp(&state, &snapshot_uid);
    assert_eq!(
        build_and_store_focus_resolution_cert(&state, &snapshot_uid, Some(fp)),
        Some(false),
        "a fallback node the resolver skips but SQLite resolves -> RED"
    );
}

/// FALLBACK (non-resident): with the LiveGraph partition unloaded, the resolver envelopes are
/// Partial (UNKNOWN, never Exact-empty) -> the cert can never claim GREEN.
#[test]
fn focus_cert_red_when_livegraph_non_resident() {
    let dir = tempdir().unwrap();
    let (db_path, snapshot_uid) = build_default_mirror(dir.path(), None);
    let state = RepoState::open(&db_path, REPO).expect("open repo state");
    let mut lg = build_default_livegraph();
    lg.unload_partition("p");
    *state.livegraph.write() = Some(lg);

    let fp = live_fp(&state, &snapshot_uid);
    let green = build_and_store_focus_resolution_cert(&state, &snapshot_uid, Some(fp));
    assert_eq!(green, Some(false), "non-resident partition -> RED");
}

/// FALLBACK (no producer): no LiveGraph at all -> RED, never a false GREEN.
#[test]
fn focus_cert_red_when_no_livegraph() {
    let dir = tempdir().unwrap();
    let (db_path, snapshot_uid) = build_default_mirror(dir.path(), None);
    let state = RepoState::open(&db_path, REPO).expect("open repo state");
    // livegraph stays None.
    let green =
        build_and_store_focus_resolution_cert(&state, &snapshot_uid, Some("fp-no-lg".to_string()));
    assert_eq!(green, Some(false));
}

/// FALLBACK (non-TS, review-1 pt4): a non-TS partition is in the `whole_graph_completeness` missing
/// set, so every resolver envelope is Partial -> the cert short-circuits to RED BEFORE any SQLite
/// read, and the serve-ladder accessor agrees -> SQLite fallback for non-TS repos.
#[test]
fn focus_cert_red_when_non_ts() {
    let dir = tempdir().unwrap();
    let (db_path, snapshot_uid) = build_default_mirror(dir.path(), None);
    let state = RepoState::open(&db_path, REPO).expect("open repo state");
    *state.livegraph.write() = Some(build_livegraph(
        &default_files(),
        &default_symbols(),
        LanguageSupport::RustPartialBeta,
    ));

    let fp = live_fp(&state, &snapshot_uid);
    let green = build_and_store_focus_resolution_cert(&state, &snapshot_uid, Some(fp));
    assert_eq!(green, Some(false), "a non-TS partition -> RED");
    assert_eq!(
        state.focus_resolution_cert.read().as_ref().unwrap().verdict,
        "RED"
    );
    assert!(
        !focus_resolution_is_green(&state, &snapshot_uid),
        "the serve-ladder accessor also reports not-green for non-TS"
    );
}

/// AMBIGUITY (review-1 pt4): >5 same-name symbols -> both the LiveGraph resolver and SQLite return
/// the SAME first 5 by stable-key ascending (count + keys + order parity), and the cert is GREEN over
/// the ambiguous corpus. Proves the `LIMIT 5` + `ORDER BY stable_key ASC` parity AGAINST SQLite.
#[test]
fn focus_cert_ambiguous_name_caps_at_five_with_sqlite_parity() {
    // Six files, each declaring a symbol named "dup".
    let mut files: Vec<String> = Vec::new();
    let mut symbols: Vec<Sym> = Vec::new();
    for i in 0..6 {
        let path = format!("src/d{i}.ts");
        symbols.push(sym(&path, "dup", "dup", "FUNCTION", 1));
        files.push(path);
    }
    let dir = tempdir().unwrap();
    let (db_path, snapshot_uid) = build_sqlite_mirror(dir.path(), &files, &symbols, None);
    let state = RepoState::open(&db_path, REPO).expect("open repo state");
    *state.livegraph.write() = Some(build_livegraph(
        &files,
        &symbols,
        LanguageSupport::TypeScriptPrimary,
    ));

    let sq = state
        .storage
        .resolve_symbol_name(&snapshot_uid, "dup")
        .unwrap();
    let lg_cands = {
        let guard = state.livegraph.read();
        guard
            .as_ref()
            .unwrap()
            .resolve_symbol_name("dup")
            .data()
            .unwrap()
            .clone()
    };
    assert_eq!(lg_cands.len(), 5, "LiveGraph caps the 6 matches at 5");
    assert_eq!(sq.len(), 5, "SQLite caps the 6 matches at 5");
    assert!(
        candidates_eq(&lg_cands, &sq),
        "same 5 candidates, same order: {lg_cands:?} vs {sq:?}"
    );

    let fp = live_fp(&state, &snapshot_uid);
    assert_eq!(
        build_and_store_focus_resolution_cert(&state, &snapshot_uid, Some(fp)),
        Some(true),
        "ambiguity is reproduced exactly -> GREEN"
    );
}

/// GREEN-PATH NO-`nodes`-READ proof (review-1 pt3 / review-2 pt3) — a REAL storage spy, not a
/// hand-written closure. After the cert is built + cached GREEN, the underlying SQLite storage is
/// SWAPPED for an EMPTY database. The serve-ladder fingerprint is computed from the LiveGraph (NOT
/// SQLite), so it is unchanged -> the accessor hits the cached GREEN and returns true. Had the green
/// decision performed ANY `nodes` read, it would have rebuilt against the empty DB and recomputed
/// RED. The accessor still reporting GREEN therefore PROVES the cached-green serve decision touched
/// zero SQLite. Step (4) arms the spy: a forced rebuild against the empty DB IS RED, so the GREEN in
/// step (3) can ONLY have come from the cache. (The cert BUILD in step (1) is the one sanctioned
/// `nodes` read, spec §7b; the resolver's own zero-storage proof is in `focus_resolver`'s tests —
/// `repo-graph-livegraph` has no storage dependency.)
#[test]
fn green_focus_resolution_decision_reads_no_sqlite() {
    let dir = tempdir().unwrap();
    let (db_path, snapshot_uid) = build_default_mirror(dir.path(), None);
    let mut state = RepoState::open(&db_path, REPO).expect("open repo state");
    *state.livegraph.write() = Some(build_default_livegraph());

    // (1) First call lazily BUILDS the cert — the ONE sanctioned SQLite read (spec §7b) -> GREEN.
    assert!(
        focus_resolution_is_green(&state, &snapshot_uid),
        "faithful mirror -> GREEN"
    );

    // (2) Swap storage for an EMPTY database (the spy). The LiveGraph + fingerprint are unchanged.
    let empty_dir = tempdir().unwrap();
    let empty_db = empty_dir.path().join("empty.db");
    state.storage = StorageConnection::open(&empty_db).expect("open empty db");

    // (3) The cached GREEN at the unchanged fingerprint must serve WITHOUT re-reading SQLite. A
    //     re-read would hit the empty DB and flip to RED; still reporting GREEN proves zero reads.
    assert!(
        focus_resolution_is_green(&state, &snapshot_uid),
        "a cached GREEN must serve without re-reading SQLite (a re-read would see the empty DB -> RED)"
    );

    // (4) Arm the spy: force a rebuild against the empty DB (reload the partition -> the fingerprint
    //     changes -> StaleOrMissing -> rebuild) -> RED. This proves the empty DB really does fail the
    //     compare, so step (3)'s GREEN could ONLY have come from the cache (no read on the decision).
    state.livegraph.write().as_mut().unwrap().load_partition(
        "p",
        build_default_ir(),
        LanguageSupport::TypeScriptPrimary,
    );
    assert!(
        !focus_resolution_is_green(&state, &snapshot_uid),
        "a rebuild against the empty DB is RED -> the spy is armed"
    );
}

/// SERVE-LADDER (review-1 pt1): the accessor reuses a cached verdict at a matching fingerprint
/// (no rebuild) and rebuilds when the fingerprint changes (invalidation). Mirrors the
/// import/cycles/stats cert-state ladder.
#[test]
fn focus_resolution_is_green_caches_then_invalidates() {
    let dir = tempdir().unwrap();
    let (db_path, snapshot_uid) = build_default_mirror(dir.path(), None);
    let state = RepoState::open(&db_path, REPO).expect("open repo state");
    *state.livegraph.write() = Some(build_default_livegraph());

    // (1) Lazy build on first call -> GREEN, cert stored at the current fingerprint.
    assert!(focus_resolution_is_green(&state, &snapshot_uid));
    let fp1 = state
        .focus_resolution_cert
        .read()
        .as_ref()
        .unwrap()
        .fingerprint
        .clone();

    // (2) Cached-verdict reuse: poison the stored verdict to RED at the SAME fingerprint. The
    // accessor must return the cached RED WITHOUT rebuilding (a rebuild would recompute GREEN).
    state
        .focus_resolution_cert
        .write()
        .as_mut()
        .unwrap()
        .verdict = "RED".into();
    assert!(
        !focus_resolution_is_green(&state, &snapshot_uid),
        "a cached verdict at the current fingerprint is reused without a rebuild"
    );
    assert_eq!(
        state.focus_resolution_cert.read().as_ref().unwrap().verdict,
        "RED",
        "the poisoned verdict survived -> no rebuild happened"
    );

    // (3) Invalidation: reload the partition -> epoch bump -> a DIFFERENT fingerprint -> the accessor
    // sees StaleOrMissing -> rebuilds -> fresh GREEN (the poison is gone, the fingerprint changed).
    state.livegraph.write().as_mut().unwrap().load_partition(
        "p",
        build_default_ir(),
        LanguageSupport::TypeScriptPrimary,
    );
    assert!(
        focus_resolution_is_green(&state, &snapshot_uid),
        "a fingerprint change forces a rebuild -> GREEN"
    );
    let fp2 = state
        .focus_resolution_cert
        .read()
        .as_ref()
        .unwrap()
        .fingerprint
        .clone();
    assert_ne!(
        fp1, fp2,
        "the epoch bump changed the fingerprint (invalidation)"
    );
    assert_eq!(
        state.focus_resolution_cert.read().as_ref().unwrap().verdict,
        "GREEN"
    );
}

/// SERVE-LADDER safe default: no LiveGraph -> the accessor reports not-green (SQLite resolution) and
/// builds no cert (no fingerprint).
#[test]
fn focus_resolution_is_green_false_without_livegraph() {
    let dir = tempdir().unwrap();
    let (db_path, snapshot_uid) = build_default_mirror(dir.path(), None);
    let state = RepoState::open(&db_path, REPO).expect("open repo state");
    // livegraph stays None.
    assert!(
        !focus_resolution_is_green(&state, &snapshot_uid),
        "no LiveGraph -> not green (safe SQLite default)"
    );
    assert!(
        state.focus_resolution_cert.read().is_none(),
        "no fingerprint -> no cert built"
    );
}

/// No fingerprint -> no cert built (the caller falls back), mirroring the other certs' `fingerprint?`
/// short-circuit.
#[test]
fn focus_cert_no_fingerprint_is_none() {
    let dir = tempdir().unwrap();
    let (db_path, snapshot_uid) = build_default_mirror(dir.path(), None);
    let state = RepoState::open(&db_path, REPO).expect("open repo state");
    *state.livegraph.write() = Some(build_default_livegraph());
    assert_eq!(
        build_and_store_focus_resolution_cert(&state, &snapshot_uid, None),
        None
    );
    assert!(state.focus_resolution_cert.read().is_none());
}
