<!-- requirements-assurance-implementation-v1
{
  "formatVersion": 1,
  "kind": "implementation-allocation",
  "workItemId": "PORTABLE-TMP-1",
  "baselinePath": "docs/requirements/baselines/PORTABLE-TMP-1-INPUT-1.json",
  "parentRequirementIds": [
    "RG-REQ-011",
    "RG-REQ-014"
  ],
  "implements": [
    "RG-REQ-011-L06",
    "RG-REQ-011-L08",
    "RG-REQ-014-L09"
  ],
  "preserves": [
    "RG-REQ-011-L01",
    "RG-REQ-011-L02",
    "RG-REQ-011-L03",
    "RG-REQ-011-L04",
    "RG-REQ-011-L05",
    "RG-REQ-011-L07",
    "RG-REQ-011-L09",
    "RG-REQ-011-L10",
    "RG-REQ-011-L11",
    "RG-REQ-014-L01",
    "RG-REQ-014-L02",
    "RG-REQ-014-L03",
    "RG-REQ-014-L04",
    "RG-REQ-014-L05",
    "RG-REQ-014-L06",
    "RG-REQ-014-L07",
    "RG-REQ-014-L08"
  ],
  "preservationObligationIds": [
    "P-PT-01",
    "P-PT-02",
    "P-PT-03",
    "P-PT-04"
  ],
  "changes": [],
  "acceptanceBoundary": "The platform-paths unit suite covers both OS arms of the sandbox temp base, the sandbox state root and its composition under a base, and the sandbox-local predicate, tested on this macOS host through the pure OS-name functions. The daemon-runtime lib suite covers state-root mode detection through the shared predicate, the A1 guard and the A2/B index proof with their macOS-only gates removed, and Global-mode tests that choose and assert their root; it runs with the snapshot_retention integration suite. The rgr lib suite covers root CREATION through the one-parameter seam against a scratch base (mode 0700, idempotent, named error), the client's root being platform-paths' root, and the EPERM/EACCES detection tests unchanged. Every cargo suite that can reach a default state root runs under a named isolated RMAP_STATE_ROOT/RMAP_SOCKET_PATH that the check creates and removes, and the operator registry is unchanged. Every check keeps the exit status of each command in its pipelines (`set -o pipefail`), reads digests through a helper that fails unless it gets a 64-hex value, and writes its logs only to per-run, never-overwritten files in the slice's relay directory. Also in scope: fmt/clippy over platform-paths, daemon-runtime and rgr; the script checks (bash -n, one `/private/tmp` literal in `scripts/`, every script's code identical to HEAD's once the base variable reads `/private/tmp`, the base resolving to `/private/tmp` on this Mac and to `/tmp` under a `uname` shim printing `Linux`); the one agent_docs row; the hunk-window preservation oracle; and the CLI proof on this Mac. In that proof the candidate `rmap doctor`, in AUTOMATIC transport, faces a socket it may not connect to (EACCES) with an explicitly isolated RMAP_STATE_ROOT under `/private/tmp`. It falls back to stdio, the daemon classifies that root sandbox-local and blocks authority writes, and doctor renders it. UNVERIFIED and not claimed (operator ruling on review-0 D-1): creation of the real per-uid root `/private/tmp/repo-graph-agent/<uid>` end to end, and any Linux host run (RG-REQ-014-L09's validation stays pending).",
  "candidatePaths": [
    "rust/crates/platform-paths/src/lib.rs",
    "rust/crates/platform-paths/src/sandbox.rs",
    "rust/crates/rgr/src/cli/paths.rs",
    "rust/crates/rgr/src/daemon_client/stdio_transport.rs",
    "rust/crates/rgr/src/platform/mod.rs",
    "rust/crates/daemon-runtime/src/lib.rs",
    "rust/crates/daemon-runtime/src/state.rs",
    "rust/crates/daemon-runtime/src/handlers/inventory/tests.rs",
    "rust/crates/daemon-runtime/tests/snapshot_retention.rs",
    "scripts/lib/sandbox-tmp-base.sh",
    "scripts/validate-layer2-fixture.sh",
    "scripts/orient-density-nginx-capture.sh",
    "scripts/dogfood-isolated.sh",
    "scripts/dv1-inflight-e2e.sh",
    "scripts/test-install-robustness-2.sh",
    "scripts/smoke-rmap.sh",
    "scripts/byte-compare-map-modules-surfaces.sh",
    "scripts/test-smoke-rmap.sh",
    "scripts/byte-compare-five-surfaces.sh",
    "scripts/smoke-validation-repos.sh",
    "scripts/find-facts-e2e.sh",
    "agent_docs/storage-architecture-v2.md"
  ],
  "postReviewRecordPaths": [
    "docs/assurance/PORTABLE-TMP-1/verification.json",
    "docs/assurance/PORTABLE-TMP-1/implementation-review.json"
  ],
  "candidateExclusions": [
    {
      "pathPrefix": ".agent-manager/",
      "reason": "local relay state and raw run/progress evidence"
    },
    {
      "pathPrefix": "rust/target/",
      "reason": "reproducible Cargo build output"
    }
  ],
  "checks": [
    {
      "checkId": "PT-C01",
      "obligationIds": [
        "RG-REQ-014-L09",
        "RG-REQ-011-L06",
        "RG-REQ-011-L08",
        "RG-REQ-011-L09",
        "P-PT-01",
        "P-PT-02"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "set -o pipefail; RUN=$(date -u +%Y%m%dT%H%M%SZ)-$$; LOGD=../.agent-manager/slices/PORTABLE-TMP-1/logs; { test -d ../.agent-manager/slices/PORTABLE-TMP-1 && test ! -e \"$LOGD/pt-c01-$RUN.txt\"; } || { echo 'PT-C01 PRECONDITION FAILED: nothing written'; exit 1; }; mkdir -p \"$LOGD\" && echo \"log: $LOGD/pt-c01-$RUN.txt\" && cargo test -p repo-graph-platform-paths 2>&1 | tee \"$LOGD/pt-c01-$RUN.txt\" | grep -E '^test result: ok\\.' && ! grep -E '^test .* FAILED$|^thread .* panicked at|^test result: FAILED' \"$LOGD/pt-c01-$RUN.txt\" && for t in macos_sandbox_temp_base_is_private_tmp every_other_os_sandbox_temp_base_is_tmp macos_sandbox_state_root_is_private_tmp_repo_graph_agent_uid linux_sandbox_state_root_is_tmp_repo_graph_agent_uid private_tmp_root_on_macos_is_sandbox_local tmp_root_on_linux_is_sandbox_local private_tmp_root_on_linux_is_not_sandbox_local non_temp_root_is_not_sandbox_local_on_either_os macos_predicate_is_component_wise_as_before sandbox_state_root_is_sandbox_local_on_both_os_arms current_host_functions_use_the_host_os_and_effective_uid sandbox_state_root_under_joins_the_agent_dir_and_uid; do grep -qE \"^test sandbox::tests::$t \\.\\.\\. ok$\" \"$LOGD/pt-c01-$RUN.txt\" || { echo \"MISSING $t\"; exit 1; }; done && for t in home::tests::effective_uid_returns_current_user socket::tests::daemon_socket_path_returns_some socket::tests::override_takes_precedence dirs::tests::data_dir_returns_some; do grep -qE \"^test $t \\.\\.\\. ok$\" \"$LOGD/pt-c01-$RUN.txt\" || { echo \"MISSING $t\"; exit 1; }; done && git diff --quiet HEAD -- crates/platform-paths/src/home.rs crates/platform-paths/src/dirs.rs crates/platform-paths/src/socket.rs crates/platform-paths/Cargo.toml && grep -q 'env::consts::OS' crates/platform-paths/src/sandbox.rs && ! grep -nE 'env::var|var_os|temp_dir|TMPDIR|cfg!?\\(target_os' crates/platform-paths/src/sandbox.rs && grep -qE '^mod sandbox;$' crates/platform-paths/src/lib.rs && grep -qE '^pub use sandbox::' crates/platform-paths/src/lib.rs && test \"$(grep -cE '\"/private/tmp\"' crates/platform-paths/src/sandbox.rs)\" = 1",
        "cwd": "rust",
        "environment": "candidate tree; the platform-paths tests compute paths only. The pre-existing `daemon_socket_path_returns_some` resolves the CANONICAL socket path, as its contract requires, and may open-and-close a connection to it with no request sent (reachability probe, socket.rs:66-81). No state root is created or read.",
        "inputs": "NEW module `rust/crates/platform-paths/src/sandbox.rs` (mod + `pub use` in lib.rs; the lib.rs platform table gains a Sandbox row). API (names binding; visibility of the `_for_os` functions the builder's — crate-private suffices): `SANDBOX_STATE_DIR: &str = \"repo-graph-agent\"`; `sandbox_temp_base_for_os(os: &str) -> &'static Path` — `\"macos\"` → `/private/tmp`, ANY other value → `/tmp`; `sandbox_state_root_under(base: &Path, uid: u32) -> PathBuf` = base/`repo-graph-agent`/`<uid>` (the ONE composition of the root; rgr's creation seam calls it with a scratch base in tests and `sandbox_temp_base()` in production); `sandbox_state_root_for(os: &str, uid: u32) -> PathBuf` = `sandbox_state_root_under(sandbox_temp_base_for_os(os), uid)`; `is_sandbox_local_state_root_for_os(os: &str, state_root: &Path) -> bool` = `state_root.starts_with(sandbox_temp_base_for_os(os))` — component-wise `Path::starts_with`, the exact semantics of today's `state_root.starts_with(\"/private/tmp/\")` at daemon-runtime/src/state.rs:876 (a trailing `/` adds no component), so on macOS `/private/tmp` itself is sandbox-local, `/private/tmpfoo/x` and `/tmp/x` are not; the current-host wrappers `sandbox_temp_base()`, `sandbox_state_root()` (with `effective_uid()`, home.rs:66) and `is_sandbox_local_state_root(&Path)` pass `std::env::consts::OS`. The predicate is deliberately broader than the injected root: every root under the base is sandbox-local (scripts' isolated roots on macOS are classified so today — D-PTMP-ROOT-1 Consequences), and its name says `sandbox_local`, not `the sandbox root`. No environment input (no `$TMPDIR`, no `env::var` — D-PTMP-ROOT-1 option B; RG-REQ-011-L09), no `#[cfg(target_os)]` / `cfg!(target_os)` in sandbox.rs, exactly one `\"/private/tmp\"` literal in sandbox.rs code (tests may spell expected paths as other literals, e.g. `\"/private/tmp/repo-graph-agent/501\"`, which the `\"/private/tmp\"` count does not match). NEW tests `sandbox::tests::…`: `macos_sandbox_temp_base_is_private_tmp`; `every_other_os_sandbox_temp_base_is_tmp` (`linux`, `freebsd`, `\"\"`); `macos_sandbox_state_root_is_private_tmp_repo_graph_agent_uid` (`(\"macos\", 501)` → `/private/tmp/repo-graph-agent/501`); `linux_sandbox_state_root_is_tmp_repo_graph_agent_uid` (`(\"linux\", 1000)` → `/tmp/repo-graph-agent/1000`); `private_tmp_root_on_macos_is_sandbox_local` (`/private/tmp/repo-graph-agent/501`, `/private/tmp/repo-graph-dogfood/run`); `tmp_root_on_linux_is_sandbox_local` (`/tmp/repo-graph-agent/1000`, `/tmp/x`); `private_tmp_root_on_linux_is_not_sandbox_local`; `non_temp_root_is_not_sandbox_local_on_either_os` (`/Users/u/Library/Application Support/repo-graph`, `/home/u/.local/share/rmap`, `/var/tmp/x` on both arms); `macos_predicate_is_component_wise_as_before` (`/private/tmp` → true, `/private/tmpfoo/x` → false, `/tmp/x` → false on `macos`); `sandbox_state_root_is_sandbox_local_on_both_os_arms` (the root the client creates is one the daemon classifies — the pairing D-PTMP-ROOT-1 option B warns `$TMPDIR` would break); `current_host_functions_use_the_host_os_and_effective_uid` (each wrapper equals its `_for_os` twin at `std::env::consts::OS` and `effective_uid()`). `sandbox_state_root_under_joins_the_agent_dir_and_uid` (`(\"/scratch/base\", 7)` → `/scratch/base/repo-graph-agent/7`). Existing home/socket/dirs tests bound by name; home.rs, dirs.rs, socket.rs and Cargo.toml unchanged (no new dependency). Seam: WHAT — the OS name is an argument of pure functions; USERS — the three host wrappers and through them rgr's stdio fallback and doctor, the daemon's startup clearing and mode detection, the daemon tests; AXIS — the host OS (macOS vs every other Unix); REJECTED — a `#[cfg(target_os)]` constant, which compiles one arm per host so this Mac's gate would never execute the Linux arm."
      },
      "expected": "exit 0: macOS resolves `/private/tmp` and `/private/tmp/repo-graph-agent/<uid>` exactly as today and classifies exactly today's set; every other OS resolves `/tmp` and `/tmp/repo-graph-agent/<uid>`; a `/private/tmp` root is not sandbox-local on Linux; a non-temp root is sandbox-local on neither; the root the client would create always satisfies the predicate the daemon applies; the functions read no environment; no behavior change in the existing socket, home and data-directory path authorities (RG-REQ-011-L09 preserved)"
    },
    {
      "checkId": "PT-C02",
      "obligationIds": [
        "RG-REQ-011-L06",
        "RG-REQ-011-L03",
        "RG-REQ-014-L09",
        "P-PT-01",
        "P-PT-02",
        "P-PT-03"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "set -o pipefail; RUN=$(date -u +%Y%m%dT%H%M%SZ)-$$; LOGD=../.agent-manager/slices/PORTABLE-TMP-1/logs; dg() { local d; d=$(shasum -a 256 \"$1\" | cut -d' ' -f1) || return 1; [[ $d =~ ^[0-9a-f]{64}$ ]] || return 1; printf %s \"$d\"; }; I=/private/tmp/PORTABLE-TMP-1-daemon-runtime-state; REG=\"$HOME/Library/Application Support/repo-graph/registry.json\"; { test -d ../.agent-manager/slices/PORTABLE-TMP-1 && test ! -e \"$LOGD/pt-c02-$RUN.txt\" && test ! -e \"$LOGD/pt-c02b-$RUN.txt\" && test -f \"$REG\" && test ! -e \"$I\" && ! ls -d /var/tmp/rg-global-state-* >/dev/null 2>&1 && test -d /var/tmp && test -w /var/tmp; } || { echo 'PT-C02 PRECONDITION FAILED: nothing created, nothing deleted'; exit 1; }; mkdir \"$I\" || { echo 'PT-C02 could not create its root: nothing deleted'; exit 1; }; trap 'rm -rf \"$I\"' EXIT; mkdir -p \"$LOGD\" && echo \"logs: $LOGD/pt-c02-$RUN.txt $LOGD/pt-c02b-$RUN.txt\" && D0=$(dg \"$REG\") && RMAP_STATE_ROOT=\"$I\" RMAP_SOCKET_PATH=\"$I/d.sock\" cargo test -p repo-graph-daemon-runtime --lib 2>&1 | tee \"$LOGD/pt-c02-$RUN.txt\" | grep -E '^test result: ok\\.' && ! grep -E '^test .* FAILED$|^thread .* panicked at|^test result: FAILED' \"$LOGD/pt-c02-$RUN.txt\" && for t in state::tests::state_root_mode_global_for_normal_paths state::tests::state_root_mode_sandbox_for_the_platform_sandbox_root state::tests::state_root_mode_sandbox_for_any_root_under_the_sandbox_temp_base state::tests::state_root_mode_enum_as_str state::tests::state_root_mode_allows_authority_writes handlers::inventory::tests::mark_baseline_blocked_in_sandbox_mode handlers::inventory::tests::unmark_baseline_blocked_in_sandbox_mode handlers::inventory::tests::mark_baseline_allowed_in_global_mode handlers::inventory::tests::index_allowed_in_sandbox_mode_proves_a2_and_b_writes; do grep -qE \"^test $t \\.\\.\\. ok$\" \"$LOGD/pt-c02-$RUN.txt\" || { echo \"MISSING $t\"; exit 1; }; done && ! grep -q 'state_root_mode_sandbox_for_private_tmp' \"$LOGD/pt-c02-$RUN.txt\" && RMAP_STATE_ROOT=\"$I\" RMAP_SOCKET_PATH=\"$I/d.sock\" cargo test -p repo-graph-daemon-runtime --test snapshot_retention 2>&1 | tee \"$LOGD/pt-c02b-$RUN.txt\" | grep -E '^test result: ok\\.' && ! grep -E '^test .* FAILED$|^thread .* panicked at|^test result: FAILED' \"$LOGD/pt-c02b-$RUN.txt\" && for t in a_dispatched_read_blocks_while_the_pass_holds_writing_then_reads_correct_data auto_trigger_queues_pass_and_records_report comparative_assess_works_against_a_narrowed_stamp_baseline daemon_info_surfaces_the_active_retention_op default_mark_is_a_stamp_that_narrows_only_after_leaving_the_serving_pair mark_baseline_cost_read_failure_precedes_the_mark_and_leaves_class_unchanged mark_baseline_rejects_non_boolean_retain_rows opt_out_disables_the_auto_trigger retain_rows_mark_keeps_rows_and_a_default_remark_never_downgrades_it retain_rows_on_a_known_empty_snapshot_is_allowed_and_labeled retention_yields_to_a_real_inflight_index_then_runs three_real_indexes_prune_to_current_only threshold_gated_vacuum_runs_above_and_skips_below_through_the_daemon_pass; do grep -qE \"^test $t \\.\\.\\. ok$\" \"$LOGD/pt-c02b-$RUN.txt\" || { echo \"MISSING $t\"; exit 1; }; done && ! git grep -n '/private/tmp' -- crates/daemon-runtime/src/lib.rs crates/daemon-runtime/src/state.rs crates/daemon-runtime/src/handlers/inventory/tests.rs crates/daemon-runtime/tests/snapshot_retention.rs && ! grep -n 'cfg(target_os = \"macos\")' crates/daemon-runtime/src/handlers/inventory/tests.rs && grep -q 'is_sandbox_local_state_root' crates/daemon-runtime/src/state.rs && grep -q 'sandbox_state_root()' crates/daemon-runtime/src/lib.rs && grep -q 'sandbox_temp_base()' crates/daemon-runtime/src/handlers/inventory/tests.rs && ! grep -q 'DaemonState::new(); // Global mode' crates/daemon-runtime/src/handlers/inventory/tests.rs && grep -q '/var/tmp/rg-global-state-control' crates/daemon-runtime/src/state.rs && grep -q '/var/tmp/rg-global-state-control' crates/daemon-runtime/src/handlers/inventory/tests.rs && ! git grep -n 'CARGO_TARGET_TMPDIR' -- crates/daemon-runtime && for f in $(git grep -lE '\"(mark_baseline|unmark_baseline|repo_alias)\"' -- 'crates/*/tests/*.rs'); do { grep -q '\"/var/tmp\"' \"$f\" && grep -q 'rg-global-state-' \"$f\" && grep -q 'is_sandbox_mode()' \"$f\"; } || { echo \"A1-WRITING TEST WITHOUT AN ASSERTED GLOBAL ROOT: $f\"; exit 1; }; done && test \"$(git diff HEAD --name-only --relative -- crates/daemon-runtime | LC_ALL=C sort | tr '\\n' ' ')\" = 'crates/daemon-runtime/src/handlers/inventory/tests.rs crates/daemon-runtime/src/lib.rs crates/daemon-runtime/src/state.rs crates/daemon-runtime/tests/snapshot_retention.rs ' && D1=$(dg \"$REG\") && test \"$D1\" = \"$D0\" && ! ls -d /var/tmp/rg-global-state-* >/dev/null 2>&1 && rm -rf \"$I\" && test ! -e \"$I\"",
        "cwd": "rust",
        "environment": "candidate tree. ISOLATION (review-0 F-1, applied to the whole class): both suites run with RMAP_STATE_ROOT=/private/tmp/PORTABLE-TMP-1-daemon-runtime-state and RMAP_SOCKET_PATH=<it>/d.sock. The command creates that root and removes it. The daemon-runtime lib tests build `DaemonState::new()`, which resolves the default state root (registry.rs:135-160) when no override is set: governance/tests.rs:44…, inventory/tests.rs:76…, quality/tests.rs:44…, state.rs:1256…, reconcile.rs:354 at HEAD. Those tests now read an empty isolated registry instead of the operator's. No daemon-runtime lib test asserts the default socket path (`daemon_socket_path()` is called only by lib.rs:330, production). FAIL-CLOSED: every precondition is evaluated in one group that exits BEFORE anything is created. The root is created with plain `mkdir`, which fails if it exists, so the EXIT trap only ever removes a directory this run created. The root is removed explicitly and its absence asserted. The operator registry digest is compared before and after. Test-owned Global-mode roots are `/var/tmp/rg-global-state-*` tempdirs (removed on drop; the command asserts none remain). The inventory A2/B test writes and removes its own `<sandbox temp base>/repo-graph-test-<nanos>`, as it does today under /private/tmp.",
        "inputs": "DETECTION (RG-REQ-011-L06): `DaemonState::state_root_mode` (state.rs:871-881) returns `SandboxLocal` iff `repo_graph_platform_paths::is_sandbox_local_state_root(state_root)`; the doc comments at state.rs:63-67 and :870-874 describe the per-OS base instead of `/private/tmp/` (no literal). CLEARING: `clear_stale_sandbox_state` (lib.rs:265-285) clears `repo_graph_platform_paths::sandbox_state_root()` (today the literal at lib.rs:269 with `libc::geteuid`); its docs at :254 and :315 and `run_daemon_stdio`'s at :389 name the per-OS root without the literal; `#[cfg(unix)]`/`#[cfg(not(unix))]` split unchanged; warnings-only error handling unchanged. TESTS (state.rs): `state_root_mode_global_for_normal_paths` stops taking its root from `tempdir()` (under `/tmp` on Linux, so sandbox-local there) and uses a fixed non-temp path through `RepoRegistry::with_test_state_root`; `state_root_mode_sandbox_for_private_tmp` is RENAMED `state_root_mode_sandbox_for_the_platform_sandbox_root` (its root: `sandbox_state_root()`; the old name states a macOS-only fact); NEW `state_root_mode_sandbox_for_any_root_under_the_sandbox_temp_base` (`sandbox_temp_base().join(\"repo-graph-dogfood/run\")` → `SandboxLocal`, A1 writes refused — the scripts' isolated roots). INVENTORY TESTS (handlers/inventory/tests.rs): `make_sandbox_state` (:64-68) uses `sandbox_state_root()`; `index_allowed_in_sandbox_mode_proves_a2_and_b_writes` (:321-436) roots at `sandbox_temp_base().join(format!(\"repo-graph-test-{}\", test_id))` and loses its `#[cfg(target_os = \"macos\")]` (:322) — nothing in it is macOS-specific once the base is the platform's (it writes one TS file, indexes it through the dispatcher, removes its root); the gated imports (:20-21, :26-27, :29-30) lose their gates; the doc lines :315/:320/:332 lose the literal and the \"macOS-only\" claim. GLOBAL-MODE RULE (P-PT-03; review-0 F-3 and the same class under F-1's isolated root): a test that needs Global mode CHOOSES its state root explicitly and ASSERTS it is Global. It never inherits Global from `tempfile::tempdir()`, `CARGO_TARGET_TMPDIR`, `DaemonState::new()` or the environment; any of those can land under the sandbox base (Linux `/tmp`; the isolated root above under `/private/tmp`). The chosen root is `/var/tmp`, writable by every user on macOS and Linux and never under `/tmp` or `/private/tmp` (it is `non_temp_root_is_not_sandbox_local_on_either_os`'s negative case). (a) `daemon-runtime/tests/snapshot_retention.rs` `isolated()` (:172-181), which serves the six `mark_baseline`/`unmark_baseline` tests, takes `tempfile::Builder::new().prefix(\"rg-global-state-\").tempdir_in(\"/var/tmp\")`. It asserts, with a message naming the rule, that the resulting `DaemonState` has `!is_sandbox_mode()`. A host whose `/var/tmp` is missing, unwritable or classified sandbox fails the helper loudly instead of silently running the tests in sandbox mode. (b) `state::tests::state_root_mode_global_for_normal_paths` uses `RepoRegistry::with_test_state_root(\"/var/tmp/rg-global-state-control\")` (no filesystem access) and asserts `!is_sandbox_local_state_root` on it and Global on the daemon. (c) `handlers::inventory::tests::mark_baseline_allowed_in_global_mode` stops using `DaemonState::new()` (\"// Global mode\", :283), which is sandbox-local under this check's isolated root and on any host whose default root is under the base. It builds its state from the same fixed control root and asserts `!state.is_sandbox_mode()` before the guard call. The guard enforces the rule over the class: every `crates/*/tests/*.rs` that sends an A1 method uses a `\"/var/tmp\"` `rg-global-state-` root and asserts `is_sandbox_mode()`. At HEAD that set is snapshot_retention.rs alone (`git grep`, 2026-09-27; no rgr test sends an A1 method). Among the lib tests, only these two assert Global or pass the A1 guard (`require_global_mode_for_authority_write` callers: baseline.rs:87/:519, dispatch.rs:887; the other `DaemonState::new()` tests expect RepoNotFound/InvalidRequest, which the mode does not change). No `CARGO_TARGET_TMPDIR` anywhere in daemon-runtime. Snapshot-retention suite (RG-REQ-011-L03's verification) bound in full by name. daemon-runtime's tracked diff is exactly the four files. Guardrail call (CLAUDE.md, files over 500 lines — state.rs 1732): the predicate body and its tests change, no responsibility is appended."
      },
      "expected": "exit 0: the daemon classifies a state root sandbox-local by the shared predicate only; the A1 guard, the A2/B index proof and the Global-mode control run on every OS (no macOS gate left); every test that needs Global mode chooses `/var/tmp` and asserts Global; the startup clearing targets the platform root; retention and baseline tests stay green (RG-REQ-011-L03 preserved, no behavior change); both suites ran against an isolated root this run created and removed, and the operator registry is unchanged"
    },
    {
      "checkId": "PT-C03",
      "obligationIds": [
        "RG-REQ-011-L08",
        "RG-REQ-011-L09",
        "RG-REQ-014-L09",
        "P-PT-01",
        "P-PT-02"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "set -o pipefail; RUN=$(date -u +%Y%m%dT%H%M%SZ)-$$; LOGD=../.agent-manager/slices/PORTABLE-TMP-1/logs; dg() { local d; d=$(shasum -a 256 \"$1\" | cut -d' ' -f1) || return 1; [[ $d =~ ^[0-9a-f]{64}$ ]] || return 1; printf %s \"$d\"; }; I=/private/tmp/PORTABLE-TMP-1-rgr-state; REG=\"$HOME/Library/Application Support/repo-graph/registry.json\"; { test -d ../.agent-manager/slices/PORTABLE-TMP-1 && test ! -e \"$LOGD/pt-c03-$RUN.txt\" && test ! -e \"$LOGD/pt-c03b-$RUN.txt\" && test -f \"$REG\" && test ! -e \"$I\" && command -v rmapd >/dev/null; } || { echo 'PT-C03 PRECONDITION FAILED: nothing created, nothing deleted'; exit 1; }; mkdir \"$I\" || { echo 'PT-C03 could not create its root: nothing deleted'; exit 1; }; trap 'rm -rf \"$I\"' EXIT; mkdir -p \"$LOGD\" && echo \"logs: $LOGD/pt-c03-$RUN.txt $LOGD/pt-c03b-$RUN.txt\" && D0=$(dg \"$REG\") && RMAP_STATE_ROOT=\"$I\" RMAP_SOCKET_PATH=\"$I/d.sock\" cargo test -p repo-graph-rgr --lib -- --skip cli::paths::tests::daemon_socket_path_returns_some 2>&1 | tee \"$LOGD/pt-c03-$RUN.txt\" | grep -E '^test result: ok\\.' && ! grep -E '^test .* FAILED$|^thread .* panicked at|^test result: FAILED' \"$LOGD/pt-c03-$RUN.txt\" && for t in daemon_client::stdio_transport::tests::client_sandbox_root_is_the_platform_paths_root daemon_client::stdio_transport::tests::sandbox_root_is_created_with_mode_0700_under_the_given_base daemon_client::stdio_transport::tests::sandbox_root_creation_is_idempotent_when_the_directory_exists daemon_client::stdio_transport::tests::sandbox_root_creation_failure_is_a_named_error daemon_client::stdio_transport::tests::stdio_transport_ping daemon_client::stdio_transport::tests::state_root_mode_display daemon_client::transport::tests::is_permission_denied_detects_eacces daemon_client::transport::tests::is_permission_denied_detects_eperm daemon_client::transport::tests::is_permission_denied_rejects_econnrefused daemon_client::transport::tests::is_permission_denied_connection_error_detects_os_error_1 daemon_client::transport::tests::is_permission_denied_connection_error_rejects_connrefused; do grep -qE \"^test $t \\.\\.\\. ok$\" \"$LOGD/pt-c03-$RUN.txt\" || { echo \"MISSING $t\"; exit 1; }; done && ! grep -q 'stdio_transport::tests::sandbox_root_path_format' \"$LOGD/pt-c03-$RUN.txt\" && test -d \"$I/databases\" && RMAP_STATE_ROOT=\"$I\" cargo test -p repo-graph-rgr --lib cli::paths::tests::daemon_socket_path_returns_some -- --exact 2>&1 | tee \"$LOGD/pt-c03b-$RUN.txt\" | grep -E '^test result: ok\\. 1 passed' && grep -qE '^test cli::paths::tests::daemon_socket_path_returns_some \\.\\.\\. ok$' \"$LOGD/pt-c03b-$RUN.txt\" && ! git grep -n '/private/tmp' -- crates/rgr/src/daemon_client/stdio_transport.rs crates/rgr/src/platform/mod.rs crates/rgr/src/cli/paths.rs && grep -q 'create_sandbox_state_root(paths::sandbox_temp_base())' crates/rgr/src/daemon_client/stdio_transport.rs && grep -q 'paths::sandbox_state_root_under(' crates/rgr/src/daemon_client/stdio_transport.rs && grep -q 'paths::sandbox_temp_base()' crates/rgr/src/platform/mod.rs && grep -q 'SANDBOX_STATE_DIR' crates/rgr/src/platform/mod.rs && git diff --quiet HEAD -- crates/rgr/src/daemon_client/mod.rs crates/rgr/src/daemon_client/transport.rs crates/rgr/src/daemon_client/socket_transport.rs && test \"$(git grep -l '/private/tmp' -- crates ':!crates/platform-paths' | LC_ALL=C sort | tr '\\n' ' ')\" = 'crates/daemon-runtime/src/reclaim.rs crates/rgr/src/commands/doctor/daemon_info.rs crates/rgr/src/commands/find/seed_render.rs ' && git diff --quiet HEAD -- crates/daemon-runtime/src/reclaim.rs crates/rgr/src/commands/doctor/daemon_info.rs crates/rgr/src/commands/find/seed_render.rs && test \"$(git diff HEAD --name-only --relative -- crates/rgr | LC_ALL=C sort | tr '\\n' ' ')\" = 'crates/rgr/src/cli/paths.rs crates/rgr/src/daemon_client/stdio_transport.rs crates/rgr/src/platform/mod.rs ' && D1=$(dg \"$REG\") && test \"$D1\" = \"$D0\" && rm -rf \"$I\" && test ! -e \"$I\"",
        "cwd": "rust",
        "environment": "candidate tree. ISOLATION (review-0 F-1): the rgr lib suite runs with RMAP_STATE_ROOT=/private/tmp/PORTABLE-TMP-1-rgr-state and RMAP_SOCKET_PATH=<it>/d.sock. The command creates that root and removes it, fail-closed exactly as PT-C02. The pre-existing `stdio_transport_ping` (stdio_transport.rs:394-411) spawns `rmapd --stdio` with no injected root (`spawn(None)`), so the spawned daemon inherits RMAP_STATE_ROOT. The command requires an `rmapd` on PATH (`find_rmapd`, :217-235: a test binary has no sibling `rmapd`), asserts the ping test PASSED, and asserts `<root>/databases` exists afterwards. That directory is created by `RepoRegistry::new()` in the spawned daemon, so it proves the daemon used the isolated root and not the operator's. ONE test is run apart: `cli::paths::tests::daemon_socket_path_returns_some` (cli/paths.rs:57-71) asserts the UNOVERRIDDEN canonical socket path (`…Application Support/repo-graph…` on macOS), so it fails by design under RMAP_SOCKET_PATH. It is skipped in the isolated run and run alone with only RMAP_STATE_ROOT set. It computes the path and may open-and-close a connection to the canonical socket with no request sent (platform-paths socket.rs:66-81). That is the only contact with the operator's socket in this check and is stated as a limitation, not hidden. The rgr lib suite has no other test that resolves the default socket (grep of test modules, 2026-09-27: every other client test uses `DaemonClient::with_socket_path`). The operator registry digest is compared before and after.",
        "inputs": "CLIENT (RG-REQ-011-L08). `StdioTransport::prepare_sandbox_state_root` (stdio_transport.rs:175-212) keeps its `RMAP_STATE_ROOT` precedence (:177-179). Otherwise it returns `Some(create_sandbox_state_root(paths::sandbox_temp_base())?)`, replacing the literal at :183 with `libc::geteuid` at :182. CREATION SEAM (review-0 D-1, operator ruling option A): the new `create_sandbox_state_root(base: &Path) -> Result<PathBuf, DaemonClientError>` holds today's creation logic unchanged (:185-209). The root is `paths::sandbox_state_root_under(base, paths::effective_uid())`. If it is absent, `create_dir_all` runs, then `set_permissions(0o700)`. If it is present, it is returned untouched, mode included, as today. The error texts are `failed to create sandbox state root <root>: <io error>` and `failed to set sandbox root permissions: <io error>`, as today. Abstraction line: `create_sandbox_state_root(base)`. Users: `prepare_sandbox_state_root` (production, passing `paths::sandbox_temp_base()`) and the three creation tests. Force: a test seam, the one parameter that lets creation be driven against a scratch base without touching `/private/tmp/repo-graph-agent/<uid>`. Rejected: testing `prepare_sandbox_state_root` directly, which creates the real per-uid root (the isolation breach review-0 ruled out); and a mockable filesystem trait, which is more structure for one call site. NEW tests (stdio_transport.rs tests, each against a `tempfile::tempdir()` scratch base, never the real root): `sandbox_root_is_created_with_mode_0700_under_the_given_base` (returns `<base>/repo-graph-agent/<euid>`, a directory, `mode & 0o777 == 0o700`); `sandbox_root_creation_is_idempotent_when_the_directory_exists` (a second call returns the same path; a file written inside the root after the first call is still there; a root pre-created with mode 0o755 is returned with 0o755, as today, never re-permissioned); `sandbox_root_creation_failure_is_a_named_error` (the base is a regular FILE, so `create_dir_all` fails whatever the uid; the result is `Err(DaemonClientError::ConnectionFailed(msg))` with `msg` starting `failed to create sandbox state root ` and naming the root path). The test `sandbox_root_path_format` (:421-432), which asserted its own `format!` of the literal, is REPLACED by `client_sandbox_root_is_the_platform_paths_root`: `paths::sandbox_state_root_under(paths::sandbox_temp_base(), paths::effective_uid())` equals `paths::sandbox_state_root()`, and `paths::is_sandbox_local_state_root` holds for it. The module doc (:11) and the comment (:181) name the per-OS root without the literal. DOCTOR: the unreachable-in-practice fallback string at platform/mod.rs:292 (`DaemonClient::state_root_mode` is `SandboxLocal` only when `sandbox_state_root` is `Some` — daemon_client/mod.rs:151-158) renders `format!(\"{}/{}/<uid>\", paths::sandbox_temp_base().display(), paths::SANDBOX_STATE_DIR)` — `/private/tmp/repo-graph-agent/<uid>` on macOS, byte-identical. The fallback trigger is untouched: daemon_client/mod.rs, transport.rs (EPERM/EACCES detection) and socket_transport.rs unchanged, their detection tests bound by name. The out-of-scope literals remain exactly in reclaim.rs:1565/:1581 and doctor/daemon_info.rs:989/:1004 (test data spelling a repo path) and find/seed_render.rs:22 (a doc comment citing a past calibration file) — unchanged. rgr's tracked diff is exactly cli/paths.rs, daemon_client/stdio_transport.rs, platform/mod.rs."
      },
      "expected": "exit 0: the client creates `<base>/repo-graph-agent/<uid>` with mode 0700, reuses an existing one untouched, and names a creation failure. Production passes the platform-paths base, whose root the daemon classifies sandbox-local. doctor's fallback text is unchanged on macOS. The EPERM/EACCES trigger and its refusal of other errors show no behavior change (RG-REQ-011-L08 trigger preserved). No path literal is duplicated in rgr. The whole rgr lib suite ran against an isolated root this run created and removed, the spawned stdio daemon used it, and the operator registry is unchanged."
    },
    {
      "checkId": "PT-C04",
      "obligationIds": [
        "P-PT-04"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "set -o pipefail; RUN=$(date -u +%Y%m%dT%H%M%SZ)-$$; LOGD=../.agent-manager/slices/PORTABLE-TMP-1/logs; { test -d ../.agent-manager/slices/PORTABLE-TMP-1 && test ! -e \"$LOGD/pt-c04-$RUN.txt\"; } || { echo 'PT-C04 PRECONDITION FAILED: nothing written'; exit 1; }; mkdir -p \"$LOGD\" && echo \"log: $LOGD/pt-c04-$RUN.txt\" && cargo fmt --check -p repo-graph-platform-paths -p repo-graph-daemon-runtime -p repo-graph-rgr && cargo clippy -p repo-graph-platform-paths -p repo-graph-daemon-runtime -p repo-graph-rgr --all-targets -- -D warnings > \"$LOGD/pt-c04-$RUN.txt\" 2>&1; rc=$?; tail -3 \"$LOGD/pt-c04-$RUN.txt\"; test $rc -eq 0 && git -C .. diff HEAD --check && ! grep -nE '[[:space:]]+$' crates/platform-paths/src/sandbox.rs ../scripts/lib/sandbox-tmp-base.sh",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "formatter over the three touched crates; clippy over them, all targets, warnings denied; whitespace hygiene of the tracked diff and of the two new (untracked) files, which `git diff HEAD --check` does not see. The clippy output is kept as `.agent-manager/slices/PORTABLE-TMP-1/logs/pt-c04-<RUN>.txt` (rule in §2.6)."
      },
      "expected": "exit 0: no formatting, lint or whitespace debt enters with the candidate"
    },
    {
      "checkId": "PT-C05",
      "obligationIds": [
        "RG-REQ-014-L09",
        "RG-REQ-011-L06",
        "P-PT-01",
        "P-PT-02"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "set -o pipefail; S='validate-layer2-fixture.sh orient-density-nginx-capture.sh dogfood-isolated.sh dv1-inflight-e2e.sh test-install-robustness-2.sh smoke-rmap.sh byte-compare-map-modules-surfaces.sh test-smoke-rmap.sh byte-compare-five-surfaces.sh smoke-validation-repos.sh find-facts-e2e.sh'; SRC='. \"$(cd \"$(dirname \"${BASH_SOURCE[0]}\")\" && pwd)/lib/sandbox-tmp-base.sh\"'; L=scripts/lib/sandbox-tmp-base.sh; P=/private/tmp/PORTABLE-TMP-1-uname-shim; { test \"$(uname -s)\" = Darwin && test ! -e \"$P\"; } || { echo 'PT-C05 PRECONDITION FAILED: nothing created, nothing deleted'; exit 1; }; mkdir \"$P\" || { echo 'PT-C05 could not create its shim directory: nothing deleted'; exit 1; }; trap 'rm -rf \"$P\"' EXIT; bash -n \"$L\" && for s in $S; do bash -n \"scripts/$s\" || { echo \"SYNTAX $s\"; exit 1; }; done && test \"$(git grep --untracked -c '/private/tmp' -- scripts)\" = 'scripts/lib/sandbox-tmp-base.sh:1' && printf '#!/bin/sh\\necho Linux\\n' > \"$P/uname\" && chmod +x \"$P/uname\" && test \"$(bash -c \". $L; printf %s \\\"\\$RG_SANDBOX_TMP_BASE\\\"\")\" = /private/tmp && test \"$(PATH=\"$P:$PATH\" bash -c \". $L; printf %s \\\"\\$RG_SANDBOX_TMP_BASE\\\"\")\" = /tmp && for s in $S; do f=\"scripts/$s\"; test \"$(grep -cxF \"$SRC\" \"$f\")\" = 1 || { echo \"SOURCE LINE $s\"; exit 1; }; awk -v src=\"$SRC\" '$0==src{a=NR} /[$][{]RG_SANDBOX_TMP_BASE[}]/ && !b {b=NR} END{exit !(a && b && a<b)}' \"$f\" || { echo \"ORDER $s\"; exit 1; }; ! grep -nE '[$]RG_SANDBOX_TMP_BASE' \"$f\" || { echo \"UNBRACED $s\"; exit 1; }; diff <(git show \"HEAD:$f\" | grep -vE '^[[:space:]]*(#|$)') <(grep -vxF \"$SRC\" \"$f\" | sed 's#[$][{]RG_SANDBOX_TMP_BASE[}]#/private/tmp#g' | grep -vE '^[[:space:]]*(#|$)') || { echo \"CODE MOVED $s\"; exit 1; }; test \"$(cd scripts && bash -c \"$SRC; printf %s \\\"\\${RG_SANDBOX_TMP_BASE}\\\"\")\" = /private/tmp || { echo \"MAC BASE $s\"; exit 1; }; test \"$(cd scripts && PATH=\"$P:$PATH\" bash -c \"$SRC; printf %s \\\"\\${RG_SANDBOX_TMP_BASE}\\\"\")\" = /tmp || { echo \"LINUX BASE $s\"; exit 1; }; done && git diff --quiet HEAD -- scripts/build-installer.sh scripts/install.template.sh scripts/lib/macos.sh scripts/lib/linux.sh scripts/dist && test \"$(git diff HEAD --name-only -- scripts | LC_ALL=C sort | tr '\\n' ' ')\" = 'scripts/byte-compare-five-surfaces.sh scripts/byte-compare-map-modules-surfaces.sh scripts/dogfood-isolated.sh scripts/dv1-inflight-e2e.sh scripts/find-facts-e2e.sh scripts/orient-density-nginx-capture.sh scripts/smoke-rmap.sh scripts/smoke-validation-repos.sh scripts/test-install-robustness-2.sh scripts/test-smoke-rmap.sh scripts/validate-layer2-fixture.sh ' && test \"$(git ls-files --others --exclude-standard -- scripts)\" = 'scripts/lib/sandbox-tmp-base.sh'",
        "cwd": ".",
        "environment": "candidate tree on this Mac; a BASH command (process substitution) — run it as `bash -c '<command>'`; the builder-made shim directory /private/tmp/PORTABLE-TMP-1-uname-shim is created with plain `mkdir` only after every precondition passed, so the EXIT trap only ever removes the directory this run created",
        "inputs": "NEW `scripts/lib/sandbox-tmp-base.sh` (sourced, not executed; not an installer module — build-installer.sh injects lib/macos.sh and lib/linux.sh by name, :173-182, and those files, the template and scripts/dist are unchanged): one statement `case \"$(uname -s)\" in Darwin) RG_SANDBOX_TMP_BASE=/private/tmp ;; *) RG_SANDBOX_TMP_BASE=/tmp ;; esac` — the rule of D-PTMP-ROOT-1 and the shell twin of `sandbox_temp_base()`, set unconditionally (no environment input; not `$TMPDIR` — option B), plus a header naming its Rust twin. It holds the ONLY `/private/tmp` in `scripts/` (header comments name the base in words). Each of the 11 scripts (34 occurrences at HEAD 2c28ff10: 16 code lines, 18 comment lines — recounted) gains exactly the one line `. \"$(cd \"$(dirname \"${BASH_SOURCE[0]}\")\" && pwd)/lib/sandbox-tmp-base.sh\"` before its first use of the base, and every former `/private/tmp` in code becomes `${RG_SANDBOX_TMP_BASE}` (braced, never `$RG_SANDBOX_TMP_BASE`); comments that named the literal name the sandbox temp base instead (other platform statements in those comments are left as they are). MACOS IDENTITY (P-PT-01): with the source line dropped, `${RG_SANDBOX_TMP_BASE}` read as `/private/tmp`, and comment/blank lines ignored, each script's code equals HEAD's exactly — so on Darwin every script resolves the same paths as today, and its isolated roots keep their SandboxLocal classification. DYNAMIC: on this Mac the base is `/private/tmp`; with a `uname` shim printing `Linux` first on PATH it is `/tmp`; each script's own source line resolves the lib from the `scripts/` directory (under `bash -c`, `${BASH_SOURCE[0]}` is empty, so the line resolves against the cwd, `scripts/`) — the per-script probe of the packet. The tracked scripts diff is exactly the 11 files and the one new untracked file is the lib."
      },
      "expected": "exit 0: every script parses; one per-OS rule in one file; on this Mac every script's code is byte-for-byte today's code once the base is read, and the base is `/private/tmp`; forced to Linux it is `/tmp`; the installer is untouched"
    },
    {
      "checkId": "PT-C06",
      "obligationIds": [
        "RG-REQ-014-L09",
        "P-PT-01"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "set -o pipefail; test \"$(git diff --numstat HEAD -- agent_docs/storage-architecture-v2.md | cut -f1-2)\" = \"$(printf '1\\t1')\" && grep -qF '`/private/tmp/repo-graph-agent/<uid>/` (macOS), `/tmp/repo-graph-agent/<uid>/` (Linux)' agent_docs/storage-architecture-v2.md && git diff --quiet HEAD -- docs/architecture/state-root-lifecycle.md README.md CLAUDE.md docs/testing docs/VISION.md docs/requirements",
        "cwd": ".",
        "environment": "candidate tree",
        "inputs": "The one living document that states the sandbox root, `agent_docs/storage-architecture-v2.md:290` (the State Root Lifecycle table: `| Sandbox | `/private/tmp/repo-graph-agent/<uid>/` | Ephemeral | … |`), changes that one row to name both roots: `` `/private/tmp/repo-graph-agent/<uid>/` (macOS), `/tmp/repo-graph-agent/<uid>/` (Linux) `` — one line changed, nothing else in the file. NOT changed (stated in §2.4): `docs/architecture/state-root-lifecycle.md` (status AUDIT, a dated record of 2026-05 code — durable records are not rewritten), README.md (states no sandbox root), CLAUDE.md and docs/testing (the operator's macOS isolation procedures — correct on macOS, governance/operator-owned), VISION and the requirement files. No Linux validation is claimed anywhere."
      },
      "expected": "exit 0: the agent-facing storage document states the sandbox root per OS; no record, requirement or governance file moves"
    },
    {
      "checkId": "PT-C07",
      "obligationIds": [
        "RG-REQ-011-L06",
        "RG-REQ-011-L08",
        "P-PT-01",
        "P-PT-04"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "set -o pipefail; R=/private/tmp/PORTABLE-TMP-1-cliproof; A=/private/tmp/repo-graph-agent; SB=\"$A/$(id -u)\"; CAND=\"$PWD/rust/target/release/rmap\"; REG=\"$HOME/Library/Application Support/repo-graph/registry.json\"; SBE() { if [ -e \"$SB\" ]; then echo present; else echo absent; fi; }; dg() { local d; d=$(shasum -a 256 \"$1\" | cut -d' ' -f1) || return 1; [[ $d =~ ^[0-9a-f]{64}$ ]] || return 1; printf %s \"$d\"; }; { test \"$(uname -s)\" = Darwin && test \"$(id -u)\" != 0 && test -x \"$CAND\" && test -x \"${CAND}d\" && test -f \"$REG\" && test ! -e \"$R\"; } || { echo 'PT-C07 PRECONDITION FAILED: nothing created, nothing deleted'; exit 1; }; mkdir \"$R\" || { echo 'PT-C07 could not create its root: nothing deleted'; exit 1; }; trap 'rm -rf \"$R\"' EXIT; D0=$(dg \"$REG\") && SB0=$(SBE) && echo \"per-uid sandbox root path before: $SB0\" && mkdir \"$R/state\" \"$R/cwd\" && python3 -c 'import socket, os, sys; p = sys.argv[1]; s = socket.socket(socket.AF_UNIX); s.bind(p); s.close(); os.chmod(p, 0)' \"$R/d.sock\" && ( cd \"$R/cwd\" && env -u RMAP_TRANSPORT RMAP_STATE_ROOT=\"$R/state\" RMAP_SOCKET_PATH=\"$R/d.sock\" RMAP_AUTO_ENRICH=off RMAP_AUTO_REINDEX=off RMAP_AUTO_RETENTION=off \"$CAND\" doctor > \"$R/doctor.out\" 2> \"$R/doctor.err\"; echo \"doctor exit $?\" ) && cat \"$R/doctor.out\" \"$R/doctor.err\" && grep -qF 'note: socket connection denied (sandbox), using stdio transport' \"$R/doctor.err\" && ! grep -qF 'note: using sandbox-local state root:' \"$R/doctor.err\" && grep -qF \"note: running in sandbox-local mode (state root: $R/state)\" \"$R/doctor.err\" && grep -qF \"state_root: override ($R/state)\" \"$R/doctor.out\" && grep -qF 'authority_policy: baselines, aliases, declarations: blocked (sandbox mode)' \"$R/doctor.out\" && test -d \"$R/state/databases\" && SB1=$(SBE) && echo \"per-uid sandbox root path after: $SB1\" && test \"$SB1\" = \"$SB0\" && D1=$(dg \"$REG\") && test \"$D1\" = \"$D0\" && rm -rf \"$R\" && test ! -e \"$R\" && echo 'PT-C07 ok'",
        "cwd": ".",
        "environment": "the candidate's release build (`rust/target/release/rmap` + `rmapd`, built at §5 step 3; `find_rmapd` prefers the executable's sibling, rgr/src/daemon_client/stdio_transport.rs:217-226), on this Mac, not as root. OPERATOR RULING (review-0 D-1, option A; the packet's field criterion is withdrawn): no run without `RMAP_STATE_ROOT`. Every rmap invocation sets RMAP_STATE_ROOT and RMAP_SOCKET_PATH inside the builder-made /private/tmp/PORTABLE-TMP-1-cliproof (CLAUDE.md isolation rule). The transport is AUTOMATIC (`RMAP_TRANSPORT` unset): explicit `RMAP_TRANSPORT=stdio` would skip the fallback path (daemon_client/mod.rs:184-187). RMAP_SOCKET_PATH names a socket file of mode 000, so `SocketTransport::connect`'s existence check passes (socket_transport.rs:75-81) and `connect` fails EACCES (EXECUTED 2026-09-27 with python on this Mac: `exists: True`, `errno 13 EACCES Permission denied`). That is the Codex-sandbox condition, and it drives the automatic fallback (:192-209). Because RMAP_STATE_ROOT is set, `prepare_sandbox_state_root` returns `None` and injects nothing (stdio_transport.rs:177-179). The proof that the per-uid root /private/tmp/repo-graph-agent/<uid> is NOT USED is behavioural. The client prints no injection note, the daemon names `<proof root>/state` as its state root, and that root's `databases/` exists. The check itself only tests whether the per-uid path EXISTS, before and after (`[ -e ]`, one stat of the path; no traversal, no read of its contents), and asserts that the existence state did not change. It never creates, modifies or deletes the path and never reads its contents. An unchanged existence state is not, and is not claimed as, proof that a pre-existing directory's contents were untouched (review-1). FAIL-CLOSED (review-0 F-2): every precondition is evaluated in one group that exits BEFORE anything is created or any trap is installed. The proof root is created with plain `mkdir`, which fails if it exists, so the EXIT trap only ever removes a directory this run created. The root is removed explicitly at the end and its absence asserted. doctor's exit status is printed, not asserted: its socket probes fail against a mode-000 socket.",
        "inputs": "`rmap doctor` from a non-repo cwd with RMAP_AUTO_ENRICH=off RMAP_AUTO_REINDEX=off RMAP_AUTO_RETENTION=off. Asserted:\n- stderr `note: socket connection denied (sandbox), using stdio transport` (daemon_client/mod.rs:197-200). The AUTOMATIC selection fell back on EACCES (RG-REQ-011-L08 end to end).\n- stderr has NO `note: using sandbox-local state root:` (stdio_transport.rs:128). The explicit root was honoured and nothing was injected.\n- stderr `note: running in sandbox-local mode (state root: /private/tmp/PORTABLE-TMP-1-cliproof/state)` (daemon-runtime lib.rs:405-411). The candidate DAEMON's new predicate classified a root under the macOS sandbox base (`/private/tmp`) as sandbox-local (RG-REQ-011-L06, macOS arm, end to end).\n- stdout `state_root: override (/private/tmp/PORTABLE-TMP-1-cliproof/state)` (platform/mod.rs:295-298).\n- stdout `authority_policy: baselines, aliases, declarations: blocked (sandbox mode)` (doctor/daemon_info.rs:795-800). The A1 block, end to end.\n- `<root>/state/databases` exists: the stdio daemon used the isolated root.\n- The per-uid sandbox root path's existence state is unchanged (existence only; see environment). The operator registry digest, read by a fail-closed helper that requires a 64-hex value, is unchanged.\nNOT established here (§3): creation of the real per-uid root end to end, which is covered only by PT-C03's creation unit tests against a scratch base plus PT-C01's composition tests; and any Linux host run."
      },
      "expected": "exit 0: through the automatic transport selection, an EACCES socket makes the candidate CLI fall back to stdio (no behavior change in the trigger, preserved from HEAD); the daemon classifies an isolated root under the macOS sandbox base as sandbox-local and doctor reports authority writes blocked. The explicit root is honoured, nothing is injected and the daemon ran on the isolated root (the per-uid path's existence state unchanged), the operator registry is unchanged, and the proof root this run created is removed."
    },
    {
      "checkId": "PT-C08",
      "obligationIds": [
        "RG-REQ-011-L06",
        "P-PT-04"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "set -o pipefail; dg() { local d; d=$(shasum -a 256 \"$1\" | cut -d' ' -f1) || return 1; [[ $d =~ ^[0-9a-f]{64}$ ]] || return 1; printf %s \"$d\"; }; REG=\"$HOME/Library/Application Support/repo-graph/registry.json\"; BP=.agent-manager/slices/PORTABLE-TMP-1/build-progress.md; RB=$(grep -E '^registry-before [0-9a-f]{64}$' \"$BP\" | tail -1 | cut -d' ' -f2) && [[ $RB =~ ^[0-9a-f]{64}$ ]] && RN=$(dg \"$REG\") && test \"$RN\" = \"$RB\" && ! ls -d /private/tmp/PORTABLE-TMP-1-* >/dev/null 2>&1 && ! ls -d /var/tmp/rg-global-state-* >/dev/null 2>&1 && { PIDS=$(pgrep -x rmapd | paste -sd, -); PRC=$?; [ \"$PRC\" -le 1 ]; } && OTHER=$( if [ -n \"$PIDS\" ]; then ps -o command= -p \"$PIDS\" | RE='/[.]local/bin/rmapd' awk '$0 !~ ENVIRON[\"RE\"] && NF'; fi ) && test -z \"$OTHER\" && ST=$(git status --short --untracked-files=all) && EXTRA=$(printf '%s\\n' \"$ST\" | RE='^( M|M |MM|\\?\\?) (rust/crates/platform-paths/src/(lib|sandbox)\\.rs|rust/crates/rgr/src/(cli/paths|daemon_client/stdio_transport|platform/mod)\\.rs|rust/crates/daemon-runtime/src/(lib|state|handlers/inventory/tests)\\.rs|rust/crates/daemon-runtime/tests/snapshot_retention\\.rs|scripts/lib/sandbox-tmp-base\\.sh|scripts/(validate-layer2-fixture|orient-density-nginx-capture|dogfood-isolated|dv1-inflight-e2e|test-install-robustness-2|smoke-rmap|byte-compare-map-modules-surfaces|test-smoke-rmap|byte-compare-five-surfaces|smoke-validation-repos|find-facts-e2e)\\.sh|agent_docs/storage-architecture-v2\\.md)$' awk '$0 !~ ENVIRON[\"RE\"] && NF') && { test -z \"$EXTRA\" || { printf 'OUTSIDE CANDIDATE PATHS:\\n%s\\n' \"$EXTRA\"; false; }; } && git diff --check",
        "cwd": ".",
        "environment": "candidate tree, after every other check; the admission commit holds this document, the baseline and D-PTMP-ROOT-1 (they are untracked at PREP-1)",
        "inputs": "the full sha256 of the operator registry recorded as `registry-before <digest>` in .agent-manager/slices/PORTABLE-TMP-1/build-progress.md at §5 step 0 equals the current one; no `/private/tmp/PORTABLE-TMP-1-*` directory and no test-owned `/var/tmp/rg-global-state-*` directory remains; no rmapd process other than the operator's installed `~/.local/bin/rmapd`; `git status` names only the 22 candidate paths; no whitespace errors. Fail-closed reads (review-1): the recorded and the current digests must both be 64-hex values. `pgrep`'s status is kept (0 found, 1 none, anything else fails). The `ps` and `git status` outputs are captured whole and filtered by `awk`, which exits 0 whether or not it prints. No `| grep -q` is left, because under `pipefail` its early exit could make an upstream command die of SIGPIPE, and a leading `!` would turn that into a false pass."
      },
      "expected": "exit 0: every proof was isolated, every builder-made directory is removed, and the tree holds only the allocated paths"
    },
    {
      "checkId": "PT-C09",
      "obligationIds": [
        "RG-REQ-011-L03",
        "RG-REQ-011-L09",
        "RG-REQ-011-L01",
        "RG-REQ-011-L02",
        "RG-REQ-011-L04",
        "RG-REQ-011-L05",
        "RG-REQ-011-L07",
        "RG-REQ-011-L10",
        "RG-REQ-011-L11",
        "RG-REQ-014-L01",
        "RG-REQ-014-L02",
        "RG-REQ-014-L03",
        "RG-REQ-014-L04",
        "RG-REQ-014-L05",
        "RG-REQ-014-L06",
        "RG-REQ-014-L07",
        "RG-REQ-014-L08"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "set -o pipefail; W() { f=$1; shift; git diff -U0 HEAD -- \"$f\" | awk -v W=\"$*\" -v F=\"$f\" 'BEGIN{n=split(W,w,\" \")} /^@@ /{split($2,o,\",\"); a=substr(o[1],2)+0; b=(o[2]==\"\"?1:o[2]+0); e=(b==0?a:a+b-1); ok=0; for(i=1;i<=n;i++){split(w[i],r,\"-\"); if(a>=r[1]+0 && e<=r[2]+0) ok=1}; if(!ok){print \"HUNK OUTSIDE ITS WINDOWS: \" F \" \" $2; bad=1}} END{exit bad}'; }; W crates/daemon-runtime/src/state.rs 30-40 45-70 85-95 862-882 1620-1670 && W crates/daemon-runtime/src/lib.rs 170-200 250-292 312-318 386-394 && W crates/rgr/src/platform/mod.rs 1-30 284-296 && W crates/rgr/src/daemon_client/stdio_transport.rs 1-40 168-216 390-433 && W crates/rgr/src/cli/paths.rs 1-15 && W crates/daemon-runtime/tests/snapshot_retention.rs 60-95 170-182 && test \"$(git diff HEAD --name-only --relative -- . | LC_ALL=C sort | tr '\\n' ' ')\" = 'crates/daemon-runtime/src/handlers/inventory/tests.rs crates/daemon-runtime/src/lib.rs crates/daemon-runtime/src/state.rs crates/daemon-runtime/tests/snapshot_retention.rs crates/platform-paths/src/lib.rs crates/rgr/src/cli/paths.rs crates/rgr/src/daemon_client/stdio_transport.rs crates/rgr/src/platform/mod.rs ' && test \"$(git ls-files --others --exclude-standard -- . | tr '\\n' ' ')\" = 'crates/platform-paths/src/sandbox.rs ' && git -C .. diff --quiet HEAD -- scripts/install.template.sh scripts/lib/macos.sh scripts/lib/linux.sh scripts/build-installer.sh scripts/dist .github tools rust/Cargo.toml rust/Cargo.lock",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "The structural no-behavior-change oracle for every L this slice does not implement. (1) Rust scope: the tracked rust diff is exactly the eight files, and the only untracked rust file is `crates/platform-paths/src/sandbox.rs`. Every crate, module and file implementing RG-REQ-011-L01 (daemon-transport, concurrency), L02 (foreground_open, storage locks), L04 (reconcile, the rebuild sentinel), L05 (reclaim, crash recovery), L07 (doc-facts self_generated, index drift), L10 (enrich_pass, retention_pass, doctor probes) and RG-REQ-014-L03..L08 (rgr commands integrate/hook/version, platform macos/linux/manifest, tools/rgistr) is therefore byte-identical. (2) Shared files: in the four touched multi-responsibility files, every diff hunk (`git diff -U0`, old-side line range at HEAD 2c28ff10) lies inside the windows of the code this slice changes, so the rest of each file is byte-identical. state.rs: 30-40 and 85-95 (imports), 45-70 (the `StateRootMode` docs), 862-882 (`state_root_mode`), 1620-1670 (its tests). lib.rs: 170-200 (imports), 250-292 (`clear_stale_sandbox_state`, both cfg arms), 312-318 (`run_daemon`'s doc), 386-394 (`run_daemon_stdio`'s doc). platform/mod.rs: 1-30 and 284-296 (the `state_root` probe). stdio_transport.rs: 1-40 (module doc, imports), 168-216 (`prepare_sandbox_state_root` and the `create_sandbox_state_root` seam inserted after it), 390-433 (tests). cli/paths.rs: 1-15 (re-exports). snapshot_retention.rs: 60-95 (imports) and 170-182 (`isolated()`). `impl StateRootMode` (state.rs:71-84), the A1 guard (lib.rs:227-250), `run_daemon`'s body and the `StdioTransport::spawn` injection (:104-149) are outside every window. (3) Distribution (RG-REQ-014-L01, L02, L06, L07, L08): the installer template, lib/macos.sh, lib/linux.sh, build-installer.sh, scripts/dist, .github, tools, rust/Cargo.toml and Cargo.lock are unchanged (no version, dependency, service or release change). (4) RG-REQ-011-L11 is WITHDRAWN: no latency bound or gate is authored. The behavioural suites behind these Ls are the daemon-runtime and rgr lib suites run whole in PT-C02/PT-C03 and the retention integration suite in PT-C02. The per-L integration suites named in the catalog (concurrency, crash recovery, rebuild, enrich lifecycle) are the operator's `cargo test --workspace` gate after acceptance (CLAUDE.md relay rules)."
      },
      "expected": "exit 0: no behavior change for every preserved L. Concurrency (L01), named Busy (L02), retention (L03, its suite green in PT-C02), rebuild (L04), crash recovery (L05), exhaust exclusion (L07), path authorities (L09), background passes (L10) and the withdrawn L11 remain as at HEAD; install, service start, host integration, hook, hosts, version, release and upgrade (RG-REQ-014-L01..L08) remain as at HEAD. Every changed line lies in the sandbox-root code this slice owns."
    }
  ]
}
-->

# PORTABLE-TMP-1 — the sandbox state root works on Linux; macOS unchanged

Status: SPECIFIED 2026-09-27 by the PREP-1 requirements author on HEAD 2c28ff10. Its rust tree is identical to 15fa3304 (`git diff --stat 15fa3304 HEAD -- rust scripts README.md docs/cli` is empty). Every line cited below was re-read at HEAD. Queued by the human 2026-09-27 ("queue portable tmp 1 now after the current"). Governing decision: D-PTMP-ROOT-1, option A (operator; the human may override). CODE slice: platform-paths (new `sandbox` module), rgr, daemon-runtime, 11 dev/test scripts plus one sourced rule file, and one agent doc. No store, schema, wire or toolchain-version change. No reindex. Builder: claude. Reviewer: codex.

## 0. Requirements allocation (the machine-readable block above is binding; this section explains it)

**Implements** (each L holds on macOS today and not on Linux; the slice makes it hold on Linux and keeps macOS byte-identical, P-PT-01):
- **RG-REQ-011-L08**: "On EPERM/EACCES connecting to the socket, the client shall fall back to running the daemon as a stdio subprocess". On Linux the fallback cannot create its root today: `stdio_transport.rs:183` builds `/private/tmp/repo-graph-agent/<uid>`, and an ordinary user cannot create `/private`. The fallback therefore fails with `failed to create sandbox state root …`. Proof: PT-C01 (the Linux arm and the root composition), PT-C03 (creation through the one-parameter seam against a scratch base: mode 0700, idempotent, named error; the client uses the platform-paths root; the trigger is unchanged), PT-C07 (the automatic fallback on EACCES through the CLI, with an isolated root). Creating the real per-uid root end to end is UNVERIFIED (§3).
- **RG-REQ-011-L06**: "a sandbox root blocks writes to the user-authority class while allowing operational and derived-cache classes". On Linux detection never fires today: `state.rs:876` tests the prefix `/private/tmp/`. Proof: PT-C01 (the predicate on both arms), PT-C02 (detection, the A1 guard, the A2/B proof, all ungated), PT-C07 (an isolated root under `/private/tmp` classified sandbox-local by the candidate daemon; `authority_policy: … blocked (sandbox mode)`), PT-C08 (operator registry digest unchanged).
- **RG-REQ-014-L09**: "Linux with systemd is code-complete, validation pending". For the sandbox path this is false today (D-PTMP-ROOT-1). The slice makes it true in code: the Linux arm compiles and its unit tests run on every host (PT-C01, PT-C02). The scripts carry the same rule (PT-C05) and the agent doc states both roots (PT-C06). **Validation stays pending.** No Linux host run is part of this slice and none is claimed.

**Preserves:**
- **RG-REQ-011-L09**: socket and store locations derive from the OS, not a mutable environment, and `rgr/src/cli/paths.rs` is the CLI's path authority. The new root is a function of the OS name and the effective uid, with no environment input (PT-C01). rgr reaches it through `cli/paths.rs` (PT-C03). home.rs, dirs.rs and socket.rs are unchanged (PT-C01).
- **RG-REQ-011-L03** (retention): its integration suite `daemon-runtime/tests/snapshot_retention.rs` is touched (P-PT-03) and bound in full by name (PT-C02).
- **Every other L of both parents** (RG-REQ-011-L01, L02, L04, L05, L07, L10, L11; RG-REQ-014-L01…L08) is outside this slice's code and is **preserved with a structural no-behavior-change oracle** (PT-C09). The oracle has three parts: the rust diff is exactly the candidate files; every hunk in the four shared files lies inside the windows of the sandbox-root code; and the installer, release and version files are unchanged. The agent-manager runtime requires every reviewed L to be allocated. The table below states why each is untouched.

**Changes:** none. No L's wording or accepted behaviour moves. The Linux classification of roots under `/tmp` is a consequence of the ratified rule, stated below in §2.5.

**Classification of every L of both parents** (from its text):

| L | Class | Reason |
|---|---|---|
| RG-REQ-011-L01 concurrency | preserves (untouched) | transport/epoch code untouched (PT-C09) |
| RG-REQ-011-L02 named Busy | preserves (untouched) | lock layers untouched (PT-C09) |
| RG-REQ-011-L03 retention | preserves | its suite's `isolated()` root moves off the sandbox base (P-PT-03); suite bound in PT-C02 |
| RG-REQ-011-L04 rebuild sentinel | preserves (untouched) | no open path changes (PT-C09) |
| RG-REQ-011-L05 crash recovery / orphans | preserves (untouched) | reclaim scans the registry and databases, not the sandbox root; reclaim.rs:1565/:1581 are test data spelling a repo path (out of scope, PT-C03 pins them unchanged) |
| RG-REQ-011-L06 isolation; sandbox root blocks A1 | **implements** | detection fails on Linux today |
| RG-REQ-011-L07 own exhaust | preserves (untouched) | exhaust predicate untouched (PT-C09); see finding F2: the packet and D-PTMP-ROOT-1 call the stdio fallback "L07"; it is L08 |
| RG-REQ-011-L08 EPERM/EACCES → stdio | **implements** | root creation fails on Linux today |
| RG-REQ-011-L09 paths from the OS | preserves | the new location obeys it |
| RG-REQ-011-L10 background passes | preserves (untouched) | enrich/retention passes and doctor probes untouched (PT-C09) |
| RG-REQ-011-L11 | preserves (untouched) | WITHDRAWN; no bound authored (PT-C09) |
| RG-REQ-014-L01 install | preserves (untouched) | the installer and its libs are unchanged (PT-C05, PT-C09) |
| RG-REQ-014-L02 service start | preserves (untouched) | the socket daemon's startup clearing now targets the platform root; it stays warnings-only and cannot fail startup (lib.rs:265-285); service registration untouched (PT-C09) |
| RG-REQ-014-L03…L08 | preserves (untouched) | host integration, hook, hosts, version, release, upgrade untouched (PT-C09) |
| RG-REQ-014-L09 platforms stated | **implements** | code-complete for this path; validation pending, stated |

**Deviation from the packet's expected reading:** the packet expected L06 and "L07" to be *preserved* on macOS "while making them hold on Linux". Making an L hold where it does not hold today is implementation. So L06 and L08 (not L07, see F2) are in `implements`. macOS byte-identity is carried by P-PT-01. The implementation packet's `IMPLEMENT_OBLIGATION_IDS` carries the allocation's full set, implements ∪ preserves: all 20 Ls. The runtime compares that union with the packet, so the IDs match the manager's placeholder; they were rewritten from this allocation and verified by the validator.

**Preservation obligations:**

| Id | Obligation | Proof |
|---|---|---|
| P-PT-01 | macOS is byte-identical. The fallback root is `/private/tmp/repo-graph-agent/<uid>`, mode 0700 when created. The predicate is today's component-wise `starts_with("/private/tmp")`: `/private/tmp` itself is sandbox-local; `/private/tmpfoo/x` and `/tmp/x` are not. doctor's strings and the startup clearing path are unchanged. Every script resolves today's paths and keeps its SandboxLocal classification. | PT-C01 (macOS arm literals); PT-C02; PT-C03 (doctor fallback text); PT-C05 (code identity after substitution; base = `/private/tmp` here); PT-C06; PT-C07 |
| P-PT-02 | One definition of the rule per language. Rust: platform-paths `sandbox.rs` holds one `"/private/tmp"`, and no switched Rust file keeps the literal. Shell: `scripts/lib/sandbox-tmp-base.sh` holds the only `/private/tmp` in `scripts/`. Neither reads the environment (no `$TMPDIR`: D-PTMP-ROOT-1 option B). The out-of-scope literals stay exactly where they are. | PT-C01; PT-C02; PT-C03; PT-C05 |
| P-PT-03 | A test that needs Global mode chooses its state root explicitly (`/var/tmp`, outside both OS sandbox bases) and asserts Global. It never inherits Global from `tempdir()`, `CARGO_TARGET_TMPDIR`, `DaemonState::new()` or the environment, any of which can land under the base (Linux `/tmp`; an isolated root in `/private/tmp`). The rule is enforced over every `crates/*/tests/*.rs` that sends an A1 method, and applied to the two lib tests that need Global. | PT-C02 |
| P-PT-04 | Isolation, cleanup, hygiene, scope, fail-closed evidence. Every check keeps each pipeline command's exit status (`set -o pipefail`). A digest counts only if it is a 64-hex value. Logs go only to per-run files the check owns (§2.6), never overwritten, and are retained as evidence. Every rmap invocation and every cargo suite that can reach a default state root runs under a named isolated root the check creates. Every check that creates a directory evaluates all preconditions first and exits before creating anything or installing a trap. It creates with plain `mkdir` (fails if present), so its trap only ever removes what it created, and it asserts the removal. No check creates, modifies or deletes the per-uid sandbox root or reads its contents (PT-C07 tests only whether the path exists). The operator registry is unchanged. The tree holds exactly the candidate paths. fmt, clippy and whitespace are clean. | PT-C02; PT-C03; PT-C04; PT-C05; PT-C07; PT-C08 |

## 1. Problem (verified at HEAD 2c28ff10; rust tree = 15fa3304)

One macOS-only literal decides both where the stdio-fallback state root is and whether a state root is sandbox-local. It is copied by hand into four sites:

- `rust/crates/rgr/src/daemon_client/stdio_transport.rs:183`: `        let sandbox_root = PathBuf::from(format!("/private/tmp/repo-graph-agent/{}", uid));` (inside `prepare_sandbox_state_root`, :175-212; it creates the directory with mode 0700 and injects it as `RMAP_STATE_ROOT`). Its test `sandbox_root_path_format` (:421-432) asserts its own `format!` of the same literal.
- `rust/crates/daemon-runtime/src/lib.rs:269`: `    let sandbox_root = PathBuf::from(format!("/private/tmp/repo-graph-agent/{}", uid));` (in `clear_stale_sandbox_state`, which the socket daemon runs at startup, :327). The same literal appears in its docs at :254 and :315, and `run_daemon_stdio`'s doc at :389 states the `/private/tmp/` rule.
- `rust/crates/daemon-runtime/src/state.rs:876`: `        if state_root.starts_with("/private/tmp/") {` (`DaemonState::state_root_mode`, :871-881; docs :63-67, :870-874). This returns `SandboxLocal`, and `require_global_mode_for_authority_write` (lib.rs:227-250) then refuses `mark_baseline`, `unmark_baseline` and `repo_alias`.
- `rust/crates/rgr/src/platform/mod.rs:292`: `                .unwrap_or_else(|| "/private/tmp/repo-graph-agent/<uid>".to_string());` (doctor's `state_root` probe).

Tests: `daemon-runtime/src/state.rs:1641` and `:1653`, and `handlers/inventory/tests.rs:65` spell the literal. `inventory/tests.rs` gates its A2/B index proof and its imports with `#[cfg(target_os = "macos")]` (:20, :26, :29, :322), and says "macOS-only: sandbox detection uses /private/tmp/ which is macOS-specific" (:320). `mark_baseline_blocked_in_sandbox_mode` is *not* gated, so on Linux it fails today: `/private/tmp/repo-graph-agent/501` is sandbox-local only on macOS (INFERRED from source; no Linux run).

`git grep /private/tmp -- rust` at HEAD lists exactly these sites plus three out-of-scope files. Two are test data spelling a repo path: `daemon-runtime/src/reclaim.rs:1565/:1581` and `rgr/src/commands/doctor/daemon_info.rs:989/:1004`. The third is a doc comment citing a past calibration file: `rgr/src/commands/find/seed_render.rs:22`. Nothing else in `rust/` depends on the literal. The 11 scripts carry 34 occurrences (16 in code, 18 in comments; recounted). They pick their isolated roots under `/private/tmp`, and several comments state the consequence ("under /private/tmp → SandboxLocal").

Never worked on Linux. The literal predates every Linux-facing slice.

**Code-under-analysis note (CLAUDE.md).** This slice changes no answer rmap gives about another repository's source: no extractor, resolver or renderer of analyzed code moves. Its outward effect is the product's own fallback runtime. The report therefore quotes rmap's own lines, above, and the doctor rendering of PT-C07, in place of an analyzed-repository witness.

## 2. Contract

### 2.1 platform-paths gains the rule (PT-C01)
New module `sandbox.rs` (declared and re-exported by `lib.rs`; its platform table gains a Sandbox row):
- `SANDBOX_STATE_DIR = "repo-graph-agent"`.
- `sandbox_temp_base_for_os(os) -> &'static Path`: `"macos"` → `/private/tmp`, anything else → `/tmp`.
- `sandbox_state_root_under(base, uid)`: base/`repo-graph-agent`/`<uid>`, the one composition of the root.
- `sandbox_state_root_for(os, uid)`: `sandbox_state_root_under(sandbox_temp_base_for_os(os), uid)`.
- `is_sandbox_local_state_root_for_os(os, root)`: `root.starts_with(base)`, component-wise, the same test state.rs:876 performs today.
- Host wrappers `sandbox_temp_base()`, `sandbox_state_root()` and `is_sandbox_local_state_root(&Path)`, which pass `std::env::consts::OS` and `effective_uid()`.
- No environment input and no `cfg(target_os)`.

**Seam (one line):** WHAT, the OS name is an argument of pure functions. USERS, the three host wrappers and, through them, rgr's fallback and doctor, the daemon's clearing and detection, and the daemon tests. AXIS, the host OS (macOS vs every other Unix). REJECTED, a `#[cfg(target_os)]` constant: it compiles one arm per host, so this Mac's gate would never run the Linux arm.

**Abstraction line:** the `sandbox` module. Its concrete users today are four call sites in two crates. Its force is the one-definition obligation shared by client and daemon (D-PTMP-ROOT-1 option A; `lib.rs`'s "single source of truth for path resolution across both the CLI and daemon"). The simpler alternative, a fifth copy of a per-OS literal at each site, is rejected: it is the defect being fixed, and option B's failure mode (client and daemon disagreeing on the root) is exactly what one definition prevents. No new crate, dependency or dependency edge: rgr and daemon-runtime already depend on platform-paths (`rgr/Cargo.toml:176`, `daemon-runtime/Cargo.toml:89`).

The predicate is deliberately broader than the injected root. Any root under the base is sandbox-local, as any root under `/private/tmp` is today. The name says `sandbox_local`, not "the sandbox root", so it does not claim to recognize only the fallback root.

### 2.2 Callers switched (PT-C02, PT-C03)
- **rgr.** `prepare_sandbox_state_root` keeps its `RMAP_STATE_ROOT` precedence and otherwise calls `create_sandbox_state_root(paths::sandbox_temp_base())`. That function holds today's creation logic unchanged: 0700 when created, an existing directory returned untouched, the same error texts. Its root is `paths::sandbox_state_root_under(base, effective_uid())`. **Abstraction line (creation seam):** `create_sandbox_state_root(base)`. Users: `prepare_sandbox_state_root` (production passes the platform-paths base) and three creation tests (scratch base). Force: the one parameter that lets creation be tested without touching the real per-uid root (review-0 D-1, option A). Rejected: testing `prepare_sandbox_state_root` directly (it creates the real root) and a filesystem trait (more structure for one call site). doctor's fallback string composes `sandbox_temp_base()` and `SANDBOX_STATE_DIR`: on macOS the rendered bytes are the same, and the branch is unreachable in practice (`DaemonClient::state_root_mode`, daemon_client/mod.rs:151-158). `cli/paths.rs` re-exports the new names. The test `sandbox_root_path_format` is replaced by `client_sandbox_root_is_the_platform_paths_root`. Three creation tests are added: `sandbox_root_is_created_with_mode_0700_under_the_given_base`, `sandbox_root_creation_is_idempotent_when_the_directory_exists` and `sandbox_root_creation_failure_is_a_named_error`. The fallback trigger is not touched: `daemon_client/mod.rs`, `transport.rs` and `socket_transport.rs` are unchanged.
- **daemon-runtime.**
  - `state_root_mode` calls `is_sandbox_local_state_root`.
  - `clear_stale_sandbox_state` clears `sandbox_state_root()`.
  - Doc comments name the per-OS base without the literal.
  - `inventory/tests.rs` loses its four macOS gates. The A2/B proof roots at `sandbox_temp_base()/repo-graph-test-<nanos>`, and nothing in it is macOS-specific.
  - state.rs tests: one is renamed (`…_for_private_tmp` → `…_for_the_platform_sandbox_root`), one is added (`…_for_any_root_under_the_sandbox_temp_base`), and the Global control uses the fixed root `/var/tmp/rg-global-state-control` and asserts it is not sandbox-local.
  - `mark_baseline_allowed_in_global_mode` stops relying on `DaemonState::new()` being Global. It uses the same control root and asserts Global.

### 2.3 Scripts (PT-C05)
`scripts/lib/sandbox-tmp-base.sh` is one `case "$(uname -s)"` statement that sets `RG_SANDBOX_TMP_BASE` (Darwin → `/private/tmp`, else → `/tmp`). It is the shell twin of `sandbox_temp_base()`. It is not `$TMPDIR` (D-PTMP-ROOT-1 option B) and it has no environment input. Each of the 11 scripts sources it with one identical line and reads `${RG_SANDBOX_TMP_BASE}` where it wrote `/private/tmp`. **Smallest shared expression:** one sourced file rather than 11 inline copies, because 11 copies are 11 definitions and the one-literal guard could not tell them apart from a leftover hard-coded path. The file lives beside the installer modules but is not one: `build-installer.sh` injects `lib/macos.sh` and `lib/linux.sh` by name (:173-182). PT-C05 proves the macOS identity mechanically: with the variable read as `/private/tmp`, each script's code is HEAD's code.

### 2.4 Documentation (PT-C06)
`agent_docs/storage-architecture-v2.md:290` is the one living document that states the sandbox root. Its row names both roots. Not edited:
- `docs/architecture/state-root-lifecycle.md`: status AUDIT, a dated record quoting 2026-05 code. Durable records are not rewritten; a later audit can supersede it.
- CLAUDE.md, `docs/testing/end-of-slice-procedure.md` and `docs/testing/rmap-test-protocol.md`: the operator's macOS isolation procedures, correct on macOS and operator/governance-owned (§8).
- README.md: it states no sandbox root.

### 2.5 The Linux consequence of the ratified predicate (finding F3; decided and recorded, P-PT-03)
Under option A the Linux predicate is "under `/tmp`". On Linux, `tempfile::tempdir()` and `std::env::temp_dir()` live under `/tmp` when `$TMPDIR` is unset. Every test whose state root comes from `tempdir()` therefore becomes sandbox-local on Linux, and any such test that performs an A1 write expecting success would be refused. Measured at HEAD by `git grep` over `crates/*/tests/*.rs` and the daemon-runtime lib tests:
- `daemon-runtime/tests/snapshot_retention.rs` is the only test file sending an A1 method. Through `isolated()` (:172-181), six of its tests would be refused on Linux: `default_mark_is_a_stamp_that_narrows_only_after_leaving_the_serving_pair`, `retain_rows_mark_keeps_rows_and_a_default_remark_never_downgrades_it`, `mark_baseline_rejects_non_boolean_retain_rows`, `comparative_assess_works_against_a_narrowed_stamp_baseline`, `mark_baseline_cost_read_failure_precedes_the_mark_and_leaves_class_unchanged` and `retain_rows_on_a_known_empty_snapshot_is_allowed_and_labeled`.
- `state_root_mode_global_for_normal_paths` would also fail on Linux.

**Rule (closes the class; revised for review-0 F-3):** a test that needs Global mode chooses its state root explicitly and asserts it is Global. It never infers Global from `tempdir()`, `CARGO_TARGET_TMPDIR`, `DaemonState::new()` or the environment; a Linux checkout or Cargo target under `/tmp`, or an isolated `RMAP_STATE_ROOT` under `/private/tmp`, defeats each of them. The chosen root is `/var/tmp`, which is writable on macOS and Linux and under neither sandbox base:
- `isolated()` takes `tempdir_in("/var/tmp")` with prefix `rg-global-state-` and asserts `!is_sandbox_mode()`;
- the state.rs control and `mark_baseline_allowed_in_global_mode` use the fixed `/var/tmp/rg-global-state-control` through `with_test_state_root` (no filesystem access) and assert Global.

PT-C02 enforces the rule over every A1-sending test file and forbids `CARGO_TARGET_TMPDIR` in daemon-runtime. A host whose `/var/tmp` is missing or unwritable fails loudly by the helper's assertion rather than running those tests in sandbox mode.

The same consequence applies to people. A Linux user who sets `RMAP_STATE_ROOT=/tmp/…` gets sandbox-local mode, as a macOS user with `/private/tmp/…` does today. The two OSes are symmetric, and it follows from the ratified rule.

This is recorded, not escalated. It is a local mechanism (a test root), cheap to unwind, and it does not change the ratified predicate. If the human prefers the narrower Linux predicate (only `/tmp/repo-graph-agent/`), that is an override of D-PTMP-ROOT-1. It would make Linux asymmetric with macOS, whose script roots are sandbox-local by the Consequences clause.

### 2.6 Local calls recorded (CLAUDE.md "decide and record")
The `_for_os` functions' visibility, `create_sandbox_state_root`'s visibility (crate-private suffices) and the header wording of the shell file are the builder's. **Isolation of the mandatory suites (review-0 F-1, applied to its class):** the daemon-runtime lib and snapshot_retention suites and the rgr lib suite run under named isolated `RMAP_STATE_ROOT`/`RMAP_SOCKET_PATH` roots, created and removed by their checks. The daemon-runtime lib tests build `DaemonState::new()` on the default root, and rgr's `stdio_transport_ping` spawns a daemon with no injected root. The single exception is `cli::paths::tests::daemon_socket_path_returns_some`, which asserts the unoverridden canonical socket path by design. It runs alone with only `RMAP_STATE_ROOT` set, and may open-and-close a connection to the canonical socket with no request sent (platform-paths socket.rs:66-81); the same holds for platform-paths' own default-path test in PT-C01. Stated, not hidden. state.rs (1732 lines) changes one method body and its tests, with no new responsibility. `index_allowed_in_sandbox_mode_proves_a2_and_b_writes` keeps its name; its behaviour is unchanged apart from the base.

**Fail-closed evidence (review-1; one rule per class):**
- *A check writes a path it does not own.* Every log goes to `.agent-manager/slices/PORTABLE-TMP-1/logs/<check>-<RUN>.txt`, with `RUN` = UTC timestamp plus PID. The check asserts the file is absent before writing and never truncates an existing file. The file is kept as evidence. The PT-C01 to PT-C04 logs previously went to fixed `/tmp/pt-c*.txt` paths; no check now writes a log under `/tmp`.
- *A filter's success masks a failed producer.* Every check command starts with `set -o pipefail`, so `cargo test … | tee … | grep …` fails when cargo fails, even after printing an `ok` summary. Digests go through `dg()`, which fails on a failed `shasum` or on anything but a 64-hex value; the registry comparisons in PT-C02, C03, C07 and C08 use it. PT-C08 captures command output whole and filters it with `awk` rather than `| grep -q`, whose early exit can kill the producer with SIGPIPE, which a leading `!` would read as a pass.
- *A check claims to leave a path unread while inspecting it.* PT-C07 no longer traverses the per-uid root. It tests only existence (`[ -e ]`) and says so. The proof that the root is not used is behavioural: no injection note, the daemon names the isolated root, and that root's `databases/` exists.

### 2.7 Evidence taxonomy

| Input | Outcome | Bound test / check |
|---|---|---|
| `("macos", 501)` / `("linux", 1000)` | `/private/tmp/repo-graph-agent/501` / `/tmp/repo-graph-agent/1000` | PT-C01 `macos_sandbox_state_root_is_private_tmp_repo_graph_agent_uid`, `linux_sandbox_state_root_is_tmp_repo_graph_agent_uid` |
| `/private/tmp/…` on macOS; `/tmp/…` on Linux | sandbox-local | PT-C01 `private_tmp_root_on_macos_is_sandbox_local`, `tmp_root_on_linux_is_sandbox_local` |
| `/private/tmp/…` on Linux | NOT sandbox-local | PT-C01 `private_tmp_root_on_linux_is_not_sandbox_local` |
| `~/Library/Application Support/repo-graph`, `~/.local/share/rmap`, `/var/tmp/x` | NOT sandbox-local, either OS | PT-C01 `non_temp_root_is_not_sandbox_local_on_either_os` |
| `/private/tmp`, `/private/tmpfoo/x`, `/tmp/x` on macOS | true, false, false (today's semantics) | PT-C01 `macos_predicate_is_component_wise_as_before` |
| the root the client creates | classified sandbox-local by the daemon's predicate, both arms | PT-C01 `sandbox_state_root_is_sandbox_local_on_both_os_arms`; PT-C03 `client_sandbox_root_is_the_platform_paths_root` |
| `("/scratch/base", 7)` | `/scratch/base/repo-graph-agent/7` | PT-C01 `sandbox_state_root_under_joins_the_agent_dir_and_uid` |
| creation under a scratch base: absent / present (0755, with a file inside) / base is a file | created 0700 / returned untouched / `failed to create sandbox state root <root>: …` | PT-C03 `sandbox_root_is_created_with_mode_0700_under_the_given_base`, `sandbox_root_creation_is_idempotent_when_the_directory_exists`, `sandbox_root_creation_failure_is_a_named_error` |
| daemon with the platform root / a root under the base / `/var/tmp/rg-global-state-control` | SandboxLocal / SandboxLocal / Global (asserted) | PT-C02 `state_root_mode_sandbox_for_the_platform_sandbox_root`, `…_for_any_root_under_the_sandbox_temp_base`, `state_root_mode_global_for_normal_paths` |
| A1 write in sandbox mode; index in sandbox mode | refused; allowed (A2 + B), on every OS | PT-C02 inventory tests (ungated) |
| each script on Darwin / under a `Linux` uname shim | base `/private/tmp`, code identical to HEAD / base `/tmp` | PT-C05 |
| candidate `rmap doctor`, automatic transport, EACCES socket, isolated `RMAP_STATE_ROOT` under `/private/tmp` | stdio fallback; no root injected; the daemon classifies the isolated root sandbox-local; A1 blocked; the per-uid path's existence state unchanged; registry unchanged | PT-C07 |

## 3. Regression watch

| Preserved | What would regress | Proof |
|---|---|---|
| P-PT-01 macOS | a different root, mode, predicate set, doctor string or script path on macOS | PT-C01 literals; PT-C03; PT-C05 code identity; PT-C07 |
| RG-REQ-011-L08 trigger | fallback on ECONNREFUSED; no fallback on EACCES/EPERM | PT-C03 (transport detection tests; transport/socket/client files unchanged); PT-C07 (EACCES → stdio) |
| RG-REQ-011-L06 | A1 allowed in a sandbox root; A2/B refused; detection by anything but the shared predicate | PT-C02; PT-C07 |
| RG-REQ-011-L09 | an env-derived location; socket/home/dirs moved; rgr bypassing `cli/paths.rs` | PT-C01; PT-C03 |
| RG-REQ-011-L03, P-PT-03 | retention or baseline tests refused on Linux; suite red | PT-C02 |
| installer (RG-REQ-014-L01/L02) | a script edit reaching the bundled installer | PT-C05 (template, lib/macos.sh, lib/linux.sh, build-installer.sh, dist unchanged) |
| every other L (PT-C09) | an edit outside the sandbox-root code in a shared file; an installer, version or release change | PT-C09 |
| P-PT-04 | a builder root left; a trap deleting a directory the run did not create; operator registry touched; the per-uid sandbox root touched | PT-C02, PT-C03, PT-C05, PT-C07 (fail-closed preconditions, plain `mkdir`, asserted removal); PT-C08 |

**UNVERIFIED, and not claimed by this slice (operator ruling on review-0 D-1, option A):**
- the creation of the REAL per-uid root `/private/tmp/repo-graph-agent/<uid>` by the CLI end to end. What stands in for it: PT-C03's creation tests drive the same function against a scratch base, PT-C01 pins the base and composition per OS, and PT-C07 proves the automatic fallback and the daemon's classification with an explicit isolated root. A proof under a dedicated disposable OS identity (review-0 option B) would close it and needs account authority;
- any Linux host run, of the suites, the scripts or the CLI. RG-REQ-014-L09's validation stays pending (LINUX-HOST-RUN-1).

## 4. Stop conditions
Frozen:
- the fallback trigger (`daemon_client/mod.rs`, `transport.rs`, `socket_transport.rs`);
- the A1 guard and its message (lib.rs:227-250);
- the `StateRootMode` enums and their strings;
- platform-paths' home/dirs/socket modules and every crate's `Cargo.toml`;
- the installer;
- every out-of-scope literal.

A moved macOS path, mode, string or classification is a STOP. No `$TMPDIR` and no new environment variable (D-PTMP-ROOT-1 options B and C were rejected). Nothing outside the 22 candidate paths; every hunk of the four shared files stays inside its PT-C09 window. If a bound test's behaviour changes beyond what §2 states, report the conflict; do not keep a name that no longer describes the test. Do not commit. Run every check to completion in the foreground. NEVER run `cargo test --workspace` or the dogfood; those are the operator's gate. Delete every `/private/tmp/PORTABLE-TMP-1-*` you create. Never create, modify or delete `/private/tmp/repo-graph-agent/<uid>`, and never read its contents. Every invocation that can access a state root runs with an isolated `RMAP_STATE_ROOT`: every `rmap`/`rmapd` run (PT-C07), and every cargo suite whose tests can construct a default state root or spawn a daemon (PT-C02, PT-C03). The checks that run no such code set none: PT-C01's platform-paths tests only compute paths, and PT-C04, C05, C06 and C09 run no product code that touches a state root. PT-C02, C03, C07 and C08 read the operator's `registry.json` only to hash it.

## 5. Validation (ordered; `.agent-manager/slices/PORTABLE-TMP-1/build-progress.md` after each step)
Every check command is a bash command: run it as `bash -c '<command>'`.
0. On the clean tree, record `git rev-parse HEAD` and `registry-before <full sha256 of ~/Library/Application Support/repo-graph/registry.json>`.
1. Write the failing tests first (§2.7 names). Then implement.
2. Run PT-C01 → PT-C02 → PT-C03 → PT-C04 → PT-C09 → PT-C05 → PT-C06.
3. Run `(cd rust && cargo build --release --bin rmap --bin rmapd)` to build the candidate, then PT-C07.
4. Run PT-C08, then hand off each check by id with its log line.

## 6. Definition of done
All nine checks pass. The report quotes:
- the five switched literals (§1) and the new call at each site;
- the PT-C01 test lines for both arms;
- one script's before/after line (e.g. `scripts/dogfood-isolated.sh:93`), its Darwin base and its shimmed base;
- the three creation-test lines of PT-C03, and `<root>/databases` created by the spawned stdio daemon under PT-C03's isolated root;
- PT-C07's stderr notes (fallback; sandbox-local mode for the isolated root) and the doctor `state_root: override (…)` and `authority_policy` lines;
- the per-uid sandbox root path's existence state before and after PT-C07 (unchanged; existence only);
- the log file and line that prove each cargo check (`.agent-manager/slices/PORTABLE-TMP-1/logs/pt-c0N-<RUN>.txt`);
- the registry digest before and after.

## 7. Roots
- Builder-made, created (plain `mkdir`, after all preconditions) and removed by their checks: `/private/tmp/PORTABLE-TMP-1-daemon-runtime-state` (PT-C02), `/private/tmp/PORTABLE-TMP-1-rgr-state` (PT-C03), `/private/tmp/PORTABLE-TMP-1-uname-shim` (PT-C05), `/private/tmp/PORTABLE-TMP-1-cliproof` (PT-C07).
- Test-owned: `/var/tmp/rg-global-state-*` (snapshot_retention's Global-mode roots, removed on drop; PT-C02 and PT-C08 assert none remain); `<sandbox temp base>/repo-graph-test-<nanos>` (the inventory A2/B test creates and removes its own); scratch bases of the creation tests (`tempfile::tempdir()`).
- Product-fixed: no check creates, modifies or deletes `/private/tmp/repo-graph-agent/<uid>` or reads its contents. PT-C07 tests whether the path exists (one `stat` of the path) and nothing more.
- Logs: `.agent-manager/slices/PORTABLE-TMP-1/logs/pt-c0N-<RUN>.txt` (gitignored relay directory, excluded from candidates). Each name carries a UTC timestamp and the shell's PID, is checked absent before the run, is never truncated or overwritten, and is RETAINED as the evidence the hand-off cites. No check writes a log or any other fixed path under `/tmp`. The directories checks create are listed above (on macOS `/tmp` is `/private/tmp`, where the builder-made roots live).
- The operator registry is read only for its digest. Retained roots are never served.

## 8. Follow-ups (not this slice)
- **LINUX-HOST-RUN-1**: a real Linux run of the three crates' suites and the dogfood, which is RG-REQ-014-L09's pending validation. Includes the Linux baseline of `mark_baseline_blocked_in_sandbox_mode`, which fails there today.
- **SANDBOX-ROOT-OWNERSHIP-1**: `/tmp` and `/private/tmp` are world-writable. Another local user could pre-create `repo-graph-agent/<uid>`, and the fallback uses an existing directory without checking its owner or mode (stdio_transport.rs:186). The hazard exists on macOS today; the slice extends the same behaviour to Linux multi-user hosts. A fix changes macOS behaviour (a refusal), so it needs its own decision.
- **SANDBOX-ROOT-IDENTITY-PROOF-1**: the end-to-end proof of the real per-uid root under a dedicated disposable OS identity (review-0 option B). It needs account authority.
- Operator/governance text: CLAUDE.md and `docs/testing/*` state `/private/tmp` for isolated roots. They are correct on macOS; a Linux operator needs the per-OS wording. D-PTMP-ROOT-1 and the PREP-1 packet cite RG-REQ-011-L07 for the stdio fallback; it is L08 (F2). Records are append-only, so the correction belongs to the operator.

## 9. Baseline history
- **INPUT-1 (2026-09-27, PREP-1).** First admission baseline, pre-written by the manager and pinned here unchanged except for the allocation digest. The author verified all 14 dependency digests against disk before editing, and found nothing wrong in the manifest's requirement or dependency set. Findings raised against the packet, not the requirements:
  - **F1:** the packet's field proof (`RMAP_TRANSPORT=stdio`, no `RMAP_STATE_ROOT`) never injects the sandbox root. It would serve the operator's real state root, a CLAUDE.md isolation breach. PT-C07 uses the auto transport against a mode-000 socket instead; the mechanism was EXECUTED with python.
  - **F2:** the fallback is RG-REQ-011-L08, not L07.
  - **F3:** the Linux `/tmp` consequence for temp-dir test roots, §2.5.

  Rule applied to F1's class (a proof that assumes a code path without reading it): every check names the source line of the path it exercises.

- **INPUT-1, revision 1 (2026-09-27, PREP-1 cycle 2; review-0 `decision-required`).** Operator ruling on D-1: option A. The packet's field criterion is withdrawn and the run without `RMAP_STATE_ROOT` removed.
  - PT-C07 becomes an automatic-transport CLI proof with an explicitly isolated root.
  - Root CREATION gains a one-parameter seam (`create_sandbox_state_root(base)`) and three unit tests against a scratch base; platform-paths gains `sandbox_state_root_under(base, uid)` as the one composition.
  - Creation of the real per-uid root end to end and any Linux run are stated UNVERIFIED (§3).
  - Findings closed as classes, one rule each:
    - **F-1** (a mandatory suite that can reach the default state root): every such cargo suite runs under a named isolated `RMAP_STATE_ROOT`/`RMAP_SOCKET_PATH`. That covers the rgr lib suite (PT-C03) and the daemon-runtime lib and retention suites (PT-C02, which build `DaemonState::new()`). `cli::paths::tests::daemon_socket_path_returns_some`, which asserts the unoverridden socket path, runs apart, with its canonical-socket reachability probe stated.
    - **F-2** (cleanup reachable after a failed precondition): every check that creates a directory evaluates all preconditions in one group that exits first, creates with plain `mkdir`, installs its trap only after that, and asserts removal (PT-C02, PT-C03, PT-C05, PT-C07).
    - **F-3** (Global mode inferred from where a temp directory happens to be): a test that needs Global chooses `/var/tmp` explicitly and asserts Global. `CARGO_TARGET_TMPDIR` is forbidden. The class also covers `mark_baseline_allowed_in_global_mode`, which relied on `DaemonState::new()` being Global.
  - Unchanged: the 22 candidate paths, the obligation classification and the manifest (apart from the allocation digest).

- **INPUT-1, revision 2 (2026-09-27, PREP-1 cycle 3; review-1 `refinement-required`, no decision).** Three defect classes closed, each with one rule (§2.6):
  - logs moved from fixed `/tmp/pt-c*.txt` to per-run, absent-checked, retained files in the relay directory;
  - `set -o pipefail` on all nine checks, the 64-hex digest helper, and PT-C08's `| grep -q` filters replaced by captured output plus `awk`;
  - PT-C07's per-uid traversal replaced by an existence test, stated truthfully.
  - Unchanged: obligation classification, candidate paths and checks' purposes.
- **INPUT-1, revision 3 (2026-09-27, PREP-1 cycle 4; review-2 `refinement-required`, text only).** Absolute claims about what checks run, read or write are narrowed to what they do (§4, §7, P-PT-04, §2.6, §0, PT-C07's environment):
  - the isolated-root rule covers invocations that can access a state root;
  - the per-uid directory is existence-tested and its contents never read;
  - no log under `/tmp`;
  - the registry is shown unchanged by digest, not "untouched".

  No check command changed.
