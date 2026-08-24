//! Parse-status footer axis (INDEX-BASIS-1, review-0 fix #2).
//!
//! The orient serving footer previously derived a `parse …` word from the
//! coherence-envelope `freshness` MEET — a name/semantics mismatch: the meet is the
//! serving freshness (it can be `PrecisionPending` mid-refresh), NOT parse status.
//! The operator ruling (2026-08-24) split them: the envelope KEEPS its `freshness`
//! name, and `parse` gets its OWN honest value computed from the unparsed-files read
//! (`get_stale_files`) — the same source `check`'s `UNPARSED_FILES` condition uses.
//!
//! This is a **domain sum type**: the three outcomes of that read are mutually
//! exclusive, and a FAILED read is `Unknown` (with the reason) — NEVER coerced to
//! `Ok`/zero (the standing honesty rule forbids `unwrap_or(0)` on a rendered read).
//! `Option<u64>` was rejected: it cannot distinguish a read failure (unknown) from a
//! missing field (old daemon), and cannot carry the failure reason.
//!
//! Pure data + one render helper. The daemon (composition root) constructs it from
//! the storage read; this crate performs no I/O.

use serde::{Deserialize, Serialize};

/// The parse status of the indexed snapshot, for the orient serving footer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ParseStatus {
    /// Every indexed file parsed (`get_stale_files` returned empty).
    Ok,
    /// `count` indexed files could not be parsed (their recorded parse state is
    /// behind the stored file version).
    Unparsed { count: u64 },
    /// The unparsed-files read failed — parse status is genuinely UNKNOWN, rendered
    /// with the reason. Never rendered as `ok`.
    Unknown { reason: String },
}

impl ParseStatus {
    /// The one reader-facing footer clause: `ok` | `N unparsed` | `unknown (reason)`.
    pub fn footer_clause(&self) -> String {
        match self {
            ParseStatus::Ok => "ok".to_string(),
            ParseStatus::Unparsed { count } => format!("{count} unparsed"),
            ParseStatus::Unknown { reason } => format!("unknown ({reason})"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn footer_clause_wording() {
        assert_eq!(ParseStatus::Ok.footer_clause(), "ok");
        assert_eq!(
            ParseStatus::Unparsed { count: 3 }.footer_clause(),
            "3 unparsed"
        );
        assert_eq!(
            ParseStatus::Unknown {
                reason: "db read failed".to_string()
            }
            .footer_clause(),
            "unknown (db read failed)"
        );
    }

    #[test]
    fn serde_round_trip_is_tagged() {
        let p = ParseStatus::Unparsed { count: 2 };
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"state\":\"unparsed\""), "{json}");
        let back: ParseStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(back, p);
    }
}
