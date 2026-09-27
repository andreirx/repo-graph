//! Sandbox state root: where the stdio fallback keeps its state, and which
//! state roots the daemon classifies as sandbox-local.
//!
//! When a client cannot connect to the daemon socket (EPERM/EACCES, e.g. an
//! agent sandbox), it runs `rmapd --stdio` with a sandbox-writable state root.
//! The client creates that root, the socket daemon clears it at startup, and
//! the daemon blocks user-authority (A1) writes for any state root under the
//! sandbox temp base. All of them use the functions below, so the client and
//! the daemon agree on one directory (D-PTMP-ROOT-1, option A).
//!
//! # Rule
//!
//! | OS | Sandbox temp base | Sandbox state root |
//! |----|-------------------|--------------------|
//! | macOS | `/private/tmp` | `/private/tmp/repo-graph-agent/<uid>` |
//! | every other OS | `/tmp` | `/tmp/repo-graph-agent/<uid>` |
//!
//! The base is a fixed directory per OS, never the session's temp-directory
//! environment variable: a launchd/systemd daemon and an agent shell can see
//! different values, and the daemon would then clear and classify a different
//! directory from the one the client created. This follows the crate's
//! principle that paths shared by client and daemon do not come from session
//! environment.
//!
//! The OS name is an argument of the `_for_os` functions so that every host
//! runs both arms in its unit tests; the host wrappers pass
//! `std::env::consts::OS`.

use std::path::{Path, PathBuf};

use crate::home::effective_uid;

/// Directory under the sandbox temp base that holds the per-uid sandbox state roots.
pub const SANDBOX_STATE_DIR: &str = "repo-graph-agent";

/// Sandbox temp base for the OS named `os` (a `std::env::consts::OS` value):
/// `/private/tmp` for `"macos"`, `/tmp` for any other value.
pub(crate) fn sandbox_temp_base_for_os(os: &str) -> &'static Path {
    match os {
        "macos" => Path::new("/private/tmp"),
        _ => Path::new("/tmp"),
    }
}

/// The sandbox state root for `uid` under `base`: `<base>/repo-graph-agent/<uid>`.
///
/// This is the one composition of the root. Production passes
/// [`sandbox_temp_base`]; tests pass a scratch base.
pub fn sandbox_state_root_under(base: &Path, uid: u32) -> PathBuf {
    base.join(SANDBOX_STATE_DIR).join(uid.to_string())
}

/// The sandbox state root for `uid` on the OS named `os`.
pub(crate) fn sandbox_state_root_for(os: &str, uid: u32) -> PathBuf {
    sandbox_state_root_under(sandbox_temp_base_for_os(os), uid)
}

/// True iff `state_root` lies under the sandbox temp base of the OS named `os`.
///
/// Component-wise (`Path::starts_with`): on macOS `/private/tmp` itself is
/// sandbox-local, `/private/tmpfoo/x` and `/tmp/x` are not. Every root under the
/// base is sandbox-local, not only the per-uid root the client creates.
pub(crate) fn is_sandbox_local_state_root_for_os(os: &str, state_root: &Path) -> bool {
    state_root.starts_with(sandbox_temp_base_for_os(os))
}

/// Sandbox temp base of the host OS (`/private/tmp` on macOS, `/tmp` elsewhere).
pub fn sandbox_temp_base() -> &'static Path {
    sandbox_temp_base_for_os(std::env::consts::OS)
}

/// Sandbox state root of the host OS for the effective uid
/// (`/private/tmp/repo-graph-agent/<uid>` on macOS, `/tmp/repo-graph-agent/<uid>` elsewhere).
///
/// Computes the path only; it does not create it.
pub fn sandbox_state_root() -> PathBuf {
    sandbox_state_root_for(std::env::consts::OS, effective_uid())
}

/// True iff `state_root` lies under the host OS's sandbox temp base.
pub fn is_sandbox_local_state_root(state_root: &Path) -> bool {
    is_sandbox_local_state_root_for_os(std::env::consts::OS, state_root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_sandbox_temp_base_is_private_tmp() {
        assert_eq!(
            sandbox_temp_base_for_os("macos"),
            Path::new("/private").join("tmp")
        );
    }

    #[test]
    fn every_other_os_sandbox_temp_base_is_tmp() {
        for os in ["linux", "freebsd", ""] {
            assert_eq!(sandbox_temp_base_for_os(os), Path::new("/tmp"), "os={os:?}");
        }
    }

    #[test]
    fn macos_sandbox_state_root_is_private_tmp_repo_graph_agent_uid() {
        assert_eq!(
            sandbox_state_root_for("macos", 501),
            PathBuf::from("/private/tmp/repo-graph-agent/501")
        );
    }

    #[test]
    fn linux_sandbox_state_root_is_tmp_repo_graph_agent_uid() {
        assert_eq!(
            sandbox_state_root_for("linux", 1000),
            PathBuf::from("/tmp/repo-graph-agent/1000")
        );
    }

    #[test]
    fn sandbox_state_root_under_joins_the_agent_dir_and_uid() {
        assert_eq!(
            sandbox_state_root_under(Path::new("/scratch/base"), 7),
            PathBuf::from("/scratch/base/repo-graph-agent/7")
        );
    }

    #[test]
    fn private_tmp_root_on_macos_is_sandbox_local() {
        for root in [
            "/private/tmp/repo-graph-agent/501",
            "/private/tmp/repo-graph-dogfood/run",
        ] {
            assert!(
                is_sandbox_local_state_root_for_os("macos", Path::new(root)),
                "{root}"
            );
        }
    }

    #[test]
    fn tmp_root_on_linux_is_sandbox_local() {
        for root in ["/tmp/repo-graph-agent/1000", "/tmp/x"] {
            assert!(
                is_sandbox_local_state_root_for_os("linux", Path::new(root)),
                "{root}"
            );
        }
    }

    #[test]
    fn private_tmp_root_on_linux_is_not_sandbox_local() {
        assert!(!is_sandbox_local_state_root_for_os(
            "linux",
            Path::new("/private/tmp/repo-graph-agent/501")
        ));
    }

    #[test]
    fn non_temp_root_is_not_sandbox_local_on_either_os() {
        for os in ["macos", "linux"] {
            for root in [
                "/Users/u/Library/Application Support/repo-graph",
                "/home/u/.local/share/rmap",
                "/var/tmp/x",
            ] {
                assert!(
                    !is_sandbox_local_state_root_for_os(os, Path::new(root)),
                    "os={os} root={root}"
                );
            }
        }
    }

    #[test]
    fn macos_predicate_is_component_wise_as_before() {
        assert!(is_sandbox_local_state_root_for_os(
            "macos",
            &Path::new("/private").join("tmp")
        ));
        assert!(!is_sandbox_local_state_root_for_os(
            "macos",
            Path::new("/private/tmpfoo/x")
        ));
        assert!(!is_sandbox_local_state_root_for_os(
            "macos",
            Path::new("/tmp/x")
        ));
    }

    #[test]
    fn sandbox_state_root_is_sandbox_local_on_both_os_arms() {
        for os in ["macos", "linux"] {
            let root = sandbox_state_root_for(os, 501);
            assert!(
                is_sandbox_local_state_root_for_os(os, &root),
                "os={os} root={}",
                root.display()
            );
        }
    }

    #[test]
    fn current_host_functions_use_the_host_os_and_effective_uid() {
        let os = std::env::consts::OS;
        assert_eq!(sandbox_temp_base(), sandbox_temp_base_for_os(os));
        assert_eq!(
            sandbox_state_root(),
            sandbox_state_root_for(os, effective_uid())
        );
        let root = sandbox_state_root();
        assert_eq!(
            is_sandbox_local_state_root(&root),
            is_sandbox_local_state_root_for_os(os, &root)
        );
        assert!(is_sandbox_local_state_root(&root));
        let outside = Path::new("/var/tmp/rg-global-state-control");
        assert_eq!(
            is_sandbox_local_state_root(outside),
            is_sandbox_local_state_root_for_os(os, outside)
        );
    }
}
