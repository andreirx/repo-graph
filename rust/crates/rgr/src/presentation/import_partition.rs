//! TEST-EDGE-SCOPE-1B (RG-REQ-004-L12, RG-REQ-002-L11; D-TESB-07) — the ONE home of the
//! import-partition wording: the remainder lines (flag form and command form), the excluded-cycle
//! rows and their elision line, the orient/explain clause, the partition-unavailable line, the
//! governance not-judged lines and `path`'s inferred-route lines. No other renderer types these
//! sentences.
//!
//! Honesty: every count comes from the daemon's payload. A payload WITHOUT `import_view` came from
//! a daemon that predates the partition — its counts may include imports from test files and
//! inferred imports — and renders the named unavailable line, never a zero remainder
//! (RG-REQ-002-L04). A present-but-malformed partition key renders "unreadable", never a default.

use serde_json::Value;

/// The line a partitioned surface prints for a payload from a daemon that predates the partition.
pub(crate) const PARTITION_UNAVAILABLE: &str = "import partition unavailable from this daemon (it \
     predates the partition) — these counts may include imports from test files and inferred imports";

/// The clause a one-line headline (orient's module-edges line) carries for a payload from a daemon
/// that predates the partition.
pub(crate) const PARTITION_UNAVAILABLE_CLAUSE: &str =
    "import partition unavailable from this daemon — these counts may include imports from test files and inferred imports";

/// The line for a present-but-malformed partition key.
pub(crate) const PARTITION_UNREADABLE: &str =
    "import partition unreadable on this response — the counts above may include imports from test \
     files and inferred imports";

/// The line an explicit `--engine livegraph|compare` answer carries: those engines serve the
/// unpartitioned import graph (D-TESB-06), so their counts include imports from test files and
/// inferred imports.
pub(crate) const EXPLICIT_ENGINE_UNPARTITIONED: &str =
    "this engine does not partition imports by test status or resolution — rmap cycles does";

/// At most this many excluded cycles are listed on `cycles` (RG-REQ-012-L04 budget).
const EXCLUDED_CYCLE_ROWS: usize = 5;
/// At most this many members are named per excluded cycle.
const EXCLUDED_CYCLE_MEMBERS: usize = 8;

fn plural(n: u64) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

/// One remainder group: imports excluded, and relations only through them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Group {
    pub imports: u64,
    pub edges: u64,
}

/// The `import_remainder` of a response.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Remainder {
    pub tests: Group,
    pub inferred: Group,
    pub tests_and_inferred: Group,
}

/// The clause form of [`PARTITION_UNREADABLE`] for a one-line headline (orient's cycle line).
pub(crate) const PARTITION_UNREADABLE_CLAUSE: &str =
    "import partition unreadable on this response — these counts may include imports from test \
     files and inferred imports";

/// What a response says about its partition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Partition {
    /// No `import_view`: the daemon predates the partition.
    Unavailable,
    /// `import_view` is present but a partition key the surface requires is missing or malformed
    /// — never read as "nothing excluded" (D-TESB-17 row U9).
    Unreadable,
    /// The remainder; every other key the surface requires is present and well formed.
    Stated(Remainder),
}

/// The partition keys a surface requires beside `import_view` and `import_remainder`
/// (D-TESB-17 row U9): `excluded_cycles` on `cycles`, orient's cycle line and explain's
/// Import-cycles block; `importer_test_status_undetermined` on the five L12 surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Requires {
    pub excluded_cycles: bool,
    pub importer_block: bool,
}

impl Requires {
    /// `imports <file>`: the view and its remainder only.
    pub(crate) const REMAINDER_ONLY: Requires = Requires {
        excluded_cycles: false,
        importer_block: false,
    };
    /// `modules list`, `modules deps`, orient's module-edges line.
    pub(crate) const MODULE_EDGES: Requires = Requires {
        excluded_cycles: false,
        importer_block: true,
    };
    /// `cycles`, orient's cycle line, explain's Import-cycles block.
    pub(crate) const CYCLES: Requires = Requires {
        excluded_cycles: true,
        importer_block: true,
    };
}

/// The ONE decode rule of a partitioned surface (D-TESB-17 rows U9, W1–W3). No `import_view`:
/// [`Partition::Unavailable`] (a daemon that predates the partition). `import_view` present: the
/// view must be an object, `import_remainder` present and well formed, and every key `req`
/// names present and well formed — else [`Partition::Unreadable`]; a partial or malformed payload
/// is never rendered as "nothing excluded" and never as a zero.
pub(crate) fn surface_partition(
    view: Option<&Value>,
    remainder: Option<&Value>,
    excluded_cycles: Option<&Value>,
    importer_block: Option<&Value>,
    req: Requires,
) -> Partition {
    let Some(view) = view else {
        return Partition::Unavailable;
    };
    if view_of(view).is_err() {
        return Partition::Unreadable;
    }
    let Some(Ok(rem)) = remainder.map(remainder_of) else {
        return Partition::Unreadable;
    };
    if req.excluded_cycles
        && excluded_cycles
            .map(parse_excluded_cycles)
            .and_then(Result::ok)
            .is_none()
    {
        return Partition::Unreadable;
    }
    if req.importer_block
        && !importer_block.is_some_and(crate::presentation::test_status::block_is_readable)
    {
        return Partition::Unreadable;
    }
    Partition::Stated(rem)
}

/// `import_view`: `{"include_tests": bool, "include_inferred": bool}`, else `Err`.
fn view_of(v: &Value) -> Result<(bool, bool), ()> {
    let obj = v.as_object().ok_or(())?;
    Ok((
        obj.get("include_tests")
            .and_then(Value::as_bool)
            .ok_or(())?,
        obj.get("include_inferred")
            .and_then(Value::as_bool)
            .ok_or(())?,
    ))
}

fn group(v: &Value) -> Result<Group, ()> {
    let obj = v.as_object().ok_or(())?;
    Ok(Group {
        imports: obj.get("imports").and_then(Value::as_u64).ok_or(())?,
        edges: obj.get("edges").and_then(Value::as_u64).ok_or(())?,
    })
}

/// Parse a surface whose partition keys are the view and its remainder only (`imports <file>`):
/// [`surface_partition`] with [`Requires::REMAINDER_ONLY`] — a present `import_view` without
/// `import_remainder` is unreadable, never "nothing excluded".
pub(crate) fn partition_from(view: Option<&Value>, remainder: Option<&Value>) -> Partition {
    surface_partition(view, remainder, None, None, Requires::REMAINDER_ONLY)
}

/// Parse an `import_remainder` value: every group present with both counts, else `Err`.
pub(crate) fn remainder_of(r: &Value) -> Result<Remainder, ()> {
    Ok(Remainder {
        tests: group(r.get("tests").ok_or(())?)?,
        inferred: group(r.get("inferred").ok_or(())?)?,
        tests_and_inferred: group(r.get("tests_and_inferred").ok_or(())?)?,
    })
}

/// The relation noun of a remainder line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EdgeNoun {
    /// `modules list`, `modules deps`: module-candidate relations.
    CrossModule,
    /// `cycles`: directory-group edges.
    DirectoryGroup,
    /// `imports <file>`: no relation count.
    None,
}

impl EdgeNoun {
    fn phrase(self, n: u64) -> Option<String> {
        let (one, many) = match self {
            EdgeNoun::CrossModule => ("cross-module dependency", "cross-module dependencies"),
            EdgeNoun::DirectoryGroup => ("directory-group edge", "directory-group edges"),
            EdgeNoun::None => return None,
        };
        (n > 0).then(|| format!("{n} {}", if n == 1 { one } else { many }))
    }
}

/// The three groups in rendering order with their subject and flags.
fn groups(r: &Remainder) -> [(Group, &'static str, &'static str); 3] {
    [
        (r.tests, "from test files", "--include-tests"),
        (r.inferred, "inferred", "--include-inferred"),
        (
            r.tests_and_inferred,
            "inferred from test files",
            "--include-tests --include-inferred",
        ),
    ]
}

/// `+{n} import{s} from test files` / `+{n} inferred import{s}` / `+{n} inferred import{s} from
/// test files`.
fn subject(n: u64, kind: &str) -> String {
    match kind {
        "from test files" => format!("+{n} import{} from test files", plural(n)),
        "inferred" => format!("+{n} inferred import{}", plural(n)),
        _ => format!("+{n} inferred import{} from test files", plural(n)),
    }
}

/// The remainder lines in FLAG form (`modules list`, `modules deps`, `cycles`, `imports`): one line
/// per excluded group with a non-zero count, each stated once under the smallest flag set that
/// shows it. Empty when nothing is excluded.
pub(crate) fn remainder_lines(r: &Remainder, noun: EdgeNoun) -> Vec<String> {
    groups(r)
        .into_iter()
        .filter(|(g, _, _)| g.imports > 0)
        .map(|(g, kind, flags)| {
            let rel = noun
                .phrase(g.edges)
                .map(|p| format!(" ({p} only through them)"))
                .unwrap_or_default();
            format!("{}, not shown{rel} — {flags}", subject(g.imports, kind))
        })
        .collect()
}

/// The remainder in COMMAND form, joined into one clause (orient's module-edges line): no relation
/// count; the flag replaced by `rmap {command} {flags}`. `None` when nothing is excluded.
pub(crate) fn remainder_command_clause(r: &Remainder, command: &str) -> Option<String> {
    let parts: Vec<String> = groups(r)
        .into_iter()
        .filter(|(g, _, _)| g.imports > 0)
        .map(|(g, kind, flags)| {
            format!(
                "{}, not shown — rmap {command} {flags}",
                subject(g.imports, kind)
            )
        })
        .collect();
    (!parts.is_empty()).then(|| parts.join("; "))
}

/// The remainder a per-file imports surface states (`imports <file>`, explain's Imports section,
/// `map`'s file maps): the INFERRED group only — test status never filters a per-file answer
/// (D-TESB-READERS-1 §1), so `imports` has no `--include-tests` and a tests group is never
/// printed with a command that cannot show it.
pub(crate) fn per_file_remainder(r: &Remainder) -> Remainder {
    Remainder {
        inferred: r.inferred,
        ..Remainder::default()
    }
}

/// The per-file imports clause in command form: `+{n} inferred import{s}, not shown — rmap
/// imports {file} --include-inferred`. `None` when `n` is 0.
pub(crate) fn imports_command_clause(inferred: u64, file: &str) -> Option<String> {
    let r = Remainder {
        inferred: Group {
            imports: inferred,
            edges: 0,
        },
        ..Remainder::default()
    };
    remainder_command_clause(&r, &format!("imports {}", shell_quote(file)))
}

/// One excluded cycle of a response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExcludedCycle {
    pub members: Vec<String>,
    pub flags: Vec<String>,
    pub contains_shown: usize,
}

/// Parse `excluded_cycles`: `None` when absent, `Some(Err(()))` when malformed.
pub(crate) fn excluded_cycles_of(response: &Value) -> Option<Result<Vec<ExcludedCycle>, ()>> {
    response.get("excluded_cycles").map(parse_excluded_cycles)
}

/// Parse an `excluded_cycles` value: an array of `{members: [str], flags: [str], contains_shown:
/// [[str]]}` (W2: an object instead of an array, a numeric member, a string `flags` or a numeric
/// `contains_shown` is `Err`).
pub(crate) fn parse_excluded_cycles(arr: &Value) -> Result<Vec<ExcludedCycle>, ()> {
    {
        arr.as_array()
            .ok_or(())?
            .iter()
            .map(|c| {
                let strs = |v: &Value| -> Result<Vec<String>, ()> {
                    v.as_array()
                        .ok_or(())?
                        .iter()
                        .map(|s| s.as_str().map(str::to_string).ok_or(()))
                        .collect()
                };
                Ok(ExcludedCycle {
                    members: strs(c.get("members").ok_or(())?)?,
                    flags: strs(c.get("flags").ok_or(())?)?,
                    contains_shown: {
                        let shown = c
                            .get("contains_shown")
                            .and_then(Value::as_array)
                            .ok_or(())?;
                        for s in shown {
                            strs(s)?;
                        }
                        shown.len()
                    },
                })
            })
            .collect()
    }
}

/// `--include-tests --include-inferred` from the daemon flag names, in that order.
pub(crate) fn flags_text(flags: &[String]) -> String {
    let mut out = Vec::new();
    if flags.iter().any(|f| f == "include_tests") {
        out.push("--include-tests");
    }
    if flags.iter().any(|f| f == "include_inferred") {
        out.push("--include-inferred");
    }
    out.join(" ")
}

/// The excluded-cycle block of `cycles`: a header, one row per cycle (at most
/// [`EXCLUDED_CYCLE_ROWS`], members capped at [`EXCLUDED_CYCLE_MEMBERS`]), the flags that show
/// each, `(contains N shown cycles)` when it grows a shown one, and the elision line. Empty when
/// there is none.
pub(crate) fn excluded_cycle_lines(cycles: &[ExcludedCycle]) -> Vec<String> {
    if cycles.is_empty() {
        return Vec::new();
    }
    let n = cycles.len() as u64;
    let mut out = vec![format!(
        "+{n} cycle{} only through excluded imports, not shown:",
        plural(n)
    )];
    for c in cycles.iter().take(EXCLUDED_CYCLE_ROWS) {
        let k = c.members.len();
        let mut names: String = c
            .members
            .iter()
            .take(EXCLUDED_CYCLE_MEMBERS)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        if k > EXCLUDED_CYCLE_MEMBERS {
            names.push_str(&format!(", + {} more", k - EXCLUDED_CYCLE_MEMBERS));
        }
        let mut row = format!("  {k} modules: {names} — {}", flags_text(&c.flags));
        if c.contains_shown > 0 {
            let j = c.contains_shown as u64;
            row.push_str(&format!(" (contains {j} shown cycle{})", plural(j)));
        }
        out.push(row);
    }
    if cycles.len() > EXCLUDED_CYCLE_ROWS {
        let m = (cycles.len() - EXCLUDED_CYCLE_ROWS) as u64;
        out.push(format!(
            "  … and {m} more cycle{} — rmap cycles --include-tests --include-inferred --json",
            plural(m)
        ));
    }
    out
}

/// The orient/explain clause for excluded cycles: `+{c} only through imports from test files —
/// rmap cycles --include-tests` (inferred / excluded imports and the matching flags when the
/// cycles need them). `None` when there is none.
pub(crate) fn excluded_cycle_clause(cycles: &[ExcludedCycle]) -> Option<String> {
    if cycles.is_empty() {
        return None;
    }
    let only = |flag: &str| cycles.iter().all(|c| c.flags == [flag.to_string()]);
    let (subject, flags) = if only("include_tests") {
        ("imports from test files", "--include-tests")
    } else if only("include_inferred") {
        ("inferred imports", "--include-inferred")
    } else {
        ("excluded imports", "--include-tests --include-inferred")
    };
    Some(format!(
        "+{} only through {subject} — rmap cycles {flags}",
        cycles.len()
    ))
}

/// Quote a query for the shell when it needs it (RG-REQ-012-L03: a printed command runs as
/// printed).
pub(crate) fn shell_quote(s: &str) -> String {
    let safe = !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "_-./:#@,+=".contains(c));
    if safe {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', r"'\''"))
    }
}

/// D-TESB-07 (declared boundaries, `gate`'s `arch_violations`): the inferred imports across a
/// boundary, not judged, with the first cited file.
pub(crate) fn not_judged_boundary_line(count: u64, boundary: &str, files: &[String]) -> String {
    let f = files.len() as u64;
    let first = files.first().map(String::as_str).unwrap_or("");
    format!(
        "+{count} inferred import{} across {boundary} not judged ({f} file{}; first: {first}) — \
         investigate with rmap imports {} --include-inferred",
        plural(count),
        plural(f),
        shell_quote(first)
    )
}

/// D-TESB-07 (discovered-module and module violations): the inferred imports not judged, with the
/// source module of the first not-judged relation.
pub(crate) fn not_judged_module_line(count: u64, module: &str) -> String {
    format!(
        "+{count} inferred import{} not judged — investigate with rmap modules deps {} \
         --include-inferred",
        plural(count),
        shell_quote(module)
    )
}

/// The line for a present-but-malformed `inferred_imports_not_judged` value.
pub(crate) const NOT_JUDGED_UNREADABLE: &str =
    "inferred imports not judged: unreadable on this response — rmap violations --json lists them";

/// A decoded `inferred_imports_not_judged` value.
enum NotJudged {
    /// `{count, files}`: sorted, distinct, non-empty source files.
    Files(u64, Vec<String>),
    /// `{count, relations}`: the first relation's source module (`None` when there is none).
    Relations(u64, Option<String>),
}

/// Decode one `inferred_imports_not_judged` value strictly (D-TESB-17 row W4): `count` a
/// non-negative integer and exactly one of `files` (non-empty strings) or `relations` (objects
/// with string `source`/`target` and integer `import_count`) — or neither at a zero count; a
/// non-zero count needs something to cite. Anything else is `Err` — never a silent zero.
fn parse_not_judged(value: &Value) -> Result<NotJudged, ()> {
    let obj = value.as_object().ok_or(())?;
    let count = obj.get("count").and_then(Value::as_u64).ok_or(())?;
    match (obj.get("files"), obj.get("relations")) {
        (Some(files), None) => {
            let files: Vec<String> = files
                .as_array()
                .ok_or(())?
                .iter()
                .map(|f| {
                    f.as_str()
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                        .ok_or(())
                })
                .collect::<Result<_, _>>()?;
            if count > 0 && files.is_empty() {
                return Err(());
            }
            Ok(NotJudged::Files(count, files))
        }
        (None, Some(relations)) => {
            let relations = relations.as_array().ok_or(())?;
            for r in relations {
                let r = r.as_object().ok_or(())?;
                r.get("source")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .ok_or(())?;
                r.get("target").and_then(Value::as_str).ok_or(())?;
                r.get("import_count").and_then(Value::as_u64).ok_or(())?;
            }
            let first = relations
                .first()
                .and_then(|r| r.get("source"))
                .and_then(Value::as_str)
                .map(str::to_string);
            if count > 0 && first.is_none() {
                return Err(());
            }
            Ok(NotJudged::Relations(count, first))
        }
        // A zero count has nothing to cite: a measured zero even without a citation list.
        (None, None) if count == 0 => Ok(NotJudged::Files(0, Vec::new())),
        _ => Err(()),
    }
}

/// Whether one `inferred_imports_not_judged` value is readable (the JSON boundary's check).
pub(crate) fn not_judged_readable(value: &Value) -> bool {
    parse_not_judged(value).is_ok()
}

/// D-TESB-11: the not-judged line of one `inferred_imports_not_judged` value — `{count, files}`
/// (a boundary, named by `boundary`) or `{count, relations}` (module violations). `None` at a
/// measured zero count; the unreadable line for any other shape (never a silent zero).
pub(crate) fn not_judged_line(value: &Value, boundary: &str) -> Option<String> {
    match parse_not_judged(value) {
        Err(()) => Some(NOT_JUDGED_UNREADABLE.to_string()),
        Ok(NotJudged::Files(0, _)) | Ok(NotJudged::Relations(0, _)) => None,
        Ok(NotJudged::Files(count, files)) => {
            Some(not_judged_boundary_line(count, boundary, &files))
        }
        Ok(NotJudged::Relations(count, first)) => Some(not_judged_module_line(
            count,
            first.as_deref().unwrap_or_default(),
        )),
    }
}

/// D-TESB-15: `path` found no certain route but an admitting walk did.
pub(crate) fn path_no_route_line(n: u64, from: &str, to: &str) -> String {
    format!(
        "a route exists using {n} inferred call/import edge{} — investigate with rmap path {} {} \
         --include-inferred",
        plural(n),
        shell_quote(from),
        shell_quote(to)
    )
}

/// D-TESB-15: `path --include-inferred` found a route with inferred hops.
pub(crate) fn path_inferred_hops_line(n: u64, hops: u64) -> String {
    format!(
        "{n} of {hops} hop{} {} inferred call/import edge{} — investigate",
        plural(hops),
        if n == 1 { "is an" } else { "are" },
        plural(n)
    )
}

/// The partition statement of a partitioned surface: the remainder lines (flag form), or the
/// unavailable/unreadable line. Empty when nothing is excluded.
pub(crate) fn partition_lines_from(partition: Partition, noun: EdgeNoun) -> Vec<String> {
    match partition {
        Partition::Unavailable => vec![PARTITION_UNAVAILABLE.to_string()],
        Partition::Unreadable => vec![PARTITION_UNREADABLE.to_string()],
        Partition::Stated(r) => remainder_lines(&r, noun),
    }
}

// ── The JSON consumer boundary (D-TESB-17, INPUT-2 cycle 3) ─────────────────────────────────

/// The ten `--json` surfaces that carry partition evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JsonSurface {
    ModulesList,
    ModulesDeps,
    Cycles,
    Imports,
    Orient,
    Explain,
    Trust,
    Gate,
    Violations,
    ModulesViolations,
}

/// The root key the JSON boundary adds.
pub(crate) const PARTITION_EVIDENCE_STATUS: &str = "partition_evidence_status";

/// The outcome ONE carrier recorded (D-TESB-17, INPUT-3): validated, the older-daemon shape, or
/// unreadable (its entries are in [`Marks::entries`]).
#[derive(Debug, Clone, PartialEq, Eq)]
enum CarrierOutcome {
    /// Every required key and item of the carrier was positively validated.
    Stated,
    /// The carrier's partition marker is absent (a daemon that predates the partition); the
    /// carrier's RFC 6901 pointer.
    Older(String),
    /// The carrier recorded at least one `unreadable` entry. `marker`: its partition marker was
    /// present (it states the partition, badly).
    Unreadable { marker: bool },
}

/// What the marker found while walking one document: every carrier FOUND, the outcome each
/// RECORDED, and the `unreadable` entries (containers, carriers, items). The status is derived
/// from these tallies only — never defaulted ([`Marks::status`]).
#[derive(Default)]
struct Marks {
    found: usize,
    outcomes: Vec<CarrierOutcome>,
    entries: Vec<Value>,
}

impl Marks {
    /// A carrier was found (call before validating it; [`record`](Self::record) its outcome).
    fn found(&mut self) {
        self.found += 1;
    }

    fn record(&mut self, outcome: CarrierOutcome) {
        self.outcomes.push(outcome);
    }

    /// Replace the value at `key` of `obj` (pointer `at`) by `null` and record why; a missing key
    /// is inserted as `null` and recorded without a received value.
    fn unreadable(
        &mut self,
        obj: &mut serde_json::Map<String, Value>,
        key: &str,
        at: &str,
        reason: &str,
    ) {
        let pointer = format!("{at}/{}", escape_pointer(key));
        match obj.insert(key.to_string(), Value::Null) {
            Some(received) => self.entry(pointer, reason.to_string(), Some(received)),
            None => self.entry(pointer, format!("{reason} (missing)"), None),
        }
    }

    /// A carrier container (a collection whose members the traversal must find) that is missing
    /// or not its named type: a present value is replaced by `null` (its `received` kept); a
    /// missing one is recorded without inserting anything.
    fn container_unreadable(
        &mut self,
        obj: &mut serde_json::Map<String, Value>,
        key: &str,
        at: &str,
        reason: &str,
    ) {
        let pointer = format!("{at}/{}", escape_pointer(key));
        match obj.get_mut(key) {
            Some(v) => {
                let received = std::mem::replace(v, Value::Null);
                self.entry(pointer, reason.to_string(), Some(received));
            }
            None => self.entry(pointer, format!("{reason} (missing)"), None),
        }
    }

    /// A member of an array that is not an object: replaced by `null`, its `received` kept.
    fn item_unreadable(&mut self, item: &mut Value, pointer: String, reason: &str) {
        let received = std::mem::replace(item, Value::Null);
        self.entry(pointer, reason.to_string(), Some(received));
    }

    fn entry(&mut self, pointer: String, reason: String, received: Option<Value>) {
        let mut e =
            serde_json::json!({"pointer": pointer, "state": "unreadable", "reason": reason});
        if let Some(r) = received {
            e["received"] = r;
        }
        self.entries.push(e);
    }

    /// The derivation (D-TESB-17, INPUT-3): `stated` ONLY when every container was validated and
    /// every carrier found recorded `Stated` (zero carriers included); `unavailable` ONLY when every
    /// carrier found (n ≥ 1) is the older-daemon shape; anything else `unreadable`. A found carrier
    /// that recorded no outcome adds the fail-closed entry; an older carrier beside a carrier that
    /// states the partition gets its own entry (a partial document).
    fn status(mut self) -> Value {
        let recorded = self.outcomes.len();
        if recorded < self.found {
            self.entries.push(serde_json::json!({
                "pointer": "",
                "state": "unreadable",
                "reason": format!(
                    "partition evidence not positively validated: {} of {} carriers",
                    self.found - recorded,
                    self.found
                ),
            }));
        }
        let stating_any = self.outcomes.iter().any(|o| {
            matches!(
                o,
                CarrierOutcome::Stated | CarrierOutcome::Unreadable { marker: true }
            )
        });
        let older: Vec<String> = self
            .outcomes
            .iter()
            .filter_map(|o| match o {
                CarrierOutcome::Older(p) => Some(p.clone()),
                _ => None,
            })
            .collect();
        if stating_any {
            for pointer in &older {
                self.entries.push(serde_json::json!({
                    "pointer": pointer,
                    "state": "unreadable",
                    "reason": "carries no partition evidence beside carriers that do",
                }));
            }
        }
        let state = if !self.entries.is_empty() {
            "unreadable"
        } else if !older.is_empty() {
            // No entry, every outcome recorded and none stating: every carrier is older-shaped.
            "unavailable"
        } else {
            "stated"
        };
        serde_json::json!({"state": state, "entries": self.entries})
    }
}

/// RFC 6901 escaping of one reference token.
fn escape_pointer(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}

/// D-TESB-17 (the JSON consumer boundary): on the `--json` print path of the ten partition
/// surfaces, before printing, read every partition key with the SAME decoders the human renderers
/// use. A well-formed key — zeros included — is left unchanged. A key that is malformed (wrong type,
/// a non-numeric or negative count, a missing sub-key), or missing beside a present `import_view`, is
/// replaced by `null` and recorded as `unreadable` with its RFC 6901 pointer, the reason and the
/// received value (nothing is erased). One additive root key,
/// `partition_evidence_status: {"state": "stated"|"unavailable"|"unreadable", "entries": [...]}`,
/// records the outcome, derived from positive validation (INPUT-3, [`Marks::status`]): every carrier
/// container is validated as its named type, every carrier found records exactly one outcome, and
/// `stated` is never a default. `Err` names an answer that is not a JSON object (the document is left
/// unchanged; the caller prints the error with the runtime-error exit, never a document without its
/// status).
#[must_use = "a refused answer must be reported, never printed without its status"]
pub(crate) fn mark_partition_evidence(doc: &mut Value, surface: JsonSurface) -> Result<(), String> {
    if !doc.is_object() {
        return Err(format!(
            "the daemon's {surface:?} answer is not a JSON object; its partition evidence cannot \
             be read"
        ));
    }
    let mut marks = Marks::default();
    match surface {
        JsonSurface::ModulesList => mark_view_holder(doc, Requires::MODULE_EDGES, Some("edges"), &mut marks),
        JsonSurface::ModulesDeps => mark_view_holder(doc, Requires::MODULE_EDGES, Some("results"), &mut marks),
        JsonSurface::Cycles => mark_view_holder(doc, Requires::CYCLES, Some("cycles"), &mut marks),
        JsonSurface::Imports => mark_view_holder(doc, Requires::REMAINDER_ONLY, None, &mut marks),
        JsonSurface::Orient => mark_signals(doc, true, &mut marks),
        JsonSurface::Explain => mark_signals(doc, false, &mut marks),
        JsonSurface::Trust => mark_trust_rows(doc, &mut marks),
        JsonSurface::Gate => mark_gate_obligations(doc, &mut marks),
        JsonSurface::Violations => mark_root_not_judged(doc, violations_not_judged_readable, "inferred_imports_not_judged is not {declared: [...], discovered: {count, relations}} with non-negative integer counts", &mut marks),
        JsonSurface::ModulesViolations => mark_root_not_judged(doc, not_judged_readable, "inferred_imports_not_judged is not {count, relations} with a non-negative integer count", &mut marks),
    }
    let status = marks.status();
    if let Some(obj) = doc.as_object_mut() {
        obj.insert(PARTITION_EVIDENCE_STATUS.to_string(), status);
    }
    Ok(())
}

/// The object holding a surface's report: the document itself when it carries `marker` or has no
/// `value`, else its `value` — which must then be an object (else an entry at `/value` and `None`).
fn report<'a>(
    doc: &'a mut Value,
    marker: &str,
    marks: &mut Marks,
) -> Option<(&'a mut serde_json::Map<String, Value>, String)> {
    let obj = doc.as_object_mut()?;
    if obj.contains_key(marker) || !obj.contains_key("value") {
        return Some((obj, String::new()));
    }
    if obj.get("value").is_some_and(Value::is_object) {
        return obj
            .get_mut("value")
            .and_then(Value::as_object_mut)
            .map(|o| (o, "/value".to_string()));
    }
    marks.container_unreadable(obj, "value", "", "value is not the answer object");
    None
}

/// `modules list`, `modules deps`, `cycles`, `imports`: the holder is the one carrier.
fn mark_view_holder(doc: &mut Value, req: Requires, items_key: Option<&str>, marks: &mut Marks) {
    if let Some((obj, at)) = report(doc, "import_view", marks) {
        mark_view_object(obj, &at, req, items_key, marks);
    }
}

/// Validate ONE partitioned carrier (`modules list`, `modules deps`, `cycles`, `imports`, orient's
/// module-edges block and cycle evidence, explain's cycle evidence) and record its outcome:
/// `import_view` marks it (absent → the older-daemon shape); beside it, `import_remainder` and the
/// keys `req` names must be present and well formed, `items_key` must be an array of objects, and
/// each item must carry `partitions` as the counts object or `null` beside a string
/// `partitions_unavailable`.
fn mark_view_object(
    obj: &mut serde_json::Map<String, Value>,
    at: &str,
    req: Requires,
    items_key: Option<&str>,
    marks: &mut Marks,
) {
    marks.found();
    let Some(view) = obj.get("import_view") else {
        marks.record(CarrierOutcome::Older(at.to_string()));
        return;
    };
    let before = marks.entries.len();
    if view_of(view).is_err() {
        marks.unreadable(
            obj,
            "import_view",
            at,
            "import_view is not {include_tests: bool, include_inferred: bool}",
        );
    }
    if obj
        .get("import_remainder")
        .map(remainder_of)
        .and_then(Result::ok)
        .is_none()
    {
        marks.unreadable(
            obj,
            "import_remainder",
            at,
            "import_remainder is not three groups of non-negative integer {imports, edges}",
        );
    }
    if req.excluded_cycles
        && obj
            .get("excluded_cycles")
            .map(parse_excluded_cycles)
            .and_then(Result::ok)
            .is_none()
    {
        marks.unreadable(
            obj,
            "excluded_cycles",
            at,
            "excluded_cycles is not a list of {members, flags, contains_shown}",
        );
    }
    if req.importer_block
        && !obj
            .get("importer_test_status_undetermined")
            .is_some_and(crate::presentation::test_status::block_is_readable)
    {
        marks.unreadable(
            obj,
            "importer_test_status_undetermined",
            at,
            "importer_test_status_undetermined is not one consistent undetermined-files block",
        );
    }
    if let Some(items_key) = items_key {
        let items_at = format!("{at}/{}", escape_pointer(items_key));
        match obj.get_mut(items_key) {
            Some(Value::Array(items)) => {
                for (i, item) in items.iter_mut().enumerate() {
                    let item_at = format!("{items_at}/{i}");
                    match item.as_object_mut() {
                        None => marks.item_unreadable(item, item_at, "the item is not an object"),
                        Some(item) => {
                            if !item_partitions_readable(item) {
                                marks.unreadable(item, "partitions", &item_at, "partitions is not the five non-negative integer counts, nor null beside a string partitions_unavailable");
                            }
                        }
                    }
                }
            }
            _ => marks.unreadable(
                obj,
                items_key,
                at,
                &format!("{items_key} is not an array of items beside a present import_view"),
            ),
        }
    }
    marks.record(if marks.entries.len() > before {
        CarrierOutcome::Unreadable { marker: true }
    } else {
        CarrierOutcome::Stated
    });
}

/// An item's `partitions`: the five counts, or `null` beside a string `partitions_unavailable`.
fn item_partitions_readable(item: &serde_json::Map<String, Value>) -> bool {
    match item.get("partitions") {
        Some(Value::Null) => item
            .get("partitions_unavailable")
            .is_some_and(Value::is_string),
        Some(Value::Object(p)) => [
            "production_certain",
            "test_certain",
            "production_inferred",
            "test_inferred",
            "unknown_test_status",
        ]
        .iter()
        .all(|k| p.get(*k).is_some_and(|v| v.as_u64().is_some())),
        _ => false,
    }
}

/// orient/explain: the container is the envelope's `signals` array; the carriers are each
/// `IMPORT_CYCLES`/`EXPLAIN_CYCLES` signal (its `evidence` must be an object) and, on orient, the
/// `top_module_edges` block when present. A signal that is not an object, or whose `value` is not
/// an object, cannot be classified and is an entry.
fn mark_signals(doc: &mut Value, orient: bool, marks: &mut Marks) {
    let Some((report, at)) = report(doc, "signals", marks) else {
        return;
    };
    match report.get_mut("signals") {
        Some(Value::Array(signals)) => {
            for (i, signal) in signals.iter_mut().enumerate() {
                let signal_at = format!("{at}/signals/{i}");
                mark_signal(signal, signal_at, marks);
            }
        }
        _ => marks.container_unreadable(report, "signals", &at, "signals is not an array"),
    }
    if orient {
        let block_at = format!("{at}/top_module_edges");
        match report.get_mut("top_module_edges") {
            None => {}
            Some(Value::Object(block)) => {
                if let Some(reason) = block.get("unavailable") {
                    // The daemon states the block unavailable: kept in place (it holds no count).
                    marks.found();
                    let reason = format!(
                        "the daemon states the module-edges block unavailable: {}",
                        reason
                            .as_str()
                            .map_or_else(|| reason.to_string(), str::to_string)
                    );
                    marks.entry(block_at, reason, None);
                    marks.record(CarrierOutcome::Unreadable { marker: false });
                } else {
                    mark_view_object(
                        block,
                        &block_at,
                        Requires::MODULE_EDGES,
                        Some("edges"),
                        marks,
                    );
                }
            }
            Some(_) => {
                marks.found();
                marks.unreadable(
                    report,
                    "top_module_edges",
                    &at,
                    "top_module_edges is not an object",
                );
                marks.record(CarrierOutcome::Unreadable { marker: false });
            }
        }
    }
}

/// One entry of the `signals` container: the leaf's `{code, evidence}` sits in its `value` (the
/// coherence leaf) or on the entry itself.
fn mark_signal(signal: &mut Value, signal_at: String, marks: &mut Marks) {
    let Some(entry) = signal.as_object_mut() else {
        marks.item_unreadable(signal, signal_at, "the signal is not an object");
        return;
    };
    let (holder, holder_at) = if entry.contains_key("code") {
        (entry, signal_at)
    } else {
        match entry.get("value") {
            Some(Value::Object(_)) => (
                entry
                    .get_mut("value")
                    .and_then(Value::as_object_mut)
                    .expect("checked: an object"),
                format!("{signal_at}/value"),
            ),
            // A leaf without a value cannot carry a code: nothing to classify.
            None => return,
            Some(_) => {
                marks.container_unreadable(
                    entry,
                    "value",
                    &signal_at,
                    "the signal's value is not an object",
                );
                return;
            }
        }
    };
    let items_key = match holder.get("code").and_then(Value::as_str) {
        Some("IMPORT_CYCLES") => "cycles",
        Some("EXPLAIN_CYCLES") => "items",
        _ => return,
    };
    match holder.get_mut("evidence") {
        Some(Value::Object(ev)) => {
            let ev_at = format!("{holder_at}/evidence");
            mark_view_object(ev, &ev_at, Requires::CYCLES, Some(items_key), marks);
        }
        _ => {
            marks.found();
            marks.unreadable(
                holder,
                "evidence",
                &holder_at,
                "the cycle signal's evidence is not an object",
            );
            marks.record(CarrierOutcome::Unreadable { marker: false });
        }
    }
}

/// trust: the container is the module row array (the `modules` section's `value` when wrapped);
/// every row is a carrier whose `excluded_connectivity` is decoded strictly (the same decode
/// `rmap trust`'s human path runs).
fn mark_trust_rows(doc: &mut Value, marks: &mut Marks) {
    let Some((report, at)) = report(doc, "modules", marks) else {
        return;
    };
    let modules_at = format!("{at}/modules");
    let (rows, rows_at) = match report.get_mut("modules") {
        Some(Value::Array(_)) => (
            report
                .get_mut("modules")
                .and_then(Value::as_array_mut)
                .expect("checked: an array"),
            modules_at,
        ),
        Some(Value::Object(section)) if section.contains_key("value") => {
            if !section.get("value").is_some_and(Value::is_array) {
                marks.container_unreadable(
                    section,
                    "value",
                    &modules_at,
                    "the modules section's value is not an array of module rows",
                );
                return;
            }
            (
                section
                    .get_mut("value")
                    .and_then(Value::as_array_mut)
                    .expect("checked: an array"),
                format!("{modules_at}/value"),
            )
        }
        _ => {
            marks.container_unreadable(
                report,
                "modules",
                &at,
                "modules is not an array of module rows",
            );
            return;
        }
    };
    for (i, row) in rows.iter_mut().enumerate() {
        let row_at = format!("{rows_at}/{i}");
        marks.found();
        let Some(row_obj) = row.as_object_mut() else {
            marks.item_unreadable(row, row_at, "the module row is not an object");
            marks.record(CarrierOutcome::Unreadable { marker: false });
            continue;
        };
        match row_obj.get("excluded_connectivity") {
            None => marks.record(CarrierOutcome::Older(row_at)),
            Some(e) => {
                if serde_json::from_value::<repo_graph_trust::storage_port::ExcludedConnectivity>(
                    e.clone(),
                )
                .is_ok()
                {
                    marks.record(CarrierOutcome::Stated);
                } else {
                    marks.unreadable(row_obj, "excluded_connectivity", &row_at, "excluded_connectivity is not three non-negative integer counts {tests, inferred, tests_and_inferred}");
                    marks.record(CarrierOutcome::Unreadable { marker: true });
                }
            }
        }
    }
}

/// gate: the container is the report's `obligations` array; each `arch_violations` /
/// `module_violations` obligation is a carrier whose `evidence` must be an object carrying a
/// readable `inferred_imports_not_judged` (absent → the older-daemon shape).
fn mark_gate_obligations(doc: &mut Value, marks: &mut Marks) {
    let Some((report, at)) = report(doc, "obligations", marks) else {
        return;
    };
    let Some(Value::Array(obligations)) = report.get_mut("obligations") else {
        marks.container_unreadable(report, "obligations", &at, "obligations is not an array");
        return;
    };
    for (i, obligation) in obligations.iter_mut().enumerate() {
        let obl_at = format!("{at}/obligations/{i}");
        let Some(obl) = obligation.as_object_mut() else {
            marks.item_unreadable(obligation, obl_at, "the obligation is not an object");
            continue;
        };
        let judged = matches!(
            obl.get("method").and_then(Value::as_str),
            Some("arch_violations") | Some("module_violations")
        );
        if !judged {
            continue;
        }
        marks.found();
        let ev_at = format!("{obl_at}/evidence");
        let Some(Value::Object(ev)) = obl.get_mut("evidence") else {
            marks.unreadable(
                obl,
                "evidence",
                &obl_at,
                "a judged obligation's evidence is not an object",
            );
            marks.record(CarrierOutcome::Unreadable { marker: false });
            continue;
        };
        match ev.get("inferred_imports_not_judged") {
            None => marks.record(CarrierOutcome::Older(ev_at)),
            Some(nj) => {
                if not_judged_readable(nj) {
                    marks.record(CarrierOutcome::Stated);
                } else {
                    marks.unreadable(ev, "inferred_imports_not_judged", &ev_at, "inferred_imports_not_judged is not {count, files|relations} with a non-negative integer count");
                    marks.record(CarrierOutcome::Unreadable { marker: true });
                }
            }
        }
    }
}

/// `violations`' root `inferred_imports_not_judged`: `{declared: [...], discovered: {...}}`.
fn violations_not_judged_readable(nj: &Value) -> bool {
    nj.as_object().is_some_and(|o| {
        o.get("declared")
            .and_then(Value::as_array)
            .is_some_and(|rows| {
                rows.iter().all(|r| {
                    not_judged_readable(r)
                        && r.get("boundary_module").is_some_and(Value::is_string)
                        && r.get("forbidden_module").is_some_and(Value::is_string)
                })
            })
            && o.get("discovered").is_some_and(not_judged_readable)
    })
}

/// violations / modules violations: the holder is the one carrier; its root
/// `inferred_imports_not_judged` is checked by `readable`.
fn mark_root_not_judged(
    doc: &mut Value,
    readable: fn(&Value) -> bool,
    reason: &str,
    marks: &mut Marks,
) {
    let Some((obj, at)) = report(doc, "inferred_imports_not_judged", marks) else {
        return;
    };
    marks.found();
    match obj.get("inferred_imports_not_judged") {
        None => marks.record(CarrierOutcome::Older(at)),
        Some(nj) => {
            if readable(nj) {
                marks.record(CarrierOutcome::Stated);
            } else {
                marks.unreadable(obj, "inferred_imports_not_judged", &at, reason);
                marks.record(CarrierOutcome::Unreadable { marker: true });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The partition statement of a whole response (both keys read from it).
    fn partition_lines(response: &Value, noun: EdgeNoun) -> Vec<String> {
        partition_lines_from(
            partition_from(
                response.get("import_view"),
                response.get("import_remainder"),
            ),
            noun,
        )
    }

    fn rem(t: (u64, u64), i: (u64, u64), ti: (u64, u64)) -> Remainder {
        Remainder {
            tests: Group {
                imports: t.0,
                edges: t.1,
            },
            inferred: Group {
                imports: i.0,
                edges: i.1,
            },
            tests_and_inferred: Group {
                imports: ti.0,
                edges: ti.1,
            },
        }
    }

    #[test]
    fn governance_not_judged_lines_name_a_runnable_command_with_a_cited_file() {
        // D-TESB-11 / review-0 F-4: the next action cites a real file (the first of the
        // not-judged imports' source files) or module (the first relation's source) from the
        // payload — never a placeholder.
        let boundary = json!({"count": 3, "files": ["src/core/a.py", "src/core/b.py"]});
        assert_eq!(
            not_judged_line(&boundary, "src/core -> src/adapters").unwrap(),
            "+3 inferred imports across src/core -> src/adapters not judged (2 files; first: \
             src/core/a.py) — investigate with rmap imports src/core/a.py --include-inferred"
        );
        let module = json!({"count": 1, "relations": [
            {"source": "db", "target": "util", "import_count": 1},
            {"source": "table", "target": "util", "import_count": 0}
        ]});
        assert_eq!(
            not_judged_line(&module, "").unwrap(),
            "+1 inferred import not judged — investigate with rmap modules deps db --include-inferred"
        );
        // Zero: no line. A shape without a citable file or module: unreadable, never a line
        // with a blank or placeholder command.
        assert_eq!(not_judged_line(&json!({"count": 0}), "b"), None);
        for bad in [
            json!({"count": 2, "files": []}),
            json!({"count": 2, "files": [""]}),
            json!({"count": 2, "relations": []}),
            json!({"count": 2}),
            json!({"files": ["a.py"]}),
        ] {
            assert_eq!(
                not_judged_line(&bad, "b").as_deref(),
                Some(NOT_JUDGED_UNREADABLE),
                "{bad}"
            );
        }
    }

    #[test]
    fn remainder_lines_state_each_excluded_partition_once_under_its_smallest_flag_set() {
        let r = rem((105, 6), (1, 0), (2, 1));
        assert_eq!(
            remainder_lines(&r, EdgeNoun::DirectoryGroup),
            vec![
                "+105 imports from test files, not shown (6 directory-group edges only through them) — --include-tests",
                "+1 inferred import, not shown — --include-inferred",
                "+2 inferred imports from test files, not shown (1 directory-group edge only through them) — --include-tests --include-inferred",
            ]
        );
        assert_eq!(
            remainder_lines(&rem((89, 3), (0, 0), (0, 0)), EdgeNoun::CrossModule),
            vec!["+89 imports from test files, not shown (3 cross-module dependencies only through them) — --include-tests"]
        );
        assert_eq!(
            remainder_lines(&rem((0, 0), (2, 0), (0, 0)), EdgeNoun::None),
            vec!["+2 inferred imports, not shown — --include-inferred"]
        );
        assert_eq!(
            remainder_command_clause(&rem((89, 3), (0, 0), (0, 0)), "modules list").as_deref(),
            Some("+89 imports from test files, not shown — rmap modules list --include-tests")
        );
    }

    #[test]
    fn remainder_lines_are_absent_when_nothing_is_excluded() {
        let r = Remainder::default();
        assert!(remainder_lines(&r, EdgeNoun::CrossModule).is_empty());
        assert!(remainder_command_clause(&r, "modules list").is_none());
        let v = json!({"import_view": {"include_tests": false, "include_inferred": false},
                       "import_remainder": {"tests": {"imports": 0, "edges": 0},
                                            "inferred": {"imports": 0, "edges": 0},
                                            "tests_and_inferred": {"imports": 0, "edges": 0}}});
        assert!(partition_lines(&v, EdgeNoun::CrossModule).is_empty());
        assert!(excluded_cycle_lines(&[]).is_empty());
        assert!(excluded_cycle_clause(&[]).is_none());
    }

    #[test]
    fn absent_partition_from_an_older_daemon_renders_unavailable_never_zero() {
        // An older payload: edges including test-file imports, no `import_view`.
        let v =
            json!({"edges": [{"source": "Foundation", "target": "CppUnit", "import_count": 415}]});
        assert_eq!(
            partition_from(v.get("import_view"), v.get("import_remainder")),
            Partition::Unavailable
        );
        assert_eq!(
            partition_lines(&v, EdgeNoun::CrossModule),
            vec![PARTITION_UNAVAILABLE.to_string()]
        );
        assert!(
            !PARTITION_UNAVAILABLE.contains('0'),
            "never a zero remainder"
        );
        // A malformed remainder is unreadable, never a default.
        let bad = json!({"import_view": {}, "import_remainder": {"tests": {"imports": "x"}}});
        assert_eq!(
            partition_lines(&bad, EdgeNoun::CrossModule),
            vec![PARTITION_UNREADABLE.to_string()]
        );
    }
    // ── D-TESB-17: one decode rule; malformed and partial payloads (rows U9, W1–W4) ─────────

    fn view() -> Value {
        json!({"include_tests": false, "include_inferred": false})
    }

    fn zero_remainder() -> Value {
        json!({"tests": {"imports": 0, "edges": 0},
               "inferred": {"imports": 0, "edges": 0},
               "tests_and_inferred": {"imports": 0, "edges": 0}})
    }

    fn zero_block(universe: &str) -> Value {
        json!({"count": 0, "paths": [], "universe": universe, "universe_count": 0,
               "unknown_count": 0})
    }

    fn cycles_partition(
        remainder: Option<Value>,
        excluded: Option<Value>,
        block: Option<Value>,
    ) -> Partition {
        surface_partition(
            Some(&view()),
            remainder.as_ref(),
            excluded.as_ref(),
            block.as_ref(),
            Requires::CYCLES,
        )
    }

    #[test]
    fn partition_with_a_view_and_no_remainder_is_unreadable_never_nothing_excluded() {
        // A partial payload: the view is stated but its remainder is missing — never read as
        // "nothing excluded" (review-0 finding 2, row U9).
        assert_eq!(partition_from(Some(&view()), None), Partition::Unreadable);
        assert_eq!(
            partition_lines_from(partition_from(Some(&view()), None), EdgeNoun::CrossModule),
            vec![PARTITION_UNREADABLE.to_string()]
        );
        // The complete zero payload is a measured "nothing excluded" (no line).
        assert_eq!(
            partition_from(Some(&view()), Some(&zero_remainder())),
            Partition::Stated(Remainder::default())
        );
        // A view that is not the two booleans is unreadable too.
        assert_eq!(
            partition_from(
                Some(&json!({"include_tests": "no"})),
                Some(&zero_remainder())
            ),
            Partition::Unreadable
        );
    }

    #[test]
    fn excluded_cycles_or_importer_block_absent_beside_a_view_is_unreadable() {
        let block = || Some(zero_block("cross_directory_importers"));
        assert_eq!(
            cycles_partition(Some(zero_remainder()), Some(json!([])), block()),
            Partition::Stated(Remainder::default())
        );
        assert_eq!(
            cycles_partition(Some(zero_remainder()), None, block()),
            Partition::Unreadable,
            "excluded_cycles absent beside a view is never 'no excluded cycle'"
        );
        assert_eq!(
            cycles_partition(Some(zero_remainder()), Some(json!([])), None),
            Partition::Unreadable,
            "the importer block absent beside a view is never 'nothing undetermined'"
        );
        // A surface that does not require them is unaffected by their absence.
        assert_eq!(
            surface_partition(
                Some(&view()),
                Some(&zero_remainder()),
                None,
                None,
                Requires::REMAINDER_ONLY
            ),
            Partition::Stated(Remainder::default())
        );
    }

    #[test]
    fn remainder_wrong_typed_or_non_numeric_is_unreadable_never_zero() {
        for bad in [
            json!("0"),
            json!([]),
            json!({"tests": {"imports": "12", "edges": 1},
                   "inferred": {"imports": 0, "edges": 0},
                   "tests_and_inferred": {"imports": 0, "edges": 0}}),
            json!({"tests": {"imports": 0},
                   "inferred": {"imports": 0, "edges": 0},
                   "tests_and_inferred": {"imports": 0, "edges": 0}}),
            json!({"tests": {"imports": -1, "edges": 0},
                   "inferred": {"imports": 0, "edges": 0},
                   "tests_and_inferred": {"imports": 0, "edges": 0}}),
            json!({"tests": {"imports": 1.5, "edges": 0},
                   "inferred": {"imports": 0, "edges": 0},
                   "tests_and_inferred": {"imports": 0, "edges": 0}}),
        ] {
            assert!(remainder_of(&bad).is_err(), "{bad}");
            assert_eq!(
                partition_from(Some(&view()), Some(&bad)),
                Partition::Unreadable,
                "{bad}"
            );
            let lines =
                partition_lines_from(partition_from(Some(&view()), Some(&bad)), EdgeNoun::None);
            assert_eq!(lines, vec![PARTITION_UNREADABLE.to_string()], "{bad}");
        }
    }

    #[test]
    fn excluded_cycles_wrong_typed_is_unreadable_never_none_excluded() {
        let ok = json!({"members": ["a", "b"], "flags": ["include_tests"], "contains_shown": []});
        assert!(parse_excluded_cycles(&json!([ok])).is_ok());
        for bad in [
            json!({"members": ["a", "b"]}),
            json!([{"members": ["a", 2], "flags": ["include_tests"], "contains_shown": []}]),
            json!([{"members": ["a", "b"], "flags": "include_tests", "contains_shown": []}]),
            json!([{"members": ["a", "b"], "flags": ["include_tests"], "contains_shown": 1}]),
            json!([{"members": ["a", "b"], "flags": ["include_tests"], "contains_shown": [[1]]}]),
            json!("[]"),
        ] {
            assert!(parse_excluded_cycles(&bad).is_err(), "{bad}");
            assert_eq!(
                cycles_partition(
                    Some(zero_remainder()),
                    Some(bad.clone()),
                    Some(zero_block("cross_directory_importers"))
                ),
                Partition::Unreadable,
                "{bad}"
            );
        }
    }

    #[test]
    fn importer_block_wrong_typed_or_non_numeric_beside_a_view_is_unreadable() {
        for bad in [
            json!("0 files"),
            json!({"count": "3", "paths": [], "universe": "cross_module_importers",
                   "universe_count": 5, "unknown_count": 0}),
            json!({"count": 0, "paths": [], "universe": "cross_module_importers",
                   "universe_count": {"n": 5}, "unknown_count": 0}),
            json!({"count": 0, "paths": [], "universe": "cross_module_importers",
                   "universe_count": -1, "unknown_count": 0}),
        ] {
            assert_eq!(
                surface_partition(
                    Some(&view()),
                    Some(&zero_remainder()),
                    None,
                    Some(&bad),
                    Requires::MODULE_EDGES
                ),
                Partition::Unreadable,
                "{bad}"
            );
        }
    }

    #[test]
    fn not_judged_wrong_typed_or_non_numeric_is_unreadable_never_zero() {
        for bad in [
            json!({"count": "x", "files": []}),
            json!({"count": 0, "files": "src/a.py"}),
            json!({"count": 0, "relations": {"source": "db"}}),
            json!({"count": -1, "files": []}),
            json!({"count": 2.5, "relations": []}),
            json!({"count": 1, "relations": [{"source": "db", "target": "u", "import_count": "1"}]}),
            json!(3),
        ] {
            assert_eq!(
                not_judged_line(&bad, "b").as_deref(),
                Some(NOT_JUDGED_UNREADABLE),
                "{bad}"
            );
            assert!(!not_judged_readable(&bad), "{bad}");
        }
        // Measured zeros are readable and render nothing.
        for zero in [
            json!({"count": 0, "files": []}),
            json!({"count": 0, "relations": []}),
        ] {
            assert!(not_judged_readable(&zero));
            assert_eq!(not_judged_line(&zero, "b"), None);
        }
    }

    // ── D-TESB-17: the JSON consumer boundary ────────────────────────────────────────────────

    fn status(doc: &Value) -> &Value {
        &doc[PARTITION_EVIDENCE_STATUS]
    }

    /// Mark a JSON document that is an object (every fixture here is one).
    fn mark(doc: &mut Value, surface: JsonSurface) {
        mark_partition_evidence(doc, surface).expect("an object answer is marked, never refused");
    }

    fn entry_at<'a>(doc: &'a Value, pointer: &str) -> &'a Value {
        status(doc)["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["pointer"] == pointer)
            .unwrap_or_else(|| panic!("no entry at {pointer}: {doc}"))
    }

    fn assert_unreadable_at(doc: &Value, pointer: &str, received: Option<&Value>) {
        assert_eq!(status(doc)["state"], "unreadable", "{doc}");
        assert!(
            doc.pointer(pointer).is_some_and(Value::is_null),
            "{pointer}: {doc}"
        );
        let e = entry_at(doc, pointer);
        assert_eq!(e["state"], "unreadable");
        assert!(e["reason"].as_str().is_some_and(|r| !r.is_empty()));
        match received {
            Some(v) => assert_eq!(&e["received"], v, "the received value is kept"),
            None => assert!(e.get("received").is_none()),
        }
    }

    fn modules_list_doc() -> Value {
        json!({
            "edges": [{"source": "db", "target": "table", "import_count": 1,
                       "partitions": {"production_certain": 1, "test_certain": 0,
                                      "production_inferred": 0, "test_inferred": 0,
                                      "unknown_test_status": 0}}],
            "import_view": view(),
            "import_remainder": zero_remainder(),
            "importer_test_status_undetermined": zero_block("cross_module_importers"),
        })
    }

    fn cycles_doc() -> Value {
        json!({
            "cycles": [{"nodes": [], "partitions": null,
                        "partitions_unavailable": "a member matches no directory module"}],
            "count": 1,
            "import_view": view(),
            "import_remainder": zero_remainder(),
            "excluded_cycles": [],
            "importer_test_status_undetermined": zero_block("cross_directory_importers"),
        })
    }

    fn orient_doc(evidence: Value) -> Value {
        json!({"value": {"signals": [{"value": {"code": "IMPORT_CYCLES", "evidence": evidence},
                                      "provenance": {"source": ["sqlite"]}}]}})
    }

    fn orient_cycle_evidence() -> Value {
        json!({"cycle_count": 0, "cycles": [], "import_view": view(),
               "import_remainder": zero_remainder(), "excluded_cycles": [],
               "importer_test_status_undetermined": zero_block("cross_directory_importers")})
    }

    #[test]
    fn json_boundary_marks_a_wrong_typed_or_non_numeric_remainder_unreadable() {
        let bad_group = json!({"tests": {"imports": "12", "edges": 1},
                               "inferred": {"imports": 0, "edges": 0},
                               "tests_and_inferred": {"imports": 0, "edges": 0}});
        for (surface, mut doc) in [
            (JsonSurface::ModulesList, modules_list_doc()),
            (JsonSurface::ModulesDeps, {
                let mut d = modules_list_doc();
                d["results"] = d["edges"].take();
                d.as_object_mut().unwrap().remove("edges");
                d
            }),
            (JsonSurface::Cycles, cycles_doc()),
            (
                JsonSurface::Imports,
                json!({"file": "a.cc", "imports": [], "count": 0,
                                          "import_view": view(), "import_remainder": zero_remainder()}),
            ),
        ] {
            doc["import_remainder"] = bad_group.clone();
            mark(&mut doc, surface);
            assert_unreadable_at(&doc, "/import_remainder", Some(&bad_group));
            let mut doc2 = match surface {
                JsonSurface::Imports => json!({"file": "a.cc", "imports": [], "count": 0,
                    "import_view": view(), "import_remainder": "0"}),
                _ => {
                    let mut d = if surface == JsonSurface::Cycles {
                        cycles_doc()
                    } else {
                        modules_list_doc()
                    };
                    d["import_remainder"] = json!("0");
                    d
                }
            };
            mark(&mut doc2, surface);
            assert_unreadable_at(&doc2, "/import_remainder", Some(&json!("0")));
        }
    }

    #[test]
    fn json_boundary_marks_a_wrong_typed_excluded_cycle_list_unreadable() {
        let bad = json!({"members": ["a", "b"]});
        let mut doc = cycles_doc();
        doc["excluded_cycles"] = bad.clone();
        mark(&mut doc, JsonSurface::Cycles);
        assert_unreadable_at(&doc, "/excluded_cycles", Some(&bad));
        // orient's IMPORT_CYCLES and explain's EXPLAIN_CYCLES evidence, wherever they sit.
        let bad = json!([{"members": [1, 2], "flags": ["include_tests"], "contains_shown": []}]);
        let mut ev = orient_cycle_evidence();
        ev["excluded_cycles"] = bad.clone();
        let mut doc = orient_doc(ev);
        mark(&mut doc, JsonSurface::Orient);
        assert_unreadable_at(
            &doc,
            "/value/signals/0/value/evidence/excluded_cycles",
            Some(&bad),
        );
        let mut ev = orient_cycle_evidence();
        ev.as_object_mut().unwrap().remove("cycles");
        ev["items"] = json!([]);
        ev["excluded_cycles"] = json!("none");
        let mut doc =
            json!({"value": {"signals": [{"value": {"code": "EXPLAIN_CYCLES", "evidence": ev}}]}});
        mark(&mut doc, JsonSurface::Explain);
        assert_unreadable_at(
            &doc,
            "/value/signals/0/value/evidence/excluded_cycles",
            Some(&json!("none")),
        );
    }

    #[test]
    fn json_boundary_marks_a_wrong_typed_or_non_numeric_importer_block_unreadable() {
        for bad in [
            json!("0"),
            json!({"count": "3", "paths": [], "universe": "cross_module_importers",
                   "universe_count": 5, "unknown_count": 0}),
        ] {
            let mut doc = modules_list_doc();
            doc["importer_test_status_undetermined"] = bad.clone();
            mark(&mut doc, JsonSurface::ModulesList);
            assert_unreadable_at(&doc, "/importer_test_status_undetermined", Some(&bad));
        }
    }

    #[test]
    fn json_boundary_marks_a_wrong_typed_or_non_numeric_not_judged_value_unreadable() {
        let bad = json!({"count": "x", "files": []});
        let mut gate = json!({"obligations": [
            {"method": "arch_violations", "evidence": {"violation_count": 0,
                "inferred_imports_not_judged": bad}},
            {"method": "hotspot_threshold", "evidence": {}}
        ]});
        mark(&mut gate, JsonSurface::Gate);
        assert_unreadable_at(
            &gate,
            "/obligations/0/evidence/inferred_imports_not_judged",
            Some(&bad),
        );
        let bad = json!({"declared": [], "discovered": {"count": -1, "relations": []}});
        let mut v = json!({"count": 0, "inferred_imports_not_judged": bad});
        mark(&mut v, JsonSurface::Violations);
        assert_unreadable_at(&v, "/inferred_imports_not_judged", Some(&bad));
        let bad = json!({"count": 1, "relations": {"source": "db"}});
        let mut mv = json!({"count": 0, "inferred_imports_not_judged": bad});
        mark(&mut mv, JsonSurface::ModulesViolations);
        assert_unreadable_at(&mv, "/inferred_imports_not_judged", Some(&bad));
    }

    #[test]
    fn json_boundary_marks_a_wrong_typed_or_non_numeric_excluded_connectivity_unreadable() {
        for bad in [
            json!(5),
            json!({"tests": "x", "inferred": 0, "tests_and_inferred": 0}),
            json!({"tests": 0, "inferred": 0}),
        ] {
            let mut doc = json!({"value": {"modules": {"value": [
                {"qualified_name": "core", "suspicious_zero_connectivity": false,
                 "excluded_connectivity": {"tests": 0, "inferred": 0, "tests_and_inferred": 0}},
                {"qualified_name": "sys", "suspicious_zero_connectivity": true,
                 "excluded_connectivity": bad}
            ]}}});
            mark(&mut doc, JsonSurface::Trust);
            assert_unreadable_at(
                &doc,
                "/value/modules/value/1/excluded_connectivity",
                Some(&bad),
            );
            assert!(doc
                .pointer("/value/modules/value/0/excluded_connectivity")
                .unwrap()
                .is_object());
        }
    }

    #[test]
    fn json_boundary_marks_a_wrong_typed_or_non_numeric_item_partitions_unreadable() {
        for bad in [
            json!("4"),
            json!({"production_certain": "4", "test_certain": 0,
                                       "production_inferred": 0, "test_inferred": 0,
                                       "unknown_test_status": 0}),
            Value::Null,
        ] {
            let mut doc = modules_list_doc();
            doc["edges"][0]["partitions"] = bad.clone();
            mark(&mut doc, JsonSurface::ModulesList);
            assert_unreadable_at(&doc, "/edges/0/partitions", Some(&bad));
        }
        // null beside a reason is readable (the producer's stated unknown).
        let mut doc = cycles_doc();
        mark(&mut doc, JsonSurface::Cycles);
        assert_eq!(status(&doc)["state"], "stated", "{doc}");
        // orient's cycle items too.
        let mut ev = orient_cycle_evidence();
        ev["cycles"] = json!([{"length": 2, "modules": ["a", "b"], "partitions": 7}]);
        let mut doc = orient_doc(ev);
        mark(&mut doc, JsonSurface::Orient);
        assert_unreadable_at(
            &doc,
            "/value/signals/0/value/evidence/cycles/0/partitions",
            Some(&json!(7)),
        );
    }

    #[test]
    fn json_boundary_marks_a_partial_payload_unreadable() {
        for key in [
            "import_remainder",
            "excluded_cycles",
            "importer_test_status_undetermined",
        ] {
            let mut doc = cycles_doc();
            doc.as_object_mut().unwrap().remove(key);
            mark(&mut doc, JsonSurface::Cycles);
            let pointer = format!("/{key}");
            assert_unreadable_at(&doc, &pointer, None);
            assert!(
                doc.get(key).is_some_and(Value::is_null),
                "inserted as null: {doc}"
            );
        }
    }

    #[test]
    fn json_boundary_states_unavailable_for_a_payload_without_partition_evidence() {
        let older =
            json!({"edges": [{"source": "Foundation", "target": "CppUnit", "import_count": 415}]});
        for (surface, doc) in [
            (JsonSurface::ModulesList, older.clone()),
            (JsonSurface::Cycles, json!({"cycles": [], "count": 0})),
            (
                JsonSurface::Imports,
                json!({"file": "a", "imports": [], "count": 0}),
            ),
            (
                JsonSurface::Orient,
                orient_doc(json!({"cycle_count": 1, "cycles": []})),
            ),
            (
                JsonSurface::Trust,
                json!({"modules": [{"qualified_name": "a",
                                                     "suspicious_zero_connectivity": true}]}),
            ),
            (
                JsonSurface::Gate,
                json!({"obligations": [{"method": "arch_violations",
                                                        "evidence": {"violation_count": 0}}]}),
            ),
            (JsonSurface::Violations, json!({"count": 0})),
            (JsonSurface::ModulesViolations, json!({"count": 0})),
        ] {
            let mut marked = doc.clone();
            mark(&mut marked, surface);
            assert_eq!(
                status(&marked),
                &json!({"state": "unavailable", "entries": []}),
                "{surface:?}"
            );
            marked
                .as_object_mut()
                .unwrap()
                .remove(PARTITION_EVIDENCE_STATUS);
            assert_eq!(
                marked, doc,
                "{surface:?}: the document is otherwise unchanged"
            );
        }
    }

    #[test]
    fn json_boundary_leaves_well_formed_evidence_unchanged_and_states_it() {
        let nonzero = json!({"tests": {"imports": 89, "edges": 3},
                             "inferred": {"imports": 0, "edges": 0},
                             "tests_and_inferred": {"imports": 0, "edges": 0}});
        let mut with_counts = modules_list_doc();
        with_counts["import_remainder"] = nonzero;
        for (surface, doc) in [
            (JsonSurface::ModulesList, modules_list_doc()),
            (JsonSurface::ModulesList, with_counts),
            (JsonSurface::Cycles, cycles_doc()),
            (
                JsonSurface::Imports,
                json!({"file": "a", "imports": [], "count": 0,
                                          "import_view": view(), "import_remainder": zero_remainder()}),
            ),
            (JsonSurface::Orient, orient_doc(orient_cycle_evidence())),
            (
                JsonSurface::Trust,
                json!({"modules": [{"qualified_name": "a",
                "suspicious_zero_connectivity": true,
                "excluded_connectivity": {"tests": 0, "inferred": 0, "tests_and_inferred": 0}}]}),
            ),
            (
                JsonSurface::Gate,
                json!({"obligations": [{"method": "module_violations",
                "evidence": {"inferred_imports_not_judged": {"count": 0, "relations": []}}}]}),
            ),
            (
                JsonSurface::Violations,
                json!({"inferred_imports_not_judged": {
                "declared": [], "discovered": {"count": 0, "relations": []}}}),
            ),
            (
                JsonSurface::ModulesViolations,
                json!({"inferred_imports_not_judged":
                {"count": 0, "relations": []}}),
            ),
        ] {
            let mut marked = doc.clone();
            mark(&mut marked, surface);
            assert_eq!(
                status(&marked),
                &json!({"state": "stated", "entries": []}),
                "{surface:?}: {marked}"
            );
            marked
                .as_object_mut()
                .unwrap()
                .remove(PARTITION_EVIDENCE_STATUS);
            assert_eq!(marked, doc, "{surface:?}: only the status key is added");
        }
    }

    // ── D-TESB-17 INPUT-3: the status is derived from positive validation ────────────────────

    fn assert_entry(doc: &Value, pointer: &str, reason_part: &str) {
        let e = entry_at(doc, pointer);
        assert_eq!(e["state"], "unreadable", "{doc}");
        assert!(
            e["reason"]
                .as_str()
                .is_some_and(|r| r.contains(reason_part)),
            "{pointer}: reason {:?} lacks {reason_part:?}",
            e["reason"]
        );
    }

    fn signals_doc(code: &str, evidence: Option<Value>) -> Value {
        let mut leaf = json!({"code": code});
        if let Some(ev) = evidence {
            leaf["evidence"] = ev;
        }
        json!({"value": {"signals": [{"value": leaf, "provenance": {"source": ["sqlite"]}}]}})
    }

    fn explain_cycle_evidence() -> Value {
        let mut ev = orient_cycle_evidence();
        ev.as_object_mut().unwrap().remove("cycles");
        ev["items"] = json!([]);
        ev
    }

    fn top_block() -> Value {
        json!({"edges": [], "import_view": view(), "import_remainder": zero_remainder(),
               "importer_test_status_undetermined": zero_block("cross_module_importers")})
    }

    /// The derivation table, row by row, on the tallies themselves (the fail-closed row cannot be
    /// reached through a document while every carrier records an outcome — it is the net under a
    /// guard written later that returns early).
    #[test]
    fn partition_evidence_status_is_stated_only_on_positive_validation() {
        // zero carriers under validated containers: stated.
        let m = Marks::default();
        assert_eq!(m.status(), json!({"state": "stated", "entries": []}));
        // every carrier validated: stated.
        let mut m = Marks::default();
        m.found();
        m.record(CarrierOutcome::Stated);
        assert_eq!(m.status(), json!({"state": "stated", "entries": []}));
        // every carrier the older-daemon shape: unavailable.
        let mut m = Marks::default();
        for p in ["/a", "/b"] {
            m.found();
            m.record(CarrierOutcome::Older(p.to_string()));
        }
        assert_eq!(m.status(), json!({"state": "unavailable", "entries": []}));
        // older carriers beside a stating carrier: unreadable, one entry per older carrier.
        let mut m = Marks::default();
        m.found();
        m.record(CarrierOutcome::Stated);
        m.found();
        m.record(CarrierOutcome::Older("/rows/1".to_string()));
        let st = m.status();
        assert_eq!(st["state"], "unreadable");
        assert_eq!(st["entries"].as_array().unwrap().len(), 1);
        assert_eq!(st["entries"][0]["pointer"], "/rows/1");
        assert_eq!(
            st["entries"][0]["reason"],
            "carries no partition evidence beside carriers that do"
        );
        // a found carrier with no recorded outcome: the fail-closed entry, never stated.
        let mut m = Marks::default();
        m.found();
        m.record(CarrierOutcome::Stated);
        m.found();
        let st = m.status();
        assert_eq!(
            st,
            json!({"state": "unreadable", "entries": [{"pointer": "", "state": "unreadable",
                "reason": "partition evidence not positively validated: 1 of 2 carriers"}]})
        );
        // likewise when the only carrier recorded nothing (never unavailable, never stated).
        let mut m = Marks::default();
        m.found();
        assert_eq!(m.status()["state"], "unreadable");
        // an unreadable carrier: unreadable.
        let mut doc = modules_list_doc();
        doc["import_remainder"] = json!(3);
        mark(&mut doc, JsonSurface::ModulesList);
        assert_eq!(status(&doc)["state"], "unreadable");
        // an unreadable carrier (it carries the marker) beside an older one names the older one.
        let mut doc = json!({"value": {"modules": {"value": [
            {"qualified_name": "a", "excluded_connectivity": "x"},
            {"qualified_name": "b"}
        ]}}});
        mark(&mut doc, JsonSurface::Trust);
        assert_unreadable_at(
            &doc,
            "/value/modules/value/0/excluded_connectivity",
            Some(&json!("x")),
        );
        assert_entry(&doc, "/value/modules/value/1", "beside carriers that do");
    }

    #[test]
    fn json_boundary_marks_a_judged_gate_obligation_without_object_evidence_unreadable() {
        for method in ["arch_violations", "module_violations"] {
            for bad in [None, Some(json!("x")), Some(json!([1])), Some(Value::Null)] {
                let mut obl = json!({"method": method});
                if let Some(b) = &bad {
                    obl["evidence"] = b.clone();
                }
                let mut doc = json!({"obligations": [obl,
                    {"method": "arch_violations",
                     "evidence": {"inferred_imports_not_judged": {"count": 0, "files": []}}}]});
                mark(&mut doc, JsonSurface::Gate);
                assert_unreadable_at(&doc, "/obligations/0/evidence", bad.as_ref());
                // the well-formed sibling is unchanged.
                assert_eq!(
                    doc["obligations"][1]["evidence"]["inferred_imports_not_judged"],
                    json!({"count": 0, "files": []})
                );
            }
        }
    }

    #[test]
    fn json_boundary_marks_cycle_signal_evidence_that_is_missing_or_not_an_object_unreadable() {
        for (surface, code) in [
            (JsonSurface::Orient, "IMPORT_CYCLES"),
            (JsonSurface::Explain, "EXPLAIN_CYCLES"),
        ] {
            for bad in [None, Some(json!("x")), Some(json!([])), Some(Value::Null)] {
                let mut doc = signals_doc(code, bad.clone());
                mark(&mut doc, surface);
                assert_unreadable_at(&doc, "/value/signals/0/value/evidence", bad.as_ref());
            }
        }
    }

    #[test]
    fn json_boundary_marks_a_view_item_collection_missing_or_not_an_array_unreadable() {
        let deps = || {
            let mut d = modules_list_doc();
            d["results"] = d["edges"].take();
            d.as_object_mut().unwrap().remove("edges");
            d
        };
        let mut orient_top = signals_doc("IMPORT_CYCLES", Some(orient_cycle_evidence()));
        orient_top["value"]["top_module_edges"] = top_block();
        let cases: Vec<(JsonSurface, Value, &str)> = vec![
            (JsonSurface::ModulesList, modules_list_doc(), "/edges"),
            (JsonSurface::ModulesDeps, deps(), "/results"),
            (JsonSurface::Cycles, cycles_doc(), "/cycles"),
            (
                JsonSurface::Orient,
                signals_doc("IMPORT_CYCLES", Some(orient_cycle_evidence())),
                "/value/signals/0/value/evidence/cycles",
            ),
            (
                JsonSurface::Orient,
                orient_top,
                "/value/top_module_edges/edges",
            ),
            (
                JsonSurface::Explain,
                signals_doc("EXPLAIN_CYCLES", Some(explain_cycle_evidence())),
                "/value/signals/0/value/evidence/items",
            ),
        ];
        for (surface, doc, pointer) in cases {
            let (parent, key) = pointer.rsplit_once('/').unwrap();
            for bad in [None, Some(json!({"a": 1})), Some(json!("x"))] {
                let mut d = doc.clone();
                let obj = d.pointer_mut(parent).unwrap().as_object_mut().unwrap();
                match &bad {
                    None => {
                        obj.remove(key);
                    }
                    Some(b) => {
                        obj.insert(key.to_string(), b.clone());
                    }
                }
                mark(&mut d, surface);
                assert_unreadable_at(&d, pointer, bad.as_ref());
            }
        }
    }

    #[test]
    fn json_boundary_marks_a_non_object_view_item_unreadable() {
        for (surface, mut doc, pointer) in [
            (JsonSurface::ModulesList, modules_list_doc(), "/edges/1"),
            (JsonSurface::Cycles, cycles_doc(), "/cycles/1"),
        ] {
            let key = &pointer[1..pointer.rfind('/').unwrap()];
            doc[key].as_array_mut().unwrap().push(json!(5));
            mark(&mut doc, surface);
            assert_unreadable_at(&doc, pointer, Some(&json!(5)));
            assert_entry(&doc, pointer, "not an object");
        }
        let mut ev = orient_cycle_evidence();
        ev["cycles"] = json!(["a"]);
        let mut doc = signals_doc("IMPORT_CYCLES", Some(ev));
        mark(&mut doc, JsonSurface::Orient);
        assert_unreadable_at(
            &doc,
            "/value/signals/0/value/evidence/cycles/0",
            Some(&json!("a")),
        );
    }

    #[test]
    fn json_boundary_refuses_a_non_object_answer_and_marks_a_non_object_value_holder_unreadable() {
        let all = [
            JsonSurface::ModulesList,
            JsonSurface::ModulesDeps,
            JsonSurface::Cycles,
            JsonSurface::Imports,
            JsonSurface::Orient,
            JsonSurface::Explain,
            JsonSurface::Trust,
            JsonSurface::Gate,
            JsonSurface::Violations,
            JsonSurface::ModulesViolations,
        ];
        for surface in all {
            for answer in [json!([1]), json!("x"), json!(3), Value::Null] {
                let mut d = answer.clone();
                let err = mark_partition_evidence(&mut d, surface)
                    .expect_err("a non-object answer is a named error");
                assert!(err.contains("not a JSON object"), "{surface:?}: {err}");
                assert_eq!(d, answer, "{surface:?}: a refused answer is not modified");
            }
            let mut d = json!({"value": "x"});
            mark(&mut d, surface);
            assert_unreadable_at(&d, "/value", Some(&json!("x")));
        }
    }

    #[test]
    fn json_boundary_marks_a_missing_or_non_array_carrier_container_unreadable() {
        let cases: Vec<(JsonSurface, Value, &str, Option<Value>)> = vec![
            (
                JsonSurface::Orient,
                json!({"value": {}}),
                "/value/signals",
                None,
            ),
            (
                JsonSurface::Orient,
                json!({"value": {"signals": {"a": 1}}}),
                "/value/signals",
                Some(json!({"a": 1})),
            ),
            (
                JsonSurface::Explain,
                json!({"value": {}}),
                "/value/signals",
                None,
            ),
            (
                JsonSurface::Explain,
                json!({"value": {"signals": "x"}}),
                "/value/signals",
                Some(json!("x")),
            ),
            (
                JsonSurface::Trust,
                json!({"value": {}}),
                "/value/modules",
                None,
            ),
            (
                JsonSurface::Trust,
                json!({"value": {"modules": {"value": "x"}}}),
                "/value/modules/value",
                Some(json!("x")),
            ),
            (
                JsonSurface::Trust,
                json!({"value": {"modules": 7}}),
                "/value/modules",
                Some(json!(7)),
            ),
            (
                JsonSurface::Gate,
                json!({"outcome": {}}),
                "/obligations",
                None,
            ),
            (
                JsonSurface::Gate,
                json!({"obligations": {"a": 1}}),
                "/obligations",
                Some(json!({"a": 1})),
            ),
        ];
        for (surface, mut doc, pointer, received) in cases {
            mark(&mut doc, surface);
            assert_eq!(status(&doc)["state"], "unreadable", "{surface:?}: {doc}");
            let e = entry_at(&doc, pointer);
            match &received {
                Some(v) => {
                    assert_eq!(&e["received"], v, "{surface:?}");
                    assert!(doc.pointer(pointer).is_some_and(Value::is_null), "{doc}");
                }
                None => assert!(e.get("received").is_none(), "{surface:?}"),
            }
        }
        // a non-object trust row or gate obligation is an entry too.
        let mut doc = json!({"value": {"modules": {"value": [3]}}});
        mark(&mut doc, JsonSurface::Trust);
        assert_unreadable_at(&doc, "/value/modules/value/0", Some(&json!(3)));
        let mut doc = json!({"obligations": ["x"]});
        mark(&mut doc, JsonSurface::Gate);
        assert_unreadable_at(&doc, "/obligations/0", Some(&json!("x")));
    }

    #[test]
    fn json_boundary_marks_a_daemon_stated_unavailable_module_edges_block_unreadable_and_keeps_it()
    {
        let block = json!({"unavailable": "module graph read failed: disk I/O error"});
        let mut doc = signals_doc("IMPORT_CYCLES", Some(orient_cycle_evidence()));
        doc["value"]["top_module_edges"] = block.clone();
        mark(&mut doc, JsonSurface::Orient);
        assert_eq!(status(&doc)["state"], "unreadable", "{doc}");
        assert_entry(&doc, "/value/top_module_edges", "disk I/O error");
        assert_eq!(doc["value"]["top_module_edges"], block, "the block is kept");
        // the stated cycle evidence beside it is unchanged.
        assert_eq!(
            doc["value"]["signals"][0]["value"]["evidence"],
            orient_cycle_evidence()
        );
    }

    #[test]
    fn json_boundary_names_each_carrier_without_partition_evidence_in_a_partial_document() {
        // trust: one row states the partition, two do not.
        let rows = json!([
            {"qualified_name": "a", "excluded_connectivity":
                {"tests": 0, "inferred": 0, "tests_and_inferred": 0}},
            {"qualified_name": "b"},
            {"qualified_name": "c", "suspicious_zero_connectivity": true}
        ]);
        let mut doc = json!({"value": {"modules": {"value": rows.clone()}}});
        mark(&mut doc, JsonSurface::Trust);
        assert_eq!(status(&doc)["state"], "unreadable");
        assert_eq!(status(&doc)["entries"].as_array().unwrap().len(), 2);
        assert_entry(&doc, "/value/modules/value/1", "beside carriers that do");
        assert_entry(&doc, "/value/modules/value/2", "beside carriers that do");
        assert_eq!(
            doc["value"]["modules"]["value"], rows,
            "older rows are kept"
        );
        // gate: one judged obligation states it, one does not.
        let mut doc = json!({"obligations": [
            {"method": "arch_violations",
             "evidence": {"inferred_imports_not_judged": {"count": 0, "files": []}}},
            {"method": "module_violations", "evidence": {"violations_count": 0}}]});
        mark(&mut doc, JsonSurface::Gate);
        assert_eq!(status(&doc)["entries"].as_array().unwrap().len(), 1);
        assert_entry(&doc, "/obligations/1/evidence", "beside carriers that do");
        // orient: the cycle evidence states it, the module-edges block does not.
        let mut doc = signals_doc("IMPORT_CYCLES", Some(orient_cycle_evidence()));
        doc["value"]["top_module_edges"] = json!({"edges": []});
        mark(&mut doc, JsonSurface::Orient);
        assert_eq!(status(&doc)["entries"].as_array().unwrap().len(), 1);
        assert_entry(&doc, "/value/top_module_edges", "beside carriers that do");
    }

    #[test]
    fn json_boundary_states_stated_for_a_valid_container_that_requires_no_carrier() {
        for (surface, mut doc) in [
            (
                JsonSurface::Orient,
                signals_doc("HIGH_COMPLEXITY", Some(json!({"items": []}))),
            ),
            (JsonSurface::Orient, json!({"value": {"signals": []}})),
            (JsonSurface::Explain, json!({"value": {"signals": []}})),
            (
                JsonSurface::Trust,
                json!({"value": {"modules": {"value": []}}}),
            ),
            (JsonSurface::Gate, json!({"obligations": []})),
            (
                JsonSurface::Gate,
                json!({"obligations": [{"method": "hotspot_threshold", "evidence": {}}]}),
            ),
        ] {
            let before = doc.clone();
            mark(&mut doc, surface);
            assert_eq!(
                status(&doc),
                &json!({"state": "stated", "entries": []}),
                "{surface:?}: {doc}"
            );
            doc.as_object_mut()
                .unwrap()
                .remove(PARTITION_EVIDENCE_STATUS);
            assert_eq!(doc, before, "{surface:?}");
        }
    }

    /// The wiring guard: each of the ten command functions calls `mark_partition_evidence` on its
    /// JSON path (a source guard over the command files; its limit: it proves the call is in the
    /// function body, not which branch — each call sits in the `json_mode` branch by review).
    #[test]
    fn every_partition_surface_json_path_marks_partition_evidence() {
        let files: [(&str, &str); 6] = [
            (
                "commands/modules/list.rs",
                include_str!("../commands/modules/list.rs"),
            ),
            (
                "commands/modules/deps.rs",
                include_str!("../commands/modules/deps.rs"),
            ),
            (
                "commands/modules/violations.rs",
                include_str!("../commands/modules/violations.rs"),
            ),
            ("commands/graph.rs", include_str!("../commands/graph.rs")),
            ("commands/orient.rs", include_str!("../commands/orient.rs")),
            ("commands/trust.rs", include_str!("../commands/trust.rs")),
        ];
        let gate = ("commands/gate.rs", include_str!("../commands/gate.rs"));
        let body_of = |src: &str, func: &str| -> Option<String> {
            let start = src.find(&format!("fn {func}("))?;
            let open = start + src[start..].find('{')?;
            let mut depth = 0i64;
            for (i, c) in src[open..].char_indices() {
                match c {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(src[open..open + i].to_string());
                        }
                    }
                    _ => {}
                }
            }
            None
        };
        let expected = [
            (
                "commands/modules/list.rs",
                "run_modules_list",
                "ModulesList",
            ),
            (
                "commands/modules/deps.rs",
                "run_modules_deps",
                "ModulesDeps",
            ),
            ("commands/graph.rs", "run_cycles", "Cycles"),
            ("commands/graph.rs", "run_imports", "Imports"),
            ("commands/orient.rs", "run_orient", "Orient"),
            ("commands/orient.rs", "run_explain_cmd", "Explain"),
            ("commands/trust.rs", "run_trust", "Trust"),
            ("commands/gate.rs", "run_gate", "Gate"),
            (
                "commands/modules/violations.rs",
                "run_violations",
                "Violations",
            ),
            (
                "commands/modules/violations.rs",
                "run_modules_violations",
                "ModulesViolations",
            ),
        ];
        let mut missing = Vec::new();
        let mut discards = Vec::new();
        for (file, func, surface) in expected {
            let src = files
                .iter()
                .chain(std::iter::once(&gate))
                .find(|(f, _)| *f == file)
                .map(|(_, s)| *s)
                .unwrap();
            let body = body_of(src, func).unwrap_or_default();
            if !(body.contains("mark_partition_evidence(")
                && body.contains(&format!("JsonSurface::{surface}")))
            {
                missing.push(format!("{file}::{func} ({surface})"));
            }
            // D-TESB-17 INPUT-3: the result is handled — a refused (non-object) answer is a
            // named runtime error, never discarded into a document without its status.
            for (at, _) in body.match_indices("mark_partition_evidence(") {
                let before = &body[at.saturating_sub(80)..at];
                let stmt_end = body[at..].find(';').map_or(body.len(), |e| at + e);
                let stmt = &body[at..stmt_end];
                let discarded = before
                    .rsplit(';')
                    .next()
                    .is_some_and(|lead| lead.contains("let _"))
                    || stmt.contains(".ok()")
                    || stmt.contains(".unwrap_or")
                    || !(stmt.contains("Err(")
                        || stmt.contains("if let Err")
                        || before.contains("if let Err")
                        || before.contains("match "));
                if discarded {
                    discards.push(format!("{file}::{func} ({surface})"));
                }
            }
        }
        assert!(
            missing.is_empty(),
            "JSON paths that do not mark partition evidence: {missing:?}"
        );
        assert!(
            discards.is_empty(),
            "JSON paths that discard the marker's refusal: {discards:?}"
        );
    }
}
