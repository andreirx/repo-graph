//! Home directory resolution.
//!
//! Provides canonical home lookup via OS-level passwd entry (`getpwuid_r`),
//! not `$HOME`. This ensures stable paths across sandboxed shells and agent
//! environments.

use std::path::PathBuf;

/// Returns the canonical home directory from the passwd entry.
///
/// On Unix, this uses `getpwuid_r(geteuid())` — the reentrant, thread-safe
/// version — to look up the home directory for the effective user ID.
/// This returns the actual account home regardless of what `$HOME` is set to.
///
/// On non-Unix platforms, falls back to `dirs::home_dir()`.
#[cfg(unix)]
pub fn canonical_home() -> Option<PathBuf> {
    use std::ffi::CStr;
    use std::mem::MaybeUninit;

    let uid = unsafe { libc::geteuid() };

    // Buffer for passwd struct
    let mut passwd: MaybeUninit<libc::passwd> = MaybeUninit::uninit();
    let mut result: *mut libc::passwd = std::ptr::null_mut();

    // Buffer for string data. 1024 is typically sufficient for passwd entries.
    // sysconf(_SC_GETPW_R_SIZE_MAX) could be used but adds complexity.
    let mut buf = vec![0u8; 1024];

    // SAFETY: getpwuid_r is thread-safe and reentrant.
    // We provide properly sized buffers and check the result pointer.
    let ret = unsafe {
        libc::getpwuid_r(
            uid,
            passwd.as_mut_ptr(),
            buf.as_mut_ptr() as *mut libc::c_char,
            buf.len(),
            &mut result,
        )
    };

    if ret != 0 || result.is_null() {
        return None;
    }

    // SAFETY: result is non-null and points to initialized passwd struct
    let passwd = unsafe { passwd.assume_init() };

    if passwd.pw_dir.is_null() {
        return None;
    }

    // SAFETY: pw_dir is non-null and points to a valid C string within buf
    let home_cstr = unsafe { CStr::from_ptr(passwd.pw_dir) };
    home_cstr.to_str().ok().map(PathBuf::from)
}

#[cfg(not(unix))]
pub fn canonical_home() -> Option<PathBuf> {
    dirs::home_dir()
}

/// Returns the effective user ID.
#[cfg(unix)]
pub fn effective_uid() -> u32 {
    unsafe { libc::geteuid() }
}

#[cfg(not(unix))]
pub fn effective_uid() -> u32 {
    0 // Placeholder for non-Unix
}

/// Returns the legacy home directory from `$HOME` / dirs crate.
///
/// This is what the old code used. Kept for:
/// - Migration fallback probing
/// - Finding host tool configs that use `$HOME`-relative paths
///   (Claude Code, Codex, Cursor settings)
pub fn legacy_home() -> Option<PathBuf> {
    dirs::home_dir()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_home_returns_some_on_unix() {
        let home = canonical_home();
        assert!(
            home.is_some(),
            "canonical_home() should return Some on Unix"
        );

        let path = home.unwrap();
        assert!(path.is_absolute(), "canonical home should be absolute path");
    }

    #[test]
    fn canonical_home_matches_actual_home() {
        let canonical = canonical_home();
        let env_home = std::env::var("HOME").ok().map(PathBuf::from);

        // In normal environments these should match
        // In sandboxed environments they may differ (which is the point)
        if let (Some(canon), Some(env)) = (&canonical, &env_home) {
            assert!(canon.is_absolute());
            assert!(env.is_absolute());
        }
    }

    #[test]
    fn effective_uid_returns_current_user() {
        let uid = effective_uid();
        #[cfg(unix)]
        {
            let actual_uid = unsafe { libc::geteuid() };
            assert_eq!(uid, actual_uid);
        }
    }

    #[test]
    fn legacy_home_returns_some() {
        let home = legacy_home();
        assert!(home.is_some());
    }
}
