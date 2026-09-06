# IMPORT-RESOLUTION-JAVA-1 — an `import a.b.C` resolves to the file that defines `C`

Status: SPECIFIED (2026-09-06) · Track: audit round five, group A, second of three (after
IMPORT-RESOLUTION-RUST-1; human-ratified order 2026-09-06). CODE slice, indexer resolver +
settings_gradle parser + the same zero-state surfaces. Maturity: MATURE.

## 1. Problem (ROOT-CAUSED — `docs/audits/2026-09-06-root-causes-v0.17.0.md` §A; seam pinned)

Outward surface: `modules list` / `cycles` / `stats` on kafka, hadoop, grpc-java, langchain4j,
spring-petclinic print "No cross-module dependencies detected" — kafka's `streams` imports
`clients` at 197 sites; hadoop's hdfs imports `Configuration` from common 1,173 times. trust
says `87,242` (kafka) / `184,959` (hadoop) unresolved imports. Kafka's Gradle modules DO own
their files (clients 1,811) — the cause is resolution, not ownership.

Cause, exactly: the Java extractor emits IMPORTS with `target_key` = the dotted FQN
(`org.apache.kafka.common.Foo`; wildcards `pkg.*`; `java-extractor/src/extractor.rs:322-341`).
No resolver stage maps an FQN to a file: `file_resolution` (`resolver.rs:799-836`) keys by
path and strips extensions only for ts/js/py; stage 4 builds `repo:org.apache.kafka.common.
Foo:FILE`, which never exists. Source roots (`src/main/java`) are recorded nowhere outside
tests; the module dir is all the catalog holds. grpc-java has an ADDITIONAL ownership defect:
`settings.gradle:97-132` relocates 41 projects with `project(':grpc-api').projectDir =
"$rootDir/api" as File`; the parser (`indexer/src/settings_gradle.rs:113,170-172`) honours
only `.name =` renames, so those 41 modules own nothing and root `grpc` owns all 1,624 files.

## 2. Contract

1. **A Java resolution stage, pure, on a suffix index.** Build once per index, from the file
   list already iterated at `orchestrator.rs:862-870`, an index from path suffix
   `<pkg path>/<Name>.java` → FILE stable key (kafka 5,878 / hadoop 12,478 `.java` files —
   one pass). For a key `a.b.C.D`: try the suffix `a/b/C/D.java`, then shorten by one trailing
   segment (`a/b/C.java` — nested class / static member), until a match; a single match
   resolves; multiple matches (shaded copies, e.g. grpc `netty/shaded`) stay unresolved with
   the basis `ambiguous_suffix` and are COUNTED. Wildcard imports (`pkg.*`) stay unresolved
   with the basis `wildcard_import` and are COUNTED — the build report states both counts as
   a share of Java imports per corpus repo; if wildcards exceed 10% on any repo, STOP +
   DECISION_REQUIRED with the options (package-level edge vs stay unresolved). Same-package
   references without an import are CALLS, not IMPORTS — out of scope, stated.
   The stage is a pure function `(key, suffix index) → Option<file key>`; no source-root
   knowledge required.
2. **`projectDir` is honoured.** `parse_settings_gradle` gains the seen form
   `project(':x').projectDir = "$rootDir/<path>" [as File]` (41 lines in grpc-java, incl.
   nested `netty/shaded`, `gcp-observability/interop`), overriding `gradle_path_to_filesystem`
   for that project; `display_name` stays the include name. Any OTHER `projectDir` form found
   in the corpus (`new File(rootDir, …)`, `file('x')`) is COUNTED and stated, not guessed
   (UNDETERMINED for kafka's settings.gradle today — the builder scans it first).
3. **Zero-state surfaces as in the Rust slice** (`N cross-module dependencies (M imports
   unresolved)`; hint only when M == 0; `cycles` over N modules / E edges) — no duplicate
   implementation; the Rust slice landed them, this slice proves them on Java.
4. **Movement measured, before/after, verbatim (isolated).** kafka: `modules list` edges
   (expect streams→clients, connect→clients, …), `cycles`, `stats` fan-in/out, `trust`
   Import-graph level and unresolved count, classification histogram, `map --dry-run`.
   grpc-java: module ownership (expect 41 modules owning files; root no longer 1,624), then
   the same surfaces. hadoop (Maven unparsed → inferred dir modules): edges between inferred
   modules appear; the Maven refusal line stays byte-stable. spring-petclinic (single
   module): byte-stable except the unresolved count. Rust/C++/TS/Python repos byte-stable.

## 3. Stop conditions

Frozen: wire protocol (additive only), storage schema shape, exit codes, the other resolver
stages, non-Java extractors, Maven (out of scope — its absence stays an honest refusal). If
the suffix index needs more than the file list (source-root inference), STOP +
DECISION_REQUIRED. STANDING HONESTY RULES. Unmet DoD → STOP + DECISION_REQUIRED. Never touch
the operator's real state root; every rmap call isolated; never stash. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Fixture FIRST: a two-project Gradle fixture under `repo-index/tests/fixtures/java/
  multi-project/` (`settings.gradle` with `include ':core', ':app'` and one `projectDir`
  relocation; `core/src/main/java/org/x/core/Util.java`, `app/src/main/java/org/x/app/
  Main.java` importing `org.x.core.Util` and `org.x.core.Util.Inner`, plus one wildcard
  import). Integration test asserts: both non-wildcard imports resolve to `Util.java`; the
  wildcard is unresolved with basis `wildcard_import`; one MODULE→MODULE edge app→core; the
  relocated project owns its files. Unit tests for the pure stage: shortening, ambiguity,
  wildcard; parser test for the `projectDir` form verbatim from grpc-java.
- Live proof (isolated; registry sha identical; each corpus repo indexed once before/after —
  hadoop is large: use the retained audit root as "before" and index once after): the §2.4
  tables.
- Gates recorded FIRST; chunked cargo; witness; dogfood-isolated.

## 5. Definition of done

Java FQN imports resolve to files by suffix with stated, counted exceptions; grpc-java's
relocated projects own their files; the JVM corpus repos render real module graphs with the
unresolved count beside them; other ecosystems byte-stable; gates green.

CORPUS PATHS: kafka, hadoop, grpc-java, spring-petclinic, langchain4j at
../legacy-codebases/<name>; repo-graph is THIS repo.
