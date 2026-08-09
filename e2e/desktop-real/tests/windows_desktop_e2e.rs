//! Windows product E2E: drives the REAL LingBi Tauri release binary through
//! tauri-driver + Edge WebDriver (WebView2), exercising the real React UI,
//! real IPC, and real Rust Core.
//!
//! This is a product gate, not a compatibility check:
//! - compiled only on Windows
//! - never `#[ignore]`d
//! - a missing release binary or driver is a hard FAIL, not a skip

#![cfg(target_os = "windows")]

use lingbi_e2e_desktop_real::{consumer_full_scenario, golden_path};
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

/// P11 #28 消费者完整旅程（AI 增强路径）：手写 golden path 精简 → 链完整
/// 跑通（机检 halt → 补计划 → 续跑 → 定稿 + Final Revision）→ 第二章 Gate
/// 返工 → 导出 → 旧项目迁移 → 重启 0 丢失。
#[cfg(target_os = "windows")]
#[test]
fn windows_consumer_full_scenario() {
    let repo = repo_root();
    let binary = release_binary(&repo);
    assert!(
        binary.exists(),
        "Windows release binary missing at {}; build with `pnpm tauri build` first",
        binary.display()
    );
    consumer_full_scenario(&binary)
        .expect("WINDOWS_DESKTOP_E2E consumer full scenario must pass");
}
