//! WINDOWS_NOVICE_GOLDEN_PATH — the complete novice journey through the
//! REAL LingBi desktop binary:
//!
//! launch → no CLI → welcome → create novel by NAME ONLY → default save
//! location → first chapter auto-created → type Chinese → save → connect AI
//! → test ok → generate → see live output → cancel → candidate → preview →
//! adopt → close → relaunch → open recent → chapter/content/candidate/
//! revision intact → export DOCX/MD/TXT.
//!
//! The flow drives the real React UI + real IPC + real Rust Core. It shares
//! the platform wiring (display/driver) through `platform`.

use crate::platform::{ChildGuard, start_display, tauri_driver_command};
use crate::webdriver::{
    chapter_state, click_button, create_session, edit_code_mirror, session_state, wait_for_text,
};
use serde_json::Value;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::thread;
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

fn invoke(driver: &crate::webdriver::WebDriver, command: &str, args: Value) -> Value {
    let command_json = serde_json::to_string(command).expect("json");
    let args_json = serde_json::to_string(&args).expect("json");
    let script = format!(
        r#"
        const done = arguments[arguments.length - 1];
        window.__TAURI_INTERNALS__.invoke({command_json}, {args_json}).then((value) => done(JSON.stringify({{ok:true,value}}))).catch((error) => done(JSON.stringify({{ok:false,error}})));
        "#
    );
    let value = driver.execute_async(&script, vec![]);
    let serialized = value.as_str().expect("invoke string");
    let parsed: Value = serde_json::from_str(serialized)
        .unwrap_or_else(|error| panic!("invoke json: {error}: {serialized}"));
    if parsed["ok"].as_bool() != Some(true) {
        panic!("invoke {command} failed: {parsed}");
    }
    parsed["value"].clone()
}

/// SSE chat-completions server.
///
/// - Request body containing "慢速": sends one chunk ("第一部分") then holds
///   the connection for up to 30s, so the UI must stream the delta and the
///   user must be able to cancel while the provider is still waiting.
/// - Any other request: streams the full "E2E候选正文" and [DONE].
pub fn start_sse_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind sse");
    let port = listener.local_addr().expect("local addr").port();
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let _ = handle_sse(&mut stream);
        }
    });
    format!("http://127.0.0.1:{port}/v1/chat/completions")
}

fn handle_sse(stream: &mut TcpStream) -> std::io::Result<()> {
    let mut buffer = Vec::with_capacity(8192);
    let mut chunk = [0u8; 4096];
    loop {
        let count = stream.read(&mut chunk)?;
        if count == 0 {
            return Ok(());
        }
        buffer.extend_from_slice(&chunk[..count]);
        if buffer.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
        if buffer.len() > 64 * 1024 {
            return Ok(());
        }
    }
    let request = String::from_utf8_lossy(&buffer);
    // Read the request body (Content-Length) so we can route by instruction.
    if let Some(length) = content_length(&request) {
        while buffer.len() < 4 + length {
            let count = stream.read(&mut chunk)?;
            if count == 0 {
                break;
            }
            buffer.extend_from_slice(&chunk[..count]);
        }
    }
    let body_text = String::from_utf8_lossy(&buffer);
    if body_text.contains("慢速") {
        let first = "data: {\"choices\":[{\"delta\":{\"content\":\"第一部分\"}}]}\n\n";
        let head = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n{first}"
        );
        write!(stream, "{head}")?;
        stream.flush()?;
        let mut hold = [0u8; 16];
        let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
        let _ = stream.read(&mut hold);
        Ok(())
    } else {
        let body =
            "data: {\"choices\":[{\"delta\":{\"content\":\"E2E候选正文\"}}]}\n\ndata: [DONE]\n\n";
        let head = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        write!(stream, "{head}")?;
        stream.flush()
    }
}

fn content_length(request: &str) -> Option<usize> {
    for line in request.lines() {
        let lower = line.to_ascii_lowercase();
        if let Some(value) = lower.strip_prefix("content-length:") {
            return value.trim().parse().ok();
        }
    }
    None
}

fn assert_contains(value: &str, needle: &str, what: &str) -> Result<(), String> {
    if value.contains(needle) {
        Ok(())
    } else {
        Err(format!("{what} missing {needle:?}: {value}"))
    }
}

fn export_paths(root: &Path) -> (PathBuf, PathBuf, PathBuf) {
    (
        root.join("export").join("第一章.md"),
        root.join("export").join("第一章.txt"),
        root.join("export").join("第一章.docx"),
    )
}

/// The complete novice golden path. `binary` must exist.
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

    let sse_url = start_sse_server();

    // ---- Session 1: create, write, AI, generate, cancel, adopt, export ----
    let first = create_session(&base, binary);
    wait_for_text(&first, "LingBi Next", Duration::from_secs(30));
    wait_for_text(&first, "开始写作", Duration::from_secs(20));

    // 开始写作: name only, no path knowledge.
    click_button(&first, "开始写作");
    fill_input(&first, "作品名", PROJECT_NAME);
    click_button(&first, "创建项目");

    // First chapter auto-created; editor visible.
    wait_for_text(&first, "第一章", Duration::from_secs(30));
    wait_for_text(&first, "保存", Duration::from_secs(20));

    // Type Chinese body and save.
    edit_code_mirror(&first, "雨夜，林渊推开旧车站的门。");
    click_button(&first, "保存");
    wait_for_text(&first, "已保存", Duration::from_secs(20));

    // Default save location: {Documents}/LingBi/<name>.
    eprintln!("PHASE: after first save");
    let state = session_state(&first);
    let root = state["root"].as_str().expect("root").to_owned();
    assert_contains(&root, "LingBi", "default root")?;
    assert_contains(&root, PROJECT_NAME, "default root project name")?;
    let root_path = PathBuf::from(&root);
    assert_contains(
        state["content"].as_str().expect("content"),
        "雨夜，林渊推开旧车站的门。",
        "chapter body",
    )?;
    assert_eq_u64(&state, "revision", 1)?;

    // 返回首页 → 连接 AI (simple: provider + key + advanced base URL/model).
    click_summary(&first, "高级功能");
    click_button(&first, "返回首页");
    wait_for_text(&first, "打开已有作品", Duration::from_secs(20));
    click_button(&first, "连接 AI");
    wait_for_text(&first, "选择 AI 服务", Duration::from_secs(20));
    fill_input(&first, "API Key", "fake");
    click_summary(&first, "高级设置");
    fill_input(&first, "自定义 Base URL", &sse_url);
    fill_input(&first, "自定义模型 ID", "e2e");
    click_button(&first, "保存设置");
    wait_for_text(&first, "AI 设置已保存", Duration::from_secs(20));
    click_button(&first, "测试连接");
    wait_for_text(&first, "连接成功", Duration::from_secs(30));

    // 返回 → 打开已有作品 → recent list → open.
    click_button(&first, "返回");
    click_button(&first, "打开已有作品");
    wait_for_text(&first, "最近作品", Duration::from_secs(20));
    wait_for_text(&first, PROJECT_NAME, Duration::from_secs(20));
    click_button(&first, PROJECT_NAME);
    wait_for_text(&first, "保存", Duration::from_secs(30));

    // Cancel path: instruction with 慢速 → one chunk then 30s hold.
    fill_input(&first, "写作要求", "慢速生成测试");
    click_button(&first, "生成");
    // Real-time streaming: the delta must appear while the provider is
    // still waiting (server holds 30s after this chunk).
    wait_for_text(&first, "第一部分", Duration::from_secs(20));
    click_button(&first, "取消");
    wait_for_text(&first, "已取消", Duration::from_secs(20));

    // Full generation → candidate preview → adopt.
    fill_input(&first, "写作要求", "写一个雨夜开场");
    click_button(&first, "生成");
    wait_for_text(&first, "候选内容", Duration::from_secs(30));
    wait_for_text(&first, "E2E候选正文", Duration::from_secs(30));
    click_button(&first, "采纳");
    wait_for_text(&first, "已采纳", Duration::from_secs(20));

    eprintln!("PHASE: after adopt");
    let adopted = session_state(&first);
    assert_contains(
        adopted["content"].as_str().expect("content"),
        "E2E候选正文",
        "adopted content",
    )?;
    assert_eq_u64(&adopted, "revision", 2)?;
    assert_hash(&adopted)?;

    eprintln!("PHASE: exports");
    // Export DOCX / MD / TXT (files must really exist on disk).
    click_button(&first, "导出 DOCX");
    click_button(&first, "导出 MD");
    click_button(&first, "导出 TXT");
    thread::sleep(Duration::from_millis(1500));
    let (md, txt, docx) = export_paths(&root_path);
    if !md.exists() || !txt.exists() || !docx.exists() {
        return Err(format!(
            "export files missing: md={} txt={} docx={}",
            md.exists(),
            txt.exists(),
            docx.exists()
        ));
    }

    first.delete();
    thread::sleep(Duration::from_secs(1));

    // ---- Session 2: relaunch, open recent, everything intact ----
    let second = create_session(&base, binary);
    wait_for_text(&second, "LingBi Next", Duration::from_secs(30));
    click_button(&second, "打开已有作品");
    wait_for_text(&second, "最近作品", Duration::from_secs(20));
    wait_for_text(&second, PROJECT_NAME, Duration::from_secs(20));
    click_button(&second, PROJECT_NAME);
    wait_for_text(&second, "保存", Duration::from_secs(30));

    eprintln!("PHASE: after reopen");
    let reopened = session_state(&second);
    assert_documents(&reopened, 1)?;
    eprintln!("PHASE: chapter state after reopen");
    let reopened_chapter = chapter_state(&second, "第一章");
    assert_contains(
        reopened_chapter["content"].as_str().expect("content"),
        "E2E候选正文",
        "reopened chapter content",
    )?;
    assert_eq_u64(&reopened_chapter, "revision", 2)?;

    // AI configuration survived the restart (keyring), without the key.
    let provider = invoke(&second, "provider_status", serde_json::json!({}));
    assert_eq!(
        provider["configured"].as_bool(),
        Some(true),
        "AI config must survive restart: {provider}"
    );
    assert_eq!(provider["provider_id"].as_str(), Some("openai"));

    // Exports survived the restart.
    if !md.exists() || !txt.exists() || !docx.exists() {
        return Err("exports missing after restart".to_owned());
    }

    second.delete();
    let _ = driver_process;

    // Clean up the real project folder created by the novice flow.
    let _ = std::fs::remove_dir_all(&root_path);
    Ok(())
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
