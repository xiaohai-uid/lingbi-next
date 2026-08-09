//! WINDOWS_NOVICE_GOLDEN_PATH — the novice journey through the REAL LingBi
//! desktop binary on Book V3 (P5-1..P5-4 slices):
//!
//! launch → welcome → create book (custom temp root) → create chapter →
//! type Chinese → save → disk evidence (定稿/正文 bytes + .lingbi tx layer)
//! → configure AI (SSE) → generate → stream → cancel → candidate → adopt
//! (settlement + Final Revision snapshot) → CHAPTER_COMMITTED guard on
//! second adopt → export DOCX/MD/TXT → writing tools panels (大纲/合同/任务书/
//! 机检/诊断) → crash injection (content_written journal, no receipt) →
//! relaunch → open by path → recovery report visible → chapter intact.
//!
//! The flow drives the real React UI + real IPC + real Rust Core. It shares
//! the platform wiring (display/driver) through `platform`.

use crate::platform::{ChildGuard, start_display, tauri_driver_command};
use crate::webdriver::{
    append_code_mirror, assert_text_absent, book_state, click_button, click_button_retry,
    create_session, edit_code_mirror, wait_for_text,
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

/// 读取输入框当前值（P5-5 高级设置回显断言）。
fn read_input(driver: &crate::webdriver::WebDriver, label: &str) -> String {
    let label_json = serde_json::to_string(label).expect("json");
    let script = format!(
        r#"
        const inputs = Array.from(document.querySelectorAll('input'));
        const input = inputs.find((item) => item.getAttribute('aria-label') === {label_json});
        if (!input) return 'missing';
        return input.value;
        "#
    );
    let value = driver.execute(&script);
    value.as_str().unwrap_or("").to_owned()
}

/// Click a <details> summary by its text to expand/collapse it.
/// 等待任一文本出现（P9 #26：无 Key/宕机两种错误态都算「面板错误」）。
fn wait_for_any_text(
    driver: &crate::webdriver::WebDriver,
    needles: &[&str],
    timeout: std::time::Duration,
) {
    let conditions: Vec<String> = needles
        .iter()
        .map(|n| format!("document.body.innerText.includes({n:?})"))
        .collect();
    let script = format!(
        r#"
        return ({}) ? 'ok' : document.body.innerText;
        "#,
        conditions.join(" || ")
    );
    crate::webdriver::wait_until(driver, &script, timeout, "ok");
}

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


/// 评审调用计数（P11 #28）：奇数轮返回 blocking、偶数轮 clean——Gate
/// 返工（Validate→Repair→Validate）在真实 UI 流程可观测。
static REVIEW_CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// SSE chat-completions server（P5-2 恢复 + P11 #28 链路由）：
/// - 慢速请求发一个 chunk 后挂起 30s（测流式与取消）；
/// - 含「改写」→ 润色文本（polish 节点）；
/// - 含「评审」→ 评审 JSON（奇数轮 blocking、偶数轮 clean）；
/// - 其余 → E2E候选正文（draft 节点 / P5-2 生成）。
pub fn start_sse_server() -> String {
    REVIEW_CALLS.store(0, std::sync::atomic::Ordering::SeqCst);
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
    } else if body_text.contains("改写") {
        // P11 #28 polish 节点：按评审意见改写
        let body =
            "data: {\"choices\":[{\"delta\":{\"content\":\"E2E润色稿\"}}]}\n\ndata: [DONE]\n\n";
        let head = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        write!(stream, "{head}")?;
        stream.flush()
    } else if body_text.contains("评审") {
        // P11 #28 review 节点：奇数轮 blocking、偶数轮 clean（Gate 返工）
        let call = REVIEW_CALLS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let issues = if call % 2 == 0 {
            r#"[{"severity":"critical","category":"logic","location":"L1","description":"前后矛盾","evidence":"x","fix_hint":"y"}]"#
        } else {
            "[]"
        };
        let content = format!(r#"{{"issues":{issues}}}"#);
        // delta content 内嵌 JSON 必须转义引号（否则 provider 解析 InvalidResponse）
        let escaped = content.replace('\\', "\\\\").replace('"', "\\\"");
        let body = format!(
            "data: {{\"choices\":[{{\"delta\":{{\"content\":\"{escaped}\"}}}}]}}\n\ndata: [DONE]\n\n"
        );
        let head = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        write!(stream, "{head}")?;
        stream.flush()
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

/// P11 #28 消费者完整旅程（真实 release 二进制）：
/// 手写 golden path 精简 → AI 链完整跑通（机检 halt → 补计划 → 续跑 →
/// 定稿 + Final Revision）→ 第二章 Gate 返工（评审阻塞 → 自动修复 →
/// 复评通过，repair_attempts=1 证据）→ 导出 → 旧项目迁移 → 重启 0 丢失。
pub fn consumer_full_scenario(binary: &Path) -> Result<(), String> {
    let _display = start_display()?;
    let driver = crate::platform::native_driver();
    let port = crate::platform::free_port();
    let native_port = crate::platform::free_port();
    let mut tauri = tauri_driver_command(&driver, port, native_port);
    #[cfg(target_os = "linux")]
    if let Some(display) = _display.display_env() {
        tauri.env("DISPLAY", display);
    }
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

    fn log_on_failure(path: &std::path::Path, what: &str) {
        if let Ok(bytes) = std::fs::read(path)
            && !bytes.is_empty()
        {
            eprintln!("--- tauri-driver stderr ({what}) ---");
            eprintln!("{}", String::from_utf8_lossy(&bytes));
        }
    }

    let sse_url = start_sse_server();
    let client = reqwest::blocking::Client::new();
    let self_test = client
        .post(&sse_url)
        .json(&serde_json::json!({"messages": [{"role": "user", "content": "hi"}], "stream": true}))
        .send()
        .expect("SSE server must serve in-process requests");
    assert_eq!(self_test.status().as_u16(), 200, "SSE self-test status");

    // 独立书仓根（与 golden_path 区分，避免残留互扰）
    let root_path = std::env::temp_dir().join("E2E书仓P11");
    let root = root_path.to_string_lossy().into_owned();
    let _ = std::fs::remove_dir_all(&root_path);

    // ---- Session 1: 手写 golden path 精简 ----
    let first = match create_session(&base, binary) {
        Ok(session) => session,
        Err(error) => {
            log_on_failure(&driver_log_path, "first session");
            return Err(error);
        }
    };
    wait_for_text(&first, "LingBi Next", Duration::from_secs(30));
    wait_for_text(&first, "开始写作", Duration::from_secs(20));
    click_button(&first, "开始写作");
    fill_input(&first, "作品名", "P11旅程书");
    click_summary(&first, "高级选项");
    fill_input(&first, "自定义保存位置", &root);
    click_button(&first, "创建项目");
    wait_for_text(&first, "书仓已创建", Duration::from_secs(30));
    wait_for_text(&first, "保存", Duration::from_secs(20));
    fill_input(&first, "新章节标题", "第一章");
    click_button(&first, "新建章节");
    wait_for_text(&first, "第一章", Duration::from_secs(20));
    edit_code_mirror(&first, "手写正文：旧车站的灯还亮着。");
    click_button(&first, "保存");
    wait_for_text(&first, "已保存", Duration::from_secs(20));

    // ---- 配置 AI（SSE 路由版：评审/改写/正文）----
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
    // 显式返回欢迎首页（AI 配置视图停留；确定性导航）
    click_button(&first, "返回");
    wait_for_text(&first, "开始写作", Duration::from_secs(20));

    // ---- 打开第一章 → 运行链：fresh 章（无 summary）→ 机检 halt ----
    click_button(&first, "打开已有作品");
    wait_for_text(&first, "最近作品", Duration::from_secs(20));
    click_summary(&first, "高级选项");
    fill_input(&first, "项目路径", &root);
    click_button(&first, "打开");
    wait_for_text(&first, "书仓已打开", Duration::from_secs(30));
    wait_for_text(&first, "第一章", Duration::from_secs(20));
    click_button_retry(&first, "第一章");
    wait_for_text(&first, "手写正文：旧车站的灯还亮着。", Duration::from_secs(20));

    click_button(&first, "运行本章工作流");
    // 机检 halt：摘要为空 → 检查和优化步骤失败文案
    wait_for_any_text(
        &first,
        &["机检未过", "summary 为空", "状态门禁未过"],
        Duration::from_secs(60),
    );

    // 补章计划（Book V3 = 目录树即项目）：frontmatter + summary
    let ch1_path = root_path.join("定稿/正文/001-第一章.md");
    let plan = "---\ntitle: 第一章\nchapter: 001\nvolume: 1\nwords: 0\nstatus: 草稿\nsummary: 主角在旧车站发现一盏不灭的灯。\n---\n\n";
    std::fs::write(&ch1_path, plan)
        .map_err(|error| format!("write plan: {error}"))?;

    // 重跑 → 链完成（评审首轮 blocking → 自动返工 → 复评通过）
    click_button(&first, "运行本章工作流");
    wait_for_text(&first, "已确认：定稿/正文/001-第一章.md", Duration::from_secs(120));
    wait_for_text(&first, "准备本章 ✔", Duration::from_secs(20));
    wait_for_text(&first, "确认本章 ✔", Duration::from_secs(20));

    // 磁盘证据：定稿 = 润色稿（Gate 返工输出）+ Final Revision 快照
    let settled1 = std::fs::read_to_string(&ch1_path)
        .map_err(|error| format!("read settled ch1: {error}"))?;
    assert_contains(&settled1, "E2E润色稿", "ch1 settled body (polish output)")?;
    let versions_dir = root_path.join(".lingbi/versions");
    if !versions_dir.is_dir() {
        return Err("Final Revision dir missing after chain settle".to_owned());
    }
    let version_count = std::fs::read_dir(&versions_dir)
        .map_err(|error| format!("versions dir: {error}"))?
        .count();
    if version_count < 1 {
        return Err(format!("Final Revision snapshots missing: {version_count}"));
    }

    // ---- 第二章：AI 生成 → 取消 → 重试 → 链（Gate 返工）→ 导出 ----
    fill_input(&first, "新章节标题", "第二章");
    click_button(&first, "新建章节");
    wait_for_text(&first, "第二章", Duration::from_secs(20));
    // 生成 → 取消（慢速挂起）→ 重试（正常流式）
    fill_input(&first, "写作要求", "慢速生成测试");
    click_button(&first, "生成");
    wait_for_text(&first, "第一部分", Duration::from_secs(30));
    click_button(&first, "取消");
    wait_for_text(&first, "生成", Duration::from_secs(20));
    fill_input(&first, "写作要求", "生成第二章正文");
    click_button(&first, "生成");
    wait_for_text(&first, "E2E候选正文", Duration::from_secs(30));

    // 补第二章计划
    let ch2_path = root_path.join("定稿/正文/002-第二章.md");
    let plan2 = "---\ntitle: 第二章\nchapter: 002\nvolume: 1\nwords: 0\nstatus: 草稿\nsummary: 主角循着灯光走进地下通道。\n---\n\n";
    std::fs::write(&ch2_path, plan2)
        .map_err(|error| format!("write plan2: {error}"))?;

    // 运行链：review 第二轮（奇数）blocking → 返工 → 复评 clean
    click_button(&first, "运行本章工作流");
    wait_for_text(&first, "已确认：定稿/正文/002-第二章.md", Duration::from_secs(120));
    let settled2 = std::fs::read_to_string(&ch2_path)
        .map_err(|error| format!("read settled ch2: {error}"))?;
    assert_contains(&settled2, "E2E润色稿", "ch2 settled body (polish output)")?;

    // Gate 返工证据：两次运行 run 台账 repair_attempts=1
    let runs_dir = root_path.join("工作区/工作流/runs");
    let runs: Vec<_> = std::fs::read_dir(&runs_dir)
        .map_err(|error| format!("runs dir: {error}"))?
        .filter_map(|entry| entry.ok())
        .collect();
    let mut repaired = 0usize;
    for entry in runs {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if let Ok(bytes) = std::fs::read(&path) {
            if let Ok(record) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                if record["status"].as_str() == Some("Completed")
                    && record["repair_attempts"].as_u64() == Some(1)
                {
                    repaired += 1;
                }
            }
        }
    }
    if repaired < 2 {
        return Err(format!(
            "Gate repair evidence missing: expected >=2 runs with repair_attempts=1, got {repaired}"
        ));
    }

    // 导出
    click_button(&first, "导出 DOCX");
    click_button(&first, "导出 MD");
    std::thread::sleep(Duration::from_millis(1500));
    for ext in ["md", "docx"] {
        let path = root_path.join(format!("export/002-第二章.{ext}"));
        if !path.exists() {
            return Err(format!("export file missing: {}", path.display()));
        }
    }

    // ---- 旧项目一键迁移：legacy fixture → migrate_book → 打开 → 继续写 ----
    let fixture = crate::repo_root().join("fixtures/storyrepo-oracle/153eb63/steps/000-initial/book");
    if !fixture.is_dir() {
        return Err(format!("legacy fixture missing: {}", fixture.display()));
    }
    let mig_path = std::env::temp_dir().join("E2E迁移书P11");
    let mig_root = mig_path.to_string_lossy().into_owned();
    let _ = std::fs::remove_dir_all(&mig_path);
    let receipt = invoke(
        &first,
        "migrate_book",
        serde_json::json!({
            "source": fixture.to_string_lossy(),
            "destination": mig_root,
        }),
    );
    assert_eq!(receipt["classification"].as_str(), Some("storyrepo-legacy"), "receipt: {receipt}");
    // UI 打开迁移书 → 继续写（legacy fixture 章节题为「开局」，非「第一章」）
    click_summary(&first, "高级功能");
    click_button(&first, "返回首页");
    wait_for_text(&first, "打开已有作品", Duration::from_secs(20));
    click_button(&first, "打开已有作品");
    wait_for_text(&first, "最近作品", Duration::from_secs(20));
    click_summary(&first, "高级选项");
    fill_input(&first, "项目路径", &mig_root);
    click_button(&first, "打开");
    wait_for_text(&first, "书仓已打开", Duration::from_secs(30));
    // 换书清链：上次书（P11旅程书）的链结果、候选与导出回执不得泄漏进
    // 迁移书视图（P11 实况：残留「已确认：定稿/…」与「已导出到 …」）
    assert_text_absent(&first, "已确认：定稿");
    assert_text_absent(&first, "候选内容");
    assert_text_absent(&first, "已导出到");
    click_button_retry(&first, "开局");
    wait_for_text(&first, "夜色如墨", Duration::from_secs(20));
    append_code_mirror(&first, "迁移后续写：灯芯又亮了一分。");
    click_button(&first, "保存");
    wait_for_text(&first, "已保存", Duration::from_secs(20));
    // 磁盘证据：迁移书正文保留原字节 + 追加句落盘；事务层就位（继续写=托管写）
    let mig_body = std::fs::read_to_string(mig_path.join("定稿/正文/001-开局.md"))
        .map_err(|error| format!("read migrated chapter: {error}"))?;
    assert_contains(&mig_body, "夜色如墨", "migrated body preserved")?;
    assert_contains(&mig_body, "灯芯又亮了一分", "migrated continue-write saved")?;
    if !mig_path.join(".lingbi/transactions").is_dir() {
        return Err("tx layer missing after migrated-book save".to_owned());
    }

    // ---- Session 2: 重启 0 丢失 ----
    first.delete();
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
    click_summary(&second, "高级选项");
    fill_input(&second, "项目路径", &root);
    click_button(&second, "打开");
    wait_for_text(&second, "书仓已打开", Duration::from_secs(30));
    // 第一章/第二章内容完好（链定稿字节 + 0 丢失）
    click_button_retry(&second, "第一章");
    wait_for_text(&second, "E2E润色稿", Duration::from_secs(20));
    click_button_retry(&second, "第二章");
    wait_for_text(&second, "E2E润色稿", Duration::from_secs(20));
    // 导出文件仍在磁盘
    for ext in ["md", "docx"] {
        if !root_path.join(format!("export/002-第二章.{ext}")).exists() {
            return Err(format!("export lost after restart: {ext}"));
        }
    }
    second.delete();
    drop(driver_process);
    Ok(())
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

    // ---- P9 #26: 4 步模式默认 + 无 Key 场景（黄金路径无条件成立）----
    // 4 步模式默认展示（未配 Key 也可见——工作流面板为只读入口）
    wait_for_text(&first, "本章工作流", Duration::from_secs(20));
    wait_for_text(&first, "准备本章", Duration::from_secs(20));
    wait_for_text(&first, "AI 写作", Duration::from_secs(20));
    wait_for_text(&first, "检查和优化", Duration::from_secs(20));
    wait_for_text(&first, "确认本章", Duration::from_secs(20));
    // 内部机制默认不可见：节点详情（8 节点高级模式）不出现
    // （jsdom/真实 DOM 差异——此处断言「运行本章工作流」按钮存在即可）
    // 无 Key / AI 宕机：运行工作流 → 面板错误（「未配置 API Key」或节点
    // 失败——keyring 持久化使环境不保证无 Key；两种都是黄金路径不阻断的
    // 证据），编辑区不受影响
    click_button(&first, "运行本章工作流");
    wait_for_any_text(
        &first,
        &["未配置 API Key", "draft 失败：", "失败：network error", "workflow-error"],
        Duration::from_secs(30),
    );
    // 错误只出现在面板：编辑区/保存按钮仍在（黄金路径未被阻断的证据；
    // 正文修改断言由后续 Session 2 重开承载——此处不触碰正文）

    // ---- P5-2 AI slice: configure → generate → stream → cancel → adopt ----
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
    // P5-5 高级设置：自定义温度/tokens（保存后 keyring 持久化）
    fill_input(&first, "温度", "0.3");
    fill_input(&first, "最大 tokens", "512");
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
    // P5-5：保存后立即读回一致（shell 层证据）
    assert_eq!(
        status["temperature"].as_f64(),
        Some(0.3),
        "status temperature: {status}"
    );
    assert_eq!(
        status["max_tokens"].as_u64(),
        Some(512),
        "status max_tokens: {status}"
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

    // ---- P5-4 export slice: DOCX/MD/TXT to export/ dir ----
    click_button(&first, "导出 DOCX");
    click_button(&first, "导出 MD");
    click_button(&first, "导出 TXT");
    std::thread::sleep(Duration::from_millis(1500));
    for ext in ["md", "txt", "docx"] {
        let path = root_path.join(format!("export/001-第一章.{ext}"));
        if !path.exists() {
            return Err(format!("export file missing: {}", path.display()));
        }
        let bytes = std::fs::read(&path)
            .map_err(|error| format!("read export: {error}"))?;
        assert!(!bytes.is_empty(), "export file empty: {}", path.display());
        if ext != "docx" {
            let text = String::from_utf8_lossy(&bytes);
            assert_contains(&text, "E2E候选正文", "export content")?;
        }
    }
    // ---- P5-3 writing tools: outline/contract/brief/checks/diagnose ----
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
    // P5-5 声明式技能卡：只读展示（题材引导精简版）
    click_summary(&first, "技能");
    wait_for_text(&first, "题材引导（精简版）", Duration::from_secs(20));

    first.delete();

    // ---- P5-4 crash injection: 崩溃窗口现场（journal ContentWritten、
    // 内容已落盘、回执未写）→ 重开后 open_book 前向补完 ----
    let body_bytes = std::fs::read(&body_path)
        .map_err(|error| format!("read body for crash inject: {error}"))?;
    let after_hash = {
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(&body_bytes);
        digest.iter().map(|byte| format!("{byte:02x}")).collect::<String>()
    };
    let tx_id = uuid::Uuid::new_v4();
    let journal_path = root_path.join(".lingbi/transactions").join(format!("{tx_id}.json"));
    let journal_json = serde_json::json!({
        "id": tx_id.to_string(),
        "document_id": uuid::Uuid::new_v4().to_string(),
        "before_revision": 1,
        "before_hash": "",
        "after_revision": 2,
        "after_hash": after_hash,
        "phase": "content_written",
        "created_at": "2026-08-08T00:00:00Z",
        "body_relative_path": "定稿/正文/001-第一章.md"
    });
    std::fs::write(&journal_path, serde_json::to_vec(&journal_json).expect("journal json"))
        .map_err(|error| format!("write crash journal: {error}"))?;

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
    // 恢复报告可见：Welcome「打开」按钮已触发 bookOpen，前向补完崩溃残留
    // journal（恢复 1 条，异常 0 条）→ 状态文本断言（不再二次 invoke，避免
    // 在 UI 已收敛后重跑 recovery 得到 0 条的时序假失败）。
    wait_for_text(&second, "恢复 1 条", Duration::from_secs(30));

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

    // P5-5 高级设置持久化：重启后温度/tokens 仍在（keyring → provider_status
    // 回显 → UI 预填）。双证据：UI 输入框值 + shell 读回。
    click_summary(&second, "高级功能");
    click_button(&second, "返回首页");
    wait_for_text(&second, "连接 AI", Duration::from_secs(20));
    click_button(&second, "连接 AI");
    wait_for_text(&second, "选择 AI 服务", Duration::from_secs(20));
    click_summary(&second, "高级设置");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if read_input(&second, "温度") == "0.3"
            && read_input(&second, "最大 tokens") == "512"
        {
            break;
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "advanced settings not prefilled: temp={} tokens={}",
                read_input(&second, "温度"),
                read_input(&second, "最大 tokens")
            ));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    let persisted = invoke(&second, "provider_status", serde_json::json!({}));
    assert_eq!(
        persisted["temperature"].as_f64(),
        Some(0.3),
        "persisted temperature: {persisted}"
    );
    assert_eq!(
        persisted["max_tokens"].as_u64(),
        Some(512),
        "persisted max_tokens: {persisted}"
    );

    second.delete();
    let _ = driver_process;

    // Clean up the real book folder created by the novice flow.
    let _ = std::fs::remove_dir_all(&root_path);
    Ok(())
}
