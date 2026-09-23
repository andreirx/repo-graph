# D-GRADLE-DECLARED-1 — Whose build block does a Gradle declaration belong to?

Raised: 2026-09-23 (audit round seven RC-8, corrected by the seam read): kafka's 61 modules all show `declared_but_unobserved org.ajoberstar.grgit` — `build.gradle:27 classpath "org.ajoberstar.grgit:grgit-core:…"` inside `buildscript { dependencies { } }`, the only literal coordinate in the file; the reader's only block rule is "the word before `{` is `dependencies`" (`repo-index/src/config.rs:545-553`) and every Java file's nearest manifest is the root script. The real declarations (`project(':clients') { dependencies { implementation libs.zstd … } }`, build.gradle:1878-1884) are aliases from `gradle/dependencies.gradle` (`libs += [ zstd: "com.github.luben:zstd-jni:$versions.zstd" … ]`, applied by build.gradle:23) — kafka has no `libs.versions.toml`; grpc-java has one and reads it through `libraries = libs` with dotted accessors. At query time `reconcile.rs:122` compares Maven groups with Java package heads by exact equality. None of RG-REQ-006-L01…L12 said whose block declares for which module.
Resolved: 2026-09-23 by the HUMAN — option A, two slices.

## Options
- **A (ratified)** — L13; 1A block scoping (buildscript/pluginManagement excluded; `project(':x')` attributed to `x`), then 1B alias catalogs (TOML and Groovy map) + `.`-segment group→package matching. Reward: kafka's 61 false rows vanish; clients shows mockito/zstd/lz4/snappy/slf4j declared-and-used; grpc-java's 37 modules get their `libraries.*` declarations. Risk: index-time (reindex); kafka's unknown-classified imports become external-declared where a group matches — large honest movement in trust/orient counts; between 1A and 1B kafka reads "declares nothing this reader can parse" (true, stated).
- **B** — 1A only. Reward: the smallest scope that removes the fabricated attribution (one scanner, one attribution rule). Risk: kafka/grpc-java declare nothing; mockito stays "undeclared".
- **C** — leave it. Reward: no parser or index change, no reindex. Risk: a build plugin stays listed as every kafka module's only dependency.

## Resolution
RG-REQ-006-L13. Queue: DEPS-GRADLE-CATALOG-1A → 1B. Follow-up DEPS-JAVA-SELF-1 (own `package` declarations as the self set); `javax.*`/`jakarta.*` missing from the Java runtime-builtin list noted.
