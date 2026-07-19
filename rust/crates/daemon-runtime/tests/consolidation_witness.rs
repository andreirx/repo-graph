//! EC-M1-WITNESS-1 — the reader-set + fact-class witness (EC-1 milestone M-1, predicate C-6).
//!
//! CONTRACT: `docs/slices/ec-m1-witness-1.md` §2; EC-1 (`engine-consolidation-1.md`)
//! §5.1 C-6 ("The reader-set witness is enforced"), §5.2 M-1, §3.3-A (the mixed-read
//! ten), §3.2/§3.3 (the fact-class taxonomy + per-arm inventory).
//!
//! WHY THIS FILE EXISTS. The ratified consolidation end-state (EC-1 §4.2) says which
//! engine owns which read path, but nothing ENFORCES it — drift was free: a new module
//! could start reading the in-memory `RepoState.livegraph` engine, or a new dispatch arm
//! could appear, with nothing going red. This is that guard (CLAUDE.md: "if a rule can be
//! enforced by script, prefer enforcement over instruction"). It is a plain `cargo test`
//! integration test, so it rides the standard workspace gate with ZERO new CI surface —
//! the least-new-surface form (recorded in the slice report). It lives under `tests/` (not
//! `src/`) deliberately: the field-read scanner walks `src/` only, so the `.livegraph`
//! strings in this file's own fixtures never pollute the scan.
//!
//! TWO GUARDS, matching the contract's two parts:
//!
//!  1. THE READER-SET WITNESS (§2.1). The `livegraph` field on `RepoState`
//!     (`state.rs`: `pub livegraph: RwLock<Option<LiveGraph>>`) is the current-state
//!     in-memory engine. EC-1 §7.3's method (from `daemon-w-b-epoch-1.md` §7.3) is to
//!     trace every FILE that reads that field crate-wide — NOT to grep `dispatch.rs` —
//!     because a request handler reaches the field through a *called* serving/cert/
//!     coherence module, not from its own match arm. This witness recomputes that file
//!     set from the tree and asserts it equals the committed, reviewed sanctioned list
//!     (`witness/livegraph_reader_set.txt`), and that every production reader serves one
//!     of the 12 ratified surfaces (the §3.3-A ten handlers + the two LiveGraph writers).
//!     A new reader goes RED until the manifest is updated under review — "new features
//!     pay ONE integration by construction". The `[test-scaffolding]` files (cfg(test)
//!     fixtures that touch the field) are additionally proven `#[cfg(test)]`-gated by
//!     resolving each one's `mod` inclusion chain to the crate root (review-0 item 1) — so
//!     removing a parent `#[cfg(test)]`, which would promote a listed test file to a
//!     production reader WITHOUT changing the file set, goes RED.
//!
//!  2. THE FACT-CLASS MANIFEST (§2.2). Every arm of the ONE dispatch match
//!     (`dispatch.rs`: `match request.method.as_str()`) must be declared in
//!     `witness/dispatch_fact_classes.txt`, and no manifest line may name an arm that no
//!     longer exists (completeness + no-stale, reconciled against the LIVE arm count —
//!     66 today per EC-1 §3.3). Per §2.2 the witness reconciles arm NAMES for completeness,
//!     and SYNTACTICALLY validates each arm's declared classes (review-0 item 3): non-empty,
//!     every token a known §3.2 taxonomy name, `none` only alone. WHICH classes an arm truly
//!     reads stays the audit's job — the manifest is the reviewed declaration surface that
//!     makes a new arm pay its integration explicitly.
//!
//! FIELD-READ PRECISION. A "field read" is the token `.livegraph` NOT followed by
//! `[A-Za-z0-9_]` — a field access or by-ref pass. This mirrors §7.3's `rg` method and
//! deliberately EXCLUDES the field DEFINITION (`pub livegraph:`), construction
//! (`livegraph: RwLock::new(..)`), the crate path `repo_graph_livegraph::`, the sibling
//! modules `livegraph_feed`/`livegraph_refresh`, and look-alike identifiers
//! (`data.livegraph_count`, `d.livegraph_only`, …). Like §7.3 it is a naming-site guard:
//! a helper that receives the `RwLock` through a param it names differently is invisible
//! here exactly as it is to `rg` — but every such helper's CALLER names the field and is
//! caught. Scope is `daemon-runtime/src` because `RepoState` (hence the field) is owned
//! there; no other crate can name it.
//!
//! DELIBERATE-VIOLATION FIXTURES (§2.3). The comparison logic is factored into pure
//! reconcilers; the `fixture_*` tests inject synthetic drift (an unlisted reader, a stale
//! entry, a non-sanctioned surface, an undeclared/stale arm, an un-gated test reader, an
//! empty/unknown/none-mixed fact-class line) and assert each is detected — proving the guard
//! FAILS on drift, in-process, without mutating the tree.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

/// The ratified sanctioned surfaces (EC-1 M-1: the sanctioned list is the §3.3-A
/// mixed-read handlers together with the two LiveGraph writers). Hard-coded here — NOT
/// derived from the manifest — so a new surface cannot be admitted silently: adding one
/// requires a reviewed change to BOTH this constant and `livegraph_reader_set.txt`
/// (i.e. a ratified §3.3-A amendment).
///
/// RECON-M-R3a AMENDMENT (2026-07-18, shipped for review with the M-R3a slice): FOUR read
/// surfaces added — `modules_show`, `modules_list`, `map`, `storage_health` — because the
/// ratified recon-design-1 §6.1 M-R3a row mandates the witness-ledger read surfaces on
/// modules (g2u), map (g3u) and doctor (the §5.4 operational block via `storage_health`),
/// and rendering a ledger FIGURE honestly requires the resident-fingerprint currency check
/// (a `.livegraph` read; "never a stale number"). All four read through ONE module
/// (`witness_projection/mod.rs`), peek-only.
const SANCTIONED_SURFACES: &[&str] = &[
    // §3.3-A mixed-read ten (the LiveGraph-field readers among request handlers)
    "callers",
    "callees",
    "path",
    "imports",
    "cycles",
    "stats",
    "orient",
    "explain",
    "trust",
    "cycle_completeness_audit",
    // the two LiveGraph writers
    "livegraph_preload",
    "livegraph_refresh",
    // RECON-M-R3a witness read surfaces — M-R3A-READER-SET-AMENDMENT, operator-RATIFIED
    // 2026-07-19 (the manifest header carries the amendment record; two-site change).
    "modules_show",
    "modules_list",
    "map",
    "storage_health",
];

/// The fact-class taxonomy (EC-1 §3.2), hard-coded like `SANCTIONED_SURFACES`. A token in
/// `dispatch_fact_classes.txt` outside this set (a typo, or an un-ratified class) goes RED;
/// admitting a genuinely new class is a reviewed change to BOTH this constant and §3.2. This
/// is SYNTAX enforcement only — that a declared class is CORRECT for its arm stays the audit's
/// job (§2.2). See `validate_fact_classes`.
const KNOWN_FACT_CLASSES: &[&str] = &[
    "FC0", "FC1", "FC2a", "FC2a-agg", "FC2b", "FC3", "FC4", "FC5", "FC6", "FC7", "FC8",
];

/// The manifest's "touches no fact class" marker (a pure stub, e.g. `ping`). Permitted only as
/// the SOLE token on a line — mixing it with a real class is contradictory and goes RED.
const NONE_FACT_CLASS: &str = "none";

// ─────────────────────────────── paths ───────────────────────────────

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn src_dir() -> PathBuf {
    crate_dir().join("src")
}

fn read_witness_manifest(name: &str) -> String {
    let path = crate_dir().join("witness").join(name);
    fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read witness manifest {}: {e}", path.display()))
}

fn sanctioned_surfaces() -> BTreeSet<String> {
    SANCTIONED_SURFACES.iter().map(|s| s.to_string()).collect()
}

// ───────────────────── field-read detection (pure) ─────────────────────

/// True iff `content` accesses the `RepoState.livegraph` field: a `.livegraph` token
/// whose following byte is not `[A-Za-z0-9_]` (or is end-of-input). See the module doc's
/// FIELD-READ PRECISION note for what this includes and excludes.
fn reads_livegraph_field(content: &str) -> bool {
    const NEEDLE: &str = ".livegraph";
    let bytes = content.as_bytes();
    let mut search_from = 0usize;
    while let Some(rel) = content[search_from..].find(NEEDLE) {
        let start = search_from + rel;
        let after = start + NEEDLE.len();
        match bytes.get(after) {
            None => return true, // `.livegraph` at end of input
            Some(&b) => {
                let is_ident = b.is_ascii_alphanumeric() || b == b'_';
                if !is_ident {
                    return true;
                }
            }
        }
        search_from = after;
    }
    false
}

// ──────────────────────── source-tree scanning ────────────────────────

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display())) {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

/// Path relative to `base`, `/`-normalized for deterministic, platform-stable keys.
fn rel_key(path: &Path, base: &Path) -> String {
    path.strip_prefix(base)
        .expect("path under base")
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// The set of `src/`-relative files that read the LiveGraph field (§7.3 method).
fn computed_field_readers() -> BTreeSet<String> {
    let base = src_dir();
    let mut files = Vec::new();
    collect_rs_files(&base, &mut files);
    let mut readers = BTreeSet::new();
    for f in files {
        let content =
            fs::read_to_string(&f).unwrap_or_else(|e| panic!("read {}: {e}", f.display()));
        if reads_livegraph_field(&content) {
            readers.insert(rel_key(&f, &base));
        }
    }
    readers
}

// ─────────────────────── reader-set manifest parse ───────────────────────

struct ReaderManifest {
    /// production reader file -> the sanctioned surfaces it serves
    prod: BTreeMap<String, BTreeSet<String>>,
    /// cfg(test) files that also touch the field (acknowledged scaffolding)
    test: BTreeSet<String>,
}

impl ReaderManifest {
    /// Every file the manifest accounts for (production ∪ test-scaffolding).
    fn listed(&self) -> BTreeSet<String> {
        let mut all: BTreeSet<String> = self.prod.keys().cloned().collect();
        all.extend(self.test.iter().cloned());
        all
    }
}

fn parse_reader_manifest(text: &str) -> ReaderManifest {
    let mut prod: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut test: BTreeSet<String> = BTreeSet::new();
    let mut section = "";
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = match line {
                "[production]" => "prod",
                "[test-scaffolding]" => "test",
                other => panic!("unknown section {other} in livegraph_reader_set.txt"),
            };
            continue;
        }
        match section {
            "prod" => {
                let (path, surfaces) = line
                    .split_once('=')
                    .unwrap_or_else(|| panic!("[production] line needs `=`: {line}"));
                let surfs: BTreeSet<String> = surfaces
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                assert!(
                    !surfs.is_empty(),
                    "[production] line has no surfaces: {line}"
                );
                let prev = prod.insert(path.trim().to_string(), surfs);
                assert!(
                    prev.is_none(),
                    "duplicate [production] entry: {}",
                    path.trim()
                );
            }
            "test" => {
                let inserted = test.insert(line.to_string());
                assert!(inserted, "duplicate [test-scaffolding] entry: {line}");
            }
            "" => panic!("manifest line before any section header: {line}"),
            _ => unreachable!(),
        }
    }
    ReaderManifest { prod, test }
}

// ───────────── module-gating resolution (test-scaffolding cfg(test) guard, §2.1) ─────────────
//
// review-0 item 1: the reader-set completeness check compares the FILE SET, so removing a parent
// `#[cfg(test)]` from an already-listed test module promotes it to a production field-reader
// WITHOUT changing the set — the gate would stay green. To close that, the witness resolves the
// crate's file-module inclusion tree and asserts every [test-scaffolding] file is compiled ONLY
// under `#[cfg(test)]` (some `mod` edge on its path to the crate root is cfg(test)-gated). This is
// a NARROW resolver (mod-decl edges + `#[path]` + `#[cfg(test)]`), not a full module system — it
// fails CLOSED (a file it cannot prove test-only goes RED), so it never mints a false green.

/// One `mod <name>;` inclusion edge: the file it pulls in (the map KEY) is compiled by the module
/// in `parent`, and `cfg_test` is true iff that declaration carries `#[cfg(test)]`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ModEdge {
    parent: String,
    cfg_test: bool,
}

/// A parsed `mod <name>;` FILE-module declaration (not an inline `mod name { … }`), plus the
/// attributes bound to it: whether it is `#[cfg(test)]`-gated and any `#[path = "…"]` override.
struct ModDecl {
    name: String,
    cfg_test: bool,
    path_override: Option<String>,
}

/// Parse the `mod <name>;` declarations in one source file, folding in the contiguous `#[…]`
/// attribute run bound to each (Rust binds an outer attribute to the item it precedes, across only
/// whitespace). Inline `mod name { … }` blocks are skipped — they cannot relocate a field-reading
/// file. Only `#[cfg(test)]` is treated as gating (strict, fail-closed): a `#[cfg(all(test,…))]` or
/// other variant is NOT recognized, so it would go RED and force a reviewed update, never a silent
/// pass.
fn parse_mod_decls(content: &str) -> Vec<ModDecl> {
    let lines: Vec<&str> = content.lines().collect();
    let mut decls = Vec::new();
    for (i, raw) in lines.iter().enumerate() {
        // Only TOP-LEVEL (column-0) `mod <name>;` declarations pull in a sibling FILE; a decl
        // nested inside an inline `mod { … }` is indented and cannot relocate a field-reading file.
        if raw.starts_with([' ', '\t']) {
            continue;
        }
        let Some(name) = mod_decl_name(raw) else {
            continue;
        };
        let (mut cfg_test, mut path_override) = (false, None);
        let mut j = i;
        while j > 0 {
            let prev = lines[j - 1].trim();
            if prev.is_empty() {
                j -= 1; // whitespace between an attribute and its item is legal — keep scanning up
                continue;
            }
            if !prev.starts_with("#[") {
                break; // hit real code (or a comment) — the attribute run ends here
            }
            if prev == "#[cfg(test)]" {
                cfg_test = true;
            } else if let Some(p) = parse_path_attr(prev) {
                path_override = Some(p);
            }
            j -= 1;
        }
        decls.push(ModDecl {
            name,
            cfg_test,
            path_override,
        });
    }
    decls
}

/// If `line` is a file-module declaration `mod <name>;` (optionally `pub` / `pub(crate)` …), return
/// `<name>`. The trailing `;` is required — an inline `mod name {` (no `;`) returns None.
fn mod_decl_name(line: &str) -> Option<String> {
    let t = line.trim();
    let t = t.strip_prefix("pub").map(str::trim_start).unwrap_or(t);
    let t = t
        .strip_prefix('(')
        .and_then(|r| r.split_once(')'))
        .map(|(_, r)| r.trim_start())
        .unwrap_or(t);
    let name = t.strip_prefix("mod ")?.strip_suffix(';')?.trim();
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return None;
    }
    Some(name.to_string())
}

/// Extract the string from a `#[path = "…"]` attribute line, if that is what `line` is.
fn parse_path_attr(line: &str) -> Option<String> {
    let inner = line.trim().strip_prefix("#[")?.strip_suffix(']')?.trim();
    let val = inner
        .strip_prefix("path")?
        .trim_start()
        .strip_prefix('=')?
        .trim()
        .strip_prefix('"')?;
    val.find('"').map(|end| val[..end].to_string())
}

/// Resolve the `src/`-relative target FILE a `mod` declaration in `declaring` pulls in, choosing the
/// candidate that actually EXISTS in `files` (so we need not perfectly re-derive Rust's
/// mod.rs-vs-`foo.rs` rule — existence disambiguates). `#[path]` wins; otherwise the submodule dir
/// is the declaring file's directory for a root-like file (`mod.rs`/`lib.rs`/`main.rs`) or its
/// `foo/` subdirectory for a plain `foo.rs`. None if no candidate exists.
fn resolve_mod_target(declaring: &str, decl: &ModDecl, files: &BTreeSet<String>) -> Option<String> {
    let dir = parent_dir(declaring);
    let file_name = declaring.rsplit('/').next().unwrap_or(declaring);
    let submodule_dir = if matches!(file_name, "mod.rs" | "lib.rs" | "main.rs") {
        dir.clone()
    } else {
        join_rel(&dir, &file_stem(declaring))
    };
    let mut candidates = Vec::new();
    if let Some(p) = &decl.path_override {
        candidates.push(join_rel(&dir, p));
    }
    candidates.push(join_rel(&submodule_dir, &format!("{}.rs", decl.name)));
    candidates.push(join_rel(&submodule_dir, &format!("{}/mod.rs", decl.name)));
    candidates.into_iter().find(|c| files.contains(c))
}

/// `/`-relative parent directory of a `/`-joined relpath (`""` for a top-level file).
fn parent_dir(rel: &str) -> String {
    match rel.rfind('/') {
        Some(i) => rel[..i].to_string(),
        None => String::new(),
    }
}

/// File stem (final path component without its `.rs`/extension) of a `/`-joined relpath.
fn file_stem(rel: &str) -> String {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    match name.rfind('.') {
        Some(i) => name[..i].to_string(),
        None => name.to_string(),
    }
}

/// Join `rel` onto directory `dir`, lexically resolving `.`/`..`/empty segments — a pure path join
/// in `/`-relpath space (no filesystem access, platform-stable keys).
fn join_rel(dir: &str, rel: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for seg in dir.split('/').chain(rel.split('/')) {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

/// The file-module inclusion tree of `src/`: for each `src/`-relative file, the edge that compiles
/// it (its parent module + whether that edge is `#[cfg(test)]`-gated). The crate roots
/// (`lib.rs`/`main.rs`) have no incoming edge and are simply absent from the map.
fn collect_mod_edges() -> BTreeMap<String, ModEdge> {
    let base = src_dir();
    let mut paths = Vec::new();
    collect_rs_files(&base, &mut paths);
    let files: BTreeSet<String> = paths.iter().map(|p| rel_key(p, &base)).collect();
    let mut edges: BTreeMap<String, ModEdge> = BTreeMap::new();
    for p in &paths {
        let declaring = rel_key(p, &base);
        let content = fs::read_to_string(p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()));
        for decl in parse_mod_decls(&content) {
            if let Some(target) = resolve_mod_target(&declaring, &decl, &files) {
                let edge = ModEdge {
                    parent: declaring.clone(),
                    cfg_test: decl.cfg_test,
                };
                if let Some(prev) = edges.insert(target.clone(), edge) {
                    // One file declared as a module from two parents is a shape this witness does
                    // not model — fail LOUD rather than silently pick one (never a silent green).
                    assert_eq!(
                        prev.parent, declaring,
                        "file {target} is declared as a module from two parents \
                         ({} and {declaring}) — the witness's mod resolver needs updating",
                        prev.parent
                    );
                }
            }
        }
    }
    edges
}

/// True iff `file` is compiled ONLY under `#[cfg(test)]`: some edge on its `mod` path to the crate
/// root is `cfg_test`. A chain that reaches a root with no cfg(test) edge means the file compiles in
/// the normal build (a production reader). Unknown target / cycle → false (fail closed).
fn is_test_reachable_only(file: &str, edges: &BTreeMap<String, ModEdge>) -> bool {
    let mut cur = file.to_string();
    let mut seen = BTreeSet::new();
    loop {
        if !seen.insert(cur.clone()) {
            return false; // cycle — cannot prove test-only
        }
        match edges.get(&cur) {
            Some(e) if e.cfg_test => return true,
            Some(e) => cur = e.parent.clone(),
            None => return false, // reached a crate root with no cfg(test) edge on the path
        }
    }
}

/// Every listed [test-scaffolding] file must be `#[cfg(test)]`-gated (directly or via an ancestor).
/// One that is not is really a PRODUCTION field-reader hiding in the test section — review-0 item 1.
fn check_test_gating(
    test_files: &BTreeSet<String>,
    edges: &BTreeMap<String, ModEdge>,
) -> Vec<Violation> {
    let mut v: Vec<Violation> = test_files
        .iter()
        .filter(|f| !is_test_reachable_only(f, edges))
        .map(|f| Violation::TestReaderNotGated((*f).clone()))
        .collect();
    v.sort();
    v
}

// ─────────────────────── dispatch-arm extraction ───────────────────────

/// Extract the method-name arms of the ONE dispatch match
/// (`dispatch.rs`: `match request.method.as_str() {`). Bounds the scan to that match by
/// anchoring on the match head and stopping at its `_ =>` catch-all (at the arm indent),
/// so unrelated `match … .as_str()` blocks elsewhere in the file are never read.
fn extract_dispatch_arms(dispatch_src: &str) -> BTreeSet<String> {
    const ANCHOR: &str = "match request.method.as_str() {";
    let start = dispatch_src
        .find(ANCHOR)
        .expect("dispatch match anchor not found — dispatcher shape changed; update the witness");
    let region = &dispatch_src[start..];

    let mut arms = BTreeSet::new();
    let mut arm_indent: Option<usize> = None;
    for raw in region.lines().skip(1) {
        let trimmed = raw.trim_start();
        let indent = raw.len() - trimmed.len();
        // The match's own catch-all (at arm indent) terminates the arm list.
        if trimmed.starts_with("_ =>") && arm_indent == Some(indent) {
            break;
        }
        if let Some(name) = parse_arm_name(trimmed) {
            match arm_indent {
                None => arm_indent = Some(indent),
                Some(ai) if indent != ai => continue, // ignore deeper-nested `"x" =>`
                _ => {}
            }
            let inserted = arms.insert(name.clone());
            assert!(inserted, "duplicate dispatch arm in match: {name}");
        }
    }
    arms
}

/// If `trimmed` begins a match arm `"<name>" =>` with a lowercase/underscore name,
/// return the name.
fn parse_arm_name(trimmed: &str) -> Option<String> {
    let rest = trimmed.strip_prefix('"')?;
    let end = rest.find('"')?;
    let name = &rest[..end];
    let after = rest[end + 1..].trim_start();
    if !after.starts_with("=>") {
        return None;
    }
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_lowercase() || b == b'_') {
        return None;
    }
    Some(name.to_string())
}

/// Parse the fact-class manifest into ordered `(arm, [class,…])` entries — the trimmed non-empty
/// tokens right of `=`. Rejects a missing `=` or a duplicate arm (structural). Token VALIDATION is
/// `validate_fact_classes` (kept separate so a fixture exercises it on synthetic entries without a
/// manifest file), and the arm-NAME set for dispatch reconciliation is `declared_arm_set`.
fn parse_fact_class_manifest(text: &str) -> Vec<(String, Vec<String>)> {
    let mut entries = Vec::new();
    let mut seen = BTreeSet::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (arm, fcs) = line
            .split_once('=')
            .unwrap_or_else(|| panic!("fact-class line needs `=`: {line}"));
        let arm = arm.trim().to_string();
        assert!(
            seen.insert(arm.clone()),
            "duplicate arm in dispatch_fact_classes.txt: {arm}"
        );
        let classes = fcs
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        entries.push((arm, classes));
    }
    entries
}

/// The arm-NAME set of the fact-class manifest (for reconciliation against the live dispatch arms).
fn declared_arm_set(entries: &[(String, Vec<String>)]) -> BTreeSet<String> {
    entries.iter().map(|(arm, _)| arm.clone()).collect()
}

/// SYNTACTIC validation of each arm's declared classes (review-0 item 3): at least one class, every
/// token a known §3.2 taxonomy name (`KNOWN_FACT_CLASSES`), and `none` only as the sole token. This
/// does NOT judge WHICH classes are correct for an arm — that stays the audit's job (§2.2); it only
/// stops an empty or typo'd declaration from passing silently.
fn validate_fact_classes(entries: &[(String, Vec<String>)]) -> Vec<Violation> {
    let known: BTreeSet<&str> = KNOWN_FACT_CLASSES.iter().copied().collect();
    let mut v = Vec::new();
    for (arm, classes) in entries {
        if classes.is_empty() {
            v.push(Violation::EmptyFactClasses(arm.clone()));
            continue;
        }
        if classes.iter().any(|c| c == NONE_FACT_CLASS) && classes.len() > 1 {
            v.push(Violation::NoneNotExclusive(arm.clone()));
        }
        for c in classes {
            if c != NONE_FACT_CLASS && !known.contains(c.as_str()) {
                v.push(Violation::UnknownFactClass {
                    arm: arm.clone(),
                    token: c.clone(),
                });
            }
        }
    }
    v.sort();
    v
}

// ───────────────────── reconciliation (pure — fixture-tested) ─────────────────────

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Violation {
    /// A file reads the LiveGraph field but is not accounted for by the manifest.
    UnsanctionedReader(String),
    /// The manifest lists a file that no longer reads the field (stale).
    StaleReaderEntry(String),
    /// A production reader declares a surface outside the sanctioned 12.
    UnsanctionedSurface { file: String, surface: String },
    /// The union of production surfaces ≠ the sanctioned 12 exactly.
    SurfaceSetMismatch { detail: String },
    /// A live dispatch arm is not declared in the fact-class manifest.
    MissingArm(String),
    /// The fact-class manifest declares an arm that no longer exists (stale).
    StaleArmEntry(String),
    /// A [test-scaffolding] file is reachable OUTSIDE `#[cfg(test)]` — its gating was removed, so it
    /// is really a production field-reader hiding in the test section (review-0 item 1).
    TestReaderNotGated(String),
    /// A fact-class manifest line declares no classes at all (review-0 item 3).
    EmptyFactClasses(String),
    /// A fact-class manifest line names a token outside the §3.2 taxonomy (review-0 item 3).
    UnknownFactClass { arm: String, token: String },
    /// `none` (no fact class) is mixed with a real class on one line — contradictory (review-0 item 3).
    NoneNotExclusive(String),
}

fn reconcile_readers(computed: &BTreeSet<String>, manifest: &ReaderManifest) -> Vec<Violation> {
    let listed = manifest.listed();
    let mut v = Vec::new();
    for f in computed.difference(&listed) {
        v.push(Violation::UnsanctionedReader(f.clone()));
    }
    for f in listed.difference(computed) {
        v.push(Violation::StaleReaderEntry(f.clone()));
    }
    v.sort();
    v
}

fn check_surfaces(manifest: &ReaderManifest, sanctioned: &BTreeSet<String>) -> Vec<Violation> {
    let mut v = Vec::new();
    let mut union = BTreeSet::new();
    for (file, surfs) in &manifest.prod {
        for s in surfs {
            if !sanctioned.contains(s) {
                v.push(Violation::UnsanctionedSurface {
                    file: file.clone(),
                    surface: s.clone(),
                });
            }
            union.insert(s.clone());
        }
    }
    if &union != sanctioned {
        let missing: Vec<_> = sanctioned.difference(&union).cloned().collect();
        let extra: Vec<_> = union.difference(sanctioned).cloned().collect();
        v.push(Violation::SurfaceSetMismatch {
            detail: format!("missing={missing:?} extra={extra:?}"),
        });
    }
    v.sort();
    v
}

fn reconcile_arms(dispatch: &BTreeSet<String>, declared: &BTreeSet<String>) -> Vec<Violation> {
    let mut v = Vec::new();
    for a in dispatch.difference(declared) {
        v.push(Violation::MissingArm(a.clone()));
    }
    for a in declared.difference(dispatch) {
        v.push(Violation::StaleArmEntry(a.clone()));
    }
    v.sort();
    v
}

// ─────────────────────────── THE GATE (PASS on HEAD) ───────────────────────────

#[test]
fn reader_set_matches_sanctioned_list_on_head() {
    let computed = computed_field_readers();
    let manifest = parse_reader_manifest(&read_witness_manifest("livegraph_reader_set.txt"));

    let drift = reconcile_readers(&computed, &manifest);
    assert!(
        drift.is_empty(),
        "LiveGraph-field reader drift vs sanctioned manifest \
         (STOP CONDITION — surface as a FINDING, do not silently edit):\n{drift:#?}\n\
         computed readers = {computed:#?}"
    );

    let surface_violations = check_surfaces(&manifest, &sanctioned_surfaces());
    assert!(
        surface_violations.is_empty(),
        "production reader surfaces violate the sanctioned 12 (§3.3-A + writers):\n{surface_violations:#?}"
    );
}

#[test]
fn every_dispatch_arm_is_declared_in_manifest() {
    let dispatch_src = fs::read_to_string(src_dir().join("dispatch.rs")).expect("read dispatch.rs");
    let arms = extract_dispatch_arms(&dispatch_src);
    assert!(
        !arms.is_empty(),
        "no dispatch arms extracted — the match anchor/shape changed; update the witness"
    );

    let entries = parse_fact_class_manifest(&read_witness_manifest("dispatch_fact_classes.txt"));
    let declared = declared_arm_set(&entries);
    let drift = reconcile_arms(&arms, &declared);
    assert!(
        drift.is_empty(),
        "dispatch-arm <-> fact-class-manifest drift (a new arm must declare its fact \
         classes; a removed arm must be dropped from the manifest):\n{drift:#?}\n\
         live arm count = {}",
        arms.len()
    );
}

#[test]
fn fact_class_declarations_are_valid() {
    let entries = parse_fact_class_manifest(&read_witness_manifest("dispatch_fact_classes.txt"));
    let v = validate_fact_classes(&entries);
    assert!(
        v.is_empty(),
        "fact-class manifest has malformed declarations — an empty line, a token outside the §3.2 \
         taxonomy, or `none` mixed with a real class (this is SYNTAX only; whether a valid class is \
         the RIGHT one stays the audit's job):\n{v:#?}"
    );
}

#[test]
fn test_scaffolding_readers_are_cfg_test_gated_on_head() {
    let manifest = parse_reader_manifest(&read_witness_manifest("livegraph_reader_set.txt"));
    let edges = collect_mod_edges();
    let v = check_test_gating(&manifest.test, &edges);
    assert!(
        v.is_empty(),
        "a [test-scaffolding] reader is reachable OUTSIDE #[cfg(test)] — i.e. a production \
         field-reader hiding in the test section (a removed parent #[cfg(test)]; surface as a \
         FINDING, do not silently edit):\n{v:#?}"
    );
}

// ──────────────────── DELIBERATE-VIOLATION FIXTURES (must FAIL on drift) ────────────────────

#[test]
fn fixture_field_read_detector_matches_real_shapes_and_ignores_lookalikes() {
    // Real access shapes (production reads, by-ref passes, test writes) — all TRUE:
    assert!(reads_livegraph_field(
        "let guard = repo_state.livegraph.read();"
    ));
    assert!(reads_livegraph_field(
        "let mut g = repo_state.livegraph.write();"
    ));
    assert!(reads_livegraph_field(
        "*state.livegraph.write() = Some(lg);"
    ));
    assert!(reads_livegraph_field(
        "OrientServeDecorator::new(&repo_state.livegraph, &storage, &epoch)"
    ));
    assert!(reads_livegraph_field("let g = self.livegraph.read();"));
    // Look-alikes that must NOT count as field reads — all FALSE:
    assert!(!reads_livegraph_field(
        "pub livegraph: parking_lot::RwLock<Option<LiveGraph>>,"
    ));
    assert!(!reads_livegraph_field(
        "livegraph: parking_lot::RwLock::new(None),"
    ));
    assert!(!reads_livegraph_field(
        "use repo_graph_livegraph::LiveGraph;"
    ));
    assert!(!reads_livegraph_field(
        "crate::livegraph_feed::feed_partition(state);"
    ));
    assert!(!reads_livegraph_field("out.livegraph_count += 1;"));
    assert!(!reads_livegraph_field(
        "assert_eq!(d.livegraph_only, vec![k]);"
    ));
}

#[test]
fn fixture_unsanctioned_reader_is_detected() {
    let manifest = ReaderManifest {
        prod: BTreeMap::new(),
        test: BTreeSet::new(),
    };
    let computed = ["rogue_serve.rs".to_string()].into_iter().collect();
    let v = reconcile_readers(&computed, &manifest);
    assert!(
        v.contains(&Violation::UnsanctionedReader("rogue_serve.rs".to_string())),
        "a new field-reading module must go red; got {v:#?}"
    );
}

#[test]
fn fixture_stale_reader_entry_is_detected() {
    let mut prod = BTreeMap::new();
    prod.insert(
        "ghost.rs".to_string(),
        ["orient".to_string()].into_iter().collect(),
    );
    let manifest = ReaderManifest {
        prod,
        test: BTreeSet::new(),
    };
    let computed = BTreeSet::new(); // ghost.rs no longer reads the field
    let v = reconcile_readers(&computed, &manifest);
    assert!(
        v.contains(&Violation::StaleReaderEntry("ghost.rs".to_string())),
        "a manifest entry that no longer reads the field must go red; got {v:#?}"
    );
}

#[test]
fn fixture_non_sanctioned_surface_is_detected() {
    let mut prod = BTreeMap::new();
    prod.insert(
        "rogue_serve.rs".to_string(),
        ["rogue_handler".to_string()].into_iter().collect(),
    );
    let manifest = ReaderManifest {
        prod,
        test: BTreeSet::new(),
    };
    let v = check_surfaces(&manifest, &sanctioned_surfaces());
    assert!(
        v.iter().any(|x| matches!(x, Violation::UnsanctionedSurface { surface, .. } if surface == "rogue_handler")),
        "a production reader serving a non-sanctioned surface must go red; got {v:#?}"
    );
}

#[test]
fn fixture_undeclared_dispatch_arm_is_detected() {
    let dispatch = ["callers", "new_secret_arm"]
        .into_iter()
        .map(String::from)
        .collect();
    let declared = ["callers"].into_iter().map(String::from).collect();
    let v = reconcile_arms(&dispatch, &declared);
    assert!(
        v.contains(&Violation::MissingArm("new_secret_arm".to_string())),
        "an undeclared dispatch arm must go red; got {v:#?}"
    );
}

#[test]
fn fixture_stale_arm_entry_is_detected() {
    let dispatch = ["callers"].into_iter().map(String::from).collect();
    let declared = ["callers", "removed_arm"]
        .into_iter()
        .map(String::from)
        .collect();
    let v = reconcile_arms(&dispatch, &declared);
    assert!(
        v.contains(&Violation::StaleArmEntry("removed_arm".to_string())),
        "a stale fact-class-manifest arm must go red; got {v:#?}"
    );
}

#[test]
fn fixture_arm_extraction_bounds_to_the_dispatch_match() {
    // A miniature of the real match: header, a comment, two arms, a block arm with a
    // nested `"x" =>` at deeper indent (must be ignored), the catch-all, then an
    // UNRELATED later match whose arms must NOT be picked up.
    let src = r#"
        let result = match request.method.as_str() {
            // comment line
            "callers" => self.handle_callers(request),
            "classify_retention" => {
                let x = match y { "nested" => 1, _ => 0 };
                self.handle_classify_retention(request)
            }
            _ => DispatchResult::unknown_method(&request.id, &request.method),
        };
        let other = match ext.as_str() { "typescript" => 1, _ => 0 };
    "#;
    let arms = extract_dispatch_arms(src);
    let expect: BTreeSet<String> = ["callers", "classify_retention"]
        .into_iter()
        .map(String::from)
        .collect();
    assert_eq!(
        arms, expect,
        "arm extraction must bound to the dispatch match only"
    );
}

#[test]
fn fixture_mod_decl_and_attr_parse_real_shapes() {
    // The real declaration shapes in daemon-runtime/src the resolver must handle.
    assert_eq!(mod_decl_name("mod tests;").as_deref(), Some("tests"));
    assert_eq!(
        mod_decl_name("pub mod explain_serve_tests;").as_deref(),
        Some("explain_serve_tests")
    );
    assert_eq!(
        mod_decl_name("pub(crate) mod test_fixture;").as_deref(),
        Some("test_fixture")
    );
    assert_eq!(mod_decl_name("mod tests {").as_deref(), None); // inline module, not a file module
    assert_eq!(mod_decl_name("// mod fake;").as_deref(), None);
    assert_eq!(
        parse_path_attr(r#"#[path = "explain_coherence_tests.rs"]"#).as_deref(),
        Some("explain_coherence_tests.rs")
    );
    // A contiguous `#[cfg(test)]` + `#[path]` run binds both to the following `mod` (the
    // explain_coherence.rs shape); the module name and the file override are captured separately.
    let decls = parse_mod_decls("#[cfg(test)]\n#[path = \"t.rs\"]\nmod served;\n");
    assert_eq!(decls.len(), 1);
    assert!(decls[0].cfg_test);
    assert_eq!(decls[0].path_override.as_deref(), Some("t.rs"));
    assert_eq!(decls[0].name, "served");
    // No cfg(test) in the run → not gated (fail-closed).
    let plain = parse_mod_decls("mod loose;\n");
    assert!(!plain[0].cfg_test);
}

#[test]
fn fixture_join_rel_resolves_path_segments() {
    // #[path] relative to the declaring file's directory (the explain_coherence.rs case: dir="").
    assert_eq!(
        join_rel("", "explain_coherence_tests.rs"),
        "explain_coherence_tests.rs"
    );
    // Submodule dir of a plain `foo.rs` is `foo/` (the orient_lg_decisions.rs case).
    assert_eq!(
        join_rel("orient_lg_decisions", "served_e2e.rs"),
        "orient_lg_decisions/served_e2e.rs"
    );
    assert_eq!(
        join_rel("callgraph_cert", "tests.rs"),
        "callgraph_cert/tests.rs"
    );
    // `.`/`..`/empty segments normalize away.
    assert_eq!(join_rel("a/b", "../c.rs"), "a/c.rs");
    assert_eq!(join_rel("a", "./x.rs"), "a/x.rs");
}

#[test]
fn fixture_ungated_test_reader_is_detected() {
    // Direct gating (PASS): the file's own `mod` edge carries #[cfg(test)].
    let mut gated = BTreeMap::new();
    gated.insert(
        "child.rs".to_string(),
        ModEdge {
            parent: "parent.rs".to_string(),
            cfg_test: true,
        },
    );
    assert!(is_test_reachable_only("child.rs", &gated));

    // Transitive gating (PASS): the file's own edge is NOT cfg(test), but an ancestor's is —
    // mirrors explain_serve_tests/fanin_fixture.rs, gated by `#[cfg(test)] mod explain_serve_tests;`.
    let mut transitive = BTreeMap::new();
    transitive.insert(
        "leaf.rs".to_string(),
        ModEdge {
            parent: "mid.rs".to_string(),
            cfg_test: false,
        },
    );
    transitive.insert(
        "mid.rs".to_string(),
        ModEdge {
            parent: "lib.rs".to_string(),
            cfg_test: true,
        },
    );
    assert!(is_test_reachable_only("leaf.rs", &transitive));

    // DRIFT (must FAIL → be detected): the parent #[cfg(test)] was removed, so the whole chain
    // reaches the crate root with NO cfg(test) edge — the file is now a production reader. This is
    // review-0 item 1: no file left/entered the set, yet a green gate would admit a production read.
    let mut drifted = BTreeMap::new();
    drifted.insert(
        "leaf.rs".to_string(),
        ModEdge {
            parent: "mid.rs".to_string(),
            cfg_test: false,
        },
    );
    drifted.insert(
        "mid.rs".to_string(),
        ModEdge {
            parent: "lib.rs".to_string(),
            cfg_test: false,
        },
    );
    assert!(!is_test_reachable_only("leaf.rs", &drifted));
    let test_files = ["leaf.rs".to_string()].into_iter().collect();
    let v = check_test_gating(&test_files, &drifted);
    assert!(
        v.contains(&Violation::TestReaderNotGated("leaf.rs".to_string())),
        "an un-gated [test-scaffolding] reader (removed parent #[cfg(test)]) must go red; got {v:#?}"
    );
}

#[test]
fn fixture_malformed_fact_classes_are_detected() {
    let entries = vec![
        ("empty".to_string(), vec![]),
        (
            "unknown".to_string(),
            vec!["FC1".to_string(), "FC99".to_string()],
        ),
        (
            "none_mixed".to_string(),
            vec!["none".to_string(), "FC1".to_string()],
        ),
        (
            "ok".to_string(),
            vec!["FC1".to_string(), "FC2a".to_string()],
        ),
        ("pure_stub".to_string(), vec!["none".to_string()]),
    ];
    let v = validate_fact_classes(&entries);
    assert!(v.contains(&Violation::EmptyFactClasses("empty".to_string())));
    assert!(v.contains(&Violation::UnknownFactClass {
        arm: "unknown".to_string(),
        token: "FC99".to_string(),
    }));
    assert!(v.contains(&Violation::NoneNotExclusive("none_mixed".to_string())));
    // A well-formed line and a pure `none` stub raise NOTHING.
    let flagged: BTreeSet<&str> = v
        .iter()
        .map(|x| match x {
            Violation::EmptyFactClasses(a) => a.as_str(),
            Violation::UnknownFactClass { arm, .. } => arm.as_str(),
            Violation::NoneNotExclusive(a) => a.as_str(),
            _ => "",
        })
        .collect();
    assert!(!flagged.contains("ok"));
    assert!(!flagged.contains("pure_stub"));
}
