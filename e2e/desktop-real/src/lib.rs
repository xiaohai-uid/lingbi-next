//! Shared infrastructure for driving the REAL LingBi desktop release binary
//! through `tauri-driver` (WebDriver protocol).
//!
//! Platform policy (see docs/qa/release-gates.md):
//!
//! - Windows: product E2E. tauri-driver + Edge WebDriver (WebView2).
//!   Missing binary or driver on Windows is a hard failure.
//! - Linux: compatibility only. tauri-driver + WebKitWebDriver under Xvfb.
//!   Missing infra on Linux prints an explicit compat-skip, never a pass.
//!
//! No user-home paths are hardcoded anywhere. Driver/binary locations are
//! resolved from environment variables, well-known machine locations, or
//! PATH, in that order.

pub mod flow;
pub mod platform;
pub mod webdriver;

pub use flow::golden_path;
pub use platform::{DisplayGuard, NativeDriver, check_infra, release_binary, start_display};
pub use webdriver::WebDriver;

use std::path::PathBuf;

/// Repository root, resolved from the crate location, never from $HOME.
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}
