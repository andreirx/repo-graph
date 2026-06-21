//! Shared, total, deterministic pre-truncation ordering for capped agent output lists.
//!
//! TRUNCATION-AUDIT-1. Every budget/cap-truncated list must be ordered by a MEANINGFUL,
//! TOTAL, DETERMINISTIC key BEFORE the cut, so that:
//!   1. the surviving top-N are the most relevant items (not a rowid/arbitrary prefix), and
//!   2. the order is SOURCE-INDEPENDENT — the DR-EXPLAIN-CALLER-ORDER principle generalised:
//!      a list truncated off a merely set-equal source (SQLite vs the LiveGraph, which order
//!      rows differently) would otherwise show a DIFFERENT top-N depending on which store
//!      served it. Ranking the FULL set by a pure function of the set first makes the
//!      surviving subset identical regardless of input order.
//!
//! Every comparator ends on a UNIQUE field (a stable key, a rule identity, an obligation
//! identity, or a path) so ties occur only between byte-identical rows — the order is fully
//! deterministic, never input-order-dependent.
//!
//! `explain/call_ranking.rs` is the model (caller/callee module-concentration ranking).
//! These are the complementary comparators:
//!   - cross-command: `sort_cycles`, `sort_boundary_violations` — each has ≥2 concrete
//!     callers across BOTH the orient aggregators AND the explain pipeline with an IDENTICAL
//!     non-trivial comparator, so the logic lives once here (the "shared generic helper"
//!     mandate; the rejected alternative — duplicating the 3-key comparator at 5 sites — is
//!     the non-trivial duplication the criteria forbid).
//!   - explain listings: `sort_explain_files` / `sort_explain_symbols` / `sort_explain_imports`
//!     / `sort_explain_gate_items` — single-caller, but homed here (not inline in
//!     `explain/mod.rs`, which is already past the 500-line structural guardrail) so the
//!     comparators and their determinism tests share one tested seam.
//!
//! ── AUDIT (TRUNCATION-AUDIT-1): every budget/cap-truncated agent output ───────────────
//!
//! Exhaustive grep of `agent/src` for `.truncate(` / `.take(` / `truncate_items` / `_TOP_N` /
//! `items_cap` / `max_signals` / `max_limits`. Every truncated output is ordered by a meaningful
//! TOTAL key BEFORE the cut, OR was already meaningful (verified), OR is test-only. NO agent
//! output truncates on rowid/arbitrary/index order. (Sites named by function, not line, so this
//! audit does not rot as code moves; `git grep` the names to locate them.)
//!
//! NEWLY ORDERED — this module owns the comparator, applied just before the existing cut:
//!   - cycles: explain symbol+path, orient `aggregators::cycles` ×2,
//!     `orient::symbol::aggregate_cycles_for_module` ............ `sort_cycles` (length DESC, ring)
//!   - boundary: explain symbol+path .......................... `sort_boundary_violations`
//!   - explain files (path focus) ............................. `sort_explain_files` (symbol_count DESC, path)
//!   - explain symbols (file focus) ........................... `sort_explain_symbols` (line_start, name, key)
//!   - explain imports (file focus) ........................... `sort_explain_imports` (target_file)
//!   - explain gate obligations ............................... `sort_explain_gate_items` (verdict severity, ids)
//!   - orient HIGH_COMPLEXITY sample (`aggregators::complexity`) `sort_complexity` (complexity DESC, key);
//!     the aggregator now fetches the FULL above-threshold set and owns the cut (was storage `LIMIT N`)
//!   - orient boundary (`aggregators::boundary` ×2,
//!     `orient::symbol::aggregate_boundary_for_module`) ........ `sort_boundary_violations` (was inline;
//!     now the shared helper — byte-identical comparator)
//!
//! ALREADY MEANINGFUL — verified, left unchanged:
//!   - signals (`ranking::truncate_signals`) ................. `sort_and_rank` (severity/category/tier)
//!   - limits (`ranking::truncate_limits`) .................. construction order — condition-derived &
//!     source-independent (no relevance rank exists to sort by; see that fn's doc + audit tests)
//!   - explain callers/callees (`call_ranking::rank_*_rows`) . module-concentration, total on stable_key (IMPL-2)
//!   - top_modules (`group_by_module`, explain + orient) ..... count DESC, module ASC (module is unique)
//!
//! TEST-ONLY cuts (not production output): `call_ranking.rs` and `complexity.rs` FakeStorage `.take`.

use crate::dto::signal::{BoundaryViolationEvidence, ExplainGateItem};
use crate::storage_port::{
    AgentComplexityMeasurement, AgentCycle, AgentFileEntry, AgentImportEntry, AgentSymbolEntry,
};

/// Module dependency cycles: bigger cycles first (`length` DESC), then ring members
/// lexicographically. Cycles are canonicalised by the storage port (each appears once,
/// rotated to its smallest member), so `modules` is unique per cycle — a TOTAL,
/// source-independent tiebreak. Used by orient (`aggregators::cycles` ×2,
/// `orient::symbol::aggregate_cycles_for_module`) and explain (symbol + path cycle sections).
pub(crate) fn sort_cycles(cycles: &mut [AgentCycle]) {
    cycles.sort_by(|a, b| {
        b.length
            .cmp(&a.length)
            .then_with(|| a.modules.cmp(&b.modules))
    });
}

/// Boundary violations: most-violating first (`edge_count` DESC), then `(source_module,
/// target_module)` ASC. The `(source, target)` pair is the rule identity — a TOTAL tiebreak.
/// IDENTICAL to the comparator the orient boundary aggregators already applied inline;
/// extracted here so the explain boundary sections (which previously truncated UNSORTED)
/// share the same meaningful order.
pub(crate) fn sort_boundary_violations(items: &mut [BoundaryViolationEvidence]) {
    items.sort_by(|a, b| {
        b.edge_count
            .cmp(&a.edge_count)
            .then_with(|| a.source_module.cmp(&b.source_module))
            .then_with(|| a.target_module.cmp(&b.target_module))
    });
}

/// High-complexity symbols (orient `HIGH_COMPLEXITY` sample): worst first (`complexity` DESC),
/// then `stable_key` ASC. `stable_key` is unique per symbol → a TOTAL tiebreak, so the surviving
/// top-N sample is a pure function of the above-threshold SET. The aggregator fetches the FULL
/// above-threshold set and applies THIS sort + the cut itself, rather than accept storage's
/// `ORDER BY complexity DESC LIMIT N` cut — whose ties at the cut boundary fall to SQLite rowid
/// order (the DR-EXPLAIN-CALLER-ORDER hazard: the surviving sample must not depend on storage row
/// order). NOTE: this orders only the DISPLAY sample; the LiveGraph complexity no-loss cert
/// (`orient_lg_decisions::complexity_cert`) compares the full `(stable_key, complexity)` SET, which
/// is order-independent, so this sort does not affect cert parity.
pub(crate) fn sort_complexity(symbols: &mut [AgentComplexityMeasurement]) {
    symbols.sort_by(|a, b| {
        b.complexity
            .cmp(&a.complexity)
            .then_with(|| a.stable_key.cmp(&b.stable_key))
    });
}

/// Explain file listing (path focus): most substantive files first (`symbol_count` DESC),
/// then `path` ASC. `path` is unique per file → TOTAL. This CHANGES which files survive the
/// budget cut versus the prior storage-supplied path order (the sanctioned improvement: a
/// truncated path view now keeps the densest files, not the alphabetically-first ones).
pub(crate) fn sort_explain_files(files: &mut [AgentFileEntry]) {
    files.sort_by(|a, b| {
        b.symbol_count
            .cmp(&a.symbol_count)
            .then_with(|| a.path.cmp(&b.path))
    });
}

/// Explain symbol listing (file focus): file reading order — `line_start` ASC (symbols with
/// no recorded line sort last), then `name` ASC, then `stable_key` ASC (the unique TOTAL
/// tiebreak). Reading order is the meaningful order for a single-file view; it is applied in
/// the agent so it is deterministic and independent of the storage `ORDER BY`.
pub(crate) fn sort_explain_symbols(symbols: &mut [AgentSymbolEntry]) {
    symbols.sort_by(|a, b| {
        line_key(a.line_start)
            .cmp(&line_key(b.line_start))
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.stable_key.cmp(&b.stable_key))
    });
}

/// Explain import listing (file focus): `target_file` ASC. `find_file_imports` returns
/// DISTINCT target files, so `target_file` is the unique TOTAL key.
pub(crate) fn sort_explain_imports(imports: &mut [AgentImportEntry]) {
    imports.sort_by(|a, b| a.target_file.cmp(&b.target_file));
}

/// Explain gate obligation listing: worst verdict first (FAIL → MISSING_EVIDENCE →
/// UNSUPPORTED → WAIVED → PASS), then `(req_id, obligation_id, method)` ASC. The
/// `(req_id, obligation_id)` pair is the obligation identity — a TOTAL tiebreak — so a
/// budget-truncated gate view keeps the most urgent obligations deterministically.
pub(crate) fn sort_explain_gate_items(items: &mut [ExplainGateItem]) {
    items.sort_by(|a, b| {
        verdict_rank(&a.effective_verdict)
            .cmp(&verdict_rank(&b.effective_verdict))
            .then_with(|| a.req_id.cmp(&b.req_id))
            .then_with(|| a.obligation_id.cmp(&b.obligation_id))
            .then_with(|| a.method.cmp(&b.method))
    });
}

/// Sort key for an `Option<u64>` line number: `None` sorts last (unknown line ⇒ end of the
/// reading-order list).
fn line_key(line: Option<u64>) -> u64 {
    line.unwrap_or(u64::MAX)
}

/// Severity rank for an effective-verdict string (lower = more urgent). The strings are the
/// `Debug` form of `repo_graph_gate::EffectiveVerdict`
/// (`PASS`/`FAIL`/`MISSING_EVIDENCE`/`UNSUPPORTED`/`WAIVED`); an unknown string sorts last
/// (defensive — keeps the comparator total even if the verdict vocabulary grows).
fn verdict_rank(verdict: &str) -> u8 {
    match verdict {
        "FAIL" => 0,
        "MISSING_EVIDENCE" => 1,
        "UNSUPPORTED" => 2,
        "WAIVED" => 3,
        "PASS" => 4,
        _ => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Determinism harness ──────────────────────────────────────────
    //
    // The load-bearing property for every comparator: the sorted order is a PURE FUNCTION of
    // the SET, so feeding the same rows in forward / reversed / shuffled order yields a
    // byte-identical result. That is exactly what makes a budget-truncated view
    // source-independent (SQLite row order != LiveGraph row order, yet both rank the same).

    /// Permute `v` deterministically (no RNG — Math.random is unavailable in this env and we
    /// want a fixed permutation): reverse, plus a couple of fixed swaps to defeat any
    /// accidental reliance on adjacency.
    fn shuffled<T: Clone>(v: &[T]) -> Vec<T> {
        let mut s: Vec<T> = v.iter().rev().cloned().collect();
        if s.len() > 3 {
            let last = s.len() - 1;
            s.swap(0, 2);
            s.swap(1, last);
        }
        s
    }

    /// Assert a sort fn is order-independent: forward, reversed, and shuffled inputs all sort
    /// to the same key sequence.
    fn assert_order_independent<T, K, S, P>(rows: &[T], mut sort: S, mut project: P)
    where
        T: Clone,
        K: PartialEq + std::fmt::Debug,
        S: FnMut(&mut [T]),
        P: FnMut(&[T]) -> K,
    {
        let mut forward = rows.to_vec();
        let mut reversed: Vec<T> = rows.iter().rev().cloned().collect();
        let mut shuf = shuffled(rows);
        sort(&mut forward);
        sort(&mut reversed);
        sort(&mut shuf);
        let f = project(&forward);
        assert_eq!(
            f,
            project(&reversed),
            "reversed input must rank identically"
        );
        assert_eq!(f, project(&shuf), "shuffled input must rank identically");
    }

    fn cycle(len: usize, modules: &[&str]) -> AgentCycle {
        AgentCycle {
            length: len,
            modules: modules.iter().map(|m| m.to_string()).collect(),
        }
    }

    fn boundary(src: &str, tgt: &str, edges: u64) -> BoundaryViolationEvidence {
        BoundaryViolationEvidence {
            source_module: src.to_string(),
            target_module: tgt.to_string(),
            edge_count: edges,
        }
    }

    fn file_entry(path: &str, symbols: u64) -> AgentFileEntry {
        AgentFileEntry {
            path: path.to_string(),
            symbol_count: symbols,
            is_test: false,
        }
    }

    fn symbol_entry(name: &str, line: Option<u64>, key: &str) -> AgentSymbolEntry {
        AgentSymbolEntry {
            stable_key: key.to_string(),
            name: name.to_string(),
            qualified_name: None,
            subtype: None,
            line_start: line,
        }
    }

    fn import_entry(target: &str) -> AgentImportEntry {
        AgentImportEntry {
            target_file: target.to_string(),
        }
    }

    fn gate_item(verdict: &str, req: &str, ob: &str) -> ExplainGateItem {
        ExplainGateItem {
            req_id: req.to_string(),
            obligation_id: ob.to_string(),
            method: "m".to_string(),
            effective_verdict: verdict.to_string(),
        }
    }

    fn measurement(key: &str, complexity: u64) -> AgentComplexityMeasurement {
        AgentComplexityMeasurement {
            stable_key: key.to_string(),
            symbol_name: format!("sym::{key}"),
            file_path: Some(format!("src/{key}.rs")),
            complexity,
        }
    }

    // ── cycles ────────────────────────────────────────────────────────

    #[test]
    fn cycles_deterministic_and_length_desc() {
        let rows = vec![
            cycle(2, &["a", "b"]),
            cycle(4, &["a", "b", "c", "d"]),
            cycle(2, &["a", "c"]),
            cycle(3, &["b", "c", "d"]),
        ];
        assert_order_independent(&rows, sort_cycles, |s| {
            s.iter()
                .map(|c| (c.length, c.modules.clone()))
                .collect::<Vec<_>>()
        });
        let mut sorted = rows.clone();
        sort_cycles(&mut sorted);
        let lengths: Vec<usize> = sorted.iter().map(|c| c.length).collect();
        assert_eq!(lengths, vec![4, 3, 2, 2], "length DESC, then modules ASC");
        // The two length-2 cycles break the tie by modules lexicographically: [a,b] < [a,c].
        assert_eq!(sorted[2].modules, vec!["a", "b"]);
        assert_eq!(sorted[3].modules, vec!["a", "c"]);
    }

    #[test]
    fn cycles_load_bearing_under_cap() {
        // Raw order leads with a small cycle; the ranked top-1 must be the BIGGEST cycle.
        let raw = vec![
            cycle(2, &["a", "b"]),
            cycle(2, &["a", "c"]),
            cycle(5, &["m", "n", "o", "p", "q"]),
        ];
        let raw_top1 = raw[0].length;
        let mut ranked = raw.clone();
        sort_cycles(&mut ranked);
        assert_ne!(
            raw_top1, ranked[0].length,
            "ranking changes the truncated top-1"
        );
        assert_eq!(ranked[0].length, 5, "biggest cycle survives a cap of 1");
    }

    // ── boundary ──────────────────────────────────────────────────────

    #[test]
    fn boundary_deterministic_and_edge_count_desc() {
        let rows = vec![
            boundary("core", "cli", 2),
            boundary("core", "adapters", 9),
            boundary("api", "db", 2),
            boundary("web", "db", 5),
        ];
        assert_order_independent(&rows, sort_boundary_violations, |s| {
            s.iter()
                .map(|b| {
                    (
                        b.edge_count,
                        b.source_module.clone(),
                        b.target_module.clone(),
                    )
                })
                .collect::<Vec<_>>()
        });
        let mut sorted = rows.clone();
        sort_boundary_violations(&mut sorted);
        let counts: Vec<u64> = sorted.iter().map(|b| b.edge_count).collect();
        assert_eq!(counts, vec![9, 5, 2, 2]);
        // Tie at 2 broken by source ASC: "api" < "core".
        assert_eq!(sorted[2].source_module, "api");
        assert_eq!(sorted[3].source_module, "core");
    }

    #[test]
    fn boundary_load_bearing_under_cap() {
        let raw = vec![boundary("api", "db", 1), boundary("core", "adapters", 50)];
        let mut ranked = raw.clone();
        sort_boundary_violations(&mut ranked);
        assert_ne!(raw[0].edge_count, ranked[0].edge_count);
        assert_eq!(
            ranked[0].edge_count, 50,
            "worst violation survives a cap of 1"
        );
    }

    // ── files ─────────────────────────────────────────────────────────

    #[test]
    fn files_deterministic_and_symbol_count_desc() {
        let rows = vec![
            file_entry("src/a.ts", 3),
            file_entry("src/z.ts", 30),
            file_entry("src/b.ts", 3),
            file_entry("src/m.ts", 10),
        ];
        assert_order_independent(&rows, sort_explain_files, |s| {
            s.iter()
                .map(|f| (f.symbol_count, f.path.clone()))
                .collect::<Vec<_>>()
        });
        let mut sorted = rows.clone();
        sort_explain_files(&mut sorted);
        let paths: Vec<&str> = sorted.iter().map(|f| f.path.as_str()).collect();
        // symbol_count DESC; tie at 3 broken by path ASC (a before b).
        assert_eq!(paths, vec!["src/z.ts", "src/m.ts", "src/a.ts", "src/b.ts"]);
    }

    #[test]
    fn files_load_bearing_under_cap() {
        // Raw order is path-ASC (the prior behaviour); the densest file is alphabetically last.
        let raw = vec![
            file_entry("src/a.ts", 1),
            file_entry("src/b.ts", 2),
            file_entry("src/z.ts", 99),
        ];
        let raw_top1 = raw[0].path.clone();
        let mut ranked = raw.clone();
        sort_explain_files(&mut ranked);
        assert_ne!(
            raw_top1, ranked[0].path,
            "ranking changes which file survives a cap of 1"
        );
        assert_eq!(ranked[0].path, "src/z.ts", "densest file survives");
    }

    // ── symbols ───────────────────────────────────────────────────────

    #[test]
    fn symbols_deterministic_and_reading_order() {
        let rows = vec![
            symbol_entry("zeta", Some(40), "k:zeta"),
            symbol_entry("alpha", Some(10), "k:alpha"),
            symbol_entry("orphan", None, "k:orphan"),
            symbol_entry("beta", Some(10), "k:beta"),
        ];
        assert_order_independent(&rows, sort_explain_symbols, |s| {
            s.iter().map(|x| x.stable_key.clone()).collect::<Vec<_>>()
        });
        let mut sorted = rows.clone();
        sort_explain_symbols(&mut sorted);
        let names: Vec<&str> = sorted.iter().map(|s| s.name.as_str()).collect();
        // line_start ASC; tie at line 10 broken by name (alpha < beta); None (orphan) last.
        assert_eq!(names, vec!["alpha", "beta", "zeta", "orphan"]);
    }

    #[test]
    fn symbols_load_bearing_under_cap() {
        // Raw order leads with a late-line symbol; reading order must promote the line-1 symbol.
        let raw = vec![
            symbol_entry("late", Some(900), "k:late"),
            symbol_entry("first", Some(1), "k:first"),
        ];
        let mut ranked = raw.clone();
        sort_explain_symbols(&mut ranked);
        assert_ne!(raw[0].name, ranked[0].name);
        assert_eq!(ranked[0].name, "first", "earliest line survives a cap of 1");
    }

    // ── imports ───────────────────────────────────────────────────────

    #[test]
    fn imports_deterministic_and_target_asc() {
        let rows = vec![
            import_entry("src/z.ts"),
            import_entry("src/a.ts"),
            import_entry("src/m.ts"),
        ];
        assert_order_independent(&rows, sort_explain_imports, |s| {
            s.iter().map(|i| i.target_file.clone()).collect::<Vec<_>>()
        });
        let mut sorted = rows.clone();
        sort_explain_imports(&mut sorted);
        let targets: Vec<&str> = sorted.iter().map(|i| i.target_file.as_str()).collect();
        assert_eq!(targets, vec!["src/a.ts", "src/m.ts", "src/z.ts"]);
    }

    #[test]
    fn imports_load_bearing_under_cap() {
        let raw = vec![import_entry("src/z.ts"), import_entry("src/a.ts")];
        let mut ranked = raw.clone();
        sort_explain_imports(&mut ranked);
        assert_ne!(raw[0].target_file, ranked[0].target_file);
        assert_eq!(ranked[0].target_file, "src/a.ts");
    }

    // ── gate ──────────────────────────────────────────────────────────

    #[test]
    fn gate_deterministic_and_severity_desc() {
        let rows = vec![
            gate_item("PASS", "R2", "o1"),
            gate_item("FAIL", "R1", "o2"),
            gate_item("WAIVED", "R1", "o1"),
            gate_item("MISSING_EVIDENCE", "R3", "o1"),
        ];
        assert_order_independent(&rows, sort_explain_gate_items, |s| {
            s.iter()
                .map(|g| {
                    (
                        g.effective_verdict.clone(),
                        g.req_id.clone(),
                        g.obligation_id.clone(),
                    )
                })
                .collect::<Vec<_>>()
        });
        let mut sorted = rows.clone();
        sort_explain_gate_items(&mut sorted);
        let verdicts: Vec<&str> = sorted
            .iter()
            .map(|g| g.effective_verdict.as_str())
            .collect();
        assert_eq!(verdicts, vec!["FAIL", "MISSING_EVIDENCE", "WAIVED", "PASS"]);
    }

    #[test]
    fn gate_load_bearing_under_cap() {
        // Raw order leads with a PASS; the FAIL must survive a cap of 1.
        let raw = vec![gate_item("PASS", "R1", "o1"), gate_item("FAIL", "R2", "o2")];
        let mut ranked = raw.clone();
        sort_explain_gate_items(&mut ranked);
        assert_ne!(raw[0].effective_verdict, ranked[0].effective_verdict);
        assert_eq!(
            ranked[0].effective_verdict, "FAIL",
            "worst verdict survives"
        );
    }

    // ── complexity ──────────────────────────────────────────────────────

    #[test]
    fn complexity_deterministic_and_complexity_desc() {
        let rows = vec![
            measurement("k:b", 30),
            measurement("k:c", 50),
            measurement("k:a", 30), // tie with k:b at complexity 30
            measurement("k:d", 20),
        ];
        assert_order_independent(&rows, sort_complexity, |s| {
            s.iter()
                .map(|m| (m.complexity, m.stable_key.clone()))
                .collect::<Vec<_>>()
        });
        let mut sorted = rows.clone();
        sort_complexity(&mut sorted);
        let keys: Vec<&str> = sorted.iter().map(|m| m.stable_key.as_str()).collect();
        // complexity DESC (50), then the tie at 30 broken by stable_key ASC (k:a < k:b), then 20.
        assert_eq!(keys, vec!["k:c", "k:a", "k:b", "k:d"]);
    }

    #[test]
    fn complexity_load_bearing_under_cap() {
        // Raw (storage-insertion) order leads with a low-complexity symbol; the ranked top-1 must
        // be the WORST. Tie-order stability: two equal-complexity symbols always survive in
        // stable_key order regardless of input order — the property the reviewer asked to pin.
        let raw = vec![
            measurement("k:low", 21),
            measurement("k:tie_b", 40),
            measurement("k:worst", 99),
            measurement("k:tie_a", 40), // tie at 40 with k:tie_b
        ];
        let mut ranked = raw.clone();
        sort_complexity(&mut ranked);
        assert_ne!(
            raw[0].stable_key, ranked[0].stable_key,
            "ranking changes the truncated top-1"
        );
        assert_eq!(
            ranked[0].stable_key, "k:worst",
            "worst complexity survives a cap of 1"
        );
        // Under a cap of 3 the surviving set — including the tie order — is deterministic.
        let top3: Vec<&str> = ranked
            .iter()
            .take(3)
            .map(|m| m.stable_key.as_str())
            .collect();
        assert_eq!(top3, vec!["k:worst", "k:tie_a", "k:tie_b"]);
    }
}
