//! Dependency reconciliation DTOs.
//!
//! These types are the contract between the reconciliation engine
//! and its consumers (CLI, future APIs).

use serde::{Deserialize, Serialize};

/// Dependency category in a reconciliation summary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyCategory {
    /// In manifest AND observed in source imports.
    DeclaredAndUsed,
    /// DEPS-CLASSIFIER-1B §2.2 item 3: in manifest, and the ONLY evidence of use is a type-only
    /// import (`import type … from "pkg"`) — a compile-time type reference, not a runtime
    /// dependency. Distinct from `DeclaredAndUsed` (a value import/call proves runtime use) and from
    /// `DeclaredButUnobserved` (no import of any kind). A package with BOTH a value use and a
    /// type-only import is `DeclaredAndUsed` (value use wins); this reaches only otherwise-unobserved
    /// declared packages.
    TypeOnlyImport,
    /// In manifest but no source imports found.
    DeclaredButUnobserved,
    /// Source imports exist but not in manifest.
    ObservedButUndeclared,
    /// DEPS-SELF-1 (FINAL-POLISH-1 §2.2): the observed specifier equals THIS repo's OWN parsed
    /// manifest package name (from the `module_kind='declared'` `display_name` fact — never a
    /// directory name), so it is a first-party self-reference, not a third-party external. django
    /// importing `django` lands here instead of `ObservedButUndeclared`. A DECLARED dependency of
    /// the same name wins over this (a real declared dep stays `DeclaredAndUsed`), so this is
    /// reached only for the otherwise-undeclared self-import.
    FirstPartySelf,
    /// Runtime builtin module (fs, path, std::*).
    RuntimeBuiltin,
    /// External-looking specifier, classification unclear.
    UnknownExternalLike,
}

impl DependencyCategory {
    /// True iff this category represents a package DECLARED in the manifest (regardless of how — or
    /// whether — it is observed in source). The three declared categories are `DeclaredAndUsed`
    /// (value use), `TypeOnlyImport` (only a compile-time `import type`), and `DeclaredButUnobserved`
    /// (no import at all). `ObservedButUndeclared`, `FirstPartySelf`, `RuntimeBuiltin`, and
    /// `UnknownExternalLike` are NOT manifest-declared.
    ///
    /// Single source of truth for "is this a declared dependency", consumed by the `deps why`
    /// declared-status flag (`daemon-runtime/dispatch.rs`) and the secondary-ecosystem declared
    /// total (`daemon-runtime/deps_ecosystem_presence.rs`). The match is EXHAUSTIVE with no wildcard
    /// arm on purpose: when a future `DependencyCategory` variant is added, the compiler forces every
    /// declared/not-declared decision to be revisited here — the safeguard whose absence let
    /// `TypeOnlyImport` (added in DEPS-CLASSIFIER-1B increment 1) silently read as not-declared at
    /// two inline `matches!` sites (review-1 findings 1 and 2).
    ///
    /// Abstraction one-liner — what: a domain predicate on the category sum; users: the `deps why`
    /// declared flag + the ecosystem-presence declared total (two concrete callers today); axis:
    /// which categories count as "declared", a set that just grew by one variant and will grow
    /// again; rejected simpler alternative: two independent inline `matches!` lists — exactly what
    /// drifted out of sync and produced both review findings.
    pub fn is_declared_manifest_dependency(self) -> bool {
        match self {
            DependencyCategory::DeclaredAndUsed
            | DependencyCategory::TypeOnlyImport
            | DependencyCategory::DeclaredButUnobserved => true,
            DependencyCategory::ObservedButUndeclared
            | DependencyCategory::FirstPartySelf
            | DependencyCategory::RuntimeBuiltin
            | DependencyCategory::UnknownExternalLike => false,
        }
    }
}

/// One observed external reference fed to reconciliation (DEPS-CLASSIFIER-1 §2.3).
///
/// Carries the raw specifier AND whether it came from an IMPORTS edge (an "import site") or a
/// CALL edge (a "call site"), so the per-package basis can report the two counts separately.
/// A plain specifier string lost that distinction; a bool alongside it recovers it without a
/// parallel array.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedImportRef {
    /// The raw observed specifier (already identifier→specifier resolved by `compose`).
    pub specifier: String,
    /// `true` = an IMPORTS-edge site; `false` = a CALL-edge site.
    pub is_import_edge: bool,
}

/// A single package usage in a dependency summary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackageUsage {
    /// Normalized package name (e.g., "react", "lodash", "tokio").
    pub package: String,
    /// Number of import statements referencing this package.
    pub import_count: usize,
    /// Dependency class from manifest (prod, dev, peer, optional).
    /// `None` if not declared or manifest doesn't provide class.
    pub dependency_class: Option<String>,
    /// Confidence score (1.0 for exact match, <1.0 for ambiguous).
    pub confidence: f64,
}

/// Provenance of the manifest a module's declared dependencies were parsed from
/// (DEPS-LIST-REWRITE-1 §2.2; operator ruling 2026-08-26).
///
/// Orthogonal to [`ModuleDependencySummary::manifest_scope_available`] (which gates
/// reconciliation): a module can carry declared deps to reconcile against while its exact
/// manifest FILE is unknown (a snapshot indexed before provenance tracking). Sum type rather
/// than `Option<String> + bool` because a "note" written into a path string is a
/// fabricated-looking value — the exact defect this slice removes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ManifestContext {
    /// The exact manifest file parsed at index time (`path` is repo-relative, e.g.
    /// `spring-petclinic/build.gradle.kts` — rendered as itself, never a fixed-name guess).
    Parsed { path: String },
    /// Declared deps were parsed, but the exact manifest FILE could not be pinned. `reason`
    /// is the SPECIFIC honest cause (operator ruling 3, item 2) — never collapsed to one label:
    /// "indexed before provenance tracking" (old snapshot), "extraction diagnostics unreadable:
    /// …" (read failure), "provenance record malformed: …" (corruption), or "declared deps parsed
    /// but no manifest record covers this module". Always unknown-with-reason, never a fabricated
    /// default and never a query-time filesystem rescan.
    ProvenanceUnavailable { reason: String },
    /// No manifest reader ran / no owning manifest for this module.
    Absent,
}

impl ManifestContext {
    /// The exact parsed manifest path, if known. `None` for both unavailable and absent —
    /// callers distinguish those via [`Self::unavailable_note`].
    pub fn path(&self) -> Option<&str> {
        match self {
            ManifestContext::Parsed { path } => Some(path.as_str()),
            _ => None,
        }
    }

    /// The specific unknown-with-reason note when deps were parsed but the exact manifest file
    /// could not be pinned (operator ruling 3, item 2 — the cause is carried verbatim, not
    /// collapsed to a single "predates tracking" label that would lie about a read failure).
    pub fn unavailable_note(&self) -> Option<&str> {
        match self {
            ManifestContext::ProvenanceUnavailable { reason } => Some(reason.as_str()),
            _ => None,
        }
    }
}

/// Outcome of reading the persisted manifest provenance for a snapshot (operator ruling 3,
/// item 2). A quad-state rather than `Option<Vec<_>>` because the three "no records" causes are NOT
/// the same fact — an old snapshot (absent) must not be reported with a corrupt-blob's reason, and
/// vice versa. Produced by the query layer's diagnostics read, consumed by [`super::compose`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProvenanceRead {
    /// Provenance was tracked; these are the recorded manifests — PARSED, or FAILED with a reason
    /// and kind (possibly empty = tracked, none recorded). Each module attaches its exact manifest
    /// by longest-ancestor-`dir` match.
    Tracked(Vec<ManifestProvenance>),
    /// The snapshot predates manifest-provenance tracking (no `deps_manifests` diagnostics record).
    /// Renders "indexed before provenance tracking".
    Absent,
    /// The provenance record could not be read/parsed (diagnostics blob unreadable, not valid JSON,
    /// or a malformed `deps_manifests` value). `reason` is the specific honest cause — NEVER
    /// conflated with the old-snapshot case.
    Unavailable { reason: String },
}

/// A persisted manifest-provenance record — PARSED, or FAILED with its reason and kind
/// (DEPS-LIST-REWRITE-1 §2.2; operator ruling 2026-08-26; D-DGC-BOUNDARY-1 (9)).
///
/// Written at index time through the extraction-diagnostics channel (the `deps_manifests` key
/// beside `index_basis`) BEFORE the Ready flip, read back at query time. Raw strings — a boundary
/// DTO, no framework/hardware types. `dir` is the manifest's repo-relative directory; the query
/// layer attaches a manifest to a module by longest-ancestor-`dir` match against these records
/// (the same nearest-manifest semantics the index used), never a filesystem rescan.
///
/// Decoding goes through [`ManifestProvenanceWire`] so an `error_kind` on a record WITHOUT `error`
/// is rejected (D-DGC-BOUNDARY-1 (9)): the whole `deps_manifests` decode then fails and the caller's
/// `ProvenanceRead::Unavailable` path renders unknown-with-reason, never a default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ManifestProvenanceWire")]
pub struct ManifestProvenance {
    /// Repo-relative path of the manifest file the resolver ENCOUNTERED (`build.gradle.kts`,
    /// `pyproject.toml`). Present whether the record parsed or FAILED — [`Self::error`] and
    /// [`Self::error_kind`] distinguish the cases.
    pub path: String,
    /// Repo-relative directory the manifest governs (used for module attribution).
    pub dir: String,
    /// Ecosystem the reader belongs to (`npm`/`cargo`/`python`/`java`).
    pub ecosystem: String,
    /// `None` = the manifest was read AND parsed (declared deps possibly empty — a legitimate
    /// measured-empty, ruling-3 item 3). `Some(reason)` = FAILED: the declared set this record
    /// stands for is unknown — [`Self::error_kind`] says whether the manifest's own read or parse
    /// failed (an io read error, or malformed content the reader could detect) or Gradle declaration
    /// attribution could not be established. Review-4 item 1: a failure is NEVER laundered into a
    /// parsed zero-dep.
    /// `#[serde(default)]` so a snapshot written before this field decodes as parsed
    /// (backward-compatible).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// D-DGC-BOUNDARY-1 (9): which failure [`Self::error`] states — `Some(Attribution)` when Gradle
    /// project/declaration attribution could not be established (an unreadable settings file or
    /// project script of the build, an unhandled `projectDir` form). It states the outcome only —
    /// not whether the recorded file was read, nor where the failed input resides (the record may
    /// be an unknown build's `settings.gradle` itself, unreadable). `None` on a FAILED record is
    /// the `parse` kind: every record written
    /// before this field, and every parse failure written after it, carries no key. A decoded
    /// `"parse"` is normalised to `None`, so a parse failure re-serializes without the key. Read it
    /// through [`Self::failure_note`], the one place a failed record is worded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_kind: Option<ManifestErrorKind>,
    /// DEPS-GRADLE-CATALOG-1A (D-DGC-CONDITIONAL-1; RG-REQ-002-L11): the Gradle `dependencies`
    /// blocks the index-time reader skipped because it cannot attribute them statically (under a
    /// condition, callback, receiver or composed scope head). One record per Gradle build carries
    /// its build's marking, so a sum over records counts distinct blocks. `None` = no marking (every
    /// non-Gradle record, a build with no such block, a record written before this field — `default`
    /// keeps those decoding as today). A malformed value fails the whole `deps_manifests` decode, so
    /// the caller's `ProvenanceRead::Unavailable` path renders unknown-with-reason, never a zero.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub undetermined_blocks: Option<UndeterminedBlocks>,
    /// DEPS-GRADLE-CATALOG-1B (D-DGC1B-ALIAS-MARKING-1; RG-REQ-002-L11): the version-catalog alias
    /// references of this record's Gradle build that the index could not bind to a group — counted,
    /// never guessed. Carried by the same one record per build as [`Self::undetermined_blocks`], so
    /// a sum over records counts each reference once. `None` = no such reference (every non-Gradle
    /// record, a record written before this field). A malformed value fails the whole
    /// `deps_manifests` decode (`ProvenanceRead::Unavailable`), never a zero.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unresolved_alias_refs: Option<UnresolvedAliasRefs>,
}

impl ManifestProvenance {
    /// The reader-facing statement of a FAILED record, by its kind (D-DGC-BOUNDARY-1 (9)) — the ONE
    /// place a failed record is worded; every surface that states one calls this. `None` for a
    /// parsed record. A `parse` failure keeps today's exact wording; an `attribution` failure says
    /// attribution failed — never "not parsed" (the kind does not state a parse failure) and never a
    /// claim that the file was read (an unknown build's `settings.gradle` record may itself be
    /// unreadable).
    pub fn failure_note(&self) -> Option<String> {
        let reason = self.error.as_deref()?;
        Some(match self.error_kind {
            Some(ManifestErrorKind::Attribution) => format!(
                "manifest {} — dependency attribution failed: {}",
                self.path, reason
            ),
            Some(ManifestErrorKind::Parse) | None => {
                format!("manifest {} present but not parsed: {}", self.path, reason)
            }
        })
    }
}

/// Which failure a FAILED [`ManifestProvenance`] record states (D-DGC-BOUNDARY-1 (9); the wire
/// mirror of `repo-index`'s record field). Wire values `"parse"` and `"attribution"`; any other
/// value fails the decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ManifestErrorKind {
    /// The manifest's own read or parse failed.
    Parse,
    /// Gradle project/declaration attribution could not be established (a settings-file failure
    /// included). No claim about whether the recorded file was read or where the failed input
    /// resides.
    Attribution,
}

/// The persisted shape of [`ManifestProvenance`] as decoded, before the kind/error consistency
/// check (`#[serde(try_from)]`). Field-for-field the same wire contract.
#[derive(Deserialize)]
struct ManifestProvenanceWire {
    path: String,
    dir: String,
    ecosystem: String,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    error_kind: Option<ManifestErrorKind>,
    #[serde(default)]
    undetermined_blocks: Option<UndeterminedBlocks>,
    #[serde(default)]
    unresolved_alias_refs: Option<UnresolvedAliasRefs>,
}

impl TryFrom<ManifestProvenanceWire> for ManifestProvenance {
    type Error = String;

    /// Reject `error_kind` on a record without `error` (a malformed record — the kind of a failure
    /// that did not happen); normalise `"parse"` to the absent key it means.
    fn try_from(w: ManifestProvenanceWire) -> Result<Self, String> {
        if w.error.is_none() && w.error_kind.is_some() {
            return Err(format!(
                "manifest record {} carries error_kind without error",
                w.path
            ));
        }
        Ok(ManifestProvenance {
            path: w.path,
            dir: w.dir,
            ecosystem: w.ecosystem,
            error: w.error,
            error_kind: w
                .error_kind
                .filter(|k| *k == ManifestErrorKind::Attribution),
            undetermined_blocks: w.undetermined_blocks,
            unresolved_alias_refs: w.unresolved_alias_refs,
        })
    }
}

/// The count and first location of the Gradle `dependencies` blocks one build's reader could not
/// attribute statically (DEPS-GRADLE-CATALOG-1A; the wire mirror of `repo-index`'s record field).
/// `count` counts skipped BLOCKS, not declarations; `first` is `<repo-relative script path>:<line>`
/// of the first one (the root project's script first).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UndeterminedBlocks {
    /// Number of skipped `dependencies` blocks in the build.
    pub count: u32,
    /// `<repo-relative path>:<line>` of the first skipped block.
    pub first: String,
}

/// The count and first site of the version-catalog alias references one Gradle build's index
/// could not bind to a group (DEPS-GRADLE-CATALOG-1B; the wire mirror of `repo-index`'s record
/// field). `first` is `<repo-relative path>:<line> <accessor>: <predicate>` — the counted site with
/// the smallest `(path, line)` and the binding test that failed for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnresolvedAliasRefs {
    /// Number of alias references of the build that bound to no group.
    pub count: u32,
    /// The first counted site and the predicate that failed for it.
    pub first: String,
}

/// A single entry in a dependency summary.
/// Note: No Eq impl because confidence is f64.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DependencyEntry {
    /// Normalized package name.
    pub package: String,
    /// Dependency category.
    pub category: DependencyCategory,
    /// Number of external references (import sites + call sites) attributed to this package.
    pub import_count: usize,
    /// DEPS-CLASSIFIER-1 §2.3: how many of `import_count` are IMPORTS-edge sites (`from x import y`,
    /// `use x::y`) — the rest (`import_count - import_sites`) are CALL sites. Lets the per-package
    /// basis read `used (N import sites, M call sites)`. `#[serde(default)]`: additive over the
    /// stored/wire shape — an old payload without this field reads 0 (renders as call-sites-only).
    #[serde(default)]
    pub import_sites: usize,
    /// Dependency class from manifest (prod, dev, peer, optional).
    pub dependency_class: Option<String>,
    /// Confidence score for classification.
    pub confidence: f64,
    /// Raw specifiers that normalized to this package (for debugging).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub raw_specifiers: Vec<String>,
}

/// Module-level dependency summary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleDependencySummary {
    /// Module identifier (canonical_root_path).
    pub module: String,
    /// Provenance of the manifest this module's declared deps were parsed from (§2.2). Never a
    /// fabricated fixed-name path: exact file when provenance was tracked, else unavailable/absent.
    pub manifest_context: ManifestContext,
    /// Whether manifest dependency context is available for RECONCILIATION (declared deps exist to
    /// join against). Orthogonal to `manifest_context`: an old snapshot can have scope available
    /// (declared deps present) while its exact manifest file is `ProvenanceUnavailable`.
    pub manifest_scope_available: bool,
    /// Dependencies by category.
    pub entries: Vec<DependencyEntry>,
    /// Count of observed external references dropped as non-import call-expression text
    /// (DEPS-LIST-REWRITE-1 §2.1) — kept for honest diagnostics, never hoisted into a
    /// package category. `0` means the classifier rejected nothing for this module.
    pub rejected_non_specifier: usize,
    /// HONESTY-GATE-1 §2.2 (arithmetic reconciliation): the DISTINCT parsed manifests that actually
    /// contributed this module's declared dependencies, each computed as the longest-ancestor parsed
    /// provenance manifest of a declared-dep source file. For a leaf module this is exactly the one
    /// manifest the row cites (byte-parity). For a COARSE module that owns files spanning several
    /// nested manifests (storybook's root `.` owning `test-storybooks/*/package.json` sources), it
    /// is the full set — so the renderer states "declared N across M manifests" instead of citing a
    /// 124-dep union against a single 13-dep `package.json` (the arithmetically-impossible defect).
    /// Empty when provenance was untracked/unreadable (no structural basis to attribute) — the row
    /// then falls back to its single `manifest_context`, never a fabricated breakdown. Sorted, distinct.
    pub declared_manifest_paths: Vec<String>,
}

impl ModuleDependencySummary {
    /// Get entries by category.
    pub fn by_category(&self, category: DependencyCategory) -> Vec<&DependencyEntry> {
        self.entries
            .iter()
            .filter(|e| e.category == category)
            .collect()
    }

    /// Count of declared and used dependencies.
    pub fn declared_and_used_count(&self) -> usize {
        self.by_category(DependencyCategory::DeclaredAndUsed).len()
    }

    /// Count of declared but unobserved dependencies.
    pub fn declared_but_unobserved_count(&self) -> usize {
        self.by_category(DependencyCategory::DeclaredButUnobserved)
            .len()
    }

    /// DEPS-CLASSIFIER-1B §2.2 item 3: count of declared dependencies whose only evidence of use is
    /// a type-only import.
    pub fn type_only_import_count(&self) -> usize {
        self.by_category(DependencyCategory::TypeOnlyImport).len()
    }

    /// Count of observed but undeclared dependencies.
    pub fn observed_but_undeclared_count(&self) -> usize {
        self.by_category(DependencyCategory::ObservedButUndeclared)
            .len()
    }

    /// Count of runtime builtin usages.
    pub fn runtime_builtins_count(&self) -> usize {
        self.by_category(DependencyCategory::RuntimeBuiltin).len()
    }

    /// DEPS-SELF-1: count of first-party self-references (observed specifier equal to this repo's
    /// own parsed-manifest package name). Rendered as a `self` note, never counted as undeclared.
    pub fn first_party_self_count(&self) -> usize {
        self.by_category(DependencyCategory::FirstPartySelf).len()
    }

    /// Count of external-looking specifiers that could not be classified declared/undeclared
    /// because no manifest scope was available (the `none-detected` per-module case). Surfaced so a
    /// scope-unavailable module row is NOT rendered as a deceptive `0/0/0/0` when it in fact has
    /// external references (leveldb's C/C++ `<algorithm>`/`<cassert>` includes).
    pub fn unknown_external_like_count(&self) -> usize {
        self.by_category(DependencyCategory::UnknownExternalLike)
            .len()
    }
}

/// Kind of dependency drift anomaly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriftKind {
    /// Dependency declared but never imported.
    UnusedDeclared,
    /// Import observed but dependency not declared.
    UndeclaredUsage,
    /// External-looking specifier that couldn't be classified.
    UnknownExternal,
}

/// A single drift anomaly entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriftEntry {
    /// Module where the anomaly was detected.
    pub module: String,
    /// Package or specifier involved.
    pub package: String,
    /// Kind of drift.
    pub kind: DriftKind,
    /// Number of imports (for usage-based drift).
    pub import_count: usize,
    /// Hint for resolution (e.g., "likely devDependency missing").
    pub hint: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Focused regression for review-1 findings 1 & 2: a `TypeOnlyImport` IS a declared manifest
    /// dependency. The pre-fix inline `matches!` sites in `deps why` and the ecosystem-presence total
    /// omitted it, returning `declared: false` for a package the manifest declares and understating
    /// an ecosystem's declared count. This pins the full declared/not-declared partition so a future
    /// variant cannot silently regress it (the match is exhaustive; adding a variant breaks this).
    #[test]
    fn is_declared_manifest_dependency_partitions_categories() {
        for declared in [
            DependencyCategory::DeclaredAndUsed,
            DependencyCategory::TypeOnlyImport,
            DependencyCategory::DeclaredButUnobserved,
        ] {
            assert!(
                declared.is_declared_manifest_dependency(),
                "{declared:?} must count as a declared manifest dependency"
            );
        }
        for not_declared in [
            DependencyCategory::ObservedButUndeclared,
            DependencyCategory::FirstPartySelf,
            DependencyCategory::RuntimeBuiltin,
            DependencyCategory::UnknownExternalLike,
        ] {
            assert!(
                !not_declared.is_declared_manifest_dependency(),
                "{not_declared:?} must NOT count as a declared manifest dependency"
            );
        }
    }

    /// A minimal JSON-shaped value for decode tests: this crate has no `serde_json` (and the slice
    /// adds no dependency), so the wire shape is driven through serde's own value deserializers.
    enum J {
        S(&'static str),
        I(i64),
        M(Vec<(&'static str, J)>),
        A(Vec<J>),
    }

    impl<'de> serde::Deserializer<'de> for J {
        type Error = serde::de::value::Error;
        fn deserialize_any<V: serde::de::Visitor<'de>>(
            self,
            v: V,
        ) -> Result<V::Value, Self::Error> {
            match self {
                J::S(s) => v.visit_borrowed_str(s),
                J::I(n) => v.visit_i64(n),
                J::M(entries) => {
                    v.visit_map(serde::de::value::MapDeserializer::new(entries.into_iter()))
                }
                J::A(items) => {
                    v.visit_seq(serde::de::value::SeqDeserializer::new(items.into_iter()))
                }
            }
        }
        fn deserialize_option<V: serde::de::Visitor<'de>>(
            self,
            v: V,
        ) -> Result<V::Value, Self::Error> {
            v.visit_some(self)
        }
        /// A string is a unit variant (as `serde_json` reads `"attribution"`); anything else is
        /// handed to the visitor as-is, which rejects it.
        fn deserialize_enum<V: serde::de::Visitor<'de>>(
            self,
            _name: &'static str,
            _variants: &'static [&'static str],
            v: V,
        ) -> Result<V::Value, Self::Error> {
            match self {
                J::S(s) => {
                    v.visit_enum(serde::de::IntoDeserializer::<Self::Error>::into_deserializer(s))
                }
                other => other.deserialize_any(v),
            }
        }
        serde::forward_to_deserialize_any! {
            bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf
            unit unit_struct newtype_struct seq tuple tuple_struct map struct identifier
            ignored_any
        }
    }

    impl<'de> serde::de::IntoDeserializer<'de, serde::de::value::Error> for J {
        type Deserializer = J;
        fn into_deserializer(self) -> J {
            self
        }
    }

    fn java_record(extra: Option<J>) -> J {
        let mut fields = vec![
            ("path", J::S("build.gradle")),
            ("dir", J::S("")),
            ("ecosystem", J::S("java")),
        ];
        if let Some(e) = extra {
            fields.push(("undetermined_blocks", e));
        }
        J::A(vec![J::M(fields)])
    }

    fn decode(v: J) -> Result<Vec<ManifestProvenance>, serde::de::value::Error> {
        serde::Deserialize::deserialize(v)
    }

    /// DEPS-GRADLE-CATALOG-1A (D-DGC-CONDITIONAL-1): the wire mirror of the index-time marking.
    /// ABSENT key → `None` (today's records, every non-Gradle record); PRESENT-VALID → `Some`;
    /// PRESENT-MALFORMED (a non-integer or negative `count`, a missing `first`) → the whole
    /// `deps_manifests` vector fails to decode, so the caller's existing
    /// `ProvenanceRead::Unavailable` path renders unknown-with-reason — never a silent zero.
    #[test]
    fn manifest_provenance_undetermined_blocks_absent_present_and_malformed() {
        let absent = decode(java_record(None)).unwrap();
        assert_eq!(absent[0].undetermined_blocks, None);
        assert_eq!(absent[0].error, None);

        let present = decode(java_record(Some(J::M(vec![
            ("count", J::I(5)),
            ("first", J::S("build.gradle:190")),
        ]))))
        .unwrap();
        assert_eq!(
            present[0].undetermined_blocks,
            Some(UndeterminedBlocks {
                count: 5,
                first: "build.gradle:190".to_string()
            })
        );

        for (label, malformed) in [
            (
                "non-integer count",
                J::M(vec![("count", J::S("five")), ("first", J::S("b:1"))]),
            ),
            ("missing first", J::M(vec![("count", J::I(5))])),
            (
                "negative count",
                J::M(vec![("count", J::I(-1)), ("first", J::S("b:1"))]),
            ),
        ] {
            assert!(
                decode(java_record(Some(malformed))).is_err(),
                "a malformed marking must fail the decode, never default: {label}"
            );
        }
    }

    /// DEPS-GRADLE-CATALOG-1B (D-DGC1B-ALIAS-MARKING-1): the wire mirror of the index-time count of
    /// alias references that bound to no group — the 1A `undetermined_blocks` triple, second
    /// instance. ABSENT → `None`; PRESENT-VALID → `Some` with its `first`; PRESENT-MALFORMED (a
    /// non-integer or negative `count`, a missing `first`) → the whole `deps_manifests` vector fails
    /// to decode (the caller's `ProvenanceRead::Unavailable`, unknown-with-reason, never a zero).
    #[test]
    fn manifest_provenance_unresolved_alias_refs_absent_present_and_malformed() {
        let record = |extra: Option<J>| {
            let mut fields = vec![
                ("path", J::S("build.gradle")),
                ("dir", J::S("")),
                ("ecosystem", J::S("java")),
            ];
            if let Some(e) = extra {
                fields.push(("unresolved_alias_refs", e));
            }
            J::A(vec![J::M(fields)])
        };
        let absent = decode(record(None)).unwrap();
        assert_eq!(absent[0].unresolved_alias_refs, None);
        assert_eq!(absent[0].undetermined_blocks, None);

        let present = decode(record(Some(J::M(vec![
            ("count", J::I(2)),
            ("first", J::S("build.gradle:5 libs.nope: no catalog entry")),
        ]))))
        .unwrap();
        assert_eq!(
            present[0].unresolved_alias_refs,
            Some(UnresolvedAliasRefs {
                count: 2,
                first: "build.gradle:5 libs.nope: no catalog entry".to_string()
            })
        );

        for (label, malformed) in [
            (
                "non-integer count",
                J::M(vec![
                    ("count", J::S("two")),
                    ("first", J::S("b:1 libs.x: no catalog entry")),
                ]),
            ),
            ("missing first", J::M(vec![("count", J::I(2))])),
            (
                "negative count",
                J::M(vec![
                    ("count", J::I(-1)),
                    ("first", J::S("b:1 libs.x: no catalog entry")),
                ]),
            ),
        ] {
            assert!(
                decode(record(Some(malformed))).is_err(),
                "a malformed alias marking must fail the decode, never default: {label}"
            );
        }
    }

    fn one_record(fields: Vec<(&'static str, J)>) -> J {
        J::A(vec![J::M(fields)])
    }

    /// D-DGC-BOUNDARY-1 (9): the failure kind on the wire. ABSENT on a FAILED record → `parse`
    /// (every record a store holds today); `"parse"` → parse; `"attribution"` → attribution, worded
    /// as such; an unknown value, or the key on a record WITHOUT `error`, fails the whole
    /// `deps_manifests` decode (the caller's `Unavailable` path) — never a default.
    #[test]
    fn manifest_provenance_error_kind_absent_reads_as_parse_and_malformed_is_rejected() {
        let base = || {
            vec![
                ("path", J::S("a/build.gradle")),
                ("dir", J::S("a")),
                ("ecosystem", J::S("java")),
            ]
        };
        let with = |extra: Vec<(&'static str, J)>| {
            let mut f = base();
            f.extend(extra);
            one_record(f)
        };
        let today = decode(with(vec![("error", J::S("unreadable: x"))])).unwrap();
        assert_eq!(today[0].error_kind, None);
        assert_eq!(
            today[0].failure_note().as_deref(),
            Some("manifest a/build.gradle present but not parsed: unreadable: x")
        );
        let parse = decode(with(vec![
            ("error", J::S("unreadable: x")),
            ("error_kind", J::S("parse")),
        ]))
        .unwrap();
        assert_eq!(
            parse[0].failure_note().as_deref(),
            Some("manifest a/build.gradle present but not parsed: unreadable: x")
        );
        assert_eq!(
            parse[0], today[0],
            "`parse` is normalised to the absent key"
        );
        let attribution = decode(with(vec![
            ("error", J::S("unreadable: x")),
            ("error_kind", J::S("attribution")),
        ]))
        .unwrap();
        assert_eq!(
            attribution[0].failure_note().as_deref(),
            Some("manifest a/build.gradle — dependency attribution failed: unreadable: x")
        );
        let parsed = decode(with(vec![])).unwrap();
        assert_eq!(parsed[0].failure_note(), None);

        assert!(
            decode(with(vec![
                ("error", J::S("unreadable: x")),
                ("error_kind", J::S("guess")),
            ]))
            .is_err(),
            "an unknown kind fails the decode"
        );
        assert!(
            decode(with(vec![("error_kind", J::S("attribution"))])).is_err(),
            "a kind without `error` fails the decode"
        );

        // A parse failure re-serializes without `error_kind` (field presence via a
        // `Serialize`-agnostic probe: the Option stays `None`, which `skip_serializing_if` omits).
        assert!(today[0].error_kind.is_none() && parse[0].error_kind.is_none());
    }
}
