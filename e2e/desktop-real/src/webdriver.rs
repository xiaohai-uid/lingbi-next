//! WebDriver client and UI helpers for driving the real LingBi binary.

use serde_json::{Value, json};
use std::error::Error;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::thread;
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
///
/// `webdriver_udd` aligns the WebView2 user data folder with the directory
/// msedgedriver reads `DevToolsActivePort` from. msedgedriver 150+ launches
/// WebView2 apps with `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` (no
/// `--user-data-dir` on the command line) and then waits for the
/// `DevToolsActivePort` file inside the `--user-data-dir` value. The WebView2
/// runtime writes that file into the environment's user data folder, so both
/// sides must point at the same directory or session creation hangs ~60s and
/// dies with `DevToolsActivePort file doesn't exist` (surfaced as
/// `hyper::Error(IncompleteMessage)`).
pub fn create_session(
    base: &str,
    binary: &Path,
    webdriver_udd: Option<&Path>,
) -> Result<WebDriver, String> {
    let client = reqwest::blocking::Client::new();
    let mut args: Vec<String> = Vec::new();
    if let Some(udd) = webdriver_udd {
        args.push(format!("--user-data-dir={}", udd.display()));
    }
    let body = json!({
        "capabilities": {
            "alwaysMatch": {
                "tauri:options": {
                    "application": binary.to_string_lossy(),
                    "args": &args
                }
            }
        },
        "desiredCapabilities": {
            "tauri:options": {
                "application": binary.to_string_lossy(),
                "args": &args
            }
        }
    });
    let response = client
        .post(format!("{base}/session"))
        .json(&body)
        .send()
        .map_err(|error| format_error("create session request failed", &error))?;
    let value: Value = response
        .json()
        .map_err(|error| format_error("session json", &error))?;
    let value = value.get("value").cloned().unwrap_or(value);
    let session = value
        .get("sessionId")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("no sessionId in response: {value}"))?
        .to_owned();
    Ok(WebDriver::new(base.to_owned(), session))
}

/// Format a request error including the underlying cause chain, so failures
/// like `hyper::Error(IncompleteMessage)` are not swallowed.
fn format_error(what: &str, error: &reqwest::Error) -> String {
    let mut message = format!("{what}: {error}");
    if let Some(source) = error.source() {
        message.push_str(&format!(" ({source})"));
    }
    message
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
        thread::sleep(Duration::from_millis(500));
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

/// Start a local SSE chat-completions server; returns its base URL.
pub fn start_sse_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind sse");
    let port = listener.local_addr().expect("local addr").port();
    thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let _ = handle_sse(&mut stream);
        }
    });
    format!("http://127.0.0.1:{port}/v1/chat/completions")
}

fn handle_sse(stream: &mut TcpStream) -> std::io::Result<()> {
    let mut buffer = [0u8; 4096];
    let mut read = 0;
    loop {
        let count = stream.read(&mut buffer[read..])?;
        if count == 0 {
            break;
        }
        read += count;
        if buffer[..read]
            .windows(4)
            .any(|window| window == b"\r\n\r\n")
        {
            break;
        }
    }
    let body =
        "data: {\"choices\":[{\"delta\":{\"content\":\"E2E候选正文\"}}]}\n\ndata: [DONE]\n\n";
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )?;
    stream.flush()
}
