#!/usr/bin/env python3
# capture_ey1b_corpus.py — capture the REAL promotion-candidate corpus + context from an
# isolated self-index DB, for the ENRICH-YIELD-2 EY1-B deterministic-replay neutrality proof
# (`promotion::tests::primitives_are_promotion_neutral_over_real_self_index_corpus`).
#
# WHY THIS EXISTS (review-2, EY2-B-PROOF):
#   The neutrality stop condition "promoted set BEFORE == AFTER" binds EY1-B *in isolation*. It
#   cannot be shown by comparing two LIVE enrichment runs (rust-analyzer is nondeterministic and
#   EY1-D adds enum promotions by design), so it is proven by a DETERMINISTIC identical-corpus
#   replay: fix ONE real captured corpus, run it through pre-EY1-B and post-EY1-B classification,
#   and assert the promoted set is byte-identical and the only candidates whose disposition moves
#   are primitives — each landing at gate 4 post-EY1-B from an already-failing pre-EY1-B rejection
#   at gate 5 (simple call, type_not_in_graph) OR gate 8 (deeper chain; gate 4 evaluates first).
#   Ratified EY2-B-GATE8 (2026-07-13): the observed broader invariant — in this corpus 49/54 moved
#   from gate 5, 5/54 from gate 8; promoted sets exactly equal. review-2 required the corpus be
#   REAL (captured from a real self-index), not hand-built representative rows. This script IS the
#   capture provenance.
#
# WHAT IT DOES (read-only; never writes the DB, never touches the operator registry/daemon):
#   Faithfully reproduces the three storage reads that feed the promotion filter
#   (rust/crates/storage/src/enrichment_impl.rs) against a SQLite DB produced by an isolated
#   `rmap index` + Rust enrichment run:
#     1. load_promotion_candidates  -> the candidate corpus (compiler-origin, receiverType present,
#        accepted category).
#     2. load_symbols_by_names      -> symbols for the distinct receiver type names (the context).
#     3. load_class_methods         -> for each usable (CLASS|ENUM) symbol, its methods.
#   then emits ey1b_selfindex_corpus.json with provenance.
#
# THE PIVOT (pre vs post classification):
#   is_external_post = the REAL persisted `$.enrichment.isExternalType` from the live resolver.
#   The capturing binary has EY1-B, so post = (name in STD_TYPES ∪ PRIMITIVES). The test derives
#   is_external_pre = is_external_post AND NOT receiver_is_primitive, i.e. STD-only (pre-EY1-B) —
#   sound because EY1-B's ONLY change to `is_external_type` was `|| PRIMITIVES.contains(name)`
#   (rust-analyzer-resolver/src/types.rs). We capture `receiver_is_primitive` (membership in the
#   resolver's PRIMITIVES set, mirrored below) so the test needs no classifier of its own.
#
# NO third-party deps (stdlib sqlite3/json/hashlib only), so no venv is required.
#
# USAGE:
#   python3 capture_ey1b_corpus.py <path-to-db.sqlite> [snapshot_uid] > ey1b_selfindex_corpus.json
#   (snapshot_uid auto-detected as the snapshot with the most compiler-enriched candidates.)

import json
import sqlite3
import sys
from datetime import datetime, timezone

# Mirror of rust-analyzer-resolver::types::PRIMITIVES (the set EY1-B added to is_external_type).
# This is the Rust-language primitive set; it labels which real receivers EY1-B moves.
PRIMITIVES = {
    "bool", "char", "str", "i8", "i16", "i32", "i64", "i128", "isize",
    "u8", "u16", "u32", "u64", "u128", "usize", "f32", "f64",
}

# The two enrichment-eligible categories (enrichment::contracts::UnresolvedCategory). The candidate
# SQL has NO category filter; load_promotion_candidates drops any other category post-query, so we
# must too, to reproduce the exact corpus.
ACCEPTED_CATEGORIES = {
    "calls_obj_method_needs_type_info",
    "calls_this_wildcard_method_needs_type_info",
}

# Symbol subtypes that count as methods for load_class_methods (enrichment_impl.rs:487).
METHOD_SUBTYPES = ("METHOD", "GETTER", "SETTER", "FUNCTION")


def die(msg):
    sys.stderr.write("capture_ey1b_corpus: error: " + msg + "\n")
    sys.exit(1)


def detect_snapshot(conn):
    """The snapshot with the most compiler-origin candidates carrying a receiverType."""
    rows = conn.execute(
        """
        SELECT snapshot_uid, COUNT(*) AS n
        FROM unresolved_edges
        WHERE metadata_json IS NOT NULL
          AND json_extract(metadata_json, '$.enrichment.receiverType') IS NOT NULL
          AND json_extract(metadata_json, '$.enrichment.origin') = 'compiler'
        GROUP BY snapshot_uid
        ORDER BY n DESC
        """
    ).fetchall()
    if not rows:
        die("no compiler-enriched candidates in any snapshot (run enrichment first)")
    return rows[0][0]


def load_candidates(conn, snapshot_uid):
    """Reproduces load_promotion_candidates (enrichment_impl.rs:279-364)."""
    rows = conn.execute(
        """
        SELECT edge_uid, target_key, category, metadata_json
        FROM unresolved_edges
        WHERE snapshot_uid = ?1
          AND metadata_json IS NOT NULL
          AND json_extract(metadata_json, '$.enrichment.receiverType') IS NOT NULL
          AND json_extract(metadata_json, '$.enrichment.origin') = 'compiler'
        ORDER BY edge_uid
        """,
        [snapshot_uid],
    ).fetchall()

    candidates = []
    for edge_uid, target_key, category, metadata_json in rows:
        if category not in ACCEPTED_CATEGORIES:
            continue  # UnresolvedCategory::parse would drop it (post-query, as the loader does).
        meta = json.loads(metadata_json).get("enrichment", {})
        receiver_type = meta.get("receiverType")
        type_display_name = meta.get("typeDisplayName")
        # The gate-5 lookup key is typeDisplayName ?? receiverType (promotion.rs:240).
        effective = type_display_name if type_display_name else receiver_type
        if effective is None:
            continue
        is_external_post = bool(meta.get("isExternalType", False))
        receiver_is_primitive = effective in PRIMITIVES
        # Self-check: the capturing binary has EY1-B, so a primitive receiver MUST be persisted
        # external. If not, the binary lacks EY1-B and the corpus would be meaningless.
        if receiver_is_primitive and not is_external_post:
            die(
                "primitive receiver %r has persisted isExternalType=false — the capturing binary "
                "lacks EY1-B; rebuild release binaries from this tree" % effective
            )
        candidates.append({
            "edge_uid": edge_uid,
            "target_key": target_key,
            "receiver_type": receiver_type,
            "type_display_name": type_display_name,
            "category": category,
            "is_external_post": is_external_post,
            "receiver_is_primitive": receiver_is_primitive,
        })
    return candidates


def load_symbols(conn, snapshot_uid, names):
    """Reproduces load_symbols_by_names (enrichment_impl.rs:366-431)."""
    if not names:
        return []
    placeholders = ",".join("?" for _ in names)
    params = [snapshot_uid] + list(names) + list(names)
    rows = conn.execute(
        """
        SELECT node_uid, stable_key, qualified_name, subtype
        FROM nodes
        WHERE snapshot_uid = ?
          AND kind = 'SYMBOL'
          AND ( name IN (%s) OR qualified_name IN (%s) )
        ORDER BY node_uid
        """ % (placeholders, placeholders),
        params,
    ).fetchall()
    return [
        {"node_uid": r[0], "stable_key": r[1], "qualified_name": r[2], "subtype": r[3]}
        for r in rows
    ]


def load_class_methods(conn, snapshot_uid, class_stable_key):
    """Reproduces load_class_methods (enrichment_impl.rs:433-530)."""
    row = conn.execute(
        "SELECT node_uid, name FROM nodes WHERE snapshot_uid = ? AND stable_key = ?",
        [snapshot_uid, class_stable_key],
    ).fetchone()
    if row is None:
        return []
    class_node_uid = row[0]
    class_name = row[1] if row[1] is not None else ""

    placeholders = ",".join("?" for _ in METHOD_SUBTYPES)
    rows = conn.execute(
        """
        SELECT node_uid, stable_key, name, qualified_name, subtype
        FROM nodes
        WHERE snapshot_uid = ?
          AND kind = 'SYMBOL'
          AND subtype IN (%s)
          AND (
              parent_node_uid = ?
              OR (? <> '' AND qualified_name = ? || '.' || name)
          )
        ORDER BY name
        """ % placeholders,
        [snapshot_uid, *METHOD_SUBTYPES, class_node_uid, class_name, class_name],
    ).fetchall()
    return [
        {
            "class_stable_key": class_stable_key,
            "method_name": r[2],
            "method": {"node_uid": r[0], "stable_key": r[1], "qualified_name": r[3], "subtype": r[4]},
        }
        for r in rows
    ]


def main():
    if len(sys.argv) < 2:
        die("usage: capture_ey1b_corpus.py <db-path> [snapshot_uid]")
    db_path = sys.argv[1]
    # Open strictly read-only so the capture provably cannot mutate the indexed DB.
    conn = sqlite3.connect("file:%s?mode=ro" % db_path, uri=True)

    snapshot_uid = sys.argv[2] if len(sys.argv) > 2 else detect_snapshot(conn)

    candidates = load_candidates(conn, snapshot_uid)
    if not candidates:
        die("zero candidates for snapshot %s" % snapshot_uid)

    # Distinct receiver type names (typeDisplayName ?? receiverType), as the pipeline collects them.
    names = []
    seen = set()
    for c in candidates:
        n = c["type_display_name"] if c["type_display_name"] else c["receiver_type"]
        if n is not None and n not in seen:
            seen.add(n)
            names.append(n)

    symbols = load_symbols(conn, snapshot_uid, names)

    class_methods = []
    for s in symbols:
        subtype_upper = (s["subtype"] or "").upper()
        if subtype_upper in ("CLASS", "ENUM"):  # is_usable_receiver_type()
            class_methods.extend(load_class_methods(conn, snapshot_uid, s["stable_key"]))

    n_prim = sum(1 for c in candidates if c["receiver_is_primitive"])
    primitive_names = sorted({
        (c["type_display_name"] if c["type_display_name"] else c["receiver_type"])
        for c in candidates if c["receiver_is_primitive"]
    })

    fixture = {
        "_provenance": {
            "slice": "ENRICH-YIELD-2 EY1-B",
            "proof": "EY2-B-PROOF deterministic identical-corpus replay of promotion-neutrality",
            "captured_from": "isolated `rmap index` + live rust-analyzer Rust enrichment of the "
                             "repo-graph rust/ workspace (custom state root + stdio/own socket; "
                             "operator registry & daemon PID untouched)",
            "capture_script": "rust/crates/enrichment/src/testdata/capture_ey1b_corpus.py "
                              "(read-only; reproduces storage::enrichment_impl "
                              "load_promotion_candidates + load_symbols_by_names + load_class_methods)",
            "db_path": db_path,
            "snapshot_uid": snapshot_uid,
            "captured_at_utc": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
            "primitive_set": "mirror of rust-analyzer-resolver::types::PRIMITIVES",
            "classification_note": (
                "is_external_post is the REAL persisted $.enrichment.isExternalType from the live "
                "resolver (post-EY1-B = STD_TYPES ∪ PRIMITIVES). The test derives "
                "is_external_pre = is_external_post AND NOT receiver_is_primitive, i.e. STD-only "
                "(pre-EY1-B), because EY1-B's only change to is_external_type was adding PRIMITIVES."
            ),
            "counts": {
                "candidates": len(candidates),
                "distinct_type_names": len(names),
                "symbols": len(symbols),
                "class_methods": len(class_methods),
                "primitive_candidates": n_prim,
            },
            "primitive_receiver_names_in_corpus": primitive_names,
        },
        "candidates": candidates,
        "symbols": symbols,
        "class_methods": class_methods,
    }
    json.dump(fixture, sys.stdout, indent=1, sort_keys=False)
    sys.stdout.write("\n")
    sys.stderr.write(
        "captured %d candidates (%d primitive), %d symbols, %d methods from snapshot %s\n"
        % (len(candidates), n_prim, len(symbols), len(class_methods), snapshot_uid)
    )


if __name__ == "__main__":
    main()
