# classification — agent notes

- `derive_module_dependency_edges_in_view` counts the five repo-wide diagnostics once, for the whole repository. (src/module_edges.rs:215-262, as of b5c67086; source: MDSS-1, RC-3)
- Since MDSS-1 the same loop also fills per-module counters: intra-module imports, unowned source into the module, unowned target from the module. Cross-module counts stay on the edges. (src/module_edges.rs, as of b5c67086; source: MDSS-1)
- A module that is absent from the per-module map has zero intra-module and zero unowned-endpoint imports. It can still have cross-module imports. (source: MDSS-1 review-3)
- An excluded import (test or inferred, not in the view) adds to its group's `imports` count. A relation with excluded imports only adds one `edges` count to the first applicable remainder group, in the order Tests, Inferred, TestsAndInferred. (import_partition.rs:420-439, as of b5c67086; source: MDSS-1 review-4)
- Test files increment `owned_test_file_count`, not `owned_file_count`. A module that owns only test files has `owned_file_count == 0`. (src/module_rollup.rs:218-224, as of b5c67086; source: DAP-1 review-2)
- The unresolved-import classifier already matches a declared Java group to an observed package on a `.` segment boundary, longest group first. (src/unresolved_classifier.rs:551, as of f69759f4; source: DGC-1B)
