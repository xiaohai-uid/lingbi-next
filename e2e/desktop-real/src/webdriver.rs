//! WebDriver client and UI helpers for driving the real LingBi binary.

use serde_json::{Value, json};
use std::path::Path;
use std::time::{Duration, Instant};

/// Blocking WebDriver client for a single session.
pub struct WebDriver {
    base: String,
    session: String,
    client: reqwest::blocking::Client,
}

impl WebDriver {
    pub fn new(base: String, session: String) -> Self {
        Self {
            base,
            session,
            client: reqwest::blocking::Client::new(),
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session
    }

    pub fn execute(&self, script: &str) -> Value {
        self.post(
            &format!("/session/{}/execute/sync", self.session),
            json!({"script": script, "args": []}),
        )
    }

    pub fn execute_async(&self, script: &str, args: Vec<Value>) -> Value {
        self.post(
            &format!("/session/{}/execute/async", self.session),
            json!({"script": script, "args": args}),
        )
    }

    fn post(&self, path: &str, body: Value) -> Value {
        let response = self
            .client
            .post(format!("{}{path}", self.base))
            .json(&body)
            .send()
            .expect("webdriver post");
        let value: Value = response.json().expect("webdriver json");
        value.get("value").cloned().unwrap_or(value)
    }

    pub fn delete(&self) {
        let _ = self
            .client
            .delete(format!("{}/session/{}", self.base, self.session))
            .send();
    }
}

/// Create a WebDriver session that launches `binary` via tauri-driver.
pub fn create_session(base: &str, binary: &Path) -> Result<WebDriver, String> {
    let client = reqwest::blocking::Client::new();
    let body = json!({
        "capabilities": {
            "alwaysMatch": {
                "tauri:options": {
                    "application": binary.to_string_lossy(),
                    "args": []
                }
            }
        },
        "desiredCapabilities": {
            "tauri:options": {
                "application": binary.to_string_lossy(),
                "args": []
            }
        }
    });
    let response = client
        .post(format!("{base}/session"))
        .json(&body)
        .send()
        .map_err(|error| format!("create session request failed: {error}"))?;
    let value: Value = response
        .json()
        .map_err(|error| format!("session json: {error}"))?;
    let value = value.get("value").cloned().unwrap_or(value);
    let session = value
        .get("sessionId")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("no sessionId in response: {value}"))?
        .to_owned();
    Ok(WebDriver::new(base.to_owned(), session))
}

/// Wait until `script` returns `ready`, panicking after `timeout`.
pub fn wait_until(driver: &WebDriver, script: &str, timeout: Duration, ready: &str) -> Value {
    let started = Instant::now();
    loop {
        let value = driver.execute(script);
        if value.as_str() == Some(ready) {
            return value;
        }
        assert!(
            started.elapsed() < timeout,
            "timed out waiting for {ready}: {value}"
        );
        std::thread::sleep(Duration::from_millis(500));
    }
}

pub fn click_button(driver: &WebDriver, text: &str) {
    let script = format!(
        r#"
        const buttons = Array.from(document.querySelectorAll('button'));
        const button = buttons.find((item) => item.textContent?.includes({text:?}));
        if (!button) return 'missing';
        button.click();
        return 'ok';
        "#
    );
    let value = driver.execute(&script);
    assert_eq!(value.as_str(), Some("ok"), "click button {text}: {value}");
}

/// 重试式点击（P11 #28）：React 异步重渲染窗口内按钮可能短暂缺席，
/// 最多重试 3 次（300ms 间隔）。
pub fn click_button_retry(driver: &WebDriver, text: &str) {
    let mut last: serde_json::Value = serde_json::Value::Null;
    for _ in 0..8 {
        let script = format!(
            r#"
        const buttons = Array.from(document.querySelectorAll('button'));
        const button = buttons.find((item) => item.textContent?.includes({text:?}));
        if (!button) return 'missing';
        button.click();
        return 'ok';
        "#
        );
        last = driver.execute(&script);
        if last.as_str() == Some("ok") {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    // 失败现场：页面文本 + 全部按钮文本（区分「按钮确实不存在」与
    // 「文本在非按钮元素上」，P11 #28 诊断）
    let scene = driver.execute(
        r#"
        const buttons = Array.from(document.querySelectorAll('button')).map((item) => item.textContent?.trim());
        return JSON.stringify({ buttons, body: document.body.innerText.slice(0, 400) });
        "#,
    );
    panic!("click button retry {text} failed: {last}; scene: {scene}");
}

pub fn wait_for_text(driver: &WebDriver, text: &str, timeout: Duration) {
    let script = format!(
        r#"
        return document.body.innerText.includes({text:?}) ? 'ok' : document.body.innerText;
        "#
    );
    wait_until(driver, &script, timeout, "ok");
}

pub fn edit_code_mirror(driver: &WebDriver, text: &str) {
    let text_json = serde_json::to_string(text).expect("json");
    let script = format!(
        r#"
        const content = document.querySelector('.cm-content');
        if (!content) return 'missing';
        content.focus();
        const selection = window.getSelection();
        const range = document.createRange();
        range.selectNodeContents(content);
        selection.removeAllRanges();
        selection.addRange(range);
        document.execCommand('insertText', false, {text_json});
        return document.body.innerText.includes({text_json}) ? 'ok' : 'wait';
        "#
    );
    wait_until(driver, &script, Duration::from_secs(10), "ok");
}

/// 追加式写入（P11 #28 迁移书续写）：不覆盖正文，仅把选区折叠到末尾后
/// 插入——迁移书已有正文不能被整段替换。
pub fn append_code_mirror(driver: &WebDriver, text: &str) {
    let text_json = serde_json::to_string(text).expect("json");
    let script = format!(
        r#"
        const content = document.querySelector('.cm-content');
        if (!content) return 'missing';
        content.focus();
        const selection = window.getSelection();
        const range = document.createRange();
        range.selectNodeContents(content);
        range.collapse(false);
        selection.removeAllRanges();
        selection.addRange(range);
        document.execCommand('insertText', false, {text_json});
        return document.body.innerText.includes({text_json}) ? 'ok' : 'wait';
        "#
    );
    wait_until(driver, &script, Duration::from_secs(10), "ok");
}

/// 单次断言页面不包含某文本（P11 #28 换书清链：上次书的链/候选不得泄漏）。
/// 书仓已打开的状态文本出现时，store 的 set 已同步完成，单次检查即可。
pub fn assert_text_absent(driver: &WebDriver, text: &str) {
    let script = format!(
        r#"
        return document.body.innerText.includes({text:?}) ? 'present' : 'absent';
        "#
    );
    let value = driver.execute(&script);
    assert_eq!(
        value.as_str(),
        Some("absent"),
        "stale text still on page: {text}"
    );
}

pub fn session_state(driver: &WebDriver) -> Value {
    let script = r#"
        const done = arguments[arguments.length - 1];
        window.__TAURI_INTERNALS__.invoke('project_get_session').then((session) => {
          const doc = session.current_document;
          return Promise.all([
            window.__TAURI_INTERNALS__.invoke('document_list'),
            window.__TAURI_INTERNALS__.invoke('document_open', { documentId: doc.id })
          ]).then(([documents, content]) => done(JSON.stringify({
            documents,
            content,
            revision: doc.revision,
            hash: doc.content_hash,
            root: session.root
          })));
        }).catch((error) => done('error:' + error));
        "#;
    let value = driver.execute_async(script, vec![]);
    let serialized = value.as_str().expect("session state string");
    serde_json::from_str(serialized)
        .unwrap_or_else(|error| panic!("session state json: {error}: {serialized}"))
}

pub fn chapter_state(driver: &WebDriver, title: &str) -> Value {
    let title_json = serde_json::to_string(title).expect("json");
    let script = format!(
        r#"
        const done = arguments[arguments.length - 1];
        window.__TAURI_INTERNALS__.invoke('document_list').then((documents) => {{
          const doc = documents.find((item) => item.title === {title_json});
          if (!doc) return done('missing:' + documents.map((item) => item.title).join(','));
          return window.__TAURI_INTERNALS__.invoke('document_open', {{ documentId: doc.id }}).then((content) => done(JSON.stringify({{
            documents,
            content,
            revision: doc.revision,
            hash: doc.content_hash
          }}))).catch((error) => done('error:' + error));
        }}).catch((error) => done('error:' + error));
        "#
    );
    let value = driver.execute_async(&script, vec![]);
    let serialized = value.as_str().expect("chapter state string");
    serde_json::from_str(serialized)
        .unwrap_or_else(|error| panic!("chapter state json: {error}: {serialized}"))
}

/// Book V3 状态（P5-1）：追踪状态 + 指定章正文（open_chapter 命令）。
/// `ch` 为 None 时取追踪状态第一章（若存在）。
pub fn book_state(driver: &WebDriver, ch: Option<i64>) -> Value {
    let script = r#"
        const done = arguments[arguments.length - 1];
        window.__TAURI_INTERNALS__.invoke('get_tracking_state').then((tracking) => {
          const chapters = Object.keys(tracking.chapters || {}).map(Number).sort((a, b) => a - b);
          const target = {targetCh} ?? chapters[0] ?? null;
          if (target === null) return done(JSON.stringify({ state_revision: tracking.state_revision, chapters, content: null }));
          return window.__TAURI_INTERNALS__.invoke('open_chapter', { ch: target }).then((chapter) => done(JSON.stringify({
            state_revision: tracking.state_revision,
            chapters,
            ch: target,
            title: chapter.title,
            content: chapter.body,
            content_hash: chapter.content_hash
          }))).catch((error) => done('error:' + error));
        }).catch((error) => done('error:' + error));
        "#;
    let target = match ch {
        Some(n) => n.to_string(),
        None => "null".to_owned(),
    };
    let script = script.replace("{targetCh}", &target);
    let value = driver.execute_async(&script, vec![]);
    let serialized = value.as_str().expect("book state string");
    serde_json::from_str(serialized)
        .unwrap_or_else(|error| panic!("book state json: {error}: {serialized}"))
}
