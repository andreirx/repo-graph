# D-DAP-JVM-COVERAGE-1 — a nested Gradle build owns the INDEXED JVM sources (today `.java`); Kotlin and Scala are a stated coverage limit, not an ownership promise

Raised: 2026-10-04 by the implementation reviewer (codex gpt-6-sol) of DGC-ATTRIBUTION-PRECISE-1 at admission 2, cycle 0 (`DECISION_REQUIRED: D-DAP-JVM-COVERAGE`).

**The problem in plain words.** The ratified record (D-DAP-NESTED-BUILDS-1, first correction), the slice document and the candidate's CLI contract said a nested Gradle build owns "the JVM files (`.java`, `.kt`, `.scala`)" under it. rmap indexes `.java` only: `rust/crates/indexer/src/routing.rs:182` maps `.java` → `java` (`detect_language`, :169-188) and the scanner's admission predicate `is_source_extension` (routing.rs:30-44, called at `repo-index/src/scanner.rs:186`) admits `.java` as the only JVM extension; a `.kt` or `.scala` file never enters the file list, so no module can own it. The field capture shows it: `rabbitmq-tutorial [kotlin]` is a declared module with `0 owned files` while `kotlin/src/commonMain/kotlin/HelloWorld.kt:1` imports `dev.kourier.amqp.Field`. An agent reading the contract would expect Kotlin sources on that row; an agent reading the row may take the zero for an empty module. The Java nested-build fix itself is correct (grpc-java 43 → 67 modules; `examples/example-tls/build.gradle:28` attributed to `examples/example-tls`).

**Options (the reviewer's, reward / risk by usefulness to an agent net of misdirection).**
- A — narrow the claim to indexed Java source. Reward: the new declared Gradle boundaries ship without a false Kotlin/Scala ownership promise; the zero-file count is qualified as indexed-file scope. Risk: the allocation, the record and the contract wording change; the row itself does not yet say WHY it owns no file.
- B — index Kotlin and Scala. Reward: the JVM claim becomes true. Risk: a new language-coverage slice (extractors, refresh, counts on every surface) far outside this slice's frozen paths.
- C — defer the candidate. Risk: the verified Java improvement waits for nothing it needs.

Resolved: 2026-10-04 by the OPERATOR (in-place manager) as **A** (the reviewer's recommendation); the human may override. Follow-ups filed: KOTLIN-SCALA-INDEXING-1 (option B as its own slice, requirements first — RG-REQ-001-L01's shipped-language set), UNINDEXED-LANGUAGE-ROWS-1 (a declared module whose sources are in an unindexed language states it on its row — the same gap the round-eight audit recorded as kafka `core`'s 58 unindexed Scala files unsaid).

## What this record rules
- Every ownership statement (record, allocation, contract, code comments and test names) reads "the indexed JVM files (today `.java`) and Gradle manifests under a nested build"; no statement names `.kt` or `.scala` as owned.
- No mechanism changes: `persist_gradle_modules`, `scanner.rs` and `routing.rs` are untouched; a test fixture may use only extensions the scanner admits for an ownership claim.
