//! PYTHON-RECEIVER-BINDING-1 (RG-REQ-005-L02/L09, RG-REQ-002-L11): certain call rows by default,
//! the remainder stated beside them — for the `callers`, `callees` and `path` handlers.
//!
//! One rule for all three handlers: a CALLS row is CERTAIN only when its resolution is `static` or
//! `dynamic` (D-PSI-R1-VOCAB); an `inferred` row is never a caller, a callee or a hop by default.
//!
//! - `callers`/`callees`: every SQLite fetch handed to a serving or comparing path (the engine
//!   routes, the `Compare` engine's `compare_keys`, the union and its witness ledger) returns the
//!   CERTAIN rows only ([`certain_only`]); the default list carries them (`count` = their number).
//!   With `include_inferred` the inferred rows are added ONCE, after serving, whatever backend
//!   served the certain rows, and `count` is restated (every row listed). An inferred-inclusive
//!   request never takes the union arm ([`union_arm_selected`], D-PRB-UNION-ROUTE-1), so a union
//!   answer never holds an inferred row and its `witness_counts` stay one-to-one with its rows.
//!   The per-route entries ([`callers_union_answer`], [`callers_engine_answer`] and their callees
//!   twins) compose fetch, serve and remainder so the dispatch arms keep only the route choice.
//!   Whatever engine served the certain rows, the remainder is read from SQLite
//!   ([`AgentStorageRead::find_symbol_call_remainders`], D-PRB-SCOPE-1 amendment 4) and attached:
//!   `inferred: {count, by_basis}` when an inferred row exists,
//!   `unresolved_naming` (callers) / `unresolved_from` (callees) when an unresolved call bears on
//!   the symbol, each with its bounded `sites` and their recorded candidates. Absent fields mean
//!   "none" — a payload without them is byte-identical to before.
//! - `path`: the SQLite walk admits certain hops only; when it finds no route and a route exists
//!   once inferred edges are admitted, the answer states `inferred_edges_on_route` and the
//!   `search_depth` both walks used (D-PRB-SCOPE-1 addendum) — an absent route is never presented
//!   as proof of absence.
//!
//! [abstraction: a module, not a type — three handlers under one rule; dispatch.rs is over the
//! size guardrail and gains only the parameter read and the calls; rejected: inline code in
//! dispatch.rs.]

use std::collections::BTreeMap;

use repo_graph_agent::{AgentBasisCount, AgentUnresolvedCallSite};
use repo_graph_storage::error::StorageError;
use repo_graph_storage::queries::{CalleeResult, CallerResult, ResolvedSymbol};
use repo_graph_storage::StorageConnection;
use serde_json::{json, Value};

use crate::livegraph_feed::{Engine, RequestEpoch};
use crate::state::RepoState;

/// The edge types a `callers`/`callees` answer reads.
const CALLS: [&str; 1] = ["CALLS"];

/// The depth bound of the SQLite `path` walk (both the certain and the admitting walk).
pub(crate) const PATH_SEARCH_DEPTH: i64 = 8;

/// Refuse a PRESENT `include_inferred` that is not a boolean. Absent means `false`; a malformed
/// value (a string, a number, `null`) is never read as `false` — the request is refused with a
/// named invalid-request error instead (RG-REQ-002-L04). Checked before [`include_inferred`].
pub(crate) fn check_include_inferred_param(params: &Value) -> Result<(), String> {
    match params.get("include_inferred") {
        None | Some(Value::Bool(_)) => Ok(()),
        Some(other) => Err(format!(
            "invalid 'include_inferred' parameter: expected a boolean, got {other}"
        )),
    }
}

/// The optional boolean request parameter `include_inferred`: `true` only when it is present and
/// `true`. Read after [`check_include_inferred_param`] accepted the parameters, so the only other
/// states it sees are `false` and absent (the default).
pub(crate) fn include_inferred(params: &Value) -> bool {
    matches!(params.get("include_inferred"), Some(Value::Bool(true)))
}

/// A caller/callee row's resolution (both row types carry it).
pub(crate) trait CallRow {
    fn resolution(&self) -> &str;
}

impl CallRow for CallerResult {
    fn resolution(&self) -> &str {
        &self.resolution
    }
}

impl CallRow for CalleeResult {
    fn resolution(&self) -> &str {
        &self.resolution
    }
}

/// The certain rows (`static`/`dynamic`) only, in their SQL order; an `inferred` row is left for
/// the remainder. A row whose resolution is outside the vocabulary `static | dynamic | inferred`
/// is unreadable: the whole answer is refused with a named error rather than the row being
/// dropped (a silent drop would read as "no such caller"; D-PSI-R1-VOCAB, RG-REQ-002-L04).
pub(crate) fn certain_only<R: CallRow>(rows: Vec<R>) -> Result<Vec<R>, StorageError> {
    let mut certain = Vec::with_capacity(rows.len());
    for row in rows {
        match row.resolution() {
            "static" | "dynamic" => certain.push(row),
            "inferred" => {}
            other => {
                return Err(StorageError::SerializationError(format!(
                    "a call row carries an unreadable resolution {other:?} \
                     (readable: static | dynamic | inferred)"
                )))
            }
        }
    }
    Ok(certain)
}

/// Whether a `callers`/`callees` request takes the union arm (RECON-M-R2): only with the union
/// flag on, on the `Auto` engine, and WITHOUT `include_inferred` — an inferred-inclusive request is
/// not a union answer and is served by the `Auto` route instead (D-PRB-UNION-ROUTE-1).
pub(crate) fn union_arm_selected(
    union_flag_on: bool,
    engine: Engine,
    include_inferred: bool,
) -> bool {
    union_flag_on && engine == Engine::Auto && !include_inferred
}

/// What one `callers`/`callees` answer is computed over: the store, the repo's resident state, the
/// pinned request epoch and the resolved symbol.
pub(crate) struct CallAnswerInputs<'a> {
    pub storage: &'a StorageConnection,
    pub repo_state: &'a RepoState,
    pub epoch: &'a RequestEpoch,
    pub target: &'a ResolvedSymbol,
}

/// The union-arm `callers` answer: the union over the CERTAIN rows, then the remainder (counts
/// only — the union arm is never selected with `include_inferred`, so no inferred row is listed).
pub(crate) fn callers_union_answer(q: &CallAnswerInputs<'_>) -> Result<Value, String> {
    let snap = q.epoch.snapshot_uid();
    let mut v = crate::union_serve::callers_union_response(q.repo_state, q.epoch, q.target, || {
        q.storage
            .find_direct_callers(snap, &q.target.stable_key, &CALLS)
            .and_then(certain_only)
    })
    .map_err(|e| e.to_string())?;
    attach_remainder(q.storage, snap, q.target, CallSide::Callers, false, &mut v)?;
    Ok(v)
}

/// The union-arm `callees` answer (the twin of [`callers_union_answer`]).
pub(crate) fn callees_union_answer(q: &CallAnswerInputs<'_>) -> Result<Value, String> {
    let snap = q.epoch.snapshot_uid();
    let mut v = crate::union_serve::callees_union_response(q.repo_state, q.epoch, q.target, || {
        q.storage
            .find_direct_callees(snap, &q.target.stable_key, &CALLS)
            .and_then(certain_only)
    })
    .map_err(|e| e.to_string())?;
    attach_remainder(q.storage, snap, q.target, CallSide::Callees, false, &mut v)?;
    Ok(v)
}

/// The engine-route `callers` answer (`Auto` — LiveGraph when its certificate is green, else
/// SQLite — or an explicit engine): the engine serves or compares the CERTAIN rows, then the
/// remainder is attached and, with `include_inferred`, the inferred rows are added once.
pub(crate) fn callers_engine_answer(
    q: &CallAnswerInputs<'_>,
    engine: Engine,
    include_inferred: bool,
    symbol: &str,
    repo_root: &str,
) -> Result<Value, String> {
    let snap = q.epoch.snapshot_uid();
    let mut v = crate::livegraph_feed::callers_engine_response(
        engine,
        q.repo_state,
        q.epoch,
        q.target,
        || {
            q.storage
                .find_direct_callers(snap, &q.target.stable_key, &CALLS)
                .and_then(certain_only)
        },
        symbol,
        repo_root,
    )
    .map_err(|e| e.to_string())?;
    let side = CallSide::Callers;
    attach_remainder(q.storage, snap, q.target, side, include_inferred, &mut v)?;
    Ok(v)
}

/// The engine-route `callees` answer (the twin of [`callers_engine_answer`]).
pub(crate) fn callees_engine_answer(
    q: &CallAnswerInputs<'_>,
    engine: Engine,
    include_inferred: bool,
    symbol: &str,
    repo_root: &str,
) -> Result<Value, String> {
    let snap = q.epoch.snapshot_uid();
    let mut v = crate::livegraph_feed::callees_engine_response(
        engine,
        q.repo_state,
        q.epoch,
        q.target,
        || {
            q.storage
                .find_direct_callees(snap, &q.target.stable_key, &CALLS)
                .and_then(certain_only)
        },
        symbol,
        repo_root,
    )
    .map_err(|e| e.to_string())?;
    let side = CallSide::Callees;
    attach_remainder(q.storage, snap, q.target, side, include_inferred, &mut v)?;
    Ok(v)
}

/// Which side of the symbol a `callers`/`callees` answer lists.
#[derive(Clone, Copy)]
pub(crate) enum CallSide {
    Callers,
    Callees,
}

/// Attach the remainder of a `callers`/`callees` answer (see the module doc). Every route served
/// the certain rows only; with `include_inferred` the inferred rows are read from SQLite and
/// appended ONCE, whatever backend served, and `count` is restated.
fn attach_remainder(
    storage: &StorageConnection,
    snapshot_uid: &str,
    target: &ResolvedSymbol,
    side: CallSide,
    include_inferred: bool,
    value: &mut Value,
) -> Result<(), String> {
    let remainders = storage
        .find_symbol_call_remainders(snapshot_uid, &target.stable_key, &target.name)
        .map_err(|e| e.to_string())?;
    let (list_key, inferred_rows) = match side {
        CallSide::Callers => ("callers", &remainders.inferred_callers),
        CallSide::Callees => ("callees", &remainders.inferred_callees),
    };
    // A served answer is always an object carrying its row list and a `count` equal to the rows
    // listed. Its shape is checked BEFORE any flag-dependent step, whatever the flag: anything
    // else is refused, never passed on (a count without its rows, an answer without its
    // remainder; RG-REQ-002-L04, review-1 F-PRB-01).
    let Some(obj) = value.as_object_mut() else {
        return Err(format!(
            "the {list_key} answer is not a JSON object; its remainder cannot be attached"
        ));
    };
    let Some(Value::Array(rows)) = obj.get(list_key) else {
        return Err(format!(
            "the {list_key} answer carries no `{list_key}` row list; its count cannot be \
             trusted and its remainder cannot be attached"
        ));
    };
    let listed = rows.len();
    match obj.get("count").and_then(Value::as_u64) {
        Some(count) if count == listed as u64 => {}
        Some(count) => {
            return Err(format!(
                "the {list_key} answer's count ({count}) does not equal the {listed} rows it lists"
            ))
        }
        None => {
            return Err(format!(
                "the {list_key} answer's count is missing or unreadable beside its {listed} rows"
            ))
        }
    }
    if include_inferred {
        let inferred: Vec<Value> = match side {
            CallSide::Callers => storage
                .find_direct_callers(snapshot_uid, &target.stable_key, &CALLS)
                .map_err(|e| e.to_string())?
                .into_iter()
                .filter(|r| r.resolution == "inferred")
                .map(|r| json!(r))
                .collect(),
            CallSide::Callees => storage
                .find_direct_callees(snapshot_uid, &target.stable_key, &CALLS)
                .map_err(|e| e.to_string())?
                .into_iter()
                .filter(|r| r.resolution == "inferred")
                .map(|r| json!(r))
                .collect(),
        };
        let Some(Value::Array(list)) = obj.get_mut(list_key) else {
            unreachable!("the row list was checked to be an array above");
        };
        list.extend(inferred);
        let count = list.len();
        obj.insert("count".to_string(), json!(count));
    }
    if !inferred_rows.is_empty() {
        let mut by_basis: BTreeMap<&str, u64> = BTreeMap::new();
        for row in inferred_rows {
            *by_basis.entry(row.basis.as_str()).or_insert(0) += 1;
        }
        obj.insert(
            "inferred".to_string(),
            json!({ "count": inferred_rows.len(), "by_basis": by_basis }),
        );
    }
    match side {
        CallSide::Callers => {
            if remainders.unresolved_naming_total() > 0 {
                let mut block = unresolved_block(
                    &remainders.unresolved_naming,
                    &remainders.unresolved_naming_sites,
                );
                block.insert("name".to_string(), json!(target.name));
                obj.insert("unresolved_naming".to_string(), Value::Object(block));
            }
        }
        CallSide::Callees => {
            if remainders.unresolved_from_total() > 0 {
                let block = unresolved_block(
                    &remainders.unresolved_from,
                    &remainders.unresolved_from_sites,
                );
                obj.insert("unresolved_from".to_string(), Value::Object(block));
            }
        }
    }
    Ok(())
}

/// `{count, by_basis, sites}` of an unresolved remainder.
fn unresolved_block(
    counts: &[AgentBasisCount],
    sites: &[AgentUnresolvedCallSite],
) -> serde_json::Map<String, Value> {
    let by_basis: BTreeMap<&str, u64> = counts
        .iter()
        .map(|b| (b.basis_code.as_str(), b.count))
        .collect();
    let total: u64 = counts.iter().map(|b| b.count).sum();
    let mut block = serde_json::Map::new();
    block.insert("count".to_string(), json!(total));
    block.insert("by_basis".to_string(), json!(by_basis));
    block.insert(
        "sites".to_string(),
        Value::Array(sites.iter().map(site_json).collect()),
    );
    block
}

/// One site: `{file, line, target_key, basis, candidate_reason?, candidate_count?, candidates?}`.
fn site_json(site: &AgentUnresolvedCallSite) -> Value {
    let mut obj = serde_json::Map::new();
    obj.insert("file".to_string(), json!(site.file));
    obj.insert("line".to_string(), json!(site.line));
    obj.insert("target_key".to_string(), json!(site.target_key));
    obj.insert("basis".to_string(), json!(site.basis));
    if let Some(reason) = &site.candidate_reason {
        obj.insert("candidate_reason".to_string(), json!(reason));
    }
    if let Some(count) = site.candidate_count {
        obj.insert("candidate_count".to_string(), json!(count));
    }
    if let Some(candidates) = &site.candidates {
        obj.insert("candidates".to_string(), json!(candidates));
    }
    Value::Object(obj)
}

/// The SQLite `path` answer: the certain walk; when it finds no route, one admitting walk with the
/// same bound, and — when that finds a route — `inferred_edges_on_route` and `search_depth`.
///
/// TEST-EDGE-SCOPE-1B (D-TESB-15): with `include_inferred` the answer IS the admitting walk's route
/// (inferred call/import hops admitted), with `inferred_edges_on_route` stating how many of its hops
/// are inferred, and `include_inferred: true`.
pub(crate) fn sqlite_path_value(
    storage: &StorageConnection,
    repo_uid: &str,
    snapshot_uid: &str,
    from_stable_key: &str,
    to_stable_key: &str,
    include_inferred: bool,
) -> Result<Value, StorageError> {
    if include_inferred {
        let admitted = storage.find_shortest_path(
            snapshot_uid,
            from_stable_key,
            to_stable_key,
            PATH_SEARCH_DEPTH,
            true,
        )?;
        let found = admitted.path.found;
        let mut value = json!({
            "repo_uid": repo_uid,
            "snapshot_uid": snapshot_uid,
            "path": admitted.path,
            "found": found,
            "include_inferred": true,
        });
        if found {
            value["inferred_edges_on_route"] = json!(admitted.inferred_edges_on_route);
        }
        value["search_depth"] = json!(PATH_SEARCH_DEPTH);
        return Ok(value);
    }
    let certain = storage.find_shortest_path(
        snapshot_uid,
        from_stable_key,
        to_stable_key,
        PATH_SEARCH_DEPTH,
        false,
    )?;
    let found = certain.path.found;
    let mut value = json!({
        "repo_uid": repo_uid,
        "snapshot_uid": snapshot_uid,
        "path": certain.path,
        "found": found,
    });
    if !found {
        let admitted = storage.find_shortest_path(
            snapshot_uid,
            from_stable_key,
            to_stable_key,
            PATH_SEARCH_DEPTH,
            true,
        )?;
        if admitted.path.found && admitted.inferred_edges_on_route > 0 {
            value["inferred_edges_on_route"] = json!(admitted.inferred_edges_on_route);
            value["search_depth"] = json!(PATH_SEARCH_DEPTH);
        }
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    //! D-PRB-UNION-ROUTE-1 (implementation review-0 F-1): the union-arm routing predicate, and the
    //! per-route answers over the shared faithful callgraph fixture — a resident LiveGraph plus a
    //! SQLite mirror whose certificate and witness ledger read certain rows only — with one
    //! `inferred` CALLS row (`callerFn -> calleeFn`, line 7) stored in SQLite in the test body.

    use super::*;
    use crate::callgraph_cert::test_fixture::{self, Fixture};
    use crate::callgraph_cert::{callgraph_cert_eligibility, callgraph_union_eligibility};
    use repo_graph_storage::types::{GraphEdge, SourceLocation};

    const INFERRED_LINE: u64 = 7;

    fn epoch_with(f: &Fixture, fingerprint: Option<String>) -> RequestEpoch {
        let storage = f.state.storage().expect("open storage");
        let snapshot =
            repo_graph_agent::AgentStorageRead::get_latest_snapshot(&storage, test_fixture::REPO)
                .expect("snapshot query")
                .expect("ready snapshot");
        RequestEpoch {
            snapshot,
            fingerprint,
        }
    }

    fn resolve(f: &Fixture, symbol: &str) -> ResolvedSymbol {
        f.state
            .storage()
            .expect("open storage")
            .resolve_symbol(&f.snapshot_uid, symbol)
            .expect("resolve symbol")
    }

    /// One `inferred` CALLS row `callerFn -> calleeFn` (the fixture's nodes `ns0`/`ns1`), stored
    /// in the fixture's SQLite store only — the LiveGraph never holds an inferred binding.
    fn store_inferred_call(f: &Fixture) {
        let mut conn = StorageConnection::open(f.state.db_path()).expect("open storage");
        conn.insert_edges(&[GraphEdge {
            edge_uid: "einf0".into(),
            snapshot_uid: f.snapshot_uid.clone(),
            repo_uid: test_fixture::REPO.into(),
            source_node_uid: "ns0".into(),
            target_node_uid: "ns1".into(),
            edge_type: "CALLS".into(),
            resolution: "inferred".into(),
            extractor: "test".into(),
            location: Some(SourceLocation {
                line_start: INFERRED_LINE as i64,
                col_start: 2,
                line_end: INFERRED_LINE as i64,
                col_end: 12,
            }),
            metadata_json: Some(
                r#"{"basis":"receiver_untyped_name_only","receiver":"obj","candidates":["calleeFn"]}"#
                    .into(),
            ),
        }])
        .expect("insert the inferred CALLS row");
    }

    fn rows(v: &Value, field: &str) -> Vec<Value> {
        v[field].as_array().expect("rows array").clone()
    }

    fn inferred_rows(v: &Value, field: &str) -> Vec<Value> {
        rows(v, field)
            .into_iter()
            .filter(|r| r["resolution"] == "inferred")
            .collect()
    }

    fn assert_count_is_rows_listed(v: &Value, field: &str) {
        assert_eq!(
            v["count"].as_u64().expect("count") as usize,
            rows(v, field).len(),
            "count equals the rows listed"
        );
    }

    #[derive(Clone, Copy)]
    enum Side {
        Callers,
        Callees,
    }

    impl Side {
        fn field(self) -> &'static str {
            match self {
                Side::Callers => "callers",
                Side::Callees => "callees",
            }
        }
        /// The symbol whose answer holds the inferred row.
        fn symbol(self) -> &'static str {
            match self {
                Side::Callers => "calleeFn",
                Side::Callees => "callerFn",
            }
        }
        /// The stable key the inferred row names on this side.
        fn inferred_key(self) -> String {
            match self {
                Side::Callers => test_fixture::caller_key(),
                Side::Callees => test_fixture::callee_key(),
            }
        }
        fn engine_answer(
            self,
            q: &CallAnswerInputs<'_>,
            engine: Engine,
            include_inferred: bool,
            repo_root: &str,
        ) -> Value {
            match self {
                Side::Callers => {
                    callers_engine_answer(q, engine, include_inferred, self.symbol(), repo_root)
                }
                Side::Callees => {
                    callees_engine_answer(q, engine, include_inferred, self.symbol(), repo_root)
                }
            }
            .expect("engine answer")
        }
        fn union_answer(self, q: &CallAnswerInputs<'_>) -> Value {
            match self {
                Side::Callers => callers_union_answer(q),
                Side::Callees => callees_union_answer(q),
            }
            .expect("union answer")
        }
    }

    /// The one inferred row, listed once, with its identity and basis.
    fn assert_the_inferred_row_once(v: &Value, side: Side) {
        let inferred = inferred_rows(v, side.field());
        assert_eq!(
            inferred.len(),
            1,
            "each inferred row is listed exactly once"
        );
        assert_eq!(inferred[0]["stable_key"], json!(side.inferred_key()));
        assert_eq!(inferred[0]["inference_basis"], "receiver_untyped_name_only");
        if let Side::Callers = side {
            assert_eq!(
                inferred[0]["line"],
                json!(INFERRED_LINE),
                "a caller row anchors its call site"
            );
        }
        assert_eq!(v["inferred"]["count"], 1, "the remainder states it");
    }

    #[test]
    fn union_arm_is_selected_only_with_the_flag_on_the_auto_engine_and_without_include_inferred() {
        let engines = [
            Engine::Auto,
            Engine::Sqlite,
            Engine::LiveGraph,
            Engine::Compare,
        ];
        let mut selected = Vec::new();
        for flag in [false, true] {
            for engine in engines {
                for include in [false, true] {
                    if union_arm_selected(flag, engine, include) {
                        selected.push((flag, engine, include));
                    }
                }
            }
        }
        assert_eq!(
            selected,
            vec![(true, Engine::Auto, false)],
            "true in exactly one of the 16 cells"
        );
    }

    fn livegraph_served_auto_route_lists_each_inferred_row_once(side: Side) {
        let f = test_fixture::build_fixture(false);
        let target = resolve(&f, side.symbol());
        let storage = f.state.storage().expect("open storage");

        // The LiveGraph rows with nothing inferred stored.
        let fp = callgraph_cert_eligibility(&f.state, &f.snapshot_uid);
        assert!(fp.is_some(), "the faithful fixture's certificate is green");
        let epoch = epoch_with(&f, fp);
        let q = CallAnswerInputs {
            storage: &storage,
            repo_state: &f.state,
            epoch: &epoch,
            target: &target,
        };
        let before = side.engine_answer(&q, Engine::Auto, true, "");
        assert_eq!(before["backend_used"], "livegraph");
        let lg_rows = rows(&before, side.field());
        assert!(!lg_rows.is_empty());
        assert!(inferred_rows(&before, side.field()).is_empty());

        store_inferred_call(&f);
        // An inferred row changes no certain fact: the certificate stays green.
        let fp = callgraph_cert_eligibility(&f.state, &f.snapshot_uid);
        assert!(fp.is_some(), "the certificate reads certain rows only");
        let epoch = epoch_with(&f, fp);
        let q = CallAnswerInputs {
            storage: &storage,
            repo_state: &f.state,
            epoch: &epoch,
            target: &target,
        };
        let v = side.engine_answer(&q, Engine::Auto, true, "");
        assert_eq!(v["backend_used"], "livegraph");
        let listed = rows(&v, side.field());
        assert_eq!(
            listed[..lg_rows.len()],
            lg_rows[..],
            "the LiveGraph rows come first, unchanged"
        );
        assert_eq!(listed.len(), lg_rows.len() + 1, "then the inferred row");
        assert_the_inferred_row_once(&v, side);
        assert_count_is_rows_listed(&v, side.field());

        // Without the flag the inferred row is stated, never listed.
        let v = side.engine_answer(&q, Engine::Auto, false, "");
        assert_eq!(rows(&v, side.field()), lg_rows);
        assert_eq!(v["inferred"]["count"], 1);
        assert_count_is_rows_listed(&v, side.field());
    }

    #[test]
    fn callers_include_inferred_on_the_livegraph_served_auto_route_lists_each_inferred_row_once() {
        livegraph_served_auto_route_lists_each_inferred_row_once(Side::Callers);
    }

    #[test]
    fn callees_include_inferred_on_the_livegraph_served_auto_route_lists_each_inferred_row_once() {
        livegraph_served_auto_route_lists_each_inferred_row_once(Side::Callees);
    }

    fn compare_engine_compares_only_certain_rows(side: Side) {
        let f = test_fixture::build_fixture(false);
        let root = tempfile::tempdir().expect("sidecar root");
        let root = root.path().to_str().expect("utf-8 root").to_string();
        let target = resolve(&f, side.symbol());
        let storage = f.state.storage().expect("open storage");
        let fp = callgraph_cert_eligibility(&f.state, &f.snapshot_uid);
        let epoch = epoch_with(&f, fp);
        let q = CallAnswerInputs {
            storage: &storage,
            repo_state: &f.state,
            epoch: &epoch,
            target: &target,
        };

        let clean = side.engine_answer(&q, Engine::Compare, true, &root);
        assert!(clean["livegraph_compare"].is_object());
        store_inferred_call(&f);
        let v = side.engine_answer(&q, Engine::Compare, true, &root);

        assert_eq!(
            v["livegraph_compare"], clean["livegraph_compare"],
            "the compare report is the one the fixture yields with no inferred row stored"
        );
        let clean_rows = rows(&clean, side.field());
        let listed = rows(&v, side.field());
        assert_eq!(listed[..clean_rows.len()], clean_rows[..]);
        assert_eq!(listed.len(), clean_rows.len() + 1);
        assert_the_inferred_row_once(&v, side);
        assert_count_is_rows_listed(&v, side.field());
    }

    #[test]
    fn callers_compare_engine_compares_only_certain_rows_with_include_inferred() {
        compare_engine_compares_only_certain_rows(Side::Callers);
    }

    #[test]
    fn callees_compare_engine_compares_only_certain_rows_with_include_inferred() {
        compare_engine_compares_only_certain_rows(Side::Callees);
    }

    fn union_answer_holds_no_inferred_row(side: Side) {
        let f = test_fixture::build_fixture(false);
        store_inferred_call(&f);
        let target = resolve(&f, side.symbol());
        let storage = f.state.storage().expect("open storage");
        // The flag-on capture, exactly as the dispatch arm makes it.
        let fp = callgraph_union_eligibility(&f.state, &f.snapshot_uid);
        assert!(fp.is_some(), "the flag-on capture returns a fingerprint");
        let epoch = epoch_with(&f, fp);
        let q = CallAnswerInputs {
            storage: &storage,
            repo_state: &f.state,
            epoch: &epoch,
            target: &target,
        };
        let v = side.union_answer(&q);
        assert_eq!(v["backend_used"], "union");
        assert!(
            inferred_rows(&v, side.field()).is_empty(),
            "a union answer holds no inferred row"
        );
        let listed = rows(&v, side.field());
        assert_eq!(listed.len(), 1, "the one certain row, corroborated");
        assert_eq!(listed[0]["witness"], "both");
        let w = &v["witness_counts"];
        let witnessed: u64 = ["both", "semantic_only", "syntactic_only", "unmeasured"]
            .iter()
            .map(|k| w[*k].as_u64().expect("witness count"))
            .sum();
        assert_eq!(
            witnessed,
            v["count"].as_u64().expect("count"),
            "witness_counts sums to count"
        );
        assert_count_is_rows_listed(&v, side.field());
        assert_eq!(v["inferred"]["count"], 1, "the remainder is still stated");
    }

    #[test]
    fn callers_union_answer_holds_no_inferred_row_and_its_witness_counts_match_its_rows() {
        union_answer_holds_no_inferred_row(Side::Callers);
    }

    #[test]
    fn callees_union_answer_holds_no_inferred_row_and_its_witness_counts_match_its_rows() {
        union_answer_holds_no_inferred_row(Side::Callees);
    }

    // ── Review F-2: a malformed input is refused by name, never read as a default ──────────

    fn row(resolution: &str) -> CallerResult {
        CallerResult {
            stable_key: format!("k-{resolution}"),
            name: "n".into(),
            qualified_name: None,
            kind: "SYMBOL".into(),
            subtype: None,
            file: None,
            line: None,
            column: None,
            edge_type: "CALLS".into(),
            resolution: resolution.into(),
            inference_basis: None,
            inference_extractor: None,
        }
    }

    #[test]
    fn certain_only_keeps_certain_rows_and_refuses_an_unreadable_resolution_never_drops_it() {
        let kept = certain_only(vec![row("static"), row("inferred"), row("dynamic")])
            .expect("a readable list");
        let kept: Vec<&str> = kept.iter().map(|r| r.resolution.as_str()).collect();
        assert_eq!(kept, vec!["static", "dynamic"]);
        for bad in ["resolved", "", "Static"] {
            let err = certain_only(vec![row("static"), row(bad)])
                .expect_err("an unreadable resolution refuses the answer");
            assert!(err.to_string().contains("unreadable resolution"), "{err}");
        }
    }

    #[test]
    fn include_inferred_param_absent_or_boolean_is_accepted_and_any_other_value_is_refused() {
        for ok in [
            json!({}),
            json!({"include_inferred": true}),
            json!({"include_inferred": false}),
        ] {
            assert!(check_include_inferred_param(&ok).is_ok(), "{ok}");
        }
        assert!(include_inferred(&json!({"include_inferred": true})));
        assert!(!include_inferred(&json!({"include_inferred": false})));
        assert!(!include_inferred(&json!({})));
        for bad in [
            json!("yes"),
            json!("true"),
            json!(1),
            Value::Null,
            json!([true]),
        ] {
            let err = check_include_inferred_param(&json!({ "include_inferred": bad }))
                .expect_err("a non-boolean is refused, never read as false");
            assert!(err.contains("include_inferred"), "{err}");
        }
    }

    #[test]
    fn a_non_object_or_listless_answer_is_refused_never_passed_on_without_its_remainder() {
        let f = test_fixture::build_fixture(false);
        let target = resolve(&f, "calleeFn");
        let storage = f.state.storage().expect("open storage");
        let snap = f.snapshot_uid.clone();
        let mut not_an_object = json!(["callers"]);
        let err = attach_remainder(
            &storage,
            &snap,
            &target,
            CallSide::Callers,
            false,
            &mut not_an_object,
        )
        .expect_err("a non-object answer is refused");
        assert!(err.contains("not a JSON object"), "{err}");
        // Refused whatever the flag: the shape is checked before any flag-dependent step.
        for include in [false, true] {
            let mut listless = json!({ "count": 1 });
            let err = attach_remainder(
                &storage,
                &snap,
                &target,
                CallSide::Callers,
                include,
                &mut listless,
            )
            .expect_err("an answer without its row list is refused");
            assert!(
                err.contains("row list"),
                "include_inferred={include}: {err}"
            );
        }
    }

    #[test]
    fn a_count_that_does_not_equal_the_rows_listed_is_refused_whatever_the_flag() {
        let f = test_fixture::build_fixture(false);
        let target = resolve(&f, "calleeFn");
        let storage = f.state.storage().expect("open storage");
        let snap = f.snapshot_uid.clone();
        let attach = |include: bool, mut v: Value| {
            attach_remainder(&storage, &snap, &target, CallSide::Callers, include, &mut v)
        };
        for include in [false, true] {
            let err = attach(include, json!({ "callers": [], "count": 1 }))
                .expect_err("a count above the rows listed is refused");
            assert!(
                err.contains("does not equal"),
                "include_inferred={include}: {err}"
            );
            let err = attach(include, json!({ "callers": [], "count": "0" }))
                .expect_err("an unreadable count is refused");
            assert!(
                err.contains("missing or unreadable"),
                "include_inferred={include}: {err}"
            );
            let err =
                attach(include, json!({ "callers": [] })).expect_err("a missing count is refused");
            assert!(
                err.contains("missing or unreadable"),
                "include_inferred={include}: {err}"
            );
            attach(include, json!({ "callers": [], "count": 0 }))
                .expect("a well-formed answer takes its remainder");
        }
    }

    #[test]
    fn an_unreadable_stored_resolution_refuses_every_route_s_answer() {
        let f = test_fixture::build_fixture(false);
        let mut conn = StorageConnection::open(f.state.db_path()).expect("open storage");
        conn.insert_edges(&[GraphEdge {
            edge_uid: "ebad0".into(),
            snapshot_uid: f.snapshot_uid.clone(),
            repo_uid: test_fixture::REPO.into(),
            source_node_uid: "ns0".into(),
            target_node_uid: "ns1".into(),
            edge_type: "CALLS".into(),
            resolution: "resolved".into(),
            extractor: "test".into(),
            location: None,
            metadata_json: None,
        }])
        .expect("insert the unreadable CALLS row");
        let target = resolve(&f, "calleeFn");
        let storage = f.state.storage().expect("open storage");
        let epoch = epoch_with(&f, None);
        let q = CallAnswerInputs {
            storage: &storage,
            repo_state: &f.state,
            epoch: &epoch,
            target: &target,
        };
        for engine in [
            Engine::Auto,
            Engine::Sqlite,
            Engine::LiveGraph,
            Engine::Compare,
        ] {
            for include in [false, true] {
                let root = tempfile::tempdir().expect("sidecar root");
                let root = root.path().to_str().expect("utf-8").to_string();
                let err = callers_engine_answer(&q, engine, include, "calleeFn", &root)
                    .expect_err("an unreadable resolution refuses the answer");
                assert!(
                    err.contains("unreadable resolution"),
                    "{engine:?} {include}: {err}"
                );
            }
        }
        let err = callers_union_answer(&q).expect_err("the union arm refuses too");
        assert!(err.contains("unreadable resolution"), "{err}");
    }
}
