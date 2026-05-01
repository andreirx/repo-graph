#!/usr/bin/env python3
"""
Update storage parity fixtures for migration 025.

This script updates all expected.json files in storage-parity-fixtures/
to include:
1. New tables: boundary_contracts, boundary_interaction_links, contract_elements,
   contract_schemas, generated_code_mappings
2. New columns on boundary_interaction_surfaces: confidence_basis, provenance, transport_class
3. New indexes for the above tables
4. Migration 025 row in schema_migrations
"""

import json
import os
from pathlib import Path

# New table definitions from migration 025
NEW_TABLES = {
    "boundary_contracts": [
        {"name": "association_basis", "type": "TEXT", "notnull": True, "dflt_value": None, "pk": 0},
        {"name": "association_uid", "type": "TEXT", "notnull": False, "dflt_value": None, "pk": 1},
        {"name": "confidence", "type": "REAL", "notnull": True, "dflt_value": None, "pk": 0},
        {"name": "contract_element_uid", "type": "TEXT", "notnull": False, "dflt_value": None, "pk": 0},
        {"name": "contract_kind", "type": "TEXT", "notnull": True, "dflt_value": None, "pk": 0},
        {"name": "evidence_json", "type": "TEXT", "notnull": False, "dflt_value": None, "pk": 0},
        {"name": "surface_uid", "type": "TEXT", "notnull": True, "dflt_value": None, "pk": 0}
    ],
    "boundary_interaction_links": [
        {"name": "confidence", "type": "REAL", "notnull": True, "dflt_value": None, "pk": 0},
        {"name": "consumer_surface_uid", "type": "TEXT", "notnull": True, "dflt_value": None, "pk": 0},
        {"name": "contract_element_uid", "type": "TEXT", "notnull": False, "dflt_value": None, "pk": 0},
        {"name": "evidence_json", "type": "TEXT", "notnull": False, "dflt_value": None, "pk": 0},
        {"name": "link_kind", "type": "TEXT", "notnull": True, "dflt_value": None, "pk": 0},
        {"name": "link_uid", "type": "TEXT", "notnull": False, "dflt_value": None, "pk": 1},
        {"name": "match_basis", "type": "TEXT", "notnull": True, "dflt_value": None, "pk": 0},
        {"name": "materialized_at", "type": "TEXT", "notnull": True, "dflt_value": None, "pk": 0},
        {"name": "provider_surface_uid", "type": "TEXT", "notnull": True, "dflt_value": None, "pk": 0},
        {"name": "snapshot_uid", "type": "TEXT", "notnull": True, "dflt_value": None, "pk": 0}
    ],
    "contract_elements": [
        {"name": "element_kind", "type": "TEXT", "notnull": True, "dflt_value": None, "pk": 0},
        {"name": "element_uid", "type": "TEXT", "notnull": False, "dflt_value": None, "pk": 1},
        {"name": "full_name", "type": "TEXT", "notnull": True, "dflt_value": None, "pk": 0},
        {"name": "line_end", "type": "INTEGER", "notnull": False, "dflt_value": None, "pk": 0},
        {"name": "line_start", "type": "INTEGER", "notnull": False, "dflt_value": None, "pk": 0},
        {"name": "metadata_json", "type": "TEXT", "notnull": False, "dflt_value": None, "pk": 0},
        {"name": "name", "type": "TEXT", "notnull": True, "dflt_value": None, "pk": 0},
        {"name": "parent_element_uid", "type": "TEXT", "notnull": False, "dflt_value": None, "pk": 0},
        {"name": "schema_uid", "type": "TEXT", "notnull": True, "dflt_value": None, "pk": 0}
    ],
    "contract_schemas": [
        {"name": "content_hash", "type": "TEXT", "notnull": True, "dflt_value": None, "pk": 0},
        {"name": "extractor", "type": "TEXT", "notnull": True, "dflt_value": None, "pk": 0},
        {"name": "file_path", "type": "TEXT", "notnull": True, "dflt_value": None, "pk": 0},
        {"name": "imports_json", "type": "TEXT", "notnull": False, "dflt_value": None, "pk": 0},
        {"name": "options_json", "type": "TEXT", "notnull": False, "dflt_value": None, "pk": 0},
        {"name": "package_name", "type": "TEXT", "notnull": False, "dflt_value": None, "pk": 0},
        {"name": "parsed_at", "type": "TEXT", "notnull": True, "dflt_value": None, "pk": 0},
        {"name": "repo_uid", "type": "TEXT", "notnull": True, "dflt_value": None, "pk": 0},
        {"name": "schema_kind", "type": "TEXT", "notnull": True, "dflt_value": None, "pk": 0},
        {"name": "schema_uid", "type": "TEXT", "notnull": False, "dflt_value": None, "pk": 1},
        {"name": "snapshot_uid", "type": "TEXT", "notnull": True, "dflt_value": None, "pk": 0},
        {"name": "syntax_version", "type": "TEXT", "notnull": False, "dflt_value": None, "pk": 0}
    ],
    "generated_code_mappings": [
        {"name": "confidence", "type": "REAL", "notnull": True, "dflt_value": None, "pk": 0},
        {"name": "generated_file", "type": "TEXT", "notnull": True, "dflt_value": None, "pk": 0},
        {"name": "generated_symbol_key", "type": "TEXT", "notnull": True, "dflt_value": None, "pk": 0},
        {"name": "language", "type": "TEXT", "notnull": True, "dflt_value": None, "pk": 0},
        {"name": "mapping_basis", "type": "TEXT", "notnull": True, "dflt_value": None, "pk": 0},
        {"name": "mapping_uid", "type": "TEXT", "notnull": False, "dflt_value": None, "pk": 1},
        {"name": "metadata_json", "type": "TEXT", "notnull": False, "dflt_value": None, "pk": 0},
        {"name": "schema_element_uid", "type": "TEXT", "notnull": True, "dflt_value": None, "pk": 0},
        {"name": "snapshot_uid", "type": "TEXT", "notnull": True, "dflt_value": None, "pk": 0}
    ]
}

# New columns for boundary_interaction_surfaces (in alphabetical order to match SQLite)
NEW_BIS_COLUMNS = [
    {"name": "confidence_basis", "type": "TEXT", "notnull": False, "dflt_value": None, "pk": 0},
    {"name": "provenance", "type": "TEXT", "notnull": False, "dflt_value": None, "pk": 0},
    {"name": "transport_class", "type": "TEXT", "notnull": False, "dflt_value": None, "pk": 0}
]

# New indexes from migration 025
NEW_INDEXES = [
    "idx_bc_contract_element",
    "idx_bc_contract_kind",
    "idx_bc_surface",
    "idx_bil_consumer",
    "idx_bil_contract",
    "idx_bil_provider",
    "idx_bil_snapshot",
    "idx_ce_full_name",
    "idx_ce_kind",
    "idx_ce_parent",
    "idx_ce_schema",
    "idx_cs_snapshot_file",
    "idx_cs_snapshot_kind",
    "idx_cs_snapshot_package",
    "idx_gcm_language",
    "idx_gcm_schema_element",
    "idx_gcm_snapshot",
    "idx_gcm_symbol"
]

MIGRATION_025 = {"version": 25, "name": "025-contract-schema", "applied_at": "<TIMESTAMP>"}


def update_fixture(fixture_path: Path) -> None:
    expected_path = fixture_path / "expected.json"
    if not expected_path.exists():
        print(f"Skipping {fixture_path.name}: no expected.json")
        return

    with open(expected_path, "r") as f:
        data = json.load(f)

    # Update schema.tables with new tables
    for table_name, columns in NEW_TABLES.items():
        data["schema"]["tables"][table_name] = columns

    # Update boundary_interaction_surfaces with new columns
    bis_columns = data["schema"]["tables"]["boundary_interaction_surfaces"]
    existing_names = {col["name"] for col in bis_columns}

    for new_col in NEW_BIS_COLUMNS:
        if new_col["name"] not in existing_names:
            bis_columns.append(new_col)

    # Sort columns alphabetically by name (SQLite PRAGMA table_info order)
    bis_columns.sort(key=lambda c: c["name"])
    data["schema"]["tables"]["boundary_interaction_surfaces"] = bis_columns

    # Sort all tables alphabetically
    data["schema"]["tables"] = dict(sorted(data["schema"]["tables"].items()))

    # Update indexes
    existing_indexes = set(data["schema"]["indexes"])
    for idx in NEW_INDEXES:
        existing_indexes.add(idx)
    data["schema"]["indexes"] = sorted(existing_indexes)

    # Add migration 025 to tables.schema_migrations
    migrations = data["tables"].get("schema_migrations", [])
    if not any(m["version"] == 25 for m in migrations):
        migrations.append(MIGRATION_025)
    data["tables"]["schema_migrations"] = migrations

    with open(expected_path, "w") as f:
        json.dump(data, f, indent=2)
        f.write("\n")

    print(f"Updated {expected_path}")


def main():
    repo_root = Path(__file__).parent.parent
    fixtures_dir = repo_root / "storage-parity-fixtures"

    if not fixtures_dir.exists():
        print(f"Fixtures directory not found: {fixtures_dir}")
        return

    for item in fixtures_dir.iterdir():
        if item.is_dir() and not item.name.startswith("."):
            update_fixture(item)

    print("\nDone. Run 'cargo test -p repo-graph-storage --test parity' to verify.")


if __name__ == "__main__":
    main()
