# Trust Parity Fixtures

Cross-runtime parity corpus for the trust reporting substrate (Rust-4).

## Structure

Each subdirectory is one fixture with `input.json` and `expected.json`.

### Naming convention

- `rules__<function>__<scenario>` — individual rule function tests (Tier A)
- `report__<scenario>` — full trust report tests via assembly layer (Tier B)

### Fixture format

**input.json:**
```json
{
  "fn": "<function_name>",
  ...function-specific fields...
}
```

**expected.json:** The exact JSON output both runtimes must produce.

### Supported functions

| `fn` value | Tier | Rust function | TS function |
|---|---|---|---|
| `detect_framework_heavy_suspicion` | A | `detect_framework_heavy_suspicion()` | `detectFrameworkHeavySuspicion()` |
| `detect_alias_resolution_suspicion` | A | `detect_alias_resolution_suspicion()` | `detectAliasResolutionSuspicion()` |
| `detect_missing_entrypoint_declarations` | A | `detect_missing_entrypoint_declarations()` | `detectMissingEntrypointDeclarations()` |
| `detect_registry_pattern_suspicion` | A | `detect_registry_pattern_suspicion()` | `detectRegistryPatternSuspicion()` |
| `compute_call_graph_reliability` | A | `compute_call_graph_reliability()` | `computeCallGraphReliability()` |
| `compute_import_graph_reliability` | A | `compute_import_graph_reliability()` | `computeImportGraphReliability()` |
| `compute_dead_code_reliability` | A | `compute_dead_code_reliability()` | `computeDeadCodeReliability()` |
| `compute_change_impact_reliability` | A | `compute_change_impact_reliability()` | `computeChangeImpactReliability()` |
| `compute_trust_report` | B | `assemble_trust_report()` (mock storage) | `computeTrustReport()` (mock storage) |

### Tier B assembly layer coverage

Both runtimes exercise the storage-backed assembly path for
`report__*` fixtures. The Rust side calls `assemble_trust_report`
with a mock `TrustStorageRead`; the TS side calls
`computeTrustReport` with a mock `StoragePort`. Toolchain and
diagnostics fields are passed as raw JSON strings to force both
runtimes through their JSON parsing paths.

### No normalization

Trust computation is pure policy. No generated UIDs, no timestamps,
no randomness. Both runtimes must produce byte-equal JSON for the
same input.

### Running

```bash
# Rust half
cargo test -p repo-graph-trust --test parity

# TS half (requires build)
pnpm exec vitest run test/trust-parity/

# Both halves via script
pnpm run test:trust:parity
```

### Generating expected.json

On first run (or when adding new fixtures), if `expected.json` is
missing, the Rust test generates it from the Rust output and fails
with a message to re-run. The TS test then validates against it.

To regenerate all expected.json files:
```bash
RGR_TRUST_PARITY_UPDATE=1 cargo test -p repo-graph-trust --test parity
```
