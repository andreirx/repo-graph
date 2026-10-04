# storage — agent notes

- `RECORDED_BASES` is the one table of bases a bound IMPORTS edge may carry (`unique_suffix`, `unique_basename`, `workspace_source_entry`, `tsconfig_paths`). `AMBIGUOUS_BASES` lists the bases only an unresolved row may carry. (src/queries.rs:225-250, as of 1510da10; source: TSA-1)
- `import_row_reason` returns `Unreadable` when a recorded basis does not agree with the edge. A static edge with an `Unreadable` reason renders `static: reason unreadable (…)` in rgr. (src/queries.rs:372-473; source: TSA-1 admission 1)
- `trust` reads only modules that own at least one file. `modules list` lists every module candidate. (src/trust_impl.rs:815-837, as of 4eafdaa1; source: DAP-1 review-1)
- `file_signals` has a row for a file only when the file has a signal. A file that gains a declared set gains a `file_signals` row. A count of `file_signals` rows is not a count of indexed files. (source: DGC-1B admission 1, OC-1)
- Every FILE stable key embeds the repo uid: `<repo_uid>:<path>:FILE`. Two indexes of one tree have two uids. Compare paths, not keys, across indexes. (agent-manager scripts/rg-store-diff.py; source: TSA-1 review-6, TWR-1)
