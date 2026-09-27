# D-DGC-ATTRIBUTION-1 — Where the nearest build script cannot decide a Gradle file's declared set, the answer says so; it is never a certain empty set

Raised: 2026-09-28 by the implementation review of DEPS-GRADLE-CATALOG-1A (INPUT-3, admission 2, cycle 1; reviewer codex gpt-6-sol; decision in `.agent-manager/slices/DEPS-GRADLE-CATALOG-1A/review-0.json`). Every check had passed. The review found two ways the candidate's attribution, which reads through the nearest build script, can report a certain answer where the truth is unknown. Both were verified in the candidate's `rust/crates/repo-index/src/manifest_deps.rs`.

1. **No ancestor script.** `resolve_gradle_dir` returns `Vec::new()` when a Java file has no ancestor `build.gradle[.kts]`. RG-REQ-006-L13 lets any project script of the build declare for a project through `project(':P') { dependencies { … } }`. So a file inside a settings-included project that has no script of its own, and no scripted ancestor, gets a certain EMPTY declared set, although another script may declare for it.
2. **A failure hidden behind a parsed record.** `ManifestProvenanceCollector::record` and `record_failed` both keep the first record per path. A settings or script failure reached through a root script that was already recorded as parsed is dropped. The query side then presents the declared set as known.

Resolved: 2026-09-28 by the OPERATOR (in-place-manager), under the human's standing ruling of 2026-09-26: where the heuristics cannot determine something, take the easiest honest rule and mark it "can't determine — open it and look inside". The human may override.

## Ruling (option D; the reviewer offered A, B and C)
- **Case 1.** For a Java file inside a Gradle build (a settings file of the build includes its project, or the build's scripts configure it) whose project is not reached through an ancestor script, the declared set is never a certain empty set.
  - Either apply the build's `project(':P')` declarations, if the existing `GradleBuild::declared_for` already determines them; or mark the set undetermined through the existing `undetermined_blocks` marking, with reader wording that tells the agent to open the build scripts.
  - The builder takes the smaller of the two and states which.
  - A Java file in NO Gradle build (no script, no settings anywhere above it) keeps today's empty set; that is not uncertainty.
- **Case 2.** A failure recorded for a path is never discarded because a parsed record for the same path came first: a failure wins over parsed for that path. This can over-mark uncertainty (other files under that script also read as unknown) but never claims certainty that does not exist.
- **Both cases:** tested at the reader AND at the `deps list` boundary (JSON and human), with the reviewer's two counterexamples as fixtures. The test names are bound in the allocation's checks.

## Why not the reviewer's options
- **A (extend attribution/provenance to express both outcomes exactly).** The right long-term shape, but the reviewer says it may change a boundary data shape and needs design review. The ruling reaches the same honesty with no new shape.
- **B (narrow L13).** Leaves valid declarations undiscovered, and needs a requirement amendment.
- **C (defer).** Keeps the false kafka/grpc-java declarations this slice removes.

## Consequences
The allocation gains the fixtures' test names in its checks; no new candidate path is expected, as `manifest_deps.rs` is allocated. Precise cross-script attribution (option A) is a follow-up, DGC-ATTRIBUTION-PRECISE-1, and waits for evidence that such layouts occur in the corpus.
