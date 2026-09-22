# DOCS-DISCOVERY-1 — oracle corrections and amendments ledger (append-only)

## OC-1 (2026-09-22) — six manager oracle defects found by the first implementation admission (INPUT-1; builder claude-opus-5-5, reviewer codex gpt-5.6-terra, cycle 1; F-DD1-002)

The candidate reproduced every measured headline on all eight corpora (hadoop 22 → 556, django 616 → 622, buildroot 4 → 310, kafka 133 + the singular honesty line, leveldb 7 → 9, grpc-java 49 → 50, FRAKTAG 14 with +11 vendored, repo-graph +1) and the builder STOPPED on the two drifted predictions instead of bending an oracle. The defects were in the manager's checks, not in the product:

| # | Check | Defect | Correction (INPUT-2) |
|---|---|---|---|
| 1 | DD-C05 | `cargo test --lib presentation::orient_tests` selected zero tests — orient_tests.rs is mounted by `#[path]` as `presentation::orient::tests` (orient.rs:569-570) | filter `presentation::orient::tests` |
| 2 | DD-C08, DD-C09 | bare `rmap orient` runs at the small budget, which omits the modules section (and therefore the recommendation) before and after | `orient --budget medium` (D-DD1-004) |
| 3 | DD-C08 | `len(site) == 519` counted every entry under `src/site` — there are 520: 519 `doc` + `hadoop-yarn-site/src/site/markdown/yarn-service/Overview.md` as `architecture` (the check's own inputs listed it separately) | 520 with the kind split asserted |
| 4 | DD-C09 | django's kinds were predicted from the ADDED files only; the already-listed `docs/README.rst` reclassifies `doc` → `readme` under RG-REQ-008-L02 | readme 7 / doc 225; `docs/README.rst` asserted `readme` |
| 5 | DD-C10 | the board predicate `endswith('/readme.txt')` was case-sensitive (223) although D-DD1-003 admits `README.txt` (2 more); the "233" was all non-root readmes, not board readmes | case-insensitive board predicate = 225; total readme entries 234 |
| 6 | DD-C11 | a python heredoc terminator glued to `&& ( cd …grpc-java… )` — unexecutable shell (the manager's own rule: a heredoc is the LAST element of a chain); literal repo-graph counts (657/658) on a LIVE checkout that had gained two decision records (659/660) | split into DD-C11 (kafka, leveldb), DD-C12 (grpc-java, FRAKTAG), DD-C13 (repo-graph, relative to the run's own before capture); isolation renumbered DD-C14 |

Also carried by INPUT-2: the F-DD1-001 fix direction (the `discovery_rule` value built infallibly — no silent `null`), D-DD1-004 (orient proof depth; operator resolution of the reviewer's D-DD1-001, overridable), and the human's D-DD1-003 already in the wording. Lessons for the packet-writing rules: run every `cargo test` filter once on HEAD (a filter that selects zero tests is a silent failure); a kind-count prediction must classify the EXISTING inventory under the new rule, not only the additions; a corpus that is the repository under edit is asserted relatively; the default budget of a surface is verified before grepping it.
