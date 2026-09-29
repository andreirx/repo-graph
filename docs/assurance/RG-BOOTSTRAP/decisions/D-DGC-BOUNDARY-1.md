# D-DGC-BOUNDARY-1 — DEPS-GRADLE-CATALOG-1A fixes the three downstream defects its attribution cases exposed, in this slice

Raised: 2026-09-28 by the implementation review of DEPS-GRADLE-CATALOG-1A (INPUT-4, admission 3, cycle 1; reviewer codex gpt-6-sol; decision D-DGC-BOUNDARY-AND-STATUS in `.agent-manager/slices/DEPS-GRADLE-CATALOG-1A/review-0.json`).
- The candidate implemented D-DGC-ATTRIBUTION-1 at the reader, and its reader tests pass.
- The two DGC-A06 boundary tests (`deps list` JSON and human) fail.
- The failures come from defects downstream of the slice's code, root-caused below.

Resolved: 2026-09-29 by the HUMAN. The manager put three options (freeze and follow up; fix everything here; revert the attribution ruling) as a self-contained question; the human chose **"Fix everything here"**.

## The three defects, root-caused (verified in the candidate tree at base 9c4a3822)
1. **The file-ownership tie between the root and a one-character module.**
   - `rust/crates/repo-index/src/compose.rs:3028` `sorted_modules.sort_by(|a, b| b.0.len().cmp(&a.0.len()))` ranks module roots by string length. The repo root `"."` has length 1 and matches every file (:3045-3047), so it ties with any one-character module directory. The stable sort then lets input order decide: files of module `a` can be owned by the root.
   - Shared by all five ecosystem callers (:2995 Rust, :3121 JS/TS, :3181 Python, :3240 JVM, :3302 uncovered). It has never worked for one-character module directories.
   - Smallest fix: the root ranks last (as length 0).
2. **A module covered only by a failed manifest record is dropped from the ecosystem view.**
   - `rust/crates/module-queries/src/deps/compose.rs` `module_covered_by_parsed_manifest` (and `governing_manifest`) admit membership only through a record with `error == None`. The module's "unknown, with reason" answer therefore never renders; the review's fixture showed `results: []`.
   - It never worked: the predicate was parsed-only by design, to avoid fabricating membership.
   - Fix: a failed record covering the module also establishes membership, and the row renders the failure as unknown with its reason, never as a parsed or measured zero.
3. **One error field for two different failures.**
   - `ManifestRecord.error`, written by `record_failed` / `record_failed_manifest`, covers both "the manifest could not be parsed" and "it parsed, but attribution failed". `attach_manifest_context` (compose.rs:519-520) renders every error as `manifest <path> present but not parsed: <reason>`, which is false for an attribution failure beside a declaration that was in fact read.
   - Fix: the record states which failure it is. The change is additive on the persisted `deps_manifests` record: an absent kind reads as the existing parse failure, so old stores keep their meaning. The renderer words each case truthfully.

## Corpus impact (measured by the manager, read-only on the retained v0.19.0 stores, `~/repo-graph-retained/audit-v0.19.0`)
- No repository in the 29-store corpus has a module candidate whose root is one character other than `"."`.
- No store's `deps_manifests` holds a failed record.

All three fixes are therefore expected to move no corpus output beyond what DGC-A04 already predicts. The allocation must prove that, and must prove each fix on its own fixture.

## Scope consequences
- The allocation gains the candidate paths these fixes need (at least `repo-index/src/compose.rs`), named tests for each defect at the reader and at the `deps list` boundary, and a byte oracle stating that the corpus moves nothing beyond DGC-A04.
- The persisted-record change is additive and backward-compatible; nothing else in the wire shape changes.
- DGC-ATTRIBUTION-PRECISE-1 (D-DGC-ATTRIBUTION-1's follow-up) stands.

## Correction (2026-09-29, in-place-manager) — defect 3's wording

Defect 3 above describes an attribution failure as a manifest that "parsed, but attribution failed", beside "a declaration that was in fact read". That is not always true.
- An attribution failure can be recorded against an unreadable `settings.gradle` itself (`resolve_gradle_dir_without_script`), whose contents were never read.
- The correct statement is: the record states whether the failure is a parse failure of the recorded manifest, or a failure to establish Gradle project/declaration attribution. The latter includes a settings-file failure, and asserts nothing about whether the recorded file was read.

Found by DEPS-GRADLE-CATALOG-1A implementation admission 5, review-0 (codex gpt-6-sol). The shipped code comments were corrected in the same slice (`24ca42ac`). The ruling itself is unchanged.
