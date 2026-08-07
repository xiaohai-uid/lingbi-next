//! Linux compatibility E2E (NOT a product gate).
//!
//! Linux is a Core/development compatibility platform. This test drives the
//! same real binary with tauri-driver + WebKitWebDriver under Xvfb when the
//! infra exists, and otherwise prints an explicit compat-skip with the
//! reason. A compat-skip is never counted as a pass and never authorizes a
//! product release (docs/qa/release-gates.md).

#![cfg(target_os = "linux")]

use lingbi_e2e_desktop_real::golden_path;
use lingbi_e2e_desktop_real::platform::{check_infra, release_binary};
use lingbi_e2e_desktop_real::repo_root;

#[cfg(target_os = "linux")]
#[test]
fn linux_compat_tauri_release_binary_e2e() {
    let repo = repo_root();
    let binary = release_binary(&repo);
    if !binary.exists() {
        eprintln!(
            "LINUX_COMPAT_SKIP: release binary missing at {} (build with `pnpm tauri build --no-bundle` to run compat)",
            binary.display()
        );
        return;
    }
    if let Err(reason) = check_infra() {
        eprintln!("LINUX_COMPAT_SKIP: {reason}");
        return;
    }
    // Infra is present, so this compat test must pass honestly.
    golden_path(&binary).expect("linux compat golden path failed");
}
