# module-queries — agent notes

- `reconcile_module_dependencies` matched a declared name to an observed package by exact set membership until DGC-1B. Since DGC-1B the `java` ecosystem also matches on a `.` segment boundary. Other ecosystems stay exact. (src/deps/reconcile.rs:71-77, :135, :195-197; source: DGC-1B, RC-8)
- `ManifestProvenance` is a raw boundary DTO written at index time and read at query time. A field with `#[serde(default)]` and `skip_serializing_if` is additive. A malformed value fails the whole `deps_manifests` decode, and the reader renders unknown-with-reason. (src/deps/types.rs:171-200; source: DGC-1A, DGC-1B)
- An observed Java package whose Maven group is not a package prefix (for example `net.jpountz.lz4` under group `at.yawk.lz4`) is never counted on the deps row. (source: DGC-1B, CC-22)
