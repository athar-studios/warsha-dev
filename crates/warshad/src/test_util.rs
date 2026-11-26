//! Test-only helpers shared by the unit-test modules in this crate.
//!
//! The daemon's unit tests each want a fresh temporary directory (a data
//! directory, a settings database, a process-scope tree). The obvious call —
//! `tempfile::tempdir()` — puts them in the system `%TEMP%`, which on some
//! Windows machines is protected and rejects even creating a tempdir
//! (`PermissionDenied`, OS error 5). The tests are not optional and the
//! filesystem is, so the temporary directories come from the target directory
//! instead, exactly like `fresh_tempdir` in `tests/child_reaping.rs` and
//! `crates/warsha-git/src/lib.rs`: the same three-way fallback, one owner.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

use std::ffi::OsString;
use std::path::PathBuf;
use tempfile::TempDir;

/// Serialise tests that mutate a process-wide environment variable such as
/// `WARSHA_GITHUB_API_BASE`.
pub(crate) static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// RAII guard that captures the prior value of an environment variable and restores (or
/// removes) it when dropped — including across `.await` points and test panics.
pub(crate) struct EnvVarGuard {
    key: &'static str,
    previous: Option<OsString>,
}

impl EnvVarGuard {
    pub(crate) fn set(key: &'static str, value: &str) -> Self {
        let previous = std::env::var_os(key);
        std::env::set_var(key, value);
        Self { key, previous }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(prev) => std::env::set_var(self.key, prev),
            None => std::env::remove_var(self.key),
        }
    }
}

/// A fresh temp directory for one test's data.
///
/// Prefers cargo's `CARGO_TARGET_TMPDIR`, then a `tmp` sibling of the test
/// binary's `deps` directory (the target dir is writable wherever the build
/// ran), and only then the system temp dir — which on some Windows machines is
/// protected and rejects even creating a tempdir (`Access is denied`).
pub(crate) fn fresh_tempdir() -> TempDir {
    if let Some(base) = std::env::var_os("CARGO_TARGET_TMPDIR") {
        let base = PathBuf::from(base);
        if base.is_dir() {
            if let Ok(dir) = TempDir::new_in(&base) {
                return dir;
            }
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(deps) = exe.parent() {
            let base = deps.join("tmp");
            if std::fs::create_dir_all(&base).is_ok() {
                if let Ok(dir) = TempDir::new_in(&base) {
                    return dir;
                }
            }
        }
    }
    TempDir::new().expect("tempdir for the test data")
}
