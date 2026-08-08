//! WINDOWS_NOVICE_GOLDEN_PATH — the novice journey through the REAL LingBi
//! desktop binary on Book V3 (P5-1 slice: project/edit/save/restart):
//!
//! launch → welcome → create book (custom temp root) → create chapter →
//! type Chinese → save → disk evidence (定稿/正文 bytes + .lingbi tx layer)
//! → close → relaunch → open by path → chapter/content intact.
//!
//! AI generation (P5-2) and export (P5-4) are re-attached in their slices;
//! until then this flow deliberately stops at the P5-1 product gate.
//!
//! The flow drives the real React UI + real IPC + real Rust Core. It shares
//! the platform wiring (display/driver) through `platform`.

use crate::platform::{ChildGuard, start_display, tauri_driver_command};
use crate::webdriver::{
    book_state, click_button, create_session, edit_code_mirror, wait_for_text,
};
use std::path::Path;
use std::time::{Duration, Instant};

const PROJECT_NAME: &str = "E2E小说";

/// Wait until the tauri-driver WebDriver endpoint responds.
fn wait_for_driver(base: &str) -> Result<(), String> {
    let started = Instant::now();
    loop {
        if reqwest::blocking::Client::new()
            .get(format!("{base}/status"))
            .send()
            .is_ok()
        {
            return Ok(());
        }
        if started.elapsed() >= Duration::from_secs(15) {
            return Err("tauri-driver did not start".to_owned());
        }
        std::thread::sleep(Duration::from_millis(300));
    }
}

fn fill_input(driver: &crate::webdriver::WebDriver, label: &str, value: &str) {
    let value_json = serde_json::to_string(value).expect("json");
    let label_json = serde_json::to_string(label).expect("json");
    let script = format!(
        r#"
        const inputs = Array.from(document.querySelectorAll('input'));
        const input = inputs.find((item) => item.getAttribute('aria-label') === {label_json});
        if (!input) return 'missing:' + inputs.map((item) => item.getAttribute('aria-label')).join(',');
        const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
        setter.call(input, {value_json});
        input.dispatchEvent(new Event('input', {{ bubbles: true }}));
        return 'ok';
        "#
    );
    let value = driver.execute(&script);
    assert_eq!(value.as_str(), Some("ok"), "fill input {label}: {value}");
}

/// Click a <details> summary by its text to expand/collapse it.
fn click_summary(driver: &crate::webdriver::WebDriver, text: &str) {
    let text_json = serde_json::to_string(text).expect("json");
    let script = format!(
        r#"
        const summaries = Array.from(document.querySelectorAll('summary'));
        const summary = summaries.find((item) => item.textContent?.includes({text_json}));
        if (!summary) return 'missing';
        summary.click();
        return 'ok';
        "#
    );
    let value = driver.execute(&script);
    assert_eq!(value.as_str(), Some("ok"), "click summary {text}: {value}");
}

fn assert_contains(value: &str, needle: &str, what: &str) -> Result<(), String> {
    if value.contains(needle) {
        Ok(())
    } else {
        Err(format!("{what} missing {needle:?}: {value}"))
    }
}

/// The complete novice golden path on Book V3 (P5-1): launch → create book
/// → create chapter → type Chinese → save → relaunch → open recent →
/// chapter/content intact. AI generation/export slices are re-attached in
/// P5-2/P5-4; `windows_novice_golden_path` keeps this as the product gate.
pub fn golden_path(binary: &Path) -> Result<(), String> {
    let _display = start_display()?;
    let driver = crate::platform::native_driver();
    let port = crate::platform::free_port();
    let native_port = crate::platform::free_port();
    let mut tauri = tauri_driver_command(&driver, port, native_port);
    #[cfg(target_os = "linux")]
    if let Some(display) = _display.display_env() {
        tauri.env("DISPLAY", display);
    }
    // Keep tauri-driver's stderr so a failing session creation can be
    // diagnosed instead of timing out silently.
    let driver_log_path = crate::platform::free_port().to_string() + ".driver.log";
    let driver_log_path = std::env::temp_dir().join(&driver_log_path);
    let driver_log_handle = std::fs::File::create(&driver_log_path)
        .map_err(|error| format!("driver log create: {error}"))?;
    tauri.stderr(std::process::Stdio::from(driver_log_handle));
    let driver_process = ChildGuard::new(
        tauri
            .spawn()
            .map_err(|error| format!("tauri-driver spawn failed: {error}"))?,
    );
    let base = format!("http://127.0.0.1:{port}");
    wait_for_driver(&base)?;

    // If a later step fails, surface the driver log for diagnosis.
    fn log_on_failure(path: &std::path::Path, what: &str) {
        if let Ok(bytes) = std::fs::read(path)
            && !bytes.is_empty()
        {
            eprintln!("--- tauri-driver stderr ({what}) ---");
            eprintln!("{}", String::from_utf8_lossy(&bytes));
        }
    }

    // Deterministic book root under the temp dir (cleanup-safe).
    let root_path = std::env::temp_dir().join("E2E书仓V3");
    let root = root_path.to_string_lossy().into_owned();
    let _ = std::fs::remove_dir_all(&root_path);

    // ---- Session 1: create book, create chapter, write, save ----
    let first = match create_session(&base, binary) {
        Ok(session) => session,
        Err(error) => {
            log_on_failure(&driver_log_path, "first session");
            return Err(error);
        }
    };
    wait_for_text(&first, "LingBi Next", Duration::from_secs(30));
    wait_for_text(&first, "开始写作", Duration::from_secs(20));

    // 开始写作 → 自定义保存位置（temp，确定性 root）→ 创建书仓。
    click_button(&first, "开始写作");
    fill_input(&first, "作品名", PROJECT_NAME);
    click_summary(&first, "高级选项");
    fill_input(&first, "自定义保存位置", &root);
    click_button(&first, "创建项目");
    wait_for_text(&first, "书仓已创建", Duration::from_secs(30));
    wait_for_text(&first, "保存", Duration::from_secs(20));

    // 建章（Book V3 无自动章）：新章节标题 → 新建章节。
    fill_input(&first, "新章节标题", "第一章");
    click_button(&first, "新建章节");
    wait_for_text(&first, "第一章", Duration::from_secs(20));

    // Type Chinese body and save.
    edit_code_mirror(&first, "雨夜，林渊推开旧车站的门。");
    click_button(&first, "保存");
    wait_for_text(&first, "已保存", Duration::from_secs(20));

    eprintln!("PHASE: after first save");
    // 磁盘证据：正文文件真实存在且含用户字节。
    let body_path = root_path.join("定稿/正文/001-第一章.md");
    if !body_path.exists() {
        return Err(format!("chapter file missing: {}", body_path.display()));
    }
    let disk = std::fs::read_to_string(&body_path)
        .map_err(|error| format!("read chapter: {error}"))?;
    assert_contains(&disk, "雨夜，林渊推开旧车站的门。", "disk chapter body")?;
    // 事务层已就位（新建书即托管）。
    if !root_path.join(".lingbi/transactions").is_dir() {
        return Err("tx layer missing after create_book".to_owned());
    }

    first.delete();
    std::thread::sleep(Duration::from_secs(1));

    // ---- Session 2: relaunch, open recent, everything intact ----
    let second = match create_session(&base, binary) {
        Ok(session) => session,
        Err(error) => {
            log_on_failure(&driver_log_path, "second session");
            return Err(error);
        }
    };
    wait_for_text(&second, "LingBi Next", Duration::from_secs(30));
    click_button(&second, "打开已有作品");
    wait_for_text(&second, "最近作品", Duration::from_secs(20));
    // 用路径打开：recent 列表可能含 V2 遗留同名记录（Documents/LingBi 旧
    // 项目），按钮文本匹配会命中错误条目；路径输入唯一且确定。
    click_summary(&second, "高级选项");
    fill_input(&second, "项目路径", &root);
    click_button(&second, "打开");
    wait_for_text(&second, "书仓已打开", Duration::from_secs(30));

    // 打开第一章 → 正文完整（UI 层 + 命令层双重证据）。
    click_button(&second, "第一章");
    wait_for_text(&second, "雨夜，林渊推开旧车站的门。", Duration::from_secs(20));
    let reopened = book_state(&second, Some(1));
    assert_contains(
        reopened["content"].as_str().expect("content"),
        "雨夜，林渊推开旧车站的门。",
        "reopened chapter content",
    )?;
    assert_eq!(reopened["ch"].as_i64(), Some(1), "reopened ch: {reopened}");
    assert_eq!(reopened["state_revision"].as_i64(), Some(1), "state revision: {reopened}");

    second.delete();
    let _ = driver_process;

    // Clean up the real book folder created by the novice flow.
    let _ = std::fs::remove_dir_all(&root_path);
    Ok(())
}
