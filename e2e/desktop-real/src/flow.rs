//! WINDOWS_NOVICE_GOLDEN_PATH — the novice journey through the REAL LingBi
//! desktop binary on Book V3 (P5-1 slice: project/edit/save/restart):
//!
//! launch → welcome → create book (custom temp root) → create chapter →
//! type Chinese → save → disk evidence (定稿/正文 bytes + .lingbi tx layer)
//! → configure AI (SSE) → generate → stream → cancel → candidate → adopt
//! (settlement + Final Revision snapshot) → CHAPTER_COMMITTED guard on
//! second adopt → close → relaunch → open by path → content intact.
//!
//! Export is re-attached in P5-4; until then the flow stops at the P5-2 gate.
//!
//! The flow drives the real React UI + real IPC + real Rust Core. It shares
//! the platform wiring (display/driver) through `platform`.

use crate::platform::{ChildGuard, start_display, tauri_driver_command};
use crate::webdriver::{
    book_state, click_button, create_session, edit_code_mirror, wait_for_text,
};
use serde_json::Value;
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


/// SSE chat-completions server（P5-2 恢复）：慢速请求发一个 chunk 后挂起
/// 30s（测流式与取消）；其它请求流式 E2E候选正文 并结束。
pub fn start_sse_server() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind sse");
    let port = listener.local_addr().expect("local addr").port();
    std::thread::spawn(move || {
        // 每连接一线程：慢速请求挂起 30s 期间，后续连接必须能并行处理
        // （生成→采纳→再生成 同会话内多次请求）
        for stream in listener.incoming().flatten() {
            std::thread::spawn(move || {
                let mut stream = stream;
                let _ = handle_sse(&mut stream);
            });
        }
    });
    format!("http://127.0.0.1:{port}/v1/chat/completions")
}

fn handle_sse(stream: &mut std::net::TcpStream) -> std::io::Result<()> {
    use std::io::{Read, Write};
    const CRLF_CRLF: &[u8] = b"\r\n\r\n";
    let mut buffer = Vec::with_capacity(8192);
    let mut chunk = [0u8; 4096];
    loop {
        let count = stream.read(&mut chunk)?;
        if count == 0 {
            return Ok(());
        }
        buffer.extend_from_slice(&chunk[..count]);
        if buffer.windows(4).any(|window| window == CRLF_CRLF) {
            break;
        }
        if buffer.len() > 64 * 1024 {
            return Ok(());
        }
    }
    let request = String::from_utf8_lossy(&buffer);
    // 读完整 body（Content-Length）才能按指令路由（慢速 vs 正常）
    let header_end = buffer
        .windows(4)
        .position(|window| window == CRLF_CRLF)
        .map(|position| position + 4)
        .unwrap_or(0);
    let content_len = content_length(&request);
    if let Some(length) = content_len {
        while buffer.len() < header_end + length {
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
        // 挂起 30s：UI 必须能在 provider 等待期间显示 delta 并取消
        let mut hold = [0u8; 16];
        let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(30)));
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

    let sse_url = start_sse_server();
    // 自测 SSE server（E2E 进程内直连，区分 server 与 app 连接问题）
    let client = reqwest::blocking::Client::new();
    let self_test = client
        .post(&sse_url)
        .json(&serde_json::json!({"messages": [{"role": "user", "content": "hi"}], "stream": true}))
        .send()
        .expect("SSE server must serve in-process requests");
    assert_eq!(self_test.status().as_u16(), 200, "SSE self-test status");

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

    // ---- P5-2 AI slice: configure → generate → stream → cancel → adopt ----
    eprintln!("PHASE: ai slice");
    // 返回首页 → 连接 AI（fake key + SSE base URL + model）
    click_summary(&first, "高级功能");
    click_button(&first, "返回首页");
    wait_for_text(&first, "连接 AI", Duration::from_secs(20));
    click_button(&first, "连接 AI");
    wait_for_text(&first, "选择 AI 服务", Duration::from_secs(20));
    fill_input(&first, "API Key", "fake");
    click_summary(&first, "高级设置");
    fill_input(&first, "自定义 Base URL", &sse_url);
    fill_input(&first, "自定义模型 ID", "e2e");
    click_button(&first, "保存设置");
    wait_for_text(&first, "AI 设置已保存", Duration::from_secs(20));
    let health = invoke(&first, "provider_test", serde_json::json!({}));
    assert_eq!(health["ok"].as_bool(), Some(true), "provider_test: {health}");
    // 验收三：UI 选中模型与请求 payload 一致（运行时快照）
    assert_eq!(
        health["model_id"].as_str(),
        Some("e2e"),
        "model_id must match configured model: {health}"
    );
    let status = invoke(&first, "provider_status", serde_json::json!({}));
    assert_eq!(
        status["model_id"].as_str(),
        Some("e2e"),
        "provider_status model: {status}"
    );

    // 返回 → 打开已有作品 → 路径打开书仓 → 第一章
    click_button(&first, "返回");
    click_button(&first, "打开已有作品");
    wait_for_text(&first, "最近作品", Duration::from_secs(20));
    click_summary(&first, "高级选项");
    fill_input(&first, "项目路径", &root);
    click_button(&first, "打开");
    wait_for_text(&first, "书仓已打开", Duration::from_secs(30));
    click_button(&first, "第一章");
    wait_for_text(&first, "雨夜，林渊推开旧车站的门。", Duration::from_secs(20));

    // 取消路径：慢速请求 → 一个 chunk 后挂起 → 流式出现 → 取消
    fill_input(&first, "写作要求", "慢速生成测试");
    click_button(&first, "生成");
    wait_for_text(&first, "第一部分", Duration::from_secs(20));
    click_button(&first, "取消");
    wait_for_text(&first, "已取消", Duration::from_secs(20));
    // 负向断言：取消不落候选（工作区/草稿 为空）
    let draft_dir = root_path.join("工作区/草稿");
    if draft_dir.is_dir() {
        let count = std::fs::read_dir(&draft_dir)
            .map(|entries| entries.flatten().count())
            .unwrap_or(0);
        assert_eq!(count, 0, "cancel must not leave draft files");
    }

    // 完整生成 → 候选预览 → 采纳 → 正文 + 追踪结算
    fill_input(&first, "写作要求", "写一个雨夜开场");
    click_button(&first, "生成");
    wait_for_text(&first, "候选内容", Duration::from_secs(30));
    wait_for_text(&first, "E2E候选正文", Duration::from_secs(30));
    click_button(&first, "采纳");
    wait_for_text(&first, "已采纳", Duration::from_secs(30));

    // 采纳后：正文 = 候选、追踪修订 bump、Final Revision 快照存在
    let adopted = book_state(&first, Some(1));
    assert_contains(
        adopted["content"].as_str().expect("content"),
        "E2E候选正文",
        "adopted content",
    )?;
    assert_eq!(adopted["state_revision"].as_i64(), Some(2), "revision after adopt: {adopted}");
    if !root_path.join(".lingbi/versions").is_dir() {
        return Err("Final Revision snapshot dir missing after adopt".to_owned());
    }

    // 已结算章再次采纳 → CHAPTER_COMMITTED 拦截（候选生成仍允许）
    fill_input(&first, "写作要求", "再写一段");
    click_button(&first, "生成");
    wait_for_text(&first, "候选内容", Duration::from_secs(30));
    click_button(&first, "采纳");
    // CHAPTER_COMMITTED 专门文案（humanizeError）：已提交章不可直写
    wait_for_text(&first, "本章已提交", Duration::from_secs(20));

    // ---- P5-3 writing tools: outline/contract/brief/checks/diagnose ----
    eprintln!("PHASE: tools slice");
    click_summary(&first, "大纲");
    wait_for_text(&first, "暂无细纲蓝图", Duration::from_secs(20));
    click_summary(&first, "合同");
    wait_for_text(&first, "goal", Duration::from_secs(20));
    click_summary(&first, "任务书");
    wait_for_text(&first, "写作任务书", Duration::from_secs(20));
    click_summary(&first, "机检");
    wait_for_text(&first, "摘要非空", Duration::from_secs(20));
    // 验收②：机检失败项明确展示（本仓第一章无 summary → 该项真实失败）
    wait_for_text(&first, "✘ 摘要非空", Duration::from_secs(20));
    click_summary(&first, "断点诊断");
    wait_for_text(&first, "next_step", Duration::from_secs(20));

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
    // P5-2 采纳后正文 = 候选内容、追踪修订 bump 到 2。
    click_button(&second, "第一章");
    wait_for_text(&second, "E2E候选正文", Duration::from_secs(20));
    let reopened = book_state(&second, Some(1));
    assert_contains(
        reopened["content"].as_str().expect("content"),
        "E2E候选正文",
        "reopened chapter content",
    )?;
    assert_eq!(reopened["ch"].as_i64(), Some(1), "reopened ch: {reopened}");
    assert_eq!(reopened["state_revision"].as_i64(), Some(2), "state revision: {reopened}");

    second.delete();
    let _ = driver_process;

    // Clean up the real book folder created by the novice flow.
    let _ = std::fs::remove_dir_all(&root_path);
    Ok(())
}
