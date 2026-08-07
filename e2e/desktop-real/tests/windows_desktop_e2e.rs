//! Windows product E2E: drives the REAL LingBi Tauri release binary through
//! tauri-driver + Edge WebDriver (WebView2), exercising the real React UI,
//! real IPC, and real Rust Core.
//!
//! This is a product gate, not a compatibility check:
//! - compiled only on Windows
//! - never `#[ignore]`d
//! - a missing release binary or driver is a hard FAIL, not a skip

#![cfg(target_os = "windows")]

use lingbi_e2e_desktop_real::golden_path;
use lingbi_e2e_desktop_real::platform::release_binary;
use lingbi_e2e_desktop_real::repo_root;

#[cfg(target_os = "windows")]
#[test]
fn windows_novice_golden_path() {
    let repo = repo_root();
    let binary = release_binary(&repo);
    assert!(
        binary.exists(),
        "Windows release binary missing at {}; build with `pnpm tauri build` first",
        binary.display()
    );
    golden_path(&binary).expect("WINDOWS_DESKTOP_E2E golden path must pass");
}
