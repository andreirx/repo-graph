//! FIND-FACTS-1 — the FACTS-tier seam proofs (TIER-BEHAVIOR half), driven through the
//! REAL `ServiceDispatcher::dispatch` `find` surface. Split out of `seed_seam.rs`
//! (review-7 item 2); the PER-CLASS honesty proofs live in the sibling
//! `find_facts_class_honesty_seam.rs` — split so each integration binary stays under
//! the 500-line structural guardrail (review-8; the `index_basis` split precedent).
//! The shared dispatcher/real-git/fake-embed harness lives in `tests/seed_harness/mod.rs`
//! (see its abstraction record).
//!
//! This file owns the TIER-LEVEL proofs: the facts tier answers a lexical query FROM
//! THE FACT TABLES — labeled by fact class, certainty layer, and a runnable next
//! command — ABOVE and independent of the demoted embedding seed tier, survives the
//! endpoint being DOWN, skips the endpoint entirely under `--exact`, and produces a
//! correctly-shaped labeled hit for all seven fact classes through the real read paths.

mod seed_harness;
use seed_harness::*;

use repo_graph_seed::SeedCorpusRead;
use repo_graph_storage::StorageConnection;
use serde_json::{json, Value};

/// The facts tier answers a lexical identifier query FROM THE FACT TABLES even when
/// the embedding endpoint is DOWN and NO seed store was ever published — and the
/// demoted seed tier says unavailable-with-reason. This is the §4 live-proof shape
/// in the isolated harness: facts survive the model being gone.
#[test]
fn find_facts_tier_answers_without_the_embedding_endpoint() {
    let _env = SeedEnv::with_endpoint("http://127.0.0.1:9/v1/embeddings"); // model down
    let (d, _root) = isolated();
    let repo = make_repo(); // helper.ts defines helperFunction; main.ts defines mainEntry
    let idx = dispatch_ok(
        &d,
        "index",
        json!({ "repo_path": repo.path().to_string_lossy() }),
    );
    let _ = coords(&idx); // NO store published — the facts tier does not need one.

    let resp = dispatch_ok(
        &d,
        "find",
        json!({ "repo": repo.path().to_string_lossy(), "query": "helper" }),
    );

    // All seven fact classes are named, in the fixed order (honest searched set).
    let facts = resp["facts"].as_array().expect("facts array present");
    let labels: Vec<&str> = facts
        .iter()
        .filter_map(|g| g["fact_class"].as_str())
        .collect();
    assert_eq!(
        labels,
        vec![
            "symbol",
            "file",
            "module",
            "http-surface",
            "dependency",
            "framework",
            "boundary"
        ],
        "every fact class searched, in order: {resp}"
    );

    // symbol class hits helperFunction, labeled → explain.
    let symbol = facts
        .iter()
        .find(|g| g["fact_class"] == "symbol")
        .expect("symbol group");
    assert_eq!(symbol["render_command"], "explain");
    assert!(
        symbol["hits"]
            .as_array()
            .unwrap()
            .iter()
            .any(|h| h["display"].as_str().unwrap_or("").contains("helper")),
        "symbol hit for helper: {symbol}"
    );

    // file class hits helper.ts, labeled → explain.
    let file = facts
        .iter()
        .find(|g| g["fact_class"] == "file")
        .expect("file group");
    assert!(
        file["hits"]
            .as_array()
            .unwrap()
            .iter()
            .any(|h| h["path"].as_str().unwrap_or("").ends_with("helper.ts")),
        "file hit for helper.ts: {file}"
    );

    // The seed tier is DEMOTED and unavailable-with-reason (endpoint down) — the
    // facts above still answered. The verb no longer dies with the endpoint.
    assert_eq!(
        resp["seeds_available"].as_bool(),
        Some(false),
        "seeds unavailable when the model is down"
    );
    assert!(
        !resp["seeds_unavailable_reason"]
            .as_str()
            .unwrap_or("")
            .is_empty(),
        "seed unavailability carries a reason: {resp}"
    );
}

/// `--exact` renders the facts tier and NEVER consults the endpoint: the seed
/// unavailability reason is the not-consulted marker, DISTINCT from the model-down
/// reason — proving the endpoint path was skipped, not merely failed.
#[test]
fn find_exact_never_consults_the_endpoint() {
    let _env = SeedEnv::with_endpoint("http://127.0.0.1:9/v1/embeddings"); // would fail IF touched
    let (d, _root) = isolated();
    let repo = make_repo();
    let idx = dispatch_ok(
        &d,
        "index",
        json!({ "repo_path": repo.path().to_string_lossy() }),
    );
    let _ = coords(&idx);

    let resp = dispatch_ok(
        &d,
        "find",
        json!({ "repo": repo.path().to_string_lossy(), "query": "helper", "exact": true }),
    );

    // Facts still answer.
    assert!(resp["facts"].as_array().is_some_and(|f| f.len() == 7));
    // Seeds not consulted — the reason is the --exact marker, not "model reachable".
    assert_eq!(resp["seeds_available"].as_bool(), Some(false));
    let reason = resp["seeds_unavailable_reason"].as_str().unwrap_or("");
    assert!(
        reason.contains("--exact"),
        "--exact skips the endpoint (not consulted), got: {reason}"
    );
    assert!(
        resp["candidates"].as_array().is_some_and(|c| c.is_empty()),
        "no seed candidates under --exact"
    );
}

/// FIND-FACTS-1 revision 1 (operator ruling item 5): the seam proves ALL SEVEN fact
/// classes end to end — each renders its VERIFIED runnable command label AND produces
/// a correctly-SHAPED hit through the REAL `find` dispatch, not just symbol/file.
///
/// `symbol` + `file` come from the indexed repo. The other five classes source from
/// tables/compose paths a bare two-file repo never populates, so we insert ONE
/// controlled row (or the minimal owning chain) per class directly — the same raw-
/// insert seam `find_attaches_genuine_owning_module_from_ownership_row` uses (FKs are
/// off on a bare `rusqlite` connection; the committed rows are visible to the daemon's
/// next read). Every inserted identity contains the shared needle `helper`, so one
/// `find helper` exercises all seven read paths at once. One `module_candidates` row
/// serves BOTH the `module` class and (as helper.ts's owner) the `dependency` compose.
#[test]
fn find_facts_all_seven_classes_produce_labeled_hits() {
    let _env = SeedEnv::with_endpoint("http://127.0.0.1:9/v1/embeddings"); // untouched under --exact
    let (d, _root) = isolated();
    let repo = make_repo(); // helper.ts defines helperFunction (symbol + file classes)
    let idx = dispatch_ok(
        &d,
        "index",
        json!({ "repo_path": repo.path().to_string_lossy() }),
    );
    let (db_path, repo_uid) = coords(&idx);
    let snapshot_uid = idx["snapshot_uid"]
        .as_str()
        .expect("index returns snapshot_uid")
        .to_string();

    // Resolve helper.ts's real file_uid from the corpus (the dependency chain + the
    // http-surface source_file reference a real tracked file).
    let corpus = StorageConnection::open(&db_path).unwrap();
    let entries = corpus.seed_corpus(&repo_uid).unwrap();
    let helper_uid = entries
        .iter()
        .find(|e| e.path.ends_with("helper.ts"))
        .expect("helper.ts in corpus")
        .file_uid
        .clone();
    drop(corpus);

    // One raw batch seeds the five remaining classes. WAL + busy_timeout let the write
    // commit while the daemon holds its cached idle connection (same pattern as the
    // ownership seam test). The FK parents (repo, snapshot) exist from `index`.
    let raw = rusqlite::Connection::open(&db_path).expect("raw open for fact-class seed");
    raw.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
    // FK enforcement OFF on this SEED connection so a fixture row need not fabricate every
    // FK parent the daemon's READ never joins (e.g. the http-surface's `project_surfaces`).
    // Product code is unaffected — the daemon reads through its own FK-checked connection.
    raw.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
    // NB: no `--` SQL comments inside this batch — the `\` line continuations strip the
    // newlines, so a `--` would comment out the REST of the (now single-line) batch.
    // Row provenance, in statement order:
    //   1. module_candidates — the `module` class AND (as helper.ts's owner) the owning
    //      module the dependency compose attributes the declared dep to. `npm:` key so
    //      the TypeScript-dominant ecosystem admits it; canonical_root_path carries the needle.
    //   2. module_file_ownership — links helper.ts → that module.
    //   3. file_signals.package_dependencies_json — the DECLARED dep name (`dependency`
    //      class); has_manifest becomes true ⇒ it surfaces as a DeclaredButUnobserved entry.
    //   4. boundary_interaction_surfaces — the `http-surface` class (evidence_json MUST
    //      carry httpMethod; route + source_file carry the needle).
    //   5. inferences — the `framework` class (the inference KIND carries the needle).
    //   6. declarations — the `boundary` class (review-6 re-home). A REPO-scoped
    //      (NULL snapshot) ACTIVE `boundary`-kind declaration whose `target_stable_key`
    //      carries the needle; `rmap violations` renders exactly this kind, so the emitted
    //      `next` (`violations`) renders the hit's declaration — proven by
    //      `find_boundary_hit_next_command_renders_the_declaration` below. This REPLACES
    //      the former orphan `surface_entrypoints` row, whose renderer never existed.
    raw.execute_batch(&format!(
        "INSERT INTO module_candidates \
           (module_candidate_uid, snapshot_uid, repo_uid, module_key, module_kind, canonical_root_path, confidence, display_name) \
         VALUES ('mc-helper-facts', '{snapshot_uid}', '{repo_uid}', 'npm:facts:helper-mod', 'inferred', 'helper-mod', 1.0, 'helper-module'); \
         INSERT INTO module_file_ownership \
           (snapshot_uid, repo_uid, file_uid, module_candidate_uid, assignment_kind, confidence) \
         VALUES ('{snapshot_uid}', '{repo_uid}', '{helper_uid}', 'mc-helper-facts', 'exact', 1.0); \
         INSERT INTO file_signals (snapshot_uid, file_uid, package_dependencies_json) \
         VALUES ('{snapshot_uid}', '{helper_uid}', '{{\"names\":[\"helper-dep\"]}}'); \
         INSERT INTO boundary_interaction_surfaces \
           (surface_uid, snapshot_uid, repo_uid, boundary_scope, channel_kind, direction, protocol, \
            protocol_family, interaction_pattern, endpoint_locality, symbol_stable_key, source_file, \
            line_start, line_end, col_start, col_end, extractor, basis, confidence, evidence_json) \
         VALUES ('bis-helper', '{snapshot_uid}', '{repo_uid}', 'unknown', 'http', 'inbound', 'http', \
            'http', 'request_response', 'unknown', 'helper.ts:route', 'helper.ts', \
            1, 1, 1, 1, 'test', 'api_call', 1.0, '{{\"httpMethod\":\"GET\",\"route\":\"/api/helper\"}}'); \
         INSERT INTO inferences \
           (inference_uid, snapshot_uid, repo_uid, target_stable_key, kind, value_json, confidence, basis_json, extractor, created_at) \
         VALUES ('inf-helper', '{snapshot_uid}', '{repo_uid}', 'helper.ts:comp', 'helper_component', '{{}}', 1.0, '{{}}', 'test', '2026-01-01T00:00:00Z'); \
         INSERT INTO declarations \
           (declaration_uid, repo_uid, snapshot_uid, target_stable_key, kind, value_json, created_at, is_active) \
         VALUES ('decl-helper', '{repo_uid}', NULL, '{repo_uid}:helper-boundary:MODULE', 'boundary', '{{\"forbids\":\"src/x\"}}', '2026-01-01T00:00:00Z', 1);"
    ))
    .expect("seed the five non-indexed fact classes");
    drop(raw); // release before the daemon reads

    // `--exact`: facts only, the (dead) endpoint never touched.
    let resp = dispatch_ok(
        &d,
        "find",
        json!({ "repo": repo.path().to_string_lossy(), "query": "helper", "exact": true }),
    );
    let facts = resp["facts"].as_array().expect("facts array present");

    // Locate a class group by tag; every class group is ALWAYS present.
    let group = |class: &str| -> &Value {
        facts
            .iter()
            .find(|g| g["fact_class"] == class)
            .unwrap_or_else(|| panic!("group {class} present: {resp}"))
    };
    // A group's hits contain at least one whose display/path/key evidences the needle.
    let has_hit = |g: &Value, pred: &dyn Fn(&Value) -> bool| -> bool {
        g["hits"]
            .as_array()
            .map(|h| h.iter().any(pred))
            .unwrap_or(false)
    };
    let display_has = |needle: &'static str| {
        move |h: &Value| h["display"].as_str().unwrap_or("").contains(needle)
    };

    // (class, verified runnable render command, a hit-shape predicate) — all SEVEN.
    // The render command is a VERIFIED runnable form (revision 1 item 1); the predicate
    // proves a correctly-shaped hit flowed through the REAL read path for that class.
    struct Case {
        class: &'static str,
        /// The single class-level render command, or `None` for a per-hit-renderer
        /// class (the `boundary` governance-declaration class — review-6 re-home).
        command: Option<&'static str>,
        certainty: &'static str,
        /// `true` when this class's per-hit `next` folds the hit key into the command
        /// (`explain <key>` / `map <path>`); `false` for the `… list` classes whose
        /// per-hit next IS the whole-listing command (review-1 item 1).
        per_hit_arg: bool,
        /// The exact per-hit `next` expected for this seeded fixture's hit (the
        /// whole-listing command for the list classes; the seeded declaration's
        /// renderer `violations` for the boundary fixture).
        expected_next_for_fixture: &'static str,
    }
    let cases = [
        Case {
            class: "symbol",
            command: Some("explain"),
            certainty: "extracted",
            per_hit_arg: true,
            expected_next_for_fixture: "explain",
        },
        Case {
            class: "file",
            command: Some("explain"),
            certainty: "extracted",
            per_hit_arg: true,
            expected_next_for_fixture: "explain",
        },
        Case {
            // `map` writes MAP.md by default; the ratified non-mutating render form is
            // `map --dry-run` (review-2 item 1), so the per-hit next is
            // `map --dry-run <path>`.
            class: "module",
            command: Some("map --dry-run"),
            certainty: "inferred",
            per_hit_arg: true,
            expected_next_for_fixture: "map --dry-run",
        },
        Case {
            class: "http-surface",
            command: Some("boundaries list"),
            certainty: "inferred",
            per_hit_arg: false,
            expected_next_for_fixture: "boundaries list",
        },
        Case {
            class: "dependency",
            command: Some("deps list"),
            certainty: "extracted",
            per_hit_arg: false,
            expected_next_for_fixture: "deps list",
        },
        Case {
            class: "framework",
            command: Some("inferences list"),
            certainty: "hint",
            per_hit_arg: false,
            expected_next_for_fixture: "inferences list",
        },
        Case {
            // review-6 re-home: the governance-declaration class has NO single group
            // render command; the seeded fixture is a `boundary`-kind declaration, whose
            // renderer is `rmap violations`. Layer-4 governance certainty.
            class: "boundary",
            command: None,
            certainty: "governance",
            per_hit_arg: false,
            expected_next_for_fixture: "violations",
        },
    ];
    for c in &cases {
        let g = group(c.class);
        // The single-renderer classes carry their verified render command; the per-hit
        // boundary class carries NONE (its renderer varies by declaration kind).
        match c.command {
            Some(cmd) => assert_eq!(
                g["render_command"].as_str(),
                Some(cmd),
                "{}: verified runnable render command: {g}",
                c.class
            ),
            None => assert!(
                g.get("render_command").is_none(),
                "{}: per-hit-renderer class carries no single render command: {g}",
                c.class
            ),
        }
        // review-1 honesty defect: every class carries its certainty layer tag, so a
        // Layer 2–4 hit is never presented as an extracted fact.
        assert_eq!(
            g["certainty"].as_str(),
            Some(c.certainty),
            "{}: certainty tag present: {g}",
            c.class
        );
        assert!(
            g.get("error").and_then(|e| e.as_str()).is_none(),
            "{}: class read did not fail: {g}",
            c.class
        );
        // review-1 item 1: every hit carries a runnable `next` command. For the
        // argument-taking classes it is `<verb> <key>` (starts with the verb + a
        // space, longer than the bare verb); for the list classes it IS the command.
        let first = g["hits"]
            .as_array()
            .and_then(|h| h.first())
            .unwrap_or_else(|| panic!("{}: at least one hit: {g}", c.class));
        let next = first["next"]
            .as_str()
            .unwrap_or_else(|| panic!("{}: hit carries a next command: {first}", c.class));
        let expected = c.expected_next_for_fixture;
        if c.per_hit_arg {
            assert!(
                next.starts_with(&format!("{expected} ")) && next.len() > expected.len() + 1,
                "{}: per-hit next folds the key into the command: {next}",
                c.class
            );
        } else {
            assert_eq!(
                next, expected,
                "{}: whole-listing per-hit next is the renderer command: {next}",
                c.class
            );
        }
    }

    // Per-class hit SHAPE (§2.2): display carries the identity; key/path present where
    // the class contract carries one (symbol→stable_key, file→path, module→root path,
    // dependency→package). http-surface + boundary carry no follow-up key (None ⇒ omitted).
    assert!(
        has_hit(group("symbol"), &|h| display_has("helper")(h)
            && h["key"].as_str().is_some()),
        "symbol hit shape (display + key): {}",
        group("symbol")
    );
    assert!(
        has_hit(group("file"), &|h| h["path"]
            .as_str()
            .unwrap_or("")
            .ends_with("helper.ts")),
        "file hit shape (path): {}",
        group("file")
    );
    assert!(
        has_hit(group("module"), &|h| {
            (display_has("helper")(h)) && h["key"].as_str() == Some("helper-mod")
        }),
        "module hit shape (display + canonical-root key): {}",
        group("module")
    );
    assert!(
        has_hit(group("http-surface"), &|h| {
            let disp = h["display"].as_str().unwrap_or("");
            disp.contains("GET") && disp.contains("/api/helper") && h.get("key").is_none()
        }),
        "http-surface hit shape (method + route display, no key): {}",
        group("http-surface")
    );
    assert!(
        has_hit(group("dependency"), &|h| {
            h["display"].as_str() == Some("helper-dep") && h["key"].as_str() == Some("helper-dep")
        }),
        "dependency hit shape (package display + key): {}",
        group("dependency")
    );
    assert!(
        has_hit(group("framework"), &|h| {
            display_has("helper_component")(h) && h["key"].as_str() == Some("helper_component")
        }),
        "framework hit shape (kind display + key): {}",
        group("framework")
    );
    assert!(
        has_hit(group("boundary"), &|h| {
            // review-6 re-home: a governance DECLARATION hit — display carries the kind +
            // the declaration's target key (which holds the needle); no follow-up key
            // (the renderer is a whole-listing command), and the per-hit next is the
            // declaration kind's renderer (`violations`).
            display_has("helper-boundary")(h)
                && h["display"]
                    .as_str()
                    .unwrap_or("")
                    .contains("boundary declaration")
                && h.get("key").is_none()
                && h["next"].as_str() == Some("violations")
        }),
        "boundary hit shape (declaration display, no key, violations next): {}",
        group("boundary")
    );
}
