//! P7-3 (#42) 真实《零界道种》全链验收 runner。
//!
//! 用法：cargo run -p lingbi-workflow --example chain_real -- <root> <ch>
//!
//! 对真实书仓副本跑 8 节点链：确定性节点（contract/brief/machine-check/
//! settlement/handoff）输出真实引擎字节；AI 节点（draft/review/polish）
//! 注入脚本化 provider（如实记录——AI 内容非真实模型产物，见票据证据）。
//! 退出码：0 = 链 Completed；1 = Halted/Failed（stderr 打印报告）。

use lingbi_ai::{AiEvent, AiProvider, AiStream, ChatRequest};
use lingbi_workflow::run_chain;
use std::sync::Arc;

/// 脚本化 provider：按提示词路由（评审 → 空评审 JSON；其余 → 正文文本）。
struct ScriptedProvider {
    draft_text: String,
}

impl ScriptedProvider {
    fn new(draft_text: &str) -> Self {
        Self {
            draft_text: draft_text.to_owned(),
        }
    }
}

impl AiProvider for ScriptedProvider {
    fn provider_id(&self) -> &str {
        "scripted-injected"
    }

    fn model_id(&self) -> &str {
        "injected-provider"
    }

    fn stream_chat(&self, request: ChatRequest) -> AiStream {
        let user = request
            .messages
            .last()
            .map(|m| m.content.clone())
            .unwrap_or_default();
        let response = if user.contains("评审") {
            r#"{"issues":[]}"#.to_owned()
        } else if user.contains("改写") {
            self.draft_text.clone()
        } else {
            self.draft_text.clone()
        };
        Box::pin(async_stream::stream! {
            yield Ok(AiEvent::ContentDelta(response));
            yield Ok(AiEvent::Usage(lingbi_ai::TokenUsage {
                prompt_tokens: 100,
                completion_tokens: 200,
            }));
            yield Ok(AiEvent::Completed);
        })
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let root = args.next().expect("usage: chain_real <root> <ch>");
    let ch: i64 = args
        .next()
        .expect("usage: chain_real <root> <ch>")
        .parse()
        .expect("ch 为整数");

    let draft = "灰烬渐冷,陆沉舟把最后一捧灰土拍进树根下的浅坑。\n\n田师蹲在坑沿,指尖捻起一粒灰里的白点,凑近灯火看了半晌,忽然说:\u{201c}有籽。\u{201d}\n\n陆沉舟没应声,只把水壶里的青水河水分作两半,一半浇进坑里,一半留给灯童。夜风从塔基的裂口灌进来,把灰烬吹得打旋。\n\n远处传来肃界卫巡夜的号角,三长两短。\n\n\u{201c}你守田,\u{201d}灯童在塔影里说,\u{201c}我守灯。\u{201d}";

    let rt = tokio::runtime::Runtime::new().expect("runtime");
    let report = rt.block_on(run_chain(
        std::path::Path::new(&root),
        ch,
        Arc::new(ScriptedProvider::new(draft)),
    ));
    match report {
        Ok(r) => {
            let json = serde_json::to_string_pretty(&r).expect("json");
            println!("{json}");
            if r.status == lingbi_workflow::ChainStatus::Completed {
                std::process::exit(0);
            } else {
                eprintln!("CHAIN NOT COMPLETED: {:?}", r.status);
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("run_chain 错误: {e}");
            std::process::exit(2);
        }
    }
}
