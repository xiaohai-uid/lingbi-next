//! brief 模块（P2-09）—— 与引擎 `brief.py` 等价：五段式章节写作任务书。
//!
//! 组装五段：①开篇委托 ②这章的故事 ③这章的人物 ④怎么写更顺 ⑤收在哪里。
//! contract/context 可显式传入（流水线复用），缺省内部调用 build_contract +
//! assemble_context 装配。纯文本输出，只读不写，缺文件容错。
//!
//! 与引擎的差异（有意，fail-closed 纪律）：
//! - 风格宪法读取失败（文件存在但不可读）引擎抛 UnicodeDecodeError/IOError 崩溃；
//!   本实现按「无风格宪法文件」容错——语料文件均由引擎 UTF-8 写出，不可达。
//! - `_distill` 用 `split('\n')`，引擎 `splitlines()`（认裸 `\r` 等 Unicode 换行）——
//!   关键词行与回退路径经 trim 后等价，语料均为 `\n`/`\r\n` 行尾，不可达。

use crate::config::load_config;
use crate::context::{assemble_context, Context};
use crate::contract::{build_contract, load_outline, Contract};
use crate::tracking::TrackingState;
use lingbi_contracts::AppError;
use regex::Regex;
use std::path::Path;

const STYLE_DIR: &str = "文风";
const STYLE_FILE: &str = "风格宪法.md";
const STYLE_LIMIT: usize = 200;
const PREV_ENDING_LIMIT: usize = 200;
const DISTILL_KEYS: [&str; 4] = ["状态", "驱动", "作用", "说话"];

/// 引擎 `_chapter_title`：细纲行标题 → 状态章节标题 → "第%d章"。
fn chapter_title(root: &Path, state: &TrackingState, ch: i64) -> String {
    let outline = load_outline(root);
    if let Some(row) = outline.iter().find(|r| r.ch == ch)
        && let Some(t) = row.title.as_deref()
        && !t.is_empty()
    {
        return t.to_owned();
    }
    if let Some(rec) = state.chapters.get(&ch.to_string())
        && !rec.title.is_empty()
    {
        return rec.title.clone();
    }
    format!("第{ch}章")
}

/// 引擎 `_style_head`：风格宪法前 200 字；缺文件 → "无风格宪法文件"。
fn style_head(root: &Path) -> String {
    let p = root.join(STYLE_DIR).join(STYLE_FILE);
    if !p.exists() {
        return "无风格宪法文件".to_owned();
    }
    let Ok(t) = std::fs::read_to_string(&p) else {
        return "无风格宪法文件".to_owned();
    };
    t.chars().take(STYLE_LIMIT).collect()
}

/// 引擎 `_split_entry`：解析 "【名】内容" 实体卡条目。
fn split_entry(entry: &str) -> (String, String) {
    let re = Regex::new(r"^【(.+?)】").expect("split entry regex");
    match re.captures(entry) {
        Some(m) => {
            let name = m.get(1).expect("group 1").as_str().to_owned();
            let rest = &entry[m.get(0).expect("group 0").end()..];
            (name, rest.to_owned())
        }
        None => (entry.to_owned(), String::new()),
    }
}

/// 引擎 `_distill`：从角色卡提炼 状态/驱动/作用/说话 行（前 6 行）；无关键词行给原文。
fn distill(name: &str, excerpt: &str) -> String {
    let lines: Vec<&str> = excerpt
        .split('\n')
        .filter(|l| DISTILL_KEYS.iter().any(|k| l.contains(k)))
        .map(str::trim)
        .collect();
    if !lines.is_empty() {
        format!("{name}: {}", lines.iter().take(6).copied().collect::<Vec<_>>().join("；"))
    } else {
        format!("{name}: {}", excerpt.chars().take(400).collect::<String>())
    }
}

/// 引擎 `_section`：段头 + 条目 + 空行。
fn section(lines: &mut Vec<String>, title: &str, items: &[String]) {
    lines.push(format!("## {title}"));
    lines.extend(items.iter().cloned());
    lines.push(String::new());
}

/// 五段任务书（引擎 `build_brief`）。contract/context 可显式传入（None 时内部装配）。
pub fn build_brief(
    root: &Path,
    state: &TrackingState,
    ch: i64,
    contract: Option<&Contract>,
    context: Option<&Context>,
) -> Result<String, AppError> {
    let owned_contract;
    let owned_context;
    let c = match contract {
        Some(c) => c,
        None => {
            owned_contract = build_contract(root, state, ch);
            &owned_contract
        }
    };
    let ctx = match context {
        Some(c) => c,
        None => {
            owned_context = assemble_context(root, state, ch);
            &owned_context
        }
    };

    let cfg = load_config(root)?;
    // 引擎 `cfg.get("name", "未命名")`：缺键时由 config.DEFAULTS 兜底（load_config
    // 同构）；显式空 `name:` 保持空串——两者 cfg.name 均已复刻，无需二次兜底。
    let book = cfg.name;
    let goal = if c.goal.is_empty() {
        "(无明确目标)".to_owned()
    } else {
        c.goal.clone()
    };
    let title = chapter_title(root, state, ch);
    let hard = &c.hard_constraints;
    let forbidden = &c.forbidden;
    let ending = &c.ending_requirement;
    let cards = &ctx.entity_cards;
    let prev_ending = ctx.recent_endings.last().cloned().unwrap_or_default();

    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("# 第{ch}章 写作任务书({book})"));
    lines.push(String::new());

    section(&mut lines, "一、开篇委托", &[
        format!("- 书名:{book}"),
        format!("- 章号:{ch}"),
        format!("- 标题:{title}"),
        format!("- 一句话目标:{goal}"),
    ]);

    let mut items2 = vec![
        format!(
            "- 前文摘要:{}",
            if ctx.summary_chain.is_empty() {
                "暂无前文摘要"
            } else {
                &ctx.summary_chain
            }
        ),
        format!("- 目标:{goal}"),
        format!("- 节点(细纲·要发生什么):{goal}"),
        format!(
            "- 必覆盖:{}",
            if hard.is_empty() {
                "无".to_owned()
            } else {
                hard.join("、")
            }
        ),
        format!(
            "- 禁区:{}",
            if forbidden.is_empty() {
                "无".to_owned()
            } else {
                forbidden.join("、")
            }
        ),
    ];
    if !cards.is_empty() {
        items2.push("- 检索线索(实体卡):".to_owned());
        items2.extend(cards.iter().map(|card| format!("  - {card}")));
    }
    section(&mut lines, "二、这章的故事", &items2);

    let items3: Vec<String> = if cards.is_empty() {
        vec!["- 无(未登记角色)".to_owned()]
    } else {
        cards
            .iter()
            .map(|card| {
                let (name, rest) = split_entry(card);
                format!("- {}", distill(&name, &rest))
            })
            .collect()
    };
    section(&mut lines, "三、这章的人物", &items3);

    section(&mut lines, "四、怎么写更顺", &[
        format!("- 风格宪法(前 {STYLE_LIMIT} 字):{}", style_head(root)),
        format!(
            "- 结尾埋伏笔要求:{}",
            if ending.is_empty() { "无明确要求" } else { ending.as_str() }
        ),
    ]);

    let mut items5 = vec![format!(
        "- 结尾要求:{}",
        if ending.is_empty() { "无明确要求" } else { ending.as_str() }
    )];
    if prev_ending.is_empty() {
        items5.push("- 上章结尾复述:尚无上章(本书开篇)".to_owned());
    } else {
        items5.push(format!(
            "- 上章结尾复述(供衔接):{}",
            prev_ending.chars().take(PREV_ENDING_LIMIT).collect::<String>()
        ));
    }
    section(&mut lines, "五、收在哪里", &items5);

    Ok(lines.join("\n"))
}
