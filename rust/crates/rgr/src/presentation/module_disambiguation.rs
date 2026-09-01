//! Shared twin-name module disambiguation (MODULES-IDENTITY-2 §2.1).
//!
//! Abstraction one-liner:
//! - WHAT: `ModuleRow` (a self-describing module row — path / declared name /
//!   owning manifest) + [`collision_disambiguator`] (given the rendered rows, the
//!   disambiguating token for a row whose effective display name COLLIDES with
//!   another: the manifest when it distinguishes this row from every collider,
//!   else the canonical path — the honest tie-break when namesakes share a
//!   manifest).
//! - CONCRETE CURRENT USERS: `orient_seg2::module_row_label` (orient's "Modules
//!   (by size)" breakdown) AND `modules_list::render_human` (the `modules list`
//!   catalog). TWO concrete callers.
//! - AXIS: de-duplication of ONE algorithm across two renderers — orient already
//!   shipped it (v0.9.0 fix #5); MODULES-IDENTITY-2 §2.1 requires modules-list to
//!   reuse the SAME derivation, "one implementation, never a second copy". Not a
//!   growth-axis abstraction (no expected variation) — a shared-logic extraction.
//! - REJECTED SIMPLER: copying the collision algorithm into `modules_list` — the
//!   slice forbids a second copy (drift between orient and modules-list is exactly
//!   the identity divergence this slice kills).
//!
//! The two renderers differ ONLY in their base label (orient renders the PATH by
//! default; modules-list renders the DISPLAY NAME), so this module owns the shared
//! COLLISION → suffix-token decision, not the full label; each caller wraps the
//! token in its own `base [token]` shape.

/// A self-describing module row for disambiguation: NOT keyed by path, because two
/// modules can share a `canonical_root_path` (django declares two `Django` modules
/// both rooted at `.`), so each row carries its own declared `name` + owning
/// `manifest`.
pub(crate) struct ModuleRow<'a> {
    pub path: &'a str,
    pub name: Option<&'a str>,
    pub manifest: Option<&'a str>,
}

impl<'a> ModuleRow<'a> {
    /// Project a row from one `top_modules[i]` JSON object (orient's evidence shape).
    pub(crate) fn from_json(m: &'a serde_json::Value) -> Self {
        let path = m
            .get("path")
            .and_then(|p| p.as_str())
            .unwrap_or("(unknown)");
        let name = m
            .get("name")
            .and_then(|v| v.as_str())
            .filter(|n| !n.is_empty());
        let manifest = m.get("manifest").and_then(|v| v.as_str());
        Self {
            path,
            name,
            manifest,
        }
    }

    /// The name a collision is detected on: the declared name, else the path.
    pub(crate) fn effective_name(&self) -> &'a str {
        self.name.unwrap_or(self.path)
    }
}

/// The disambiguating token for `rows[idx]`, or `None` when its effective display
/// name is UNIQUE among the rendered rows (no disambiguation needed — the caller
/// renders its base label verbatim, so unique rows stay byte-identical).
///
/// When it collides: the MANIFEST when it distinguishes this row from EVERY
/// collider (`Django [pyproject.toml]` vs `Django [package.json]`); otherwise the
/// canonical PATH — the honest tie-break when two namesakes share a manifest (the
/// manifest alone would leave them label-identical), or when this row has no
/// manifest at all.
pub(crate) fn collision_disambiguator<'a>(
    rows: &[ModuleRow<'a>],
    effective_names: &[&str],
    idx: usize,
) -> Option<&'a str> {
    let this_name = effective_names[idx];
    let colliders: Vec<usize> = (0..effective_names.len())
        .filter(|&j| j != idx && effective_names[j] == this_name)
        .collect();
    if colliders.is_empty() {
        return None;
    }
    // Does the manifest distinguish this row from EVERY collider? Only then is it a
    // valid disambiguator; else same-manifest namesakes fall back to the path.
    let manifest_disambiguates = rows[idx].manifest.is_some()
        && colliders
            .iter()
            .all(|&j| rows[j].manifest != rows[idx].manifest);
    Some(if manifest_disambiguates {
        rows[idx].manifest.unwrap()
    } else {
        rows[idx].path
    })
}
