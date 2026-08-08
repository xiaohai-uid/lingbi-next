//! context 模块（P2-08）—— 与引擎 `context.py` 等价：四路上下文装配 + 8KB 预算判定。
//!
//! assemble_context 汇总四路上下文（recent_endings / summary_chain / entity_cards /
//! next_chapter_injection）+ budget_kb；budget_check 判四路合计 ≤ 8KB。
//! 预算口径：UTF-8 字节估算（中文 1 字 ≈ 3 字节），四路合计 > 8KB 视为超预算
//! （各路上限 800/1000/400/150 字是软上限，预算为最终硬闸）。
//!
//! 与引擎的差异（有意，fail-closed 纪律）：
//! - `round(x, 2)` 与 `"%.1f"` 均为 Python 银行家舍入（half-even）；本实现用
//!   `round_ties_even` 在相同 IEEE754 浮点运算路径上复刻（0.xx5 显示边界，见
//!   budget_125 场景锁定）。
//! - 行切分与非 UTF-8 降级同 contract 模块（`\n` 切行、读取失败按缺失处理）——
//!   语料均为引擎 UTF-8 写出，等价范围内不可达。

use crate::contract::{load_outline, split_list, OutlineRow};
use crate::state::{body, chapter_files, chapter_num, BODY_DIR};
use crate::tracking::TrackingState;
use regex::Regex;
use std::path::Path;

pub const BUDGET_BYTES: usize = 8 * 1024;
const BANNER_RE: &str = r"^[—–-]{2,}";

const ENDINGS_COUNT: i64 = 3; // 最近 3 章
const END_LIMIT: usize = 800; // 末段 ≤800 字
const SUMMARY_COUNT: i64 = 5; // 最近 5 篇章摘要
const SUMMARY_LIMIT: usize = 1000; // 拼接 ≤1000 字
const CARD_COUNT: usize = 3; // 实体卡 ≤3 张
const CARD_LIMIT: usize = 400; // 每张前 400 字
const INJECT_LIMIT: usize = 150; // 下章注入 ≤150 字

/// 第 ch 章写作上下文（键序与引擎 dict 一致）。
#[derive(Debug, Clone, PartialEq)]
pub struct Context {
    pub recent_endings: Vec<String>,
    pub summary_chain: String,
    pub entity_cards: Vec<String>,
    pub next_chapter_injection: String,
    pub budget_kb: f64,
}

/// 引擎 `_paragraphs`：按 `\n\s*\n` 切段，strip 后滤空。
fn paragraphs(text: &str) -> Vec<&str> {
    let re = Regex::new(r"\n\s*\n").expect("paragraph regex");
    re.split(text)
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect()
}

/// 引擎 `_last_ending`：正文末段（去 "——" 横幅），≤limit 字。
fn last_ending(text: &str) -> String {
    let mut paras = paragraphs(text);
    let banner = Regex::new(BANNER_RE).expect("banner regex");
    while let Some(last) = paras.last() {
        if banner.is_match(last) {
            paras.pop();
        } else {
            break;
        }
    }
    let Some(last) = paras.last() else {
        return String::new();
    };
    last.trim_matches(|c: char| matches!(c, '—' | '–' | '-' | ' ' | '\t'))
        .chars()
        .take(END_LIMIT)
        .collect()
}

/// 引擎 `_chapter_body`：按章号取正文；无此章返回 ""。
fn chapter_body(root: &Path, n: i64) -> String {
    for f in chapter_files(root) {
        if chapter_num(&f) == n {
            return body(&root.join(BODY_DIR).join(&f)).unwrap_or_default();
        }
    }
    String::new()
}

/// 引擎 `_recent_endings`：最近 ≤3 章结尾。
fn recent_endings(root: &Path, ch: i64) -> Vec<String> {
    let mut out = Vec::new();
    for n in i64::max(1, ch - ENDINGS_COUNT)..ch {
        let e = last_ending(&chapter_body(root, n));
        if !e.is_empty() {
            out.push(e);
        }
    }
    out
}

/// 引擎 `_summary_chain`：最近 ≤5 篇章摘要（去 # 行），拼接 ≤1000 字。
fn summary_chain(root: &Path, ch: i64) -> String {
    let mut parts = Vec::new();
    for n in i64::max(1, ch - SUMMARY_COUNT)..ch {
        let p = root
            .join("定稿")
            .join("记忆")
            .join("章摘要")
            .join(format!("{n:03}.md"));
        if !p.exists() {
            continue;
        }
        let Ok(t) = std::fs::read_to_string(&p) else {
            continue;
        };
        let body = t
            .lines()
            .filter(|l| !l.trim().starts_with('#'))
            .collect::<Vec<_>>()
            .join("\n");
        if !body.trim().is_empty() {
            parts.push(body.trim().to_owned());
        }
    }
    parts.join("\n").chars().take(SUMMARY_LIMIT).collect()
}

/// 引擎 `_entity_cards`：细纲行 "角色" 列（若有）或 state.entity_states 前 3，
/// 读角色卡前 400 字；缺卡占位（内嵌绝对路径，与引擎 os.path.join 一致——
/// 因此角色目录用组件序 join 而非 "定稿/设定/角色" 单段，保证反斜杠展示）。
fn entity_cards(root: &Path, state: &TrackingState, ch: i64) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let outline = load_outline(root);
    if let Some(row) = outline.iter().find(|r| r.ch == ch)
        && let Some(roles) = row.roles.as_deref()
        && !roles.is_empty()
    {
        names = split_list(roles).into_iter().take(CARD_COUNT).collect();
    }
    if names.is_empty() {
        names = state
            .entity_states
            .keys()
            .take(CARD_COUNT)
            .cloned()
            .collect();
    }
    let mut cards = Vec::new();
    for name in names {
        let p = root
            .join("定稿")
            .join("设定")
            .join("角色")
            .join(format!("{name}.md"));
        if !p.exists() {
            cards.push(format!("【{name}】(角色卡缺失:{})", p.display()));
            continue;
        }
        let t = std::fs::read_to_string(&p).unwrap_or_default();
        cards.push(format!(
            "【{name}】{}",
            t.chars().take(CARD_LIMIT).collect::<String>()
        ));
    }
    cards
}

/// 细纲行规范键取值（引擎 `row.get(k) or ""` 的 strip）。
fn field_str(row: &OutlineRow, key: &str) -> String {
    let v: Option<&str> = match key {
        "goal" => row.goal.as_deref(),
        "suspense" => row.suspense.as_deref(),
        "foreshadow" => row.foreshadow.as_deref(),
        "twist" => row.twist.as_deref(),
        _ => None,
    };
    v.unwrap_or_default().trim().to_owned()
}

/// 引擎 `_next_injection`：细纲 ch+1 行的 goal+suspense/foreshadow/twist，截 150 字。
fn next_injection(root: &Path, ch: i64) -> String {
    let outline = load_outline(root);
    let Some(row) = outline.iter().find(|r| r.ch == ch + 1) else {
        return String::new();
    };
    let parts = ["goal", "suspense", "foreshadow", "twist"]
        .into_iter()
        .map(|k| field_str(row, k))
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("；");
    parts.chars().take(INJECT_LIMIT).collect()
}

/// 引擎 `_total_bytes`：四路 UTF-8 字节合计。
fn total_bytes(ctx: &Context) -> usize {
    ctx.recent_endings.iter().map(String::len).sum::<usize>()
        + ctx.summary_chain.len()
        + ctx.entity_cards.iter().map(String::len).sum::<usize>()
        + ctx.next_chapter_injection.len()
}

/// Python `round(x, 2)`：银行家舍入（half-even）。与引擎相同的 IEEE754 运算路径。
fn py_round2(x: f64) -> f64 {
    (x * 100.0).round_ties_even() / 100.0
}

/// `x·10` 的正确舍入（ties-even），不经中间 f64 乘法。
///
/// 引擎 `"%.1f"` 由 dtoa 对真实值正确舍入；朴素 `(x*10.0).round_ties_even()`
/// 会先把真实值 ×10 舍入到 f64——当真实值贴近 .5 边界时被吸到整半点，随后
/// ties-even 走错方向（如 budget_kb=0.95：真实值 9.4999…96 应舍为 9，朴素路径
/// 得 9.5→10 显示 "1.0"，引擎 "0.9"；budget_kb=0.05 同理反向）。本实现按
/// x = m·2^e 的位表示精确计算 m·10·2^e 与 2^(s-1) 的大小/相等关系（u128 整数
/// 运算，无二次舍入）。budget_kb ∈ [0, 16) 且 ≥0.01，m·10 ≤ 2^57，u128 足够。
fn round10_ties_even(x: f64) -> i64 {
    if x == 0.0 {
        return 0;
    }
    let bits = x.to_bits();
    let exp = ((bits >> 52) & 0x7ff) as i32 - 1023; // 归一化指数（值 = m·2^(exp-52)）
    let mant = (bits & ((1u64 << 52) - 1)) | (1u64 << 52);
    let s = 52 - exp; // 值 = m·2^-s
    let n = (mant as u128) * 10;
    if s <= 0 {
        // x·10 为整数（exp ≥ 52；budget_kb 值不可达，保守路径）
        return (n << (-s) as u32) as i64;
    }
    let q = n >> s;
    let r = n & ((1u128 << s) - 1);
    let half = 1u128 << (s - 1);
    let rounded = if r > half {
        q + 1
    } else if r < half {
        q
    } else if q & 1 == 1 {
        q + 1
    } else {
        q
    };
    rounded as i64
}

/// Python `"%.1f"`：一位小数（ties-even，见 round10_ties_even）。
fn py_fmt_1f(x: f64) -> String {
    let tenths = round10_ties_even(x);
    format!("{}.{}", tenths.div_euclid(10), tenths.rem_euclid(10))
}

/// 引擎 `assemble_context`：装配第 ch 章四路上下文 + budget_kb。
pub fn assemble_context(root: &Path, state: &TrackingState, ch: i64) -> Context {
    let mut ctx = Context {
        recent_endings: recent_endings(root, ch),
        summary_chain: summary_chain(root, ch),
        entity_cards: entity_cards(root, state, ch),
        next_chapter_injection: next_injection(root, ch),
        budget_kb: 0.0,
    };
    ctx.budget_kb = py_round2(total_bytes(&ctx) as f64 / 1024.0);
    ctx
}

/// 引擎 `budget_check`：四路合计 ≤ 8KB（按已舍入的 budget_kb 判定）。
pub fn budget_check(ctx: &Context) -> bool {
    ctx.budget_kb <= BUDGET_BYTES as f64 / 1024.0
}

/// 引擎 `cli.cmd_context` 的 print 序列（含尾换行，逐字节一致）。
pub fn context_stdout(ctx: &Context) -> String {
    let mut out = format!(
        "budget: {}KB {}\n",
        py_fmt_1f(ctx.budget_kb),
        if ctx.budget_kb <= BUDGET_BYTES as f64 / 1024.0 {
            "OK"
        } else {
            "超限!"
        }
    );
    out.push_str("\n[recent_endings]\n");
    for item in &ctx.recent_endings {
        out.push_str(&format!("  {}\n", trunc_200(item)));
    }
    out.push_str("\n[summary_chain]\n");
    out.push_str(&format!("  {}\n", trunc_200(&ctx.summary_chain)));
    out.push_str("\n[entity_cards]\n");
    for item in &ctx.entity_cards {
        out.push_str(&format!("  {}\n", trunc_200(item)));
    }
    out.push_str("\n[next_chapter_injection]\n");
    out.push_str(&format!("  {}\n", trunc_200(&ctx.next_chapter_injection)));
    out
}

/// 引擎 `str(item)[:200]`：显示截断 200 码点。
fn trunc_200(s: &str) -> String {
    s.chars().take(200).collect()
}
