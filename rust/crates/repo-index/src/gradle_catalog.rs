//! Gradle version-catalog aliases (DEPS-GRADLE-CATALOG-1B; RG-REQ-006-L13; D-DGC1B-TOML-1;
//! D-DGC1B-ALIAS-MARKING-1; D-DGC1B-ARTIFACT-INTERPOLATION-1; D-DGC1B-ALIAS-APPLICABILITY-1;
//! D-DGC1B-MAP-GRAMMAR-1).
//!
//! Crate-private module, split from `config.rs` and `manifest_deps.rs` (both over the 500-line
//! structural guardrail): catalog reading is a new responsibility. It owns four things:
//!
//! 1. [`AliasMiner`] — finds the alias references (`libs.<alias>` / `libraries.<alias>`) that stand
//!    in a configuration argument position of ONE direct `dependencies` block. The brace walk of
//!    `config::extract_gradle_dependencies` feeds it the block's own-depth characters.
//! 2. [`script_statement`] — recognises the two script statements the binding needs: `apply from:`
//!    (a possible Groovy alias-map source) and `libraries = libs` (the catalog rename). The reader
//!    records each with its line and enclosing block heads; [`statement_scope`] turns those heads
//!    into the scope the statement sits in (`ext { }` frames transparent).
//! 3. The catalog sources of a build: the TOML `[libraries]` table of
//!    `<build dir>/gradle/libs.versions.toml` (the whole build), and the `libs` map of every script
//!    an `apply from:` of the build names (the projects that statement's scope reaches), read only
//!    under the closed grammar of D-DGC1B-MAP-GRAMMAR-1 ([`evaluate_alias_map`]).
//! 4. [`bind_build_aliases`] — the ONE BINDING RULE: a reference binds to a group for project P only
//!    when every source that applies to P was read, every Groovy map that applies to P is readable,
//!    exactly one group answers its accessor (a `libraries.` accessor through a rename that applies
//!    to P), and no `apply from:` not shown to apply to P could change the answer; every other
//!    reference is counted, with the first counted site and the predicate that failed for it.
//!
//! Users: `config.rs` (1, 2 — the reader), `manifest_deps.rs` (3, 4 — at the build).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::path::Path;

use crate::config::{
    coordinate_group, is_coordinate_segment, project_block_path, strip_gradle_comments,
    DirectScope, GradleScriptScopes,
};
use crate::manifest_deps::{is_gradle_ancestor, resolve_gradle_path, UnresolvedAliasRefs};

/// One alias reference of a direct `dependencies` block: the scope of its block, the accessor token
/// as written (`libs.zstd`, `libraries.mockito.core`) and its 1-based line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GradleAliasRef {
    pub(crate) scope: DirectScope,
    pub(crate) accessor: String,
    pub(crate) line: u32,
}

/// One `apply from:` statement of a script: its argument as written, its 1-based line and the
/// heads of its enclosing blocks, outermost first (empty = the script's top level).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GradleApplyFrom {
    pub(crate) argument: String,
    pub(crate) line: u32,
    pub(crate) enclosing: Vec<String>,
}

/// One `libraries = libs` statement of a script: its 1-based line and the heads of its enclosing
/// blocks, outermost first (empty = the script's top level).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GradleRename {
    pub(crate) line: u32,
    pub(crate) enclosing: Vec<String>,
}

/// A script statement the alias binding needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ScriptStatement {
    /// `apply from: <argument>`.
    ApplyFrom(String),
    /// `libraries = libs` — `libraries.` accessors resolve through `libs`.
    LibrariesIsLibs,
}

/// Recognise one statement segment of a script (the text between two of `\n`, `;`, `{`, `}`).
pub(crate) fn script_statement(segment: &str) -> Option<ScriptStatement> {
    let s = segment.trim();
    let compact: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if compact == "libraries=libs" {
        return Some(ScriptStatement::LibrariesIsLibs);
    }
    let rest = s.strip_prefix("apply")?;
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let rest = rest.trim_start().strip_prefix("from")?.trim_start();
    let argument = rest.strip_prefix(':')?.trim();
    (!argument.is_empty()).then(|| ScriptStatement::ApplyFrom(argument.to_string()))
}

/// The scope a rename or `apply from:` statement sits in, from its enclosing heads by the 1A brace
/// walk with every `ext { }` frame transparent (it configures the enclosing project's extra
/// properties; grpc-java's rename sits in `subprojects { ext { … } }`). D-DGC1B-ALIAS-APPLICABILITY-1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StatementScope {
    /// The script's top level.
    TopLevel,
    /// Directly inside the script's top-level `buildscript { }`.
    Buildscript,
    /// Directly inside a top-level `allprojects { }`.
    AllProjects,
    /// Directly inside a top-level `subprojects { }`.
    SubProjects,
    /// Directly inside a top-level `project('<path>') { }` (the path as written).
    Project(String),
    /// Anywhere else: a condition, a callback, a task or any other closure, or a scope head below
    /// the top level.
    NotApplicable,
}

/// The [`StatementScope`] of a statement whose enclosing heads are `enclosing` (outermost first).
pub(crate) fn statement_scope(enclosing: &[String]) -> StatementScope {
    let heads: Vec<&str> = enclosing
        .iter()
        .map(String::as_str)
        .filter(|h| *h != "ext")
        .collect();
    match heads.as_slice() {
        [] => StatementScope::TopLevel,
        ["buildscript"] => StatementScope::Buildscript,
        ["allprojects"] => StatementScope::AllProjects,
        ["subprojects"] => StatementScope::SubProjects,
        [h] => project_block_path(h).map_or(StatementScope::NotApplicable, StatementScope::Project),
        _ => StatementScope::NotApplicable,
    }
}

// ── 1. Alias position (section 2.1 item 1) ───────────────────────────────────────────────────────

/// Mines the alias references of ONE direct `dependencies` block. It is fed only the characters at
/// the block's own brace depth (never the text of a closure nested in it) and is told when a
/// nested closure opens; it splits that text into statements and keeps the accessor tokens that
/// stand in a configuration argument position.
///
/// A statement ends at a `;` outside parentheses, at a nested closure, or at a newline — unless the
/// statement's line ends with `,` or an argument parenthesis is still open (a continuation line).
pub(crate) struct AliasMiner {
    scope: DirectScope,
    stmt: Vec<(char, u32)>,
    paren_depth: u32,
    quote: Option<char>,
    escape: bool,
    refs: Vec<GradleAliasRef>,
}

impl AliasMiner {
    pub(crate) fn new(scope: DirectScope) -> Self {
        Self {
            scope,
            stmt: Vec::new(),
            paren_depth: 0,
            quote: None,
            escape: false,
            refs: Vec::new(),
        }
    }

    /// Feed one own-depth character on 1-based line `line`.
    pub(crate) fn feed(&mut self, c: char, line: u32) {
        if let Some(q) = self.quote {
            if self.escape {
                self.escape = false;
            } else if c == '\\' {
                self.escape = true;
            } else if c == q {
                self.quote = None;
            }
            self.stmt.push((c, line));
            return;
        }
        match c {
            '\n' => {
                let ends_with_comma = self
                    .stmt
                    .iter()
                    .rev()
                    .find(|(ch, _)| !ch.is_whitespace())
                    .is_some_and(|(ch, _)| *ch == ',');
                if self.paren_depth > 0 || ends_with_comma {
                    self.stmt.push((' ', line));
                } else {
                    self.end_statement();
                }
            }
            ';' if self.paren_depth == 0 => self.end_statement(),
            '\'' | '"' => {
                self.quote = Some(c);
                self.stmt.push((c, line));
            }
            '(' => {
                self.paren_depth += 1;
                self.stmt.push((c, line));
            }
            ')' => {
                self.paren_depth = self.paren_depth.saturating_sub(1);
                self.stmt.push((c, line));
            }
            _ => self.stmt.push((c, line)),
        }
    }

    /// A closure nested in the block opens: the current statement's argument list ends here.
    pub(crate) fn closure(&mut self) {
        self.end_statement();
    }

    /// The block closed: the alias references found, in order.
    pub(crate) fn finish(mut self) -> Vec<GradleAliasRef> {
        self.end_statement();
        self.refs
    }

    fn end_statement(&mut self) {
        let stmt = std::mem::take(&mut self.stmt);
        self.paren_depth = 0;
        self.quote = None;
        self.escape = false;
        for (accessor, line) in configuration_argument_accessors(&stmt) {
            self.refs.push(GradleAliasRef {
                scope: self.scope.clone(),
                accessor,
                line,
            });
        }
    }
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '$'
}

/// The accessor tokens of one statement that stand in a configuration argument position: outside
/// every string literal, in the argument list of the statement's leading word — unparenthesised,
/// inside the leading word's own parentheses, or inside `platform(…)` / `enforcedPlatform(…)`
/// there. A statement that assigns (`=` at its top level) yields none; a token that is itself the
/// statement's leading word is not an argument.
fn configuration_argument_accessors(stmt: &[(char, u32)]) -> Vec<(String, u32)> {
    let chars: Vec<char> = stmt.iter().map(|(c, _)| *c).collect();
    let n = chars.len();
    let mut i = 0;
    while i < n && chars[i].is_whitespace() {
        i += 1;
    }
    let lead_start = i;
    while i < n && (is_ident_char(chars[i]) || chars[i] == '.') {
        i += 1;
    }
    if i == lead_start {
        return Vec::new();
    }
    if assigns_at_top_level(&chars) {
        return Vec::new();
    }
    let mut j = i;
    while j < n && chars[j].is_whitespace() {
        j += 1;
    }
    let verb_parens = j < n && chars[j] == '(';

    // Paren frames after the leading word: `true` = an admitted frame (the verb's own parentheses,
    // or `platform` / `enforcedPlatform`), `false` = any other call.
    let mut frames: Vec<bool> = Vec::new();
    let mut quote: Option<char> = None;
    let mut escape = false;
    let mut out = Vec::new();
    let mut k = i;
    while k < n {
        let c = chars[k];
        if let Some(q) = quote {
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == q {
                quote = None;
            }
            k += 1;
            continue;
        }
        match c {
            '\'' | '"' => quote = Some(c),
            '(' => {
                let admitted = if verb_parens && frames.is_empty() && k == j {
                    true
                } else {
                    let name = preceding_word(&chars, k);
                    name == "platform" || name == "enforcedPlatform"
                };
                frames.push(admitted);
            }
            ')' => {
                frames.pop();
            }
            _ => {
                if let Some(len) = accessor_token_len(&chars, k) {
                    let in_position =
                        frames.iter().all(|a| *a) && (!verb_parens || !frames.is_empty());
                    if in_position {
                        let token: String = chars[k..k + len].iter().collect();
                        out.push((token, stmt[k].1));
                    }
                    k += len;
                    continue;
                }
            }
        }
        k += 1;
    }
    out
}

/// True iff `chars` holds an assignment `=` at paren depth 0 outside strings (not `==`, `!=`, `<=`,
/// `>=`, `=~`).
fn assigns_at_top_level(chars: &[char]) -> bool {
    let mut depth = 0u32;
    let mut quote: Option<char> = None;
    let mut escape = false;
    for (k, &c) in chars.iter().enumerate() {
        if let Some(q) = quote {
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '\'' | '"' => quote = Some(c),
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            '=' if depth == 0 => {
                let prev = if k > 0 { chars[k - 1] } else { ' ' };
                let next = chars.get(k + 1).copied().unwrap_or(' ');
                if !matches!(prev, '=' | '!' | '<' | '>') && !matches!(next, '=' | '~') {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// The identifier immediately before position `k` (whitespace skipped).
fn preceding_word(chars: &[char], k: usize) -> String {
    let mut e = k;
    while e > 0 && chars[e - 1].is_whitespace() {
        e -= 1;
    }
    let mut s = e;
    while s > 0 && is_ident_char(chars[s - 1]) {
        s -= 1;
    }
    chars[s..e].iter().collect()
}

/// The length of the accessor token `libs.<alias>` / `libraries.<alias>` starting at `k`, if one
/// starts there on a word boundary (not preceded by an identifier character or a `.`).
fn accessor_token_len(chars: &[char], k: usize) -> Option<usize> {
    if k > 0 && (is_ident_char(chars[k - 1]) || chars[k - 1] == '.') {
        return None;
    }
    let prefix_len = ["libs.", "libraries."].iter().find_map(|p| {
        let pc: Vec<char> = p.chars().collect();
        (chars.len() >= k + pc.len() && chars[k..k + pc.len()] == pc[..]).then_some(pc.len())
    })?;
    let mut e = k + prefix_len;
    while e < chars.len()
        && (chars[e].is_ascii_alphanumeric() || chars[e] == '_' || chars[e] == '.')
    {
        e += 1;
    }
    while e > k + prefix_len && chars[e - 1] == '.' {
        e -= 1;
    }
    (e > k + prefix_len).then_some(e - k)
}

/// Gradle's accessor rule: `-` and `_` in a catalog key read as `.` (`mockito-core` ↔
/// `libraries.mockito.core`).
pub(crate) fn normalise_alias_key(key: &str) -> String {
    key.replace(['-', '_'], ".")
}

// ── 2. Catalog sources (section 2.1 item 2) ──────────────────────────────────────────────────────

/// One catalog entry: its key as written, the group it names and the repo-relative catalog file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CatalogEntry {
    key: String,
    group: String,
    catalog: String,
}

/// What reading one catalog source established.
#[derive(Debug, Clone, PartialEq, Eq)]
enum SourceRead {
    /// Read: its catalog entries, and every key its map holds after evaluation (an entry whose
    /// value is not coordinate-shaped is a key without a catalog entry).
    Read {
        entries: Vec<CatalogEntry>,
        keys: Vec<String>,
    },
    /// It could not be read: the predicate `catalog <path> could not be read (<error>)`.
    Unreadable(String),
    /// A Groovy alias map the closed grammar cannot read (D-DGC1B-MAP-GRAMMAR-1): the applied script
    /// and the line of the first statement that uses the map and is not an admitted literal
    /// assignment, in a use the reader cannot prove does not write the map
    /// ([`AliasMap::NotLiteral`]). Such a map could hold any key.
    NotLiteral { script: String, line: u32 },
}

impl SourceRead {
    fn empty() -> Self {
        SourceRead::Read {
            entries: Vec::new(),
            keys: Vec::new(),
        }
    }
}

/// `dir/rel`, or `rel` at the repository root.
fn join_rel(dir: &str, rel: &str) -> String {
    if dir.is_empty() {
        rel.to_string()
    } else {
        format!("{dir}/{rel}")
    }
}

/// The directory of repo-relative `path` (`""` at the root).
fn dir_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(d, _)| d)
}

/// The first line of an error's text.
fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or("").trim().to_string()
}

/// Read the TOML catalog `<build dir>/gradle/libs.versions.toml` (D-DGC1B-TOML-1): absent → no
/// entries; present and parsed → its `[libraries]` entries; unreadable or unparsable → the
/// `could not be read` predicate (never a partial table).
fn read_toml_catalog(repo_root: &Path, build_dir: &str) -> SourceRead {
    let toml_rel = join_rel(build_dir, "gradle/libs.versions.toml");
    match std::fs::read_to_string(repo_root.join(&toml_rel)) {
        Ok(content) => match toml_catalog_entries(&content, &toml_rel) {
            Ok(entries) => {
                let keys = entries.iter().map(|e| e.key.clone()).collect();
                SourceRead::Read { entries, keys }
            }
            Err(error) => {
                SourceRead::Unreadable(format!("catalog {toml_rel} could not be read ({error})"))
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => SourceRead::empty(),
        Err(e) => SourceRead::Unreadable(format!(
            "catalog {toml_rel} could not be read ({})",
            first_line(&e.to_string())
        )),
    }
}

/// Read the script at repo-relative `rel` that an `apply from:` names: its `libs` map under the
/// closed grammar ([`evaluate_alias_map`]) and the `apply from:` statements it holds itself (each
/// applies to no project — D-DGC1B-ALIAS-APPLICABILITY-1 scopes statements of the build's own
/// scripts — so their scripts are read for keys only).
fn read_applied_script(repo_root: &Path, rel: &str) -> (SourceRead, Vec<GradleApplyFrom>) {
    let content = match std::fs::read_to_string(repo_root.join(rel)) {
        Ok(content) => content,
        Err(e) => {
            return (
                SourceRead::Unreadable(format!(
                    "catalog {rel} could not be read ({})",
                    first_line(&e.to_string())
                )),
                Vec::new(),
            )
        }
    };
    let nested = applied_statements(&content);
    let read = match evaluate_alias_map(&content) {
        AliasMap::Malformed(error) => {
            SourceRead::Unreadable(format!("catalog {rel} could not be read ({error})"))
        }
        AliasMap::NotLiteral(line) => SourceRead::NotLiteral {
            script: rel.to_string(),
            line,
        },
        AliasMap::Entries(map) => {
            let entries = map
                .iter()
                .filter_map(|(key, value)| {
                    alias_map_entry_group(value).map(|group| CatalogEntry {
                        key: key.clone(),
                        group,
                        catalog: rel.to_string(),
                    })
                })
                .collect();
            SourceRead::Read {
                entries,
                keys: map.into_keys().collect(),
            }
        }
    };
    (read, nested)
}

/// Every `apply from:` statement of an applied script, with its line and enclosing heads — the
/// statement segmentation of `config::extract_gradle_dependencies` (a statement is the text since
/// the last `{`, `}`, `;` or newline; a `{` opens a frame headed by that text), applied to the
/// whole script whatever its `dependencies` blocks hold.
fn applied_statements(content: &str) -> Vec<GradleApplyFrom> {
    let mut out = Vec::new();
    let mut stack: Vec<String> = Vec::new();
    let mut head = String::new();
    let mut line: u32 = 1;
    let mut note = |head: &str, line: u32, stack: &[String]| {
        if let Some(ScriptStatement::ApplyFrom(argument)) = script_statement(head) {
            out.push(GradleApplyFrom {
                argument,
                line,
                enclosing: stack.to_vec(),
            });
        }
    };
    for c in strip_gradle_comments(content).chars() {
        match c {
            '\n' | ';' => {
                note(&head, line, &stack);
                head.clear();
                if c == '\n' {
                    line += 1;
                }
            }
            '{' => {
                stack.push(head.trim().to_string());
                head.clear();
            }
            '}' => {
                note(&head, line, &stack);
                stack.pop();
                head.clear();
            }
            c => head.push(c),
        }
    }
    note(&head, line, &stack);
    out
}

/// The group an alias-map entry value binds to (D-DGC1B-ARTIFACT-INTERPOLATION-1): the value is one
/// string literal `"<group>:<artifact>[:…]"` whose group — the text before the first `:` — passes
/// the coordinate-segment test and whose artifact segment is non-empty, whatever interpolation the
/// artifact or the version carries. Any other value (an interpolated group, no `:`, not a string
/// literal) is no catalog entry. The 1A literal-coordinate reader `coordinate_group` is unchanged.
fn alias_map_entry_group(value: &str) -> Option<String> {
    let text = string_literal(value)?;
    let (group, rest) = text.split_once(':')?;
    let artifact = rest.split(':').next().unwrap_or("");
    (is_coordinate_segment(group) && !artifact.trim().is_empty()).then(|| group.to_string())
}

/// The repo-relative path an `apply from:` argument names, relative to the applying script's
/// directory `script_dir`: a quoted literal, optionally prefixed `$rootDir/` / `${rootDir}/` /
/// `$projectDir/` / `${projectDir}/` (the root script's directory) and optionally wrapped in
/// `file(…)`. `Err` names why it is not readable: `not a literal path` or `outside the repository`.
fn applied_path(repo_root: &Path, script_dir: &str, argument: &str) -> Result<String, String> {
    let mut arg = argument.trim();
    if let Some(inner) = arg
        .strip_prefix("file")
        .map(str::trim_start)
        .and_then(|r| r.strip_prefix('('))
        .and_then(|r| r.trim_end().strip_suffix(')'))
    {
        arg = inner.trim();
    }
    let quote = arg
        .chars()
        .next()
        .filter(|c| *c == '\'' || *c == '"')
        .ok_or_else(|| "not a literal path".to_string())?;
    let literal = arg
        .strip_prefix(quote)
        .and_then(|r| r.strip_suffix(quote))
        .filter(|l| !l.contains(quote))
        .ok_or_else(|| "not a literal path".to_string())?;
    let mut rel = literal;
    for prefix in ["$rootDir/", "${rootDir}/", "$projectDir/", "${projectDir}/"] {
        if let Some(r) = rel.strip_prefix(prefix) {
            rel = r;
            break;
        }
    }
    if rel.contains('$') {
        return Err("not a literal path".to_string());
    }
    if rel.contains("://") {
        return Err("outside the repository".to_string());
    }
    let joined = if let Some(abs) = rel.strip_prefix('/') {
        let root = repo_root.to_string_lossy();
        let root = root.trim_start_matches('/').trim_end_matches('/');
        abs.strip_prefix(root)
            .and_then(|r| r.strip_prefix('/'))
            .ok_or_else(|| "outside the repository".to_string())?
            .to_string()
    } else {
        join_rel(script_dir, rel)
    };
    let mut parts: Vec<&str> = Vec::new();
    for seg in joined.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts
                    .pop()
                    .ok_or_else(|| "outside the repository".to_string())?;
            }
            s => parts.push(s),
        }
    }
    if parts.is_empty() {
        return Err("not a literal path".to_string());
    }
    Ok(parts.join("/"))
}

/// The `[libraries]` entries of a TOML catalog (D-DGC1B-TOML-1): a string `"group:artifact[:…]"`
/// or a table with `module = "group:artifact"` → its group. Every other value (`{ group, name }`, a
/// `version` without `module`) and every other table (`[bundles]`, `[plugins]`, `[versions]`) is
/// not a library alias. `Err` = the document does not parse (the first line of the error).
fn toml_catalog_entries(content: &str, catalog: &str) -> Result<Vec<CatalogEntry>, String> {
    let doc: toml::Table = content
        .parse()
        .map_err(|e: toml::de::Error| first_line(&e.to_string()))?;
    let Some(toml::Value::Table(libraries)) = doc.get("libraries") else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for (key, value) in libraries {
        let coordinate = match value {
            toml::Value::String(s) => Some(s.as_str()),
            toml::Value::Table(t) => t.get("module").and_then(toml::Value::as_str),
            _ => None,
        };
        if let Some(group) = coordinate.and_then(coordinate_group) {
            out.push(CatalogEntry {
                key: key.clone(),
                group,
                catalog: catalog.to_string(),
            });
        }
    }
    Ok(out)
}

// ── 2a. The closed map grammar (D-DGC1B-MAP-GRAMMAR-1) ──────────────────────────────────────────

/// What the closed grammar establishes about the `libs` map one applied script builds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AliasMap {
    /// Every statement assigning to the map is admitted: the map's keys and value texts after the
    /// statements are evaluated in order (`=` replaces the map, `+=` adds entries; a key added
    /// again takes its later value). A script with no such statement yields an empty map.
    Entries(BTreeMap<String, String>),
    /// The 1-based line of the first statement that uses the map and is not an admitted literal
    /// assignment, in a use the reader cannot prove does not write the map — an assignment outside
    /// the grammar, or a use whose effect on the map the reader cannot determine. The whole map is
    /// unreadable.
    NotLiteral(u32),
    /// A `libs` map literal whose delimiters do not close correctly (unterminated, or closed by a
    /// delimiter other than the one it opened): its extent is unknown, so the script could not be
    /// read (review-0 F2 of admission 1; never a partial table).
    Malformed(String),
}

/// How an admitted literal statement assigns to the map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LiteralAssignment {
    /// `libs = [ … ]` — replaces the map.
    Replace,
    /// `libs += [ … ]` — adds entries.
    Add,
}

/// Evaluate the `libs` map of one applied script under the closed grammar of D-DGC1B-MAP-GRAMMAR-1.
///
/// The map is READABLE only when every statement that assigns to it is a literal map
/// `libs = [ … ]` / `libs += [ … ]` — `[:]` or `[ key: <value>, … ]`, nothing after its `]` in the
/// statement — standing at the start of a statement at the script's top level or directly inside
/// an `ext { }` block opened at the top level; those statements are evaluated in order.
///
/// ONE CLASSIFICATION of every use of the map name (review-0 F1, review-1 F2 and review-2 F2/F3 of
/// admission 3). Each use is exactly one of:
/// 1. an ADMITTED literal statement (above) — evaluated;
/// 2. PROVEN NOT TO WRITE the map — ignored:
///    - a map-key label `libs:` in another map or call (`[ libs: … ]`, `f(libs: …)`) — not the map;
///    - one ENTRY READ ([`proven_read`]): one member access `libs.name` or one string-keyed
///      subscript `libs['name']`, ending the expression (`implementation libs.zstd`,
///      `v = libs['x']`, `libs.x == null`, `"${libs.zstd}"`), behind any receiver
///      (`ext.libs.zstd`, `ext."libs".zstd`);
///    - the map RENDERED AS TEXT ([`rendered_as_text`]): the whole of a `${ … }` interpolation
///      (`"${libs}"`), the sole argument of the script's `print` / `println`, or a GString
///      reference without braces (`"$libs"`, `"$libs.zstd"`), which reads properties only;
///    - a string literal `"libs"` that does not name the map as a property key
///      (`println "libs"`, `[ "libs" ]`);
/// 3. every other use — an assignment outside the grammar, or a use whose effect on the map the
///    reader cannot determine — makes the whole map unreadable ([`AliasMap::NotLiteral`], naming
///    that statement's line). Counted: an assignment form (`libs = …` outside the grammar,
///    `ext.libs = …`, `libs.k = …`, `libs."k" = …`, `libs[…] = …`, compound operators,
///    `++`/`--`), a call or closure on it (`libs.put(…)`, `libs?.put(…)`, `libs.each { }`), a
///    chained or safe-navigated access, a reference handed to anything other than text rendering
///    (`f(libs)`, `def m = libs`, `obj.println(libs)`), `def libs = …`, the same inside a `${ … }`
///    interpolation, and a string literal `"libs"` used as a property key
///    ([`string_names_map_key`]: `ext."libs" = …`, `ext.'libs'`, `ext["libs"] = …`,
///    `ext.set('libs', …)`, `findProperty("libs")`).
///
/// Comments are stripped first.
pub(crate) fn evaluate_alias_map(content: &str) -> AliasMap {
    let chars: Vec<char> = strip_gradle_comments(content).chars().collect();
    let n = chars.len();
    let line_of = |at: usize| -> u32 {
        let lines = 1 + chars[..at].iter().filter(|c| **c == '\n').count();
        u32::try_from(lines).unwrap_or(u32::MAX)
    };
    let mut map: BTreeMap<String, String> = BTreeMap::new();
    let mut not_admitted: Option<u32> = None;
    // Open delimiters: `(delimiter, head)`; a `{` keeps the trimmed statement text before it.
    let mut stack: Vec<(char, String)> = Vec::new();
    let mut head = String::new();
    // The open string literal: its quote, start index, text, and (double quotes only) the brace
    // depth of the `${ … }` interpolation the scan is in (0 = literal text).
    let mut string: Option<(char, usize, String, u32)> = None;
    let mut escape = false;
    let mut k = 0;
    while k < n {
        let c = chars[k];
        if let Some((q, start, text, interp)) = string.as_mut() {
            if *interp > 0 {
                match c {
                    '{' => *interp += 1,
                    '}' => *interp -= 1,
                    _ if is_map_name_at(&chars, k)
                        && !proven_read(&chars, k)
                        && !rendered_as_text(&chars, k) =>
                    {
                        not_admitted.get_or_insert(line_of(k));
                    }
                    _ => {}
                }
                text.push(c);
                k += 1;
                continue;
            }
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == *q {
                if text == "libs" && string_names_map_key(&chars, *start, k) {
                    not_admitted.get_or_insert(line_of(*start));
                }
                string = None;
                head.push(c);
                k += 1;
                continue;
            } else if *q == '"' && c == '$' && chars.get(k + 1) == Some(&'{') {
                *interp = 1;
                text.push_str("${");
                k += 2;
                continue;
            }
            // A `$libs` / `$libs.a.b` reference without braces reads properties only: not counted.
            text.push(c);
            k += 1;
            continue;
        }
        match c {
            '\'' | '"' => {
                string = Some((c, k, String::new(), 0));
                head.push(c);
            }
            '{' => {
                stack.push(('{', head.trim().to_string()));
                head.clear();
            }
            '(' | '[' => {
                stack.push((c, String::new()));
                head.push(c);
            }
            '}' => {
                stack.pop();
                head.clear();
            }
            ')' | ']' => {
                stack.pop();
                head.push(c);
            }
            '\n' | ';' if stack.last().is_none_or(|(d, _)| *d == '{') => head.clear(),
            _ if is_map_name_at(&chars, k) => {
                let qualified = k > 0 && matches!(chars[k - 1], '.' | '@');
                let in_position = stack.is_empty()
                    || (stack.len() == 1 && stack[0].0 == '{' && stack[0].1 == "ext");
                if let (false, Some((assignment, rhs_start))) =
                    (qualified, literal_assignment_at(&chars, k))
                {
                    let literal = match literal_map_statement(&chars, rhs_start) {
                        Err(error) => return AliasMap::Malformed(error),
                        Ok(found) => found,
                    };
                    if let Some((entries, end)) = literal {
                        if in_position && statement_initial(&chars, k) {
                            if assignment == LiteralAssignment::Replace {
                                map.clear();
                            }
                            map.extend(entries);
                            k = end;
                            continue;
                        }
                    }
                }
                let key_label = !qualified && map_key_label(&chars, k);
                let text = !qualified && rendered_as_text(&chars, k);
                if !key_label && !text && !proven_read(&chars, k) {
                    not_admitted.get_or_insert(line_of(k));
                }
                k += 4;
                continue;
            }
            _ => head.push(c),
        }
        k += 1;
    }
    match not_admitted {
        Some(line) => AliasMap::NotLiteral(line),
        None => AliasMap::Entries(map),
    }
}

/// The Groovy calls that render their argument as text: `print` / `println` of the script.
const TEXT_RENDERING_CALLS: [&str; 2] = ["print", "println"];

/// True iff the unqualified `libs` at `k` is the whole value RENDERED AS TEXT — the whole of a
/// `${ … }` interpolation (`"${libs}"`), or the sole argument of a script-level
/// [`TEXT_RENDERING_CALLS`] call (`println libs`, `println(libs)`) that ends the statement. Rendering
/// reads the map's entries and cannot write the map (review-2 F2 of admission 3).
fn rendered_as_text(chars: &[char], k: usize) -> bool {
    let at = |i: usize| chars.get(i).copied().unwrap_or('\0');
    let mut p = k;
    while p > 0 && (chars[p - 1] == ' ' || chars[p - 1] == '\t') {
        p -= 1;
    }
    let mut e = k + 4;
    while at(e) == ' ' || at(e) == '\t' {
        e += 1;
    }
    if p >= 2 && chars[p - 1] == '{' && chars[p - 2] == '$' {
        return at(e) == '}';
    }
    // The call's name, unqualified and starting the statement; `(` is optional.
    let (open_paren, name_end) = if p > 0 && chars[p - 1] == '(' {
        let mut q = p - 1;
        while q > 0 && (chars[q - 1] == ' ' || chars[q - 1] == '\t') {
            q -= 1;
        }
        (true, q)
    } else if p < k {
        (false, p)
    } else {
        return false;
    };
    // The name ends where the blanks before `(` or `libs` begin, on the same line.
    if name_end == 0 || !is_ident_char(chars[name_end - 1]) {
        return false;
    }
    let name = preceding_word(chars, name_end);
    let name_start = name_end - name.chars().count();
    if !TEXT_RENDERING_CALLS.contains(&name.as_str())
        || (name_start > 0 && matches!(chars[name_start - 1], '.' | '@'))
        || !statement_initial(chars, name_start)
    {
        return false;
    }
    let mut end = e;
    if open_paren {
        if at(end) != ')' {
            return false;
        }
        end += 1;
        while at(end) == ' ' || at(end) == '\t' || at(end) == '\r' {
            end += 1;
        }
    }
    matches!(at(end), '\0' | '\n' | ';' | '}')
}

/// True iff the `libs` at `k` is a PROVEN READ: exactly one access — a member `.name` or a subscript
/// whose key is one string literal `['name']` — followed by the end of the expression (the end of
/// the script, a newline, `;`, `,`, `)`, `]`, `}`, `:` that is not `::`, `==`, `!=`, `&&`, `||`, `?:`,
/// or a `+` that is not `++` / `+=`), with no `++` / `--` before it. Such a use yields one entry's
/// value and cannot write the map.
fn proven_read(chars: &[char], k: usize) -> bool {
    let mut p = k;
    while p > 0 && (chars[p - 1] == ' ' || chars[p - 1] == '\t') {
        p -= 1;
    }
    if p >= 2 && matches!(&chars[p - 2..p], ['+', '+'] | ['-', '-']) {
        return false;
    }
    single_access_end(chars, k + 4).is_some_and(|end| expression_ends_at(chars, end))
}

/// The index just past one access starting at `j`: `.name` (an identifier, not quoted) or
/// `[ '<literal>' ]` / `[ "<literal>" ]`.
fn single_access_end(chars: &[char], j: usize) -> Option<usize> {
    let at = |i: usize| chars.get(i).copied().unwrap_or('\0');
    match at(j) {
        '.' if at(j + 1).is_ascii_alphabetic() || at(j + 1) == '_' => {
            let mut e = j + 1;
            while e < chars.len() && is_ident_char(chars[e]) {
                e += 1;
            }
            Some(e)
        }
        '[' => {
            let mut i = j + 1;
            while at(i) == ' ' || at(i) == '\t' {
                i += 1;
            }
            let q = at(i);
            if q != '\'' && q != '"' {
                return None;
            }
            let close = (i + 1..chars.len()).find(|&x| chars[x] == q || chars[x] == '\n')?;
            if chars[close] != q {
                return None;
            }
            let mut e = close + 1;
            while at(e) == ' ' || at(e) == '\t' {
                e += 1;
            }
            (at(e) == ']').then_some(e + 1)
        }
        _ => None,
    }
}

/// True iff the expression ends at `j` (blanks skipped): see [`proven_read`].
fn expression_ends_at(chars: &[char], mut j: usize) -> bool {
    let at = |i: usize| chars.get(i).copied().unwrap_or('\0');
    while at(j) == ' ' || at(j) == '\t' {
        j += 1;
    }
    match at(j) {
        '\0' | '\n' | '\r' | ';' | ',' | ')' | ']' | '}' => true,
        ':' => at(j + 1) != ':',
        '=' | '!' => at(j + 1) == '=',
        '&' => at(j + 1) == '&',
        '|' => at(j + 1) == '|',
        '?' => at(j + 1) == ':',
        '+' => !matches!(at(j + 1), '+' | '='),
        _ => false,
    }
}

/// The property-accessor calls whose first argument names a property: `"libs"` there names the map.
const PROPERTY_KEY_CALLS: [&str; 10] = [
    "set",
    "setProperty",
    "getProperty",
    "property",
    "findProperty",
    "get",
    "getAt",
    "put",
    "putAt",
    "remove",
];

/// True iff the string literal `"libs"` whose quotes are at `start` and `end` names the map as a
/// property key in a use the reader cannot prove does not write the map: a quoted property name
/// after a navigation (`ext."libs"`, `ext?.'libs'`, `ext.@'libs'`) or a subscript key `x["libs"]`
/// (the `[` directly after an identifier, `)` or `]`), either not followed by one entry read that
/// ends the expression; or the first argument of a [`PROPERTY_KEY_CALLS`] call, with or without
/// parentheses (`ext.set('libs', …)`, `ext.set 'libs', …`).
fn string_names_map_key(chars: &[char], start: usize, end: usize) -> bool {
    let at = |i: usize| chars.get(i).copied().unwrap_or('\0');
    let mut p = start;
    while p > 0 && chars[p - 1].is_whitespace() {
        p -= 1;
    }
    if p == 0 {
        return false;
    }
    match chars[p - 1] {
        '[' => {
            let subscript =
                p >= 2 && (is_ident_char(chars[p - 2]) || matches!(chars[p - 2], ')' | ']'));
            let mut e = end + 1;
            while at(e) == ' ' || at(e) == '\t' {
                e += 1;
            }
            subscript
                && at(e) == ']'
                && !single_access_end(chars, e + 1).is_some_and(|x| expression_ends_at(chars, x))
        }
        // A quoted property name after a navigation (`.`, `?.`, `*.`, `.@`): `ext."libs"`,
        // `ext.'libs'` (review-2 F3 of admission 3) — the map itself, unless one entry read
        // ending the expression follows (`ext."libs".zstd`).
        '.' | '@' => {
            !single_access_end(chars, end + 1).is_some_and(|x| expression_ends_at(chars, x))
        }
        '(' => PROPERTY_KEY_CALLS.contains(&preceding_word(chars, p - 1).as_str()),
        c if is_ident_char(c) => {
            PROPERTY_KEY_CALLS.contains(&preceding_word(chars, start).as_str())
        }
        _ => false,
    }
}

/// True iff the identifier `libs` stands at `k`: neither preceded nor followed by an identifier
/// character (a receiver or navigation before it — `.`, `?.`, `*.`, `.@` — does not change that).
fn is_map_name_at(chars: &[char], k: usize) -> bool {
    chars.len() >= k + 4
        && chars[k..k + 4] == ['l', 'i', 'b', 's']
        && !chars.get(k + 4).is_some_and(|c| is_ident_char(*c))
        && (k == 0 || !is_ident_char(chars[k - 1]))
}

/// If the unqualified `libs` at `k` is followed by `+=` or `=` (not `==` / `=~`), which, and where
/// its right-hand side starts.
fn literal_assignment_at(chars: &[char], k: usize) -> Option<(LiteralAssignment, usize)> {
    let at = |i: usize| chars.get(i).copied().unwrap_or('\0');
    let mut j = k + 4;
    while at(j) == ' ' || at(j) == '\t' {
        j += 1;
    }
    if at(j) == '+' && at(j + 1) == '=' {
        Some((LiteralAssignment::Add, j + 2))
    } else if at(j) == '=' && at(j + 1) != '=' && at(j + 1) != '~' {
        Some((LiteralAssignment::Replace, j + 1))
    } else {
        None
    }
}

/// True iff the `libs` at `k` is a map-key label — in a key position (the previous non-blank
/// character is `[`, `(` or `,`) and followed by `:` (not `::`): `[ libs: … ]`, `f(libs: …)`.
fn map_key_label(chars: &[char], k: usize) -> bool {
    let at = |i: usize| chars.get(i).copied().unwrap_or('\0');
    let mut j = k + 4;
    while at(j) == ' ' || at(j) == '\t' {
        j += 1;
    }
    if at(j) != ':' || at(j + 1) == ':' {
        return false;
    }
    let mut p = k;
    while p > 0 && chars[p - 1].is_whitespace() {
        p -= 1;
    }
    p > 0 && matches!(chars[p - 1], '[' | '(' | ',')
}

/// True iff the token at `k` starts a statement: only blanks lie between it and the start of the
/// script or a `;`, `{` or `}`, or it begins a line whose previous non-blank character does not
/// continue an expression onto it (an operator, `,`, `(`, `[`, `.`).
fn statement_initial(chars: &[char], k: usize) -> bool {
    let mut j = k;
    let mut crossed_newline = false;
    while j > 0 && chars[j - 1].is_whitespace() {
        crossed_newline |= chars[j - 1] == '\n';
        j -= 1;
    }
    if j == 0 {
        return true;
    }
    let prev = chars[j - 1];
    if matches!(prev, ';' | '{' | '}') {
        return true;
    }
    crossed_newline
        && !matches!(
            prev,
            '=' | ','
                | '('
                | '['
                | '+'
                | '-'
                | '*'
                | '/'
                | '%'
                | '?'
                | ':'
                | '&'
                | '|'
                | '!'
                | '<'
                | '>'
                | '.'
                | '~'
                | '^'
        )
}

/// The `(key, value text)` entries of one map literal, in source order.
type MapEntries = Vec<(String, String)>;

/// The right-hand side of a `libs =` / `libs +=` statement starting at `start`, when it is a
/// literal map that ends the statement: its `(key, value text)` entries and the index just past its
/// `]`. `Ok(None)` when the right-hand side is anything else (not `[`, a list, an expression
/// continuing after `]`). `Err` when the literal's delimiters do not close correctly.
fn literal_map_statement(
    chars: &[char],
    start: usize,
) -> Result<Option<(MapEntries, usize)>, String> {
    let mut i = start;
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }
    if chars.get(i) != Some(&'[') {
        return Ok(None);
    }
    let (entries, end) = map_literal_entries(chars, i)?;
    let mut j = end;
    while j < chars.len() && (chars[j] == ' ' || chars[j] == '\t' || chars[j] == '\r') {
        j += 1;
    }
    let ends_statement = j >= chars.len() || matches!(chars[j], '\n' | ';' | '}');
    Ok(entries.filter(|_| ends_statement).map(|e| (e, end)))
}

/// The `key: value` entries (texts trimmed, a quoted key unquoted) of the literal whose `[` is at
/// `open` — `None` when it is not a map literal (`[]`, a list, an item without a top-level `:`) —
/// and the index just past its matching `]`. `Err` when the literal's delimiters do not close
/// correctly: the input ends before its `]` (or inside a string), or a closing delimiter does not
/// match the one it closes.
fn map_literal_entries(chars: &[char], open: usize) -> Result<(Option<MapEntries>, usize), String> {
    let line_of = |at: usize| 1 + chars[..at].iter().filter(|c| **c == '\n').count();
    let mut open_delims: Vec<char> = Vec::new();
    let mut closed = false;
    let mut quote: Option<char> = None;
    let mut escape = false;
    let mut items: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut k = open;
    while k < chars.len() {
        let c = chars[k];
        k += 1;
        if let Some(q) = quote {
            cur.push(c);
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '\'' | '"' => {
                quote = Some(c);
                cur.push(c);
            }
            '[' | '(' | '{' => {
                if !open_delims.is_empty() {
                    cur.push(c);
                }
                open_delims.push(c);
            }
            ']' | ')' | '}' => {
                let expected = match open_delims.pop() {
                    Some('[') => ']',
                    Some('(') => ')',
                    _ => '}',
                };
                if c != expected {
                    return Err(format!(
                        "libs map opened at line {} closes with `{c}` at line {}",
                        line_of(open),
                        line_of(k - 1)
                    ));
                }
                if open_delims.is_empty() {
                    items.push(std::mem::take(&mut cur));
                    closed = true;
                    break;
                }
                cur.push(c);
            }
            ',' if open_delims.len() == 1 => items.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    if !closed {
        return Err(format!(
            "libs map opened at line {} is not closed",
            line_of(open)
        ));
    }
    let trimmed: Vec<&str> = items.iter().map(|i| i.trim()).collect();
    if trimmed == [":"] {
        return Ok((Some(Vec::new()), k));
    }
    let mut body: &[&str] = &trimmed;
    if let Some((last, rest)) = body.split_last() {
        if last.is_empty() {
            body = rest;
        }
    }
    if body.is_empty() {
        return Ok((None, k));
    }
    let mut entries = Vec::with_capacity(body.len());
    for item in body {
        let Some(colon) = top_level_colon(item) else {
            return Ok((None, k));
        };
        let key = item[..colon].trim();
        let key = string_literal(key).unwrap_or_else(|| key.to_string());
        if key.is_empty() {
            return Ok((None, k));
        }
        entries.push((key, item[colon + 1..].trim().to_string()));
    }
    Ok((Some(entries), k))
}

/// The byte index of the first `:` outside quotes and brackets in `item`.
fn top_level_colon(item: &str) -> Option<usize> {
    let mut quote: Option<char> = None;
    let mut depth = 0u32;
    for (i, c) in item.char_indices() {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '\'' | '"' => quote = Some(c),
            '[' | '(' | '{' => depth += 1,
            ']' | ')' | '}' => depth = depth.saturating_sub(1),
            ':' if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

/// The content of `text` when it is exactly one single- or double-quoted string literal.
fn string_literal(text: &str) -> Option<String> {
    let t = text.trim();
    let quote = t.chars().next().filter(|c| *c == '\'' || *c == '"')?;
    let inner = t.strip_prefix(quote)?.strip_suffix(quote)?;
    (!inner.contains(quote)).then(|| inner.to_string())
}

// ── 3. The one binding rule (section 2.1 item 2; section 2.2) ────────────────────────────────────

/// The shape of one Gradle build the binding needs: its directory (the settings directory, or the
/// script's directory for a single-script build), its projects' Gradle paths (the root `:` first)
/// and each project's script, parallel to the paths (one script may serve several projects).
pub(crate) struct BuildShape<'a> {
    pub(crate) dir: &'a str,
    pub(crate) project_paths: Vec<&'a str>,
    pub(crate) scripts: Vec<Option<(&'a str, &'a GradleScriptScopes)>>,
}

/// Which projects a scope of one script reaches, as 1A attributes it.
enum Reach<'s> {
    /// The script's own project(s).
    Owners,
    /// Every project of the build.
    Whole,
    /// The owners and their Gradle-path descendants (`allprojects`).
    AllOf,
    /// The owners' strict descendants (`subprojects`).
    SubOf,
    /// The project a `project('<path>')` block names, resolved against each owner.
    Path(&'s str),
    /// No project.
    Nothing,
}

impl<'a> BuildShape<'a> {
    /// The distinct scripts of the build, each once, in project order.
    fn unique_scripts(&self) -> Vec<(&'a str, &'a GradleScriptScopes)> {
        let mut seen: HashSet<&str> = HashSet::new();
        self.scripts
            .iter()
            .flatten()
            .filter(|(p, _)| seen.insert(*p))
            .copied()
            .collect()
    }

    /// The indices of the projects whose script is `path`.
    fn owners(&self, path: &str) -> Vec<usize> {
        self.scripts
            .iter()
            .enumerate()
            .filter(|(_, s)| s.is_some_and(|(p, _)| p == path))
            .map(|(i, _)| i)
            .collect()
    }

    /// The projects `reach` covers for a script owned by `owners`.
    fn reach(&self, owners: &[usize], reach: &Reach<'_>) -> BTreeSet<usize> {
        let all = 0..self.project_paths.len();
        let path = |i: usize| self.project_paths[i];
        match reach {
            Reach::Owners => owners.iter().copied().collect(),
            Reach::Whole => all.collect(),
            Reach::AllOf => all
                .filter(|&q| {
                    owners
                        .iter()
                        .any(|&p| path(q) == path(p) || is_gradle_ancestor(path(p), path(q)))
                })
                .collect(),
            Reach::SubOf => all
                .filter(|&q| owners.iter().any(|&p| is_gradle_ancestor(path(p), path(q))))
                .collect(),
            Reach::Path(written) => all
                .filter(|&q| {
                    owners
                        .iter()
                        .any(|&p| path(q) == resolve_gradle_path(written, path(p)))
                })
                .collect(),
            Reach::Nothing => BTreeSet::new(),
        }
    }

    /// The projects a DIRECT `dependencies` block of a script owned by `owners` declares for.
    fn block_reach(&self, owners: &[usize], scope: &DirectScope) -> BTreeSet<usize> {
        let reach = match scope {
            DirectScope::Own => Reach::Owners,
            DirectScope::AllProjects => Reach::AllOf,
            DirectScope::SubProjects => Reach::SubOf,
            DirectScope::Project(written) => Reach::Path(written),
        };
        self.reach(owners, &reach)
    }

    /// The projects a rename or `apply from:` statement applies to (D-DGC1B-ALIAS-APPLICABILITY-1):
    /// the root script's top level or `buildscript { }` → the whole build; a non-root script's top
    /// level → its own project; directly inside a top-level `allprojects` / `subprojects` /
    /// `project('<path>')` block → the projects 1A attributes that scope to; anywhere else → none.
    fn statement_reach(&self, owners: &[usize], enclosing: &[String]) -> BTreeSet<usize> {
        let root_script = owners.contains(&0);
        let scope = statement_scope(enclosing);
        let reach = match &scope {
            StatementScope::TopLevel if root_script => Reach::Whole,
            StatementScope::TopLevel => Reach::Owners,
            StatementScope::Buildscript if root_script => Reach::Whole,
            StatementScope::Buildscript => Reach::Nothing,
            StatementScope::AllProjects => Reach::AllOf,
            StatementScope::SubProjects => Reach::SubOf,
            StatementScope::Project(written) => Reach::Path(written),
            StatementScope::NotApplicable => Reach::Nothing,
        };
        self.reach(owners, &reach)
    }
}

/// One `apply from:` statement — of a build script, or nested in an applied script — with the
/// projects it applies to and what its script holds.
struct AppliedSource {
    stmt_path: String,
    stmt_line: u32,
    reach: BTreeSet<usize>,
    read: SourceRead,
}

/// One `libraries = libs` statement with the projects it applies to.
struct Rename {
    path: String,
    line: u32,
    reach: BTreeSet<usize>,
}

/// Every catalog source and rename of one build, read once.
struct BuildSources {
    toml: SourceRead,
    /// By `(stmt_path, stmt_line)`.
    applied: Vec<AppliedSource>,
    /// By `(path, line)`.
    renames: Vec<Rename>,
}

impl BuildSources {
    fn read(repo_root: &Path, shape: &BuildShape<'_>) -> Self {
        let toml = read_toml_catalog(repo_root, shape.dir);
        let mut renames = Vec::new();
        let mut queue: VecDeque<(String, GradleApplyFrom, BTreeSet<usize>)> = VecDeque::new();
        for (path, scopes) in shape.unique_scripts() {
            let owners = shape.owners(path);
            for r in &scopes.catalog_renames {
                renames.push(Rename {
                    path: path.to_string(),
                    line: r.line,
                    reach: shape.statement_reach(&owners, &r.enclosing),
                });
            }
            for a in &scopes.applied_scripts {
                let reach = shape.statement_reach(&owners, &a.enclosing);
                queue.push_back((path.to_string(), a.clone(), reach));
            }
        }
        let mut cache: HashMap<String, (SourceRead, Vec<GradleApplyFrom>)> = HashMap::new();
        let mut nested_queued: HashSet<String> = HashSet::new();
        let mut applied = Vec::new();
        while let Some((stmt_path, apply, reach)) = queue.pop_front() {
            let read = match applied_path(repo_root, dir_of(&stmt_path), &apply.argument) {
                Err(error) => SourceRead::Unreadable(format!(
                    "catalog {} could not be read ({error})",
                    apply.argument
                )),
                Ok(rel) => {
                    let (read, nested) = cache
                        .entry(rel.clone())
                        .or_insert_with(|| read_applied_script(repo_root, &rel))
                        .clone();
                    if nested_queued.insert(rel.clone()) {
                        for n in nested {
                            queue.push_back((rel.clone(), n, BTreeSet::new()));
                        }
                    }
                    read
                }
            };
            applied.push(AppliedSource {
                stmt_path,
                stmt_line: apply.line,
                reach,
                read,
            });
        }
        applied.sort_by(|a, b| (&a.stmt_path, a.stmt_line).cmp(&(&b.stmt_path, b.stmt_line)));
        renames.sort_by(|a, b| (&a.path, a.line).cmp(&(&b.path, b.line)));
        Self {
            toml,
            applied,
            renames,
        }
    }

    /// Bind accessor token `accessor` (`libs.<alias>` / `libraries.<alias>`, as written) for project
    /// `p`: `Ok(group)` or `Err` with the first test that failed, in section 2.2's order — a source
    /// that applies to `p` could not be read; a Groovy map that applies to `p` is not a top-level
    /// literal assignment; a `libraries.` accessor that no rename applying to `p` resolves; an
    /// `apply from:` not shown to apply to `p` could change the answer; two keys answer with
    /// different groups; no source that applies to `p` answers.
    fn bind(&self, accessor: &str, p: usize) -> Result<String, String> {
        if let SourceRead::Unreadable(predicate) = &self.toml {
            return Err(predicate.clone());
        }
        let applying = || self.applied.iter().filter(|a| a.reach.contains(&p));
        if let Some(predicate) = applying().find_map(|a| match &a.read {
            SourceRead::Unreadable(predicate) => Some(predicate.clone()),
            _ => None,
        }) {
            return Err(predicate);
        }
        if let Some((script, line)) = applying()
            .filter_map(|a| match &a.read {
                SourceRead::NotLiteral { script, line } => Some((script, *line)),
                _ => None,
            })
            .min()
        {
            return Err(format!(
                "alias map in {script}:{line} is not a top-level literal assignment"
            ));
        }
        let alias = if let Some(a) = accessor.strip_prefix("libs.") {
            a
        } else if let Some(a) = accessor.strip_prefix("libraries.") {
            if !self.renames.iter().any(|r| r.reach.contains(&p)) {
                return Err(match self.renames.first() {
                    Some(r) => format!(
                        "rename at {}:{} not shown to apply to this project",
                        r.path, r.line
                    ),
                    None => "no catalog entry".to_string(),
                });
            }
            a
        } else {
            return Err("no catalog entry".to_string());
        };
        let wanted = normalise_alias_key(alias);
        for a in self.applied.iter().filter(|a| !a.reach.contains(&p)) {
            let could_answer = match &a.read {
                SourceRead::Unreadable(_) | SourceRead::NotLiteral { .. } => true,
                SourceRead::Read { keys, .. } => {
                    keys.iter().any(|k| normalise_alias_key(k) == wanted)
                }
            };
            if could_answer {
                return Err(format!(
                    "map at {}:{} not shown to apply to this project",
                    a.stmt_path, a.stmt_line
                ));
            }
        }
        let mut answers: Vec<&CatalogEntry> = Vec::new();
        let sources = std::iter::once(&self.toml).chain(applying().map(|a| &a.read));
        for read in sources {
            if let SourceRead::Read { entries, .. } = read {
                answers.extend(
                    entries
                        .iter()
                        .filter(|e| normalise_alias_key(&e.key) == wanted),
                );
            }
        }
        let groups: BTreeSet<&str> = answers.iter().map(|e| e.group.as_str()).collect();
        match groups.len() {
            0 => Err("no catalog entry".to_string()),
            1 => Ok(answers[0].group.clone()),
            _ => {
                let keys: BTreeSet<&str> = answers.iter().map(|e| e.key.as_str()).collect();
                let catalogs: BTreeSet<&str> = answers.iter().map(|e| e.catalog.as_str()).collect();
                Err(format!(
                    "ambiguous accessor ({} in {})",
                    keys.into_iter().collect::<Vec<_>>().join(", "),
                    catalogs.into_iter().collect::<Vec<_>>().join(", ")
                ))
            }
        }
    }
}

/// The outcome of binding a build's alias references: per script path, the groups that bound and
/// the scope each joins; and the build's count of references that did not bind with its first site.
#[derive(Debug, Default)]
pub(crate) struct BuildAliasBinding {
    pub(crate) bound: HashMap<String, Vec<(DirectScope, String)>>,
    pub(crate) unresolved: Option<UnresolvedAliasRefs>,
}

/// Bind every alias reference of the build `shape` (each script once). A reference is tested for
/// each project its block's scope reaches ([`BuildSources::bind`]). When it binds to one group for
/// every such project, that group joins the reference's own scope; otherwise each project it bound
/// for gets its group through a `project('<absolute path>')` entry, and the site is counted once —
/// a reference that reaches no project is not tested. `first` is the counted site with the smallest
/// `(path, line)` and states the first test that failed for it, for the first project (in project
/// order) it failed for: `<path>:<line> <accessor>: <predicate>`.
pub(crate) fn bind_build_aliases(repo_root: &Path, shape: &BuildShape<'_>) -> BuildAliasBinding {
    let sources = BuildSources::read(repo_root, shape);
    let mut binding = BuildAliasBinding::default();
    let mut count: u32 = 0;
    let mut first: Option<((String, u32), String)> = None;
    for (path, scopes) in shape.unique_scripts() {
        let owners = shape.owners(path);
        for r in &scopes.alias_refs {
            let results: Vec<(usize, Result<String, String>)> = shape
                .block_reach(&owners, &r.scope)
                .into_iter()
                .map(|p| (p, sources.bind(&r.accessor, p)))
                .collect();
            let failed = results.iter().find_map(|(_, res)| res.as_ref().err());
            let groups: BTreeSet<&String> = results
                .iter()
                .filter_map(|(_, res)| res.as_ref().ok())
                .collect();
            let bound = binding.bound.entry(path.to_string()).or_default();
            match (failed, groups.iter().next()) {
                (None, Some(group)) if groups.len() == 1 => {
                    bound.push((r.scope.clone(), (*group).clone()));
                }
                _ => {
                    for (p, res) in &results {
                        if let Ok(group) = res {
                            let at = DirectScope::Project(shape.project_paths[*p].to_string());
                            bound.push((at, group.clone()));
                        }
                    }
                }
            }
            if let Some(predicate) = failed {
                count = count.saturating_add(1);
                let site = (path.to_string(), r.line);
                if first.as_ref().is_none_or(|(at, _)| site < *at) {
                    let text = format!("{path}:{} {}: {predicate}", r.line, r.accessor);
                    first = Some((site, text));
                }
            }
        }
    }
    binding.unresolved = first.map(|(_, first)| UnresolvedAliasRefs { count, first });
    binding
}

/// `scopes` with the groups `bound` to its alias references joined to the scope of their line
/// (own / allprojects / subprojects / project path) — each group list sorted and unique, as the
/// reader writes them.
pub(crate) fn with_bound_groups(
    scopes: &GradleScriptScopes,
    bound: &[(DirectScope, String)],
) -> GradleScriptScopes {
    if bound.is_empty() {
        return scopes.clone();
    }
    let mut out = scopes.clone();
    let mut own: BTreeSet<String> = out.own.drain(..).collect();
    let mut all: BTreeSet<String> = out.allprojects.drain(..).collect();
    let mut sub: BTreeSet<String> = out.subprojects.drain(..).collect();
    let mut projects: BTreeMap<String, BTreeSet<String>> = std::mem::take(&mut out.projects)
        .into_iter()
        .map(|(p, g)| (p, g.into_iter().collect()))
        .collect();
    for (scope, group) in bound {
        match scope {
            DirectScope::Own => own.insert(group.clone()),
            DirectScope::AllProjects => all.insert(group.clone()),
            DirectScope::SubProjects => sub.insert(group.clone()),
            DirectScope::Project(path) => projects
                .entry(path.clone())
                .or_default()
                .insert(group.clone()),
        };
    }
    out.own = own.into_iter().collect();
    out.allprojects = all.into_iter().collect();
    out.subprojects = sub.into_iter().collect();
    out.projects = projects
        .into_iter()
        .map(|(p, g)| (p, g.into_iter().collect()))
        .collect();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RepoConfigContext;
    use crate::manifest_deps::ManifestRecord;

    /// Write `content` at repo-relative `rel` under `root`, creating parent directories.
    fn put(root: &Path, rel: &str, content: &str) {
        let p = root.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(p, content).unwrap();
    }

    fn names(ctx: &mut RepoConfigContext, root: &Path, file: &str) -> Vec<String> {
        ctx.resolve_gradle_deps(file, root).names
    }

    fn record<'a>(ctx: &'a RepoConfigContext, path: &str) -> &'a ManifestRecord {
        ctx.manifest_records()
            .iter()
            .find(|r| r.path == path)
            .unwrap_or_else(|| panic!("no record {path}: {:?}", ctx.manifest_records()))
    }

    fn unresolved(ctx: &RepoConfigContext, path: &str) -> Option<(u32, String)> {
        record(ctx, path)
            .unresolved_alias_refs
            .as_ref()
            .map(|u| (u.count, u.first.clone()))
    }

    const ONE_PROJECT_SETTINGS: &str = "rootProject.name = 'fx'\n";

    /// TOML `[libraries]` string form `"group:artifact:version"` → the group joins the declared set
    /// of the reference's scope; nothing is counted.
    #[test]
    fn toml_catalog_libraries_string_form_resolves_to_its_group() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        put(root, "settings.gradle", ONE_PROJECT_SETTINGS);
        put(
            root,
            "gradle/libs.versions.toml",
            "[versions]\nslf = \"2.0.9\"\n\n[libraries]\nslf = \"org.slf4j:slf4j-api:2.0.9\"\n",
        );
        put(
            root,
            "build.gradle",
            "dependencies {\n  implementation libs.slf\n  implementation 'org.lit:x:1'\n}\n",
        );
        let mut ctx = RepoConfigContext::new();
        assert_eq!(
            names(&mut ctx, root, "src/main/java/A.java"),
            vec!["org.lit", "org.slf4j"]
        );
        assert_eq!(unresolved(&ctx, "build.gradle"), None);
    }

    /// TOML `[libraries]` table form `{ module = "group:artifact", version.ref = … }` → the group;
    /// a `{ group, name }` table is not a library alias this reader reads (a stated limit), so a
    /// reference to it is counted, never guessed.
    #[test]
    fn toml_catalog_libraries_table_form_module_resolves_to_its_group() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        put(root, "settings.gradle", ONE_PROJECT_SETTINGS);
        put(
            root,
            "gradle/libs.versions.toml",
            "[versions]\nzstd = \"1.5\"\n\n[libraries]\n\
             zstd = { module = \"com.github.luben:zstd-jni\", version.ref = \"zstd\" }\n\
             gn = { group = \"org.gn\", name = \"gn\", version = \"1\" }\n\n\
             [bundles]\nb = [\"zstd\"]\n",
        );
        put(
            root,
            "build.gradle",
            "dependencies {\n  implementation libs.zstd\n  implementation libs.gn\n}\n",
        );
        let mut ctx = RepoConfigContext::new();
        assert_eq!(
            names(&mut ctx, root, "src/main/java/A.java"),
            vec!["com.github.luben"]
        );
        assert_eq!(
            unresolved(&ctx, "build.gradle"),
            Some((1, "build.gradle:3 libs.gn: no catalog entry".to_string()))
        );
    }

    /// The catalog is `<settings dir>/gradle/libs.versions.toml` — one per build, serving every
    /// project script; a `gradle/libs.versions.toml` under a project's own directory is not read.
    #[test]
    fn toml_catalog_is_read_from_the_settings_directory_gradle_folder() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        put(root, "settings.gradle", "include 'core'\n");
        put(
            root,
            "gradle/libs.versions.toml",
            "[libraries]\nx = \"org.root.catalog:x:1\"\n",
        );
        put(
            root,
            "core/gradle/libs.versions.toml",
            "[libraries]\nx = \"org.decoy:x:1\"\n",
        );
        put(
            root,
            "core/build.gradle",
            "dependencies {\n  implementation libs.x\n}\n",
        );
        let mut ctx = RepoConfigContext::new();
        assert_eq!(
            names(&mut ctx, root, "core/src/main/java/C.java"),
            vec!["org.root.catalog"]
        );
        assert_eq!(unresolved(&ctx, "core/build.gradle"), None);
    }

    /// A TOML catalog that does not parse is a FAILED read for the build's alias resolution:
    /// every alias reference of the build is counted with the parse error named, none binds, and
    /// the literal declaration beside them stays declared.
    #[test]
    fn toml_catalog_that_fails_to_parse_counts_every_alias_reference_of_the_build() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        put(root, "settings.gradle", ONE_PROJECT_SETTINGS);
        put(
            root,
            "gradle/libs.versions.toml",
            "[libraries]\nslf = { module = \n",
        );
        put(
            root,
            "build.gradle",
            "dependencies {\n  implementation 'org.lit:x:1'\n  implementation libs.slf\n  implementation libs.other\n}\n",
        );
        let mut ctx = RepoConfigContext::new();
        assert_eq!(
            names(&mut ctx, root, "src/main/java/A.java"),
            vec!["org.lit"]
        );
        let (count, first) = unresolved(&ctx, "build.gradle").expect("counted");
        assert_eq!(count, 2);
        assert!(
            first.starts_with(
                "build.gradle:3 libs.slf: catalog gradle/libs.versions.toml could not be read ("
            ) && first.ends_with(')'),
            "{first}"
        );
        assert!(
            record(&ctx, "build.gradle").error.is_none(),
            "the script itself parsed"
        );
    }

    /// The kafka form: a Groovy `libs += [ alias: "group:artifact:$versions.x" ]` map in a script
    /// the ROOT project script applies by `apply from:` INSIDE `buildscript { }`
    /// (`build.gradle:22-23`) is read, and a `project(':clients')` alias resolves to its group; a
    /// top-level `apply from: file('wrapper.gradle')` without a map is read and adds nothing.
    #[test]
    fn groovy_alias_map_applied_from_the_root_script_resolves_to_its_group() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        put(root, "settings.gradle", "include 'clients'\n");
        put(
            root,
            "build.gradle",
            "buildscript {\n  repositories {\n    mavenCentral()\n  }\n  apply from: \"$rootDir/gradle/dependencies.gradle\"\n\n  dependencies {\n    classpath \"org.tool:plugin:$versions.grgit\"\n  }\n}\napply from: file('wrapper.gradle')\nproject(':clients') {\n  dependencies {\n    implementation libs.zstd\n    implementation libs.slf4jApi\n  }\n}\n",
        );
        put(
            root,
            "gradle/dependencies.gradle",
            "ext {\n  versions = [:]\n  libs = [:]\n}\nversions += [\n  zstd: \"1.5\",\n]\nlibs += [\n  // compression\n  zstd: \"com.github.luben:zstd-jni:$versions.zstd\",\n  slf4jApi:\"org.slf4j:slf4j-api:$versions.slf4j\"\n]\n",
        );
        put(root, "wrapper.gradle", "task wrapperX { }\n");
        let mut ctx = RepoConfigContext::new();
        assert_eq!(
            names(&mut ctx, root, "clients/src/main/java/C.java"),
            vec!["com.github.luben", "org.slf4j"]
        );
        assert_eq!(
            names(&mut ctx, root, "src/main/java/R.java"),
            Vec::<String>::new()
        );
        assert_eq!(unresolved(&ctx, "build.gradle"), None);
    }

    /// The group of a Groovy map entry is the literal prefix of its value; an interpolated version
    /// plays no part. A value whose group is not literal, or that is not a string literal, is not a
    /// catalog entry, so its reference is counted.
    #[test]
    fn groovy_alias_map_value_with_interpolated_version_keeps_its_literal_group() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        put(
            root,
            "build.gradle",
            "apply from: 'deps.gradle'\ndependencies {\n  implementation libs.slf\n  implementation libs.dyn\n  implementation libs.ref\n}\n",
        );
        put(
            root,
            "deps.gradle",
            "libs = [\n  slf: \"org.slf4j:slf4j-api:${versions.slf4j}\",\n  dyn: \"${grp}:x:1\",\n  ref: versions.x,\n]\n",
        );
        let mut ctx = RepoConfigContext::new();
        assert_eq!(
            names(&mut ctx, root, "src/main/java/A.java"),
            vec!["org.slf4j"]
        );
        assert_eq!(
            unresolved(&ctx, "build.gradle"),
            Some((2, "build.gradle:4 libs.dyn: no catalog entry".to_string()))
        );
    }

    /// An alias no catalog of the build defines — no catalog at all, or a catalog without that key
    /// — is counted with `no catalog entry`, never a group guessed from the alias text.
    #[test]
    fn alias_defined_in_no_catalog_is_counted_never_guessed() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        put(
            root,
            "build.gradle",
            "dependencies {\n  implementation libs.guava\n}\n",
        );
        let mut ctx = RepoConfigContext::new();
        assert!(names(&mut ctx, root, "src/main/java/A.java").is_empty());
        assert_eq!(
            unresolved(&ctx, "build.gradle"),
            Some((1, "build.gradle:2 libs.guava: no catalog entry".to_string()))
        );

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        put(root, "settings.gradle", ONE_PROJECT_SETTINGS);
        put(
            root,
            "gradle/libs.versions.toml",
            "[libraries]\nslf = \"org.slf4j:a:1\"\n",
        );
        put(
            root,
            "build.gradle",
            "dependencies {\n  implementation libs.slf\n  implementation libs.nope\n}\n",
        );
        let mut ctx = RepoConfigContext::new();
        assert_eq!(
            names(&mut ctx, root, "src/main/java/A.java"),
            vec!["org.slf4j"]
        );
        assert_eq!(
            unresolved(&ctx, "build.gradle"),
            Some((1, "build.gradle:3 libs.nope: no catalog entry".to_string()))
        );
    }

    /// Accessor normalisation: a catalog key's `-` and `_` read as `.` (`mockito-core` ↔
    /// `libs.mockito.core`, `truth_ext` ↔ `libs.truth.ext`); a key that differs otherwise does not
    /// answer.
    #[test]
    fn gradle_alias_accessor_normalises_dash_and_underscore_to_dot() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        put(root, "settings.gradle", ONE_PROJECT_SETTINGS);
        put(
            root,
            "gradle/libs.versions.toml",
            "[libraries]\nmockito-core = \"org.mockito:mockito-core:5\"\ntruth_ext = \"com.google.truth.extensions:truth-java8-extension:1\"\n",
        );
        put(
            root,
            "build.gradle",
            "dependencies {\n  implementation libs.mockito.core\n  implementation libs.truth.ext\n  implementation libs.mockitocore\n}\n",
        );
        let mut ctx = RepoConfigContext::new();
        assert_eq!(
            names(&mut ctx, root, "src/main/java/A.java"),
            vec!["com.google.truth.extensions", "org.mockito"]
        );
        assert_eq!(
            unresolved(&ctx, "build.gradle"),
            Some((
                1,
                "build.gradle:4 libs.mockitocore: no catalog entry".to_string()
            ))
        );
        assert_eq!(normalise_alias_key("a-b_c.d"), "a.b.c.d");
    }

    /// `libraries = libs` at the root script's top level (the whole build) makes `libraries.` of a
    /// subproject resolve through `libs`; continuation lines included. Without any rename in the
    /// build a `libraries.` reference resolves to nothing and is counted `no catalog entry`.
    #[test]
    fn catalog_rename_libraries_equals_libs_resolves_through_libs() {
        let catalog = "[libraries]\nguava = \"com.google.guava:guava:33\"\nmockito-core = \"org.mockito:mockito-core:5\"\n";
        let core = "dependencies {\n    implementation libraries.guava,\n            libraries.mockito.core\n}\n";

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        put(root, "settings.gradle", "include 'core'\n");
        put(root, "gradle/libs.versions.toml", catalog);
        put(root, "build.gradle", "libraries = libs\n");
        put(root, "core/build.gradle", core);
        let mut ctx = RepoConfigContext::new();
        assert_eq!(
            names(&mut ctx, root, "core/src/main/java/C.java"),
            vec!["com.google.guava", "org.mockito"]
        );
        assert_eq!(unresolved(&ctx, "core/build.gradle"), None);

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        put(root, "settings.gradle", "include 'core'\n");
        put(root, "gradle/libs.versions.toml", catalog);
        put(
            root,
            "build.gradle",
            "subprojects {\n    description = 'x'\n}\n",
        );
        put(root, "core/build.gradle", core);
        let mut ctx = RepoConfigContext::new();
        assert!(names(&mut ctx, root, "core/src/main/java/C.java").is_empty());
        assert_eq!(
            unresolved(&ctx, "core/build.gradle"),
            Some((
                2,
                "core/build.gradle:2 libraries.guava: no catalog entry".to_string()
            ))
        );
    }

    /// The marking rides exactly one record per build — the build's smallest-path recorded
    /// non-FAILED manifest, whatever order files resolve in — with the build's total count and its
    /// first counted site by path, then line; the other record carries nothing; serialized only when
    /// present.
    #[test]
    fn unresolved_alias_refs_ride_one_record_per_build_with_count_and_first() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        put(root, "settings.gradle", "include 'a', 'b'\n");
        put(
            root,
            "b/build.gradle",
            "dependencies {\n  implementation libs.nope1\n  implementation libs.nope2\n}\n",
        );
        put(
            root,
            "a/build.gradle",
            "dependencies {\n  implementation 'org.a:x:1'\n  implementation libs.nope3\n}\n",
        );
        for order in [
            ["a/src/A.java", "b/src/B.java"],
            ["b/src/B.java", "a/src/A.java"],
        ] {
            let mut ctx = RepoConfigContext::new();
            for f in order {
                let _ = names(&mut ctx, root, f);
            }
            assert_eq!(names(&mut ctx, root, "a/src/A.java"), vec!["org.a"]);
            assert!(names(&mut ctx, root, "b/src/B.java").is_empty());
            assert_eq!(
                unresolved(&ctx, "a/build.gradle"),
                Some((
                    3,
                    "a/build.gradle:3 libs.nope3: no catalog entry".to_string()
                )),
                "order {order:?}"
            );
            assert_eq!(unresolved(&ctx, "b/build.gradle"), None, "order {order:?}");
            let json = serde_json::to_string(record(&ctx, "a/build.gradle")).unwrap();
            assert!(
                json.contains(r#""unresolved_alias_refs":{"count":3,"first":"a/build.gradle:3 libs.nope3: no catalog entry"}"#),
                "{json}"
            );
            let json_b = serde_json::to_string(record(&ctx, "b/build.gradle")).unwrap();
            assert!(!json_b.contains("unresolved_alias_refs"), "{json_b}");
        }
    }

    /// A bound alias joins exactly the scope of its line, as 1A scopes coordinates: own → the
    /// root project only; `allprojects` → every project; `subprojects` → every non-root project;
    /// `project(':a')` → `a` only.
    #[test]
    fn alias_reference_joins_the_declared_set_of_exactly_its_blocks_scope() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        put(root, "settings.gradle", "include 'a', 'b'\n");
        put(
            root,
            "gradle/libs.versions.toml",
            "[libraries]\nown = \"org.own:x:1\"\nall = \"org.all:x:1\"\nsub = \"org.sub:x:1\"\npa = \"org.pa:x:1\"\n",
        );
        put(
            root,
            "build.gradle",
            "dependencies {\n  implementation libs.own\n}\nallprojects {\n  dependencies {\n    implementation libs.all\n  }\n}\nsubprojects {\n  dependencies {\n    implementation libs.sub\n  }\n}\nproject(':a') {\n  dependencies {\n    implementation libs.pa\n  }\n}\n",
        );
        put(root, "a/build.gradle", "description = 'a'\n");
        put(root, "b/build.gradle", "description = 'b'\n");
        let mut ctx = RepoConfigContext::new();
        assert_eq!(
            names(&mut ctx, root, "src/main/java/R.java"),
            vec!["org.all", "org.own"]
        );
        assert_eq!(
            names(&mut ctx, root, "a/src/main/java/A.java"),
            vec!["org.all", "org.pa", "org.sub"]
        );
        assert_eq!(
            names(&mut ctx, root, "b/src/main/java/B.java"),
            vec!["org.all", "org.sub"]
        );
    }

    /// An applied script the reader cannot read — missing, outside the repository, named by an
    /// argument that is not a literal path, or holding a `libs` map literal whose delimiters do not
    /// close correctly (unterminated, or closed by `)` / `}`) — leaves the build's catalog unread: every alias
    /// reference of the build is counted with that reason and none binds (even one the TOML
    /// answers); the literal declaration stays declared.
    #[test]
    fn applied_alias_script_that_cannot_be_read_counts_every_alias_reference_of_the_build() {
        for (apply, applied, expected_prefix) in [
            (
                "apply from: 'gradle/missing.gradle'",
                None,
                "build.gradle:4 libs.a: catalog gradle/missing.gradle could not be read (",
            ),
            (
                "apply from: '../outside.gradle'",
                None,
                "build.gradle:4 libs.a: catalog '../outside.gradle' could not be read (outside the repository)",
            ),
            (
                "apply from: \"$someDir/deps.gradle\"",
                None,
                "build.gradle:4 libs.a: catalog \"$someDir/deps.gradle\" could not be read (not a literal path)",
            ),
            // Review-0 F2: a map literal whose delimiters do not close correctly is not read — no
            // entry of it (`b`) binds, unterminated or closed by the wrong delimiter.
            (
                "apply from: 'gradle/deps.gradle'",
                Some("libs = [\n  b: \"org.b:x:1\",\n"),
                "build.gradle:4 libs.a: catalog gradle/deps.gradle could not be read (libs map opened at line 1 is not closed)",
            ),
            (
                "apply from: 'gradle/deps.gradle'",
                Some("libs = [\n  b: \"org.b:x:1\",\n)\n"),
                "build.gradle:4 libs.a: catalog gradle/deps.gradle could not be read (libs map opened at line 1 closes with `)` at line 3)",
            ),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            put(root, "settings.gradle", ONE_PROJECT_SETTINGS);
            put(root, "gradle/libs.versions.toml", "[libraries]\na = \"org.a:x:1\"\n");
            if let Some(content) = applied {
                put(root, "gradle/deps.gradle", content);
            }
            put(
                root,
                "build.gradle",
                &format!("{apply}\ndependencies {{\n  implementation 'org.lit:x:1'\n  implementation libs.a\n  implementation libs.b\n}}\n"),
            );
            let mut ctx = RepoConfigContext::new();
            assert_eq!(
                names(&mut ctx, root, "src/main/java/A.java"),
                vec!["org.lit"],
                "{apply}"
            );
            let (count, first) = unresolved(&ctx, "build.gradle").expect("counted");
            assert_eq!(count, 2, "{apply}");
            assert!(
                first.starts_with(expected_prefix) && first.ends_with(')'),
                "{apply}: {first}"
            );
        }
    }

    /// Two catalog keys that normalise to one accessor and name different groups: that reference
    /// binds to neither and is counted naming both keys; another reference of the build still binds.
    #[test]
    fn two_catalog_keys_normalising_to_one_accessor_with_different_groups_resolve_to_neither() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        put(root, "settings.gradle", ONE_PROJECT_SETTINGS);
        put(
            root,
            "gradle/libs.versions.toml",
            "[libraries]\nmockito-core = \"org.mockito:mockito-core:5\"\nmockito_core = \"org.other:mc:1\"\nfine = \"org.fine:f:1\"\n",
        );
        put(
            root,
            "build.gradle",
            "dependencies {\n  implementation libs.fine\n  implementation libs.mockito.core\n}\n",
        );
        let mut ctx = RepoConfigContext::new();
        assert_eq!(
            names(&mut ctx, root, "src/main/java/A.java"),
            vec!["org.fine"]
        );
        assert_eq!(
            unresolved(&ctx, "build.gradle"),
            Some((
                1,
                "build.gradle:3 libs.mockito.core: ambiguous accessor (mockito-core, mockito_core in gradle/libs.versions.toml)".to_string()
            ))
        );
    }

    /// The build's marking wherever it rides (exactly one record of a build carries it).
    fn build_unresolved(ctx: &RepoConfigContext) -> Option<(u32, String)> {
        let carried: Vec<(u32, String)> = ctx
            .manifest_records()
            .iter()
            .filter_map(|r| r.unresolved_alias_refs.as_ref())
            .map(|u| (u.count, u.first.clone()))
            .collect();
        assert!(carried.len() <= 1, "one carrier per build: {carried:?}");
        carried.into_iter().next()
    }

    /// A one-project build: `build.gradle` = `root_script`, `gradle/deps.gradle` = `deps` (when
    /// given), optional TOML catalog; returns the root Java file's declared set and the marking.
    fn one_project(
        root_script: &str,
        deps: Option<&str>,
        toml: Option<&str>,
    ) -> (Vec<String>, Option<(u32, String)>) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        put(root, "settings.gradle", ONE_PROJECT_SETTINGS);
        put(root, "build.gradle", root_script);
        if let Some(d) = deps {
            put(root, "gradle/deps.gradle", d);
        }
        if let Some(t) = toml {
            put(root, "gradle/libs.versions.toml", t);
        }
        let mut ctx = RepoConfigContext::new();
        let n = names(&mut ctx, root, "src/main/java/A.java");
        (n, build_unresolved(&ctx))
    }

    /// D-DGC1B-ARTIFACT-INTERPOLATION-1: a map entry whose GROUP segment is literal binds to that
    /// group whatever interpolation its artifact (and version) carries — kafka
    /// `gradle/dependencies.gradle:235` `scalaLogging: "com.typesafe.scala-logging:scala-logging_$versions.baseScala:$versions.scalaLogging"`.
    #[test]
    fn groovy_alias_map_entry_with_interpolated_artifact_binds_to_its_literal_group() {
        let (n, u) = one_project(
            "apply from: 'gradle/deps.gradle'\ndependencies {\n  implementation libs.scalaLogging\n  implementation libs.ok\n}\n",
            Some("libs = [\n  scalaLogging: \"com.typesafe.scala-logging:scala-logging_$versions.baseScala:$versions.scalaLogging\",\n  ok: \"org.ok:x_${v}:1\",\n]\n"),
            None,
        );
        assert_eq!(n, vec!["com.typesafe.scala-logging", "org.ok"]);
        assert_eq!(u, None);
    }

    /// D-DGC1B-ARTIFACT-INTERPOLATION-1: an entry whose GROUP carries an interpolation is no catalog
    /// entry — its reference is counted `no catalog entry`, never guessed; a literal entry beside it
    /// still binds.
    #[test]
    fn groovy_alias_map_entry_with_interpolated_group_binds_nothing_and_is_counted() {
        let (n, u) = one_project(
            "apply from: 'gradle/deps.gradle'\ndependencies {\n  implementation libs.ok\n  implementation libs.dyn1\n  implementation libs.dyn2\n}\n",
            Some("libs = [\n  ok: \"org.ok:x:1\",\n  dyn1: \"${grp}.x:art:1\",\n  dyn2: \"com.$g:art:1\",\n]\n"),
            None,
        );
        assert_eq!(n, vec!["org.ok"]);
        assert_eq!(
            u,
            Some((2, "build.gradle:4 libs.dyn1: no catalog entry".to_string()))
        );
    }

    /// D-DGC1B-ALIAS-APPLICABILITY-1, the record's counterexample: a rename in a SIBLING project's
    /// script applies to that project only — `a`'s reference binds, `b`'s is counted naming the
    /// rename it does not reach.
    #[test]
    fn catalog_rename_in_a_sibling_script_does_not_apply_and_its_dependent_references_are_counted()
    {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        put(root, "settings.gradle", "include 'a', 'b'\n");
        put(
            root,
            "gradle/libs.versions.toml",
            "[libraries]\nfoo = \"org.slf4j:slf4j-api:2.0.9\"\n",
        );
        put(
            root,
            "a/build.gradle",
            "ext {\n  libraries = libs\n}\ndependencies {\n  implementation libraries.foo\n}\n",
        );
        put(
            root,
            "b/build.gradle",
            "dependencies {\n  implementation libraries.foo\n}\n",
        );
        let mut ctx = RepoConfigContext::new();
        assert_eq!(
            names(&mut ctx, root, "a/src/main/java/A.java"),
            vec!["org.slf4j"]
        );
        assert!(names(&mut ctx, root, "b/src/main/java/B.java").is_empty());
        assert_eq!(
            build_unresolved(&ctx),
            Some((
                1,
                "b/build.gradle:2 libraries.foo: rename at a/build.gradle:2 not shown to apply to this project"
                    .to_string()
            ))
        );
    }

    /// D-DGC1B-ALIAS-APPLICABILITY-1: an `apply from:` under `if (…)` or `afterEvaluate` applies to
    /// no project — only the references its map could answer are counted (`map at … not shown to
    /// apply`), its group is in no declared set, and a reference the TOML answers still binds; a
    /// conditionally applied script that cannot be read counts every reference of the build.
    #[test]
    fn conditional_apply_from_does_not_apply_and_its_dependent_references_are_counted() {
        let refs = "dependencies {\n  implementation libs.x\n  implementation libs.y\n}\n";
        let toml = "[libraries]\ny = \"org.y:y:1\"\n";
        for wrapper in ["if (useAliases) {", "afterEvaluate {"] {
            let script = format!("{wrapper}\n  apply from: \"gradle/deps.gradle\"\n}}\n{refs}");
            let (n, u) = one_project(&script, Some("libs = [ x: \"org.x:x:1\" ]\n"), Some(toml));
            assert_eq!(n, vec!["org.y"], "{wrapper}");
            assert_eq!(
                u,
                Some((
                    1,
                    "build.gradle:5 libs.x: map at build.gradle:2 not shown to apply to this project"
                        .to_string()
                )),
                "{wrapper}"
            );
        }
        let script =
            format!("if (useAliases) {{\n  apply from: \"gradle/missing.gradle\"\n}}\n{refs}");
        let (n, u) = one_project(&script, None, Some(toml));
        assert!(n.is_empty(), "{n:?}");
        assert_eq!(
            u,
            Some((
                2,
                "build.gradle:5 libs.x: map at build.gradle:2 not shown to apply to this project"
                    .to_string()
            ))
        );

        // An `apply from:` inside an applied script applies to no project; its script is read for
        // keys only, so a reference its map could answer is counted naming that statement.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        put(root, "settings.gradle", ONE_PROJECT_SETTINGS);
        put(root, "gradle/libs.versions.toml", toml);
        put(
            root,
            "build.gradle",
            &format!("apply from: \"gradle/deps.gradle\"\n{refs}"),
        );
        put(root, "gradle/deps.gradle", "apply from: \"inner.gradle\"\n");
        put(root, "gradle/inner.gradle", "libs = [ x: \"org.x:x:1\" ]\n");
        let mut ctx = RepoConfigContext::new();
        assert_eq!(names(&mut ctx, root, "src/main/java/A.java"), vec!["org.y"]);
        assert_eq!(
            build_unresolved(&ctx),
            Some((
                1,
                "build.gradle:3 libs.x: map at gradle/deps.gradle:1 not shown to apply to this project"
                    .to_string()
            ))
        );
    }

    /// grpc-java's shape (`build.gradle:159` inside `subprojects { ext { … } }`): the rename reaches
    /// every non-root project — two subprojects' `libraries.` references bind — and not the root
    /// project, whose top-level block's reference is counted with the `rename at` form.
    #[test]
    fn catalog_rename_in_root_subprojects_applies_to_every_non_root_project() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        put(root, "settings.gradle", "include 'core', 'api'\n");
        put(
            root,
            "gradle/libs.versions.toml",
            "[libraries]\nguava = \"com.google.guava:guava:33\"\n",
        );
        put(
            root,
            "build.gradle",
            "subprojects {\n  ext {\n    libraries = libs\n  }\n}\ndependencies {\n  implementation libraries.guava\n}\n",
        );
        let sub = "dependencies {\n  implementation libraries.guava\n}\n";
        put(root, "core/build.gradle", sub);
        put(root, "api/build.gradle", sub);
        let mut ctx = RepoConfigContext::new();
        assert_eq!(
            names(&mut ctx, root, "core/src/main/java/C.java"),
            vec!["com.google.guava"]
        );
        assert_eq!(
            names(&mut ctx, root, "api/src/main/java/A.java"),
            vec!["com.google.guava"]
        );
        assert!(names(&mut ctx, root, "src/main/java/R.java").is_empty());
        assert_eq!(
            build_unresolved(&ctx),
            Some((
                1,
                "build.gradle:7 libraries.guava: rename at build.gradle:3 not shown to apply to this project"
                    .to_string()
            ))
        );
    }

    /// kafka's shape (`build.gradle:22-23`): an `apply from:` inside the root `buildscript { }`
    /// reaches the whole build — a root `project(':clients')` block's reference and another
    /// subproject's own reference both bind; nothing is counted.
    #[test]
    fn apply_from_in_root_buildscript_applies_to_the_whole_build() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        put(root, "settings.gradle", "include 'clients', 'core'\n");
        put(
            root,
            "build.gradle",
            "buildscript {\n  apply from: \"$rootDir/gradle/dependencies.gradle\"\n}\nproject(':clients') {\n  dependencies {\n    implementation libs.zstd\n  }\n}\n",
        );
        put(
            root,
            "core/build.gradle",
            "dependencies {\n  implementation libs.zstd\n}\n",
        );
        put(
            root,
            "gradle/dependencies.gradle",
            "ext {\n  versions = [:]\n  libs = [:]\n}\nlibs += [\n  zstd: \"com.github.luben:zstd-jni:$versions.zstd\",\n]\n",
        );
        let mut ctx = RepoConfigContext::new();
        assert_eq!(
            names(&mut ctx, root, "clients/src/main/java/C.java"),
            vec!["com.github.luben"]
        );
        assert_eq!(
            names(&mut ctx, root, "core/src/main/java/K.java"),
            vec!["com.github.luben"]
        );
        assert_eq!(build_unresolved(&ctx), None);
    }

    const APPLY_DEPS: &str = "apply from: \"gradle/deps.gradle\"\n";

    /// D-DGC1B-MAP-GRAMMAR-1 (review-4's counterexample): statements are evaluated in order — a
    /// later `libs = [:]` erases an earlier alias, which is then counted `no catalog entry`.
    #[test]
    fn alias_map_later_replacement_erases_an_earlier_alias() {
        let (n, u) = one_project(
            &format!("{APPLY_DEPS}dependencies {{\n  implementation libs.foo\n}}\n"),
            Some("libs = [ foo: \"com.acme:foo:1\" ]\nlibs = [:]\n"),
            None,
        );
        assert!(n.is_empty(), "com.acme is in no declared set: {n:?}");
        assert_eq!(
            u,
            Some((1, "build.gradle:3 libs.foo: no catalog entry".to_string()))
        );
    }

    /// D-DGC1B-MAP-GRAMMAR-1: a later `+=` adds, and a key added again takes its later value.
    #[test]
    fn alias_map_later_addition_adds_an_alias() {
        let (n, u) = one_project(
            &format!("{APPLY_DEPS}dependencies {{\n  implementation libs.foo\n}}\n"),
            Some("libs = [:]\nlibs += [ foo: \"com.old:foo:1\" ]\nlibs += [ foo: \"com.acme:foo:1\" ]\n"),
            None,
        );
        assert_eq!(n, vec!["com.acme"]);
        assert_eq!(u, None);
    }

    const ABC_REFS: &str = "dependencies {\n  implementation libs.a\n  implementation libs.b\n  implementation libs.c\n}\n";

    /// D-DGC1B-MAP-GRAMMAR-1: an assignment under `if`, in a `.each { }` closure or in a `for` loop
    /// makes the WHOLE map unreadable — every dependent reference is counted naming that statement,
    /// and none of the map's entries (the unconditional ones included) is declared.
    #[test]
    fn alias_map_assignment_under_a_condition_or_loop_makes_the_map_unreadable_and_dependent_references_are_counted(
    ) {
        for wrapper in [
            "if (project.hasProperty(\"x\")) {",
            "[\"x\"].each {",
            "for (k in [\"x\"]) {",
        ] {
            let deps = format!(
                "ext {{ libs = [ a: \"com.acme:a:1\" ] }}\nlibs += [ b: \"com.acme.b:b:1\" ]\n{wrapper}\n  libs += [ c: \"com.acme.c:c:1\" ]\n}}\n"
            );
            let (n, u) = one_project(&format!("{APPLY_DEPS}{ABC_REFS}"), Some(&deps), None);
            assert!(n.is_empty(), "{wrapper}: {n:?}");
            assert_eq!(
                u,
                Some((
                    3,
                    "build.gradle:3 libs.a: alias map in gradle/deps.gradle:4 is not a top-level literal assignment"
                        .to_string()
                )),
                "{wrapper}"
            );
        }
    }

    /// D-DGC1B-MAP-GRAMMAR-1: an assignment in a closure other than an unconditional top-level
    /// `ext { }` — `afterEvaluate { }`, or an `ext { }` nested in `allprojects { }` — makes the map
    /// unreadable.
    #[test]
    fn alias_map_assignment_in_a_non_ext_closure_makes_the_map_unreadable() {
        for (tail, refs, line) in [
            (
                "afterEvaluate {\n  libs = [ d: \"com.acme.d:d:1\" ]\n}\n",
                "dependencies {\n  implementation libs.a\n  implementation libs.d\n}\n",
                3,
            ),
            (
                "allprojects {\n  ext {\n    libs += [ e: \"com.acme.e:e:1\" ]\n  }\n}\n",
                "dependencies {\n  implementation libs.a\n  implementation libs.e\n}\n",
                4,
            ),
        ] {
            let deps = format!("ext {{ libs = [ a: \"com.acme:a:1\" ] }}\n{tail}");
            let (n, u) = one_project(&format!("{APPLY_DEPS}{refs}"), Some(&deps), None);
            assert!(n.is_empty(), "{tail}: {n:?}");
            assert_eq!(
                u,
                Some((
                    2,
                    format!(
                        "build.gradle:3 libs.a: alias map in gradle/deps.gradle:{line} is not a top-level literal assignment"
                    )
                )),
                "{tail}"
            );
        }
    }

    /// D-DGC1B-MAP-GRAMMAR-1: every other assigning form — a non-literal right-hand side,
    /// `libs.put(…)`, `libs[…] =`, `ext.libs =`, `libs.<key> =`, `libs << …` — and every other
    /// occurrence of the map name (safe navigation `libs?.put(…)`, a quoted key `libs."a" =`, a
    /// receiver, `ext["libs"]`, `ext.set 'libs'`, `findProperty("libs")`, a `${ … }` call, a compound
    /// operator or a call on an entry, a bare reference handed on, `def libs`) makes the map
    /// unreadable, naming that statement's line.
    #[test]
    fn alias_map_assignment_in_any_other_statement_form_makes_the_map_unreadable() {
        for form in [
            "libs += other",
            "libs.put(\"e\", \"com.acme.e:e:1\")",
            "libs[\"e\"] = \"com.acme.e:e:1\"",
            "ext.libs = [ e: \"com.acme.e:e:1\" ]",
            "libs.e = \"com.acme.e:e:1\"",
            "libs << [ e: \"com.acme.e:e:1\" ]",
            "libs = [ e: \"com.acme.e:e:1\" ] + other",
            // Review-0 F1 (admission 3): every other occurrence of the map name fails closed —
            // safe navigation, a quoted key, a receiver, a string-keyed access, a GString
            // interpolation, a bare read passed on, a local `def`.
            "libs?.put(\"e\", \"com.acme.e:e:1\")",
            "libs.\"a\" = \"com.other:a:1\"",
            "libs*.value",
            "project.ext.libs.put(\"a\", \"com.other:a:1\")",
            "rootProject.libs = [ e: \"com.acme.e:e:1\" ]",
            "ext[\"libs\"] = [ e: \"com.acme.e:e:1\" ]",
            "ext.set('libs', [ e: \"com.acme.e:e:1\" ])",
            "println \"${libs.put('a', 'com.other:a:1')}\"",
            "def m = findProperty(\"libs\")",
            "ext.set 'libs', [ e: \"com.acme.e:e:1\" ]",
            "libs.a += \"x\"",
            "libs.a.trim()",
            "def m = libs",
            "mutate(libs)",
            "def libs = [ e: \"com.acme.e:e:1\" ]",
            // Review-2 F3 (admission 3): a quoted property name after a navigation dot names the
            // map as a property key, like a subscript key — a write through it, or any use of it
            // other than one entry read, counts.
            "ext.\"libs\" = [ a: \"com.other:a:1\" ]",
            "ext.'libs' = [ a: \"com.other:a:1\" ]",
            "project.ext?.\"libs\".put(\"a\", \"com.other:a:1\")",
            "ext.\"libs\".a = \"com.other:a:1\"",
            "ext.@'libs' = [ a: \"com.other:a:1\" ]",
            // Only the script's own `print` / `println` is known to render its argument as text;
            // any other callee handed the map is a use the reader cannot determine.
            "obj.println(libs)",
            "println libs, other",
        ] {
            let deps = format!("ext {{ libs = [ a: \"com.acme:a:1\" ] }}\n{form}\n");
            let (n, u) = one_project(
                &format!("{APPLY_DEPS}dependencies {{\n  implementation libs.a\n  implementation libs.e\n}}\n"),
                Some(&deps),
                None,
            );
            assert!(n.is_empty(), "{form}: {n:?}");
            assert_eq!(
                u,
                Some((
                    2,
                    "build.gradle:3 libs.a: alias map in gradle/deps.gradle:2 is not a top-level literal assignment"
                        .to_string()
                )),
                "{form}"
            );
        }
    }

    /// The kafka shape under D-DGC1B-MAP-GRAMMAR-1: `ext { versions = [:]; libs = [:] }`, a
    /// condition that assigns `versions` only, and a top-level `libs += [ … ]` — readable; the alias
    /// binds and nothing is counted. Uses proven not to write the map do not make it unreadable: a
    /// map-key label `libs:`, unrelated `"libs"` string literals (`println "libs"`), and proven reads
    /// (`libs.zstd`, `libs['zstd']`, `"${libs.zstd}"`, `"$libs"`, `ext.libs.zstd`).
    #[test]
    fn alias_map_in_an_unconditional_ext_block_then_a_top_level_addition_binds() {
        let (n, u) = one_project(
            &format!("{APPLY_DEPS}dependencies {{\n  implementation libs.zstd\n}}\n"),
            Some(KAFKA_SHAPE_WITH_READS),
            None,
        );
        assert_eq!(n, vec!["com.github.luben"]);
        assert_eq!(u, None);
    }

    /// The kafka shape plus uses that are proven not to write the map (review-1 F2 of admission 3):
    /// unrelated `"libs"` string literals, a key label, proven reads in code and in GStrings.
    const KAFKA_SHAPE_WITH_READS: &str = "ext { versions = [:]; libs = [:] }\n\
        if (hasProperty(\"scalaVersion\")) {\n  versions[\"scala\"] = \"2.13\"\n}\n\
        versions += [ libs: \"2.0\" ]\n\
        libs += [\n  zstd: \"com.github.luben:zstd-jni:$versions.zstd\",\n]\n\
        println \"libs\"\n\
        logger.info('libs')\n\
        def names = [\"libs\", \"other\"]\n\
        ext.zstdCoordinate = libs.zstd\n\
        def z = libs['zstd']\n\
        if (libs.zstd == null) { println \"missing\" }\n\
        println \"${libs.zstd} $libs.zstd $libs\"\n\
        println ext.libs.zstd\n\
        println ext.\"libs\".zstd\n\
        println libs\n\
        print(libs)\n\
        println \"${libs}\"\n";

    /// An unconditional empty map yields no entries: a reference is counted `no catalog entry`,
    /// never with the grammar form, and no group is declared.
    #[test]
    fn alias_map_with_only_an_empty_literal_yields_no_entries() {
        let (n, u) = one_project(
            &format!("{APPLY_DEPS}dependencies {{\n  implementation libs.foo\n}}\n"),
            Some("ext { libs = [:] }\n"),
            None,
        );
        assert!(n.is_empty(), "{n:?}");
        assert_eq!(
            u,
            Some((1, "build.gradle:3 libs.foo: no catalog entry".to_string()))
        );
    }
}
