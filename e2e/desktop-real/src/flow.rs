//! The real-desktop golden path flow, shared by the Windows product E2E and
//! the Linux compatibility test. Returns `Ok(())` only when the full journey
//! through the real binary passes.

use crate::platform::{ChildGuard, start_display, tauri_driver_command};
use crate::repo_root;
use crate::webdriver::{
    chapter_state, click_button, create_session, edit_code_mirror, session_state, start_sse_server,
    wait_for_text,
};
use serde_json::Value;
use std::path::Path;
use std::time::{Duration, Instant};
use tempfile::TempDir;

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

/// Full journey: create project → chapters → save → AI generate (real IPC)
/// → adopt → close → reopen → verify everything survived.
///
/// `binary` must exist. This flow does not know or care about the platform;
/// the platform-specific display/driver wiring happens in `spawn_driver`.
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
    let driver_process = ChildGuard::new(
        tauri
            .spawn()
            .map_err(|error| format!("tauri-driver spawn failed: {error}"))?,
    );
    let base = format!("http://127.0.0.1:{port}");
    wait_for_driver(&base)?;

    let temp = TempDir::new().map_err(|error| format!("temp dir: {error}"))?;
    let root = temp.path().join("e2e-novel");
    let root_str = root.to_string_lossy().into_owned();
    let sse_url = start_sse_server();

    let first = create_session(&base, binary);
    wait_for_text(&first, "LingBi Next", Duration::from_secs(30));
    set_welcome_inputs(&first, &root_str);
    click_button(&first, "创建项目");
    wait_for_text(&first, "保存", Duration::from_secs(30));
    click_button(&first, "新建章节");
    wait_for_text(&first, "第二章", Duration::from_secs(20));
    edit_code_mirror(&first, "第二章正文 E2E");
    click_button(&first, "保存");
    wait_for_text(&first, "已保存", Duration::from_secs(20));

    configure_provider(&first, &sse_url);
    click_button(&first, "生成");
    wait_for_text(&first, "E2E候选正文", Duration::from_secs(30));
    click_button(&first, "采纳");
    wait_for_text(&first, "已采纳", Duration::from_secs(20));

    let state = session_state(&first);
    assert_documents(&state, 2)?;
    assert_content(&state, "E2E候选正文")?;
    assert_eq_u64(&state, "revision", 2)?;
    assert_hash(&state)?;
    first.delete();
    std::thread::sleep(Duration::from_secs(1));

    let second = create_session(&base, binary);
    wait_for_text(&second, "LingBi Next", Duration::from_secs(30));
    set_welcome_inputs(&second, &root_str);
    click_button(&second, "打开项目");
    wait_for_text(&second, "保存", Duration::from_secs(30));
    let reopened = session_state(&second);
    assert_documents(&reopened, 2)?;
    let reopened_chapter = chapter_state(&second, "第二章");
    assert_content(&reopened_chapter, "E2E候选正文")?;
    assert_eq_u64(&reopened_chapter, "revision", 2)?;
    second.delete();

    let _ = driver_process;
    Ok(())
}

fn set_welcome_inputs(driver: &crate::webdriver::WebDriver, root: &str) {
    let root_json = serde_json::to_string(root).expect("json");
    let script = format!(
        r#"
        const inputs = document.querySelectorAll('input');
        if (inputs.length < 2) return 'missing';
        const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
        setter.call(inputs[0], 'E2E小说');
        inputs[0].dispatchEvent(new Event('input', {{ bubbles: true }}));
        setter.call(inputs[1], {root_json});
        inputs[1].dispatchEvent(new Event('input', {{ bubbles: true }}));
        return 'ok';
        "#
    );
    let value = driver.execute(&script);
    assert_eq!(value.as_str(), Some("ok"), "set inputs: {value}");
}

fn configure_provider(driver: &crate::webdriver::WebDriver, base_url: &str) {
    let url_json = serde_json::to_string(base_url).expect("json");
    let script = format!(
        r#"
        const done = arguments[arguments.length - 1];
        window.__TAURI_INTERNALS__.invoke('provider_configure', {{
          key: 'fake',
          baseUrl: {url_json},
          model: 'e2e'
        }}).then(() => done('ok')).catch((error) => done('error:' + error));
        "#
    );
    let value = driver.execute_async(&script, vec![]);
    assert_eq!(value.as_str(), Some("ok"), "configure provider: {value}");
}

fn assert_documents(state: &Value, expected: usize) -> Result<(), String> {
    let count = state["documents"]
        .as_array()
        .map(|items| items.len())
        .unwrap_or(0);
    if count != expected {
        return Err(format!("expected {expected} documents, found {count}"));
    }
    Ok(())
}

fn assert_content(state: &Value, needle: &str) -> Result<(), String> {
    let content = state["content"].as_str().unwrap_or_default();
    if !content.contains(needle) {
        return Err(format!("content missing {needle:?}: {content}"));
    }
    Ok(())
}

fn assert_eq_u64(state: &Value, field: &str, expected: u64) -> Result<(), String> {
    let actual = state[field].as_u64();
    if actual != Some(expected) {
        return Err(format!("expected {field}={expected}, found {actual:?}"));
    }
    Ok(())
}

fn assert_hash(state: &Value) -> Result<(), String> {
    let hash = state["hash"].as_str().unwrap_or_default();
    if hash.len() != 64 {
        return Err(format!("expected 64-char hash, found {:?}", hash.len()));
    }
    Ok(())
}

/// Convenience used by tests that want the repo root too.
pub fn repo() -> std::path::PathBuf {
    repo_root()
}
