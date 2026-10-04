# rgr — agent notes

- `format_import_row` prints `static` for every static edge, except a `tsconfig_paths` edge, which prints `static (resolved through tsconfig paths)`, and an `Unreadable` reason, which prints `static: reason unreadable (…)`. (src/presentation/imports.rs `static_row_state`, as of 1510da10; source: TSA-1)
- `ImportDiagnostics` decodes each counter with `#[serde(default)]`. A missing key renders as 0. This is older than MDSS-1 and is next to RG-REQ-002-L04. (src/presentation/modules_deps.rs:28-38; source: MDSS-1 §8)
- Under `Module: X` the Summary header is `Summary (module X, <direction label>):` when the answer carries `diagnostics_scope: module`. An older answer without the key renders `Summary:`. (src/presentation/modules_deps.rs; source: MDSS-1)
- The `cycles` human text names its module population only in the no-cycles branch. (src/presentation/cycles/mod.rs:261-309, as of 4eafdaa1; source: DAP-1 review-1)
