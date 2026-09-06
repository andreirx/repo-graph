// Crate `a`: imports across the crate boundary into crate `b`.
//
// `use b::util::helper` → IMPORTS target_key `b::util` → must resolve to
//   b/src/util.rs (candidate `<crate_root>/src/util.rs`).
// `use b::Thing`        → IMPORTS target_key `b`       → must resolve to
//   b/src/lib.rs (the crate entrypoint, after the path shortens to empty).
use b::util::helper;
use b::Thing;

pub fn run() -> String {
    let t = Thing::new();
    format!("{} / {}", helper(), t.label())
}
