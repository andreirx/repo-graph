# daemon-runtime — agent notes

- `handle_modules_deps` filters the edges by module and direction. Until MDSS-1 it copied the repo-wide diagnostics unfiltered into the answer. Since MDSS-1 it emits the module's counters and `diagnostics_scope`. (src/dispatch.rs:8460-8545, as of b5c67086; source: MDSS-1)
- The method line groups module candidates by `module_kind` and the evidence `source_type`. A declared module with no evidence makes the method "not recorded". (src/modules_method.rs:116-160, :196-215; source: MODULES-METHOD-1, DAP-1)
- `modules violations` reads the same derivation diagnostics as `modules deps`, repo-wide. (src/dispatch.rs:8601 handle_modules_violations, its diagnostics block :8728-8734; modules deps' scoped block :8516-8564; source: MDSS-1)
- The daemon serializer for unresolved import rows carries exactly the fields of `AgentUnresolvedImportEntry`. A new field needs the agent port, storage and this serializer together. (src/import_partition_view.rs:449-463; source: TSA-1 review-4, D-TSA-BOUNDED-SCOPE-1)
- `tests/consolidation_witness.rs` counts every file that reads `.livegraph`. A new test that reads it directly breaks the witness. Bind the witness by name in any slice that touches the daemon. (source: IUR-1 OC-1)
