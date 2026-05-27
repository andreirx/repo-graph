# Performance Baselines (PERF-OBS-1)

Volume baselines captured 2026-05-27 for storage architecture decisions.

## Captured Baselines

| Repo | Size Class | DB Size | Status |
|------|------------|---------|--------|
| repo-graph | small-medium | 1.4 GB | Captured |
| glamCRM | medium | ~200 MB | Captured |
| django | medium-large | ~300 MB | Captured |
| hadoop | monorepo | 9.5 GB | TIMEOUT BOUNDARY (not baseline) |

## Not Captured

- **Global vs sandbox comparison** — same repo indexed in both modes for comparison
- **Phase timing** — handler/query level timing breakdown (removed, was only wall-clock)

## Usage

View baseline with `jq`:

```bash
jq '.db_size_bytes, .tiers, .layers' docs/perf-baselines/repo-graph.json
```

Compare tier distribution:

```bash
for f in docs/perf-baselines/*.json; do
  echo "=== $(basename $f .json) ==="
  jq -r '"Tier A: \(.tiers.tier_a_rows), Tier B: \(.tiers.tier_b_rows)"' "$f" 2>/dev/null
done
```

## Findings

### hadoop Timeout

The `rmap perf` query times out (>300s) on the 9.5GB hadoop database. Root cause:
- `COUNT(*)` on 44 tables is O(n) in row count
- `dbstat` aggregation is O(pages)
- Combined query exceeds 5-minute daemon timeout

**Mitigation options:**
1. Sample counts (`LIMIT 1000`) for large tables
2. Cache row counts in table metadata
3. Use `sqlite_stat1` if available
4. Increase timeout for perf-specific queries

This is useful baseline data: it documents the performance boundary where current instrumentation breaks down.

## Daemon Startup Timing

Captured from daemon log:

```
info: daemon startup (warm) - total: 19.468708ms, sandbox_clear: 9.343ms, state_init: 10.125417ms, dispatcher: 291ns
```

- Mode: warm (registry.json exists)
- Total startup: ~20ms
- Sandbox clear: ~9ms (deletes temp directory if exists)
- State init: ~10ms
- Dispatcher init: <1us

## Next Steps

- CACHE-SEMANTICS-1: Tier B refresh/invalidation semantics
- Consider optimized metrics path for large databases
