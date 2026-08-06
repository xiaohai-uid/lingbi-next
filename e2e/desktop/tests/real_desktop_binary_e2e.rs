use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct WebDriver {
    base: String,
    session: String,
    client: reqwest::blocking::Client,
}

impl WebDriver {
    fn new(base: String, session: String) -> Self {
        Self {
            base,
            session,
            client: reqwest::blocking::Client::new(),
        }
    }

    fn execute(&self, script: &str) -> Value {
        self.post(
            &format!("/session/{}/execute/sync", self.session),
            json!({"script": script, "args": []}),
        )
    }

    fn execute_async(&self, script: &str, args: Vec<Value>) -> Value {
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

    fn delete(&self) {
        let _ = self
            .client
            .delete(format!("{}/session/{}", self.base, self.session))
            .send();
    }
}

fn create_session(base: &str, binary: &Path) -> WebDriver {
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
        .expect("create session");
    let value: Value = response.json().expect("session json");
    let value = value.get("value").cloned().unwrap_or(value);
    let session = value
        .get("sessionId")
        .and_then(Value::as_str)
        .expect("session id")
        .to_owned();
    WebDriver::new(base.to_owned(), session)
}

fn wait_until(
    driver: &WebDriver,
    script: &str,
    timeout: Duration,
    ready: &str,
) -> Value {
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

fn click_button(driver: &WebDriver, text: &str) {
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

fn set_welcome_inputs(driver: &WebDriver, root: &str) {
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

fn wait_for_text(driver: &WebDriver, text: &str, timeout: Duration) {
    let script = format!(
        r#"
        return document.body.innerText.includes({text:?}) ? 'ok' : document.body.innerText;
        "#
    );
    wait_until(driver, &script, timeout, "ok");
}

fn edit_code_mirror(driver: &WebDriver, text: &str) {
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

fn configure_provider(driver: &WebDriver, base_url: &str) {
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

fn session_state(driver: &WebDriver) -> Value {
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
            hash: doc.content_hash
          })));
        }).catch((error) => done('error:' + error));
        "#;
    let value = driver.execute_async(script, vec![]);
    let serialized = value.as_str().expect("session state string");
    serde_json::from_str(serialized).unwrap_or_else(|error| {
        panic!("session state json: {error}: {serialized}")
    })
}

fn chapter_state(driver: &WebDriver, title: &str) -> Value {
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
    serde_json::from_str(serialized).unwrap_or_else(|error| {
        panic!("chapter state json: {error}: {serialized}")
    })
}

fn start_sse_server() -> String {
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
        if buffer[..read].windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    let body = "data: {\"choices\":[{\"delta\":{\"content\":\"E2E候选正文\"}}]}\n\ndata: [DONE]\n\n";
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )?;
    stream.flush()
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

#[test]
fn real_tauri_release_binary_e2e() {
    let repo = repo_root();
    let binary = repo.join("apps/desktop/src-tauri/target/release/lingbi-desktop");
    assert!(
        binary.exists(),
        "release binary missing; run pnpm tauri build --no-bundle first"
    );

    let temp = TempDir::new().expect("temp");
    let root = temp.path().join("e2e-novel");
    let sse_url = start_sse_server();
    // TCP-only Xvfb avoids depending on a writable /tmp/.X11-unix, which is
    // often read-only or misowned in sandboxed CI environments.
    let server_display = ":97";
    let client_display = "127.0.0.1:97";
    let xvfb = ChildGuard(
        Command::new("Xvfb")
            .args([
                server_display,
                "-screen",
                "0",
                "1280x800x24",
                "-nolisten",
                "unix",
                "-nolisten",
                "local",
                "-listen",
                "tcp",
            ])
            .spawn()
            .expect("Xvfb"),
    );
    let native_driver = std::env::var("LINGBI_WEBKIT_DRIVER")
        .unwrap_or_else(|_| "/home/a1691/.local/webdriver/usr/bin/WebKitWebDriver".to_owned());
    let driver_process = ChildGuard(
        Command::new("tauri-driver")
            .args([
                "--port",
                "4444",
                "--native-port",
                "4445",
                "--native-driver",
                native_driver.as_str(),
            ])
            .env("DISPLAY", client_display)
            .env("WEBKIT_DISABLE_COMPOSITING_MODE", "1")
            .spawn()
            .expect("tauri-driver"),
    );
    let _ = (&xvfb, &driver_process);

    let base = "http://127.0.0.1:4444";
    let started = Instant::now();
    loop {
        if reqwest::blocking::Client::new()
            .get(format!("{base}/status"))
            .send()
            .is_ok()
        {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(15),
            "tauri-driver did not start"
        );
        thread::sleep(Duration::from_millis(300));
    }

    let first = create_session(base, &binary);
    wait_for_text(&first, "LingBi Next", Duration::from_secs(30));
    set_welcome_inputs(&first, root.to_str().expect("utf8"));
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
    assert_eq!(state["documents"].as_array().expect("documents").len(), 2);
    assert!(
        state["content"].as_str().expect("content").contains("E2E候选正文"),
        "adopted content: {}",
        state["content"]
    );
    assert_eq!(state["revision"].as_u64(), Some(2));
    assert_eq!(state["hash"].as_str().expect("hash").len(), 64);
    first.delete();
    thread::sleep(Duration::from_secs(1));

    let second = create_session(base, &binary);
    wait_for_text(&second, "LingBi Next", Duration::from_secs(30));
    set_welcome_inputs(&second, root.to_str().expect("utf8"));
    click_button(&second, "打开项目");
    wait_for_text(&second, "保存", Duration::from_secs(30));
    let reopened = session_state(&second);
    assert_eq!(
        reopened["documents"].as_array().expect("documents").len(),
        2
    );
    let reopened_chapter = chapter_state(&second, "第二章");
    assert!(
        reopened_chapter["content"]
            .as_str()
            .expect("content")
            .contains("E2E候选正文"),
        "reopened chapter content: {}",
        reopened_chapter["content"]
    );
    assert_eq!(reopened_chapter["revision"].as_u64(), Some(2));
    second.delete();
}
