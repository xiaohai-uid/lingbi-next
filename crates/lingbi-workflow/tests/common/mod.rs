//! P7 链测试公共设施：书仓 fixture + ScriptedProvider（按提示词路由响应）。
//!
//! 测试纪律（#24）：只测外部行为——跑链 → 断言书仓产物（定稿字节/追踪/
//! checkpoint/handoff）与记录字段（哈希/状态/AI 调用次数），不测内部实现。

use lingbi_ai::{AiEvent, AiError, AiProvider, AiStream, ChatRequest, TokenUsage};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// 脚本化 provider：按 user 提示词路由响应（评审/改写/其余），可计数调用、
/// 可配置 provider/model 快照、可按标记注入失败。
pub struct ScriptedProvider {
    script: Arc<dyn Fn(&ChatRequest) -> String + Send + Sync>,
    fail_marker: Option<String>,
    provider_id: String,
    model_id: String,
    calls: Arc<AtomicUsize>,
}

impl ScriptedProvider {
    pub fn new(script: impl Fn(&ChatRequest) -> String + Send + Sync + 'static) -> Self {
        Self {
            script: Arc::new(script),
            fail_marker: None,
            provider_id: "scripted".to_owned(),
            model_id: "scripted-model".to_owned(),
            calls: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn with_ids(mut self, provider_id: &str, model_id: &str) -> Self {
        self.provider_id = provider_id.to_owned();
        self.model_id = model_id.to_owned();
        self
    }

    /// user/system 内容含标记时节点注入 AiError::Network（模拟节点失败）。
    pub fn fail_on(mut self, marker: &str) -> Self {
        self.fail_marker = Some(marker.to_owned());
        self
    }

    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl AiProvider for ScriptedProvider {
    fn provider_id(&self) -> &str {
        &self.provider_id
    }

    fn model_id(&self) -> &str {
        &self.model_id
    }

    fn stream_chat(&self, request: ChatRequest) -> AiStream {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let response = (self.script)(&request);
        let fail = self
            .fail_marker
            .as_ref()
            .map(|m| {
                request
                    .messages
                    .iter()
                    .any(|msg| msg.content.contains(m))
            })
            .unwrap_or(false);
        Box::pin(async_stream::stream! {
            if fail {
                yield Err(AiError::Network);
                return;
            }
            yield Ok(AiEvent::ContentDelta(response));
            yield Ok(AiEvent::Usage(TokenUsage {
                prompt_tokens: 11,
                completion_tokens: 22,
            }));
            yield Ok(AiEvent::Completed);
        })
    }
}

/// 标准脚本：草稿文本 / 评审 JSON / 润色文本 按提示词路由。
pub fn standard_script(
    review_json: &'static str,
    polish_text: &'static str,
    draft_text: &'static str,
) -> impl Fn(&ChatRequest) -> String {
    move |req| {
        let user = req
            .messages
            .last()
            .map(|m| m.content.clone())
            .unwrap_or_default();
        if user.contains("改写") {
            polish_text.to_owned()
        } else if user.contains("评审") {
            review_json.to_owned()
        } else {
            draft_text.to_owned()
        }
    }
}

/// 建书仓 fixture（managed/legacy 两态）。
/// 章文件 frontmatter 完整（title/chapter/volume/words/status/summary——
/// prewrite 门禁摘要项必需；`summary: None` 传 None 则构造 halt 场景）。
pub fn init_book(dir: &Path, managed: bool, chapter_title: &str, summary: Option<&str>) {
    for d in ["大纲", "定稿/正文", "文风", "工作区", "追踪"] {
        std::fs::create_dir_all(dir.join(d)).expect("dir");
    }
    std::fs::write(
        dir.join("book.yaml"),
        "---\nname: 测试书\ntotal_chapters: 1\nmin_words_per_chapter: 10\nmax_words_per_chapter: 300\ntarget_words: 100000\n---\n",
    )
    .expect("book.yaml");
    std::fs::write(dir.join("大纲/章纲.md"), "# 第一章\n\n主角开局遇险。\n").expect("outline");
    std::fs::write(dir.join("文风/风格宪法.md"), "简洁叙事。\n").expect("style");
    let mut fm = format!(
        "---\ntitle: {chapter_title}\nchapter: 001\nvolume: 1\nwords: 0\nstatus: 草稿\n"
    );
    if let Some(s) = summary {
        fm.push_str(&format!("summary: {s}\n"));
    }
    fm.push_str("---\n\n");
    let chapter = dir.join("定稿/正文").join(format!("001-{chapter_title}.md"));
    std::fs::write(&chapter, fm).expect("chapter");
    if managed {
        lingbi_book::tx::ensure_tx_layer(dir).expect("tx layer");
    }
    lingbi_book::tracking::init_state(dir).expect("init state");
}

/// 标准草稿文本（无占位符/无红线，可通过草稿机检）。
pub const DRAFT: &str = "夜色笼罩山城，主角推开木门踏入雨幕。";
/// 无阻塞评审 JSON。
pub const REVIEW_OK: &str = r#"{"issues":[]}"#;
/// 阻塞评审 JSON（critical 强制 blocking）。
pub const REVIEW_BLOCKING: &str = r#"{"issues":[{"severity":"critical","category":"logic","location":"L3","description":"前后矛盾","evidence":"前文说 A","fix_hint":"改为 B"}]}"#;
pub const POLISH: &str = "夜色中，主角在雨幕里疾行，思绪翻涌。";
