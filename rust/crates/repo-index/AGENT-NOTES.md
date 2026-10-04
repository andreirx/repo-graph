# repo-index — agent notes

- Gradle module discovery reads only the root `settings.gradle` until DAP-1. After DAP-1 every settings file defines a build. (src/compose.rs:599, :1126-1160, as of f91162f7; source: DAP-1, RC-9)
- File ownership uses the longest module path prefix. The repository root ranks last. (src/compose.rs `compute_cargo_file_ownership`, as of b5c67086; source: D-DGC-BOUNDARY-1)
- Ownership is per toolchain. Gradle modules own `.java`, `.kt` and `.scala` files only. C/C++ files under a Gradle project keep their directory-group owner. (src/compose.rs `persist_gradle_modules`; source: DAP-1 review-0, D-DAP-NESTED-BUILDS-1 correction)
- The Gradle reader mines literal `"group:artifact[:version]"` strings and, since DGC-1B, alias references `libs.<x>` / `libraries.<x>` in dependency argument positions. (src/config.rs:506, :692; src/gradle_catalog.rs; source: DGC-1B)
- A Groovy alias map is readable only when every assignment to it is a literal `libs = […]` or `libs += […]` at the top level or in an unconditional `ext { }`. Any other use that is not a proven read makes the whole map unreadable. (src/gradle_catalog.rs; source: D-DGC1B-MAP-GRAMMAR-1)
- A nested `settings.gradle` is its own build for declared-set attribution. (src/manifest_deps.rs `gradle_nested_settings_file_defines_its_own_build`; source: DGC-1A)
- The tsconfig reader applies a project's `paths` to a file only when the project's membership covers the file. Membership uses `files`, `include`, `exclude`, `allowJs` and `outDir`. (src/config.rs; source: TSA-1, D-TSA-CONFIG-APPLICABILITY-1)
