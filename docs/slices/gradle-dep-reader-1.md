# GRADLE-DEP-READER-1 — declared-dependency reader for Gradle (Java attribution prerequisite)

Status: SPECIFIED (2026-07-20) · Track: Resolution & attribution (ROADMAP R3; the last open
attribution prerequisite). Depends: ATTRIBUTION-1 (adfd0cf — the surface + PackageDependencySet
consumer), the cargo/npm reader pattern (config.rs).

## 1. Problem

Java references today take the honest degraded path — "(dependency not identified)" — because
no declared-dependency set is captured for Gradle projects. cargo (`extract_cargo_dependencies`)
and npm (`extract_package_dependencies`) both feed a `PackageDependencySet` that
`resolve_external_dependency_name` (ATTRIBUTION-1) reduces import specifiers against; Gradle has
no equivalent reader. Java repos are in the deployment path (the monorepo's JVM components; test
repos in the bucket: spring-petclinic, grpc-java, kafka, hadoop, langchain4j).

## 2. Contract

1. **A gradle reader mirroring the existing pattern:** `extract_gradle_dependencies(content) ->
   Option<PackageDependencySet>` in `repo-index/src/config.rs`, parsing the `dependencies { … }`
   block of `build.gradle` (Groovy DSL) AND `build.gradle.kts` (Kotlin DSL) — the configuration
   verbs (`implementation`, `api`, `compileOnly`, `runtimeOnly`, `testImplementation`, etc.) with
   both string-literal (`'group:artifact:version'`, `"group:artifact:version"`) and
   `group:/name:/version:` map forms. Line-based parsing like the cargo reader (no new build-tool
   dependency) unless a real parser is already vendored. Wire it into the manifest-resolution
   dispatch alongside `resolve_cargo_deps` / the package.json resolver (walk-up to the nearest
   build.gradle; a broken leaf manifest does NOT inherit parent deps — same rule as the others).
2. **The Java coordinate-vs-namespace gap — named honestly (the load-bearing decision):** a
   Gradle coordinate is `group:artifact:version` (`com.google.guava:guava`), but a Java import is
   a PACKAGE namespace (`com.google.common.collect.ImmutableList`). The group id is OFTEN a prefix
   of the import package but NOT always (guava is the canonical counterexample). The captured
   declared-dependency NAME must be chosen so `resolve_external_dependency_name` can match Java
   imports against it where the group-prefixes-package relationship holds, and DEGRADE HONESTLY
   ("dependency not identified") where it does not — NEVER fabricate a name, NEVER force a wrong
   attribution. Record the exact name chosen (group id? full coordinate? both retained?) and the
   matching rule as a decision line. If the measured match rate on a real Java repo is very low
   (the namespace gap dominates), that is a FINDING to surface, not a number to hide.
3. **Deep vertical — it must RENDER (no dormant capability):** the delivered reader must be
   WIRED THROUGH to the attribution output surface for a Java repo. The DoD names the surface:
   `rmap` attribution / the "Unresolved references — where they go" section on spring-petclinic
   (or grpc-java) shows at least one Java reference attributed to its declared Gradle dependency,
   where the namespace relationship permits — proven live.

## 3. Stop conditions

Frozen: the attribution surface's honesty contract (unknown never fabricated), witness/union
surfaces, storage write schema unless the declared-dep persistence path genuinely requires it
(cargo/npm already have a path — mirror it, don't invent). The coordinate-matching rule is the
one ratification-class decision — if ambiguous on the evidence, surface it. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Unit tests for `extract_gradle_dependencies`: Groovy + Kotlin DSL, string + map forms, all
  configuration verbs, malformed block → None (mirror the cargo/npm test shape).
- Live proof on spring-petclinic (and/or grpc-java) under an ISOLATED state root
  (/private/tmp/… — NEVER the operator registry): index, then attribution shows a Java reference
  named to its declared Gradle dep; the match rate recorded honestly.
- Byte-parity on a non-Gradle repo (attribution outputs unchanged where no build.gradle exists).
- Chunked cargo gates (standing pattern); consolidation witness 15/15; SMOKE_ONLY logged run on
  a Java repo green.

## 5. Definition of done

Gradle declared dependencies feed the `PackageDependencySet`; Java attribution names its
declared Gradle deps where the namespace relationship permits and degrades honestly elsewhere;
the capability renders live on spring-petclinic's attribution surface; the coordinate-matching
rule + measured match rate are recorded; gates green.
