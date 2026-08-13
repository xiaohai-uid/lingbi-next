//! learn 模块（P2-13）—— 与引擎 `learn.py` 等价：金句收割 + AI 味反例吸收 +
//! 每章文风统计/趋势。
//!
//! 收割：独立成段短句（≤25 字、句末标点、无 他说/她道）每章 ≤5 条，与既有金句库
//! 去重后追加 文风/金句库/{ch:03}.md。吸收：评审 severity∈{critical,high} 且
//! category=ai_flavor 的 evidence 短句（≤20 字）去重后追加 文风/审查反例库.md
//! （上限 200 条超限裁最旧），机检红线动态生效。统计：每章 hanzi/段落数/对话占比/
//! 红线命中数；趋势复用统计 + 平均句长（round(x,1) 银行家舍入）。
//!
//! 与引擎的差异（有意，fail-closed 纪律）：
//! - 引擎 `harvest_quotes` 缺章时 raise FileNotFoundError（CLI 崩溃）；本实现返回
//!   `BookNotFound`（fail-closed，语义一致）。
//! - `absorb_review` 评审损坏时引擎 raise ValueError（CLI 崩溃）；本实现返回
//!   `BookCorrupted`（fail-closed，语义一致）。
//! - 读-改-写循环中的既有文件读取失败（金句库条目/反例库头部）按缺失处理——
//!   引擎 open 会崩溃；等价范围内不可达（文件刚被列出/写入）。
//! - P3 #20 扩展：托管书仓（`.lingbi/` 存在）的两处权威追加（金句库/反例库）经事务门
//!   `crate::tx::write_authoritative`（期望哈希 CAS + 日志 + 回执）；引擎无此概念，
//!   legacy 书仓保持引擎 fs::write 原路径（回放门禁字节锁定）。

use crate::checks::redlines;
use crate::py_num::py_round1;
use crate::review::{load_review, review_path};
use crate::state::{body, chapter_files, chapter_num, hanzi, BODY_DIR};
use lingbi_contracts::{AppError, ErrorCode};
use regex::Regex;
use std::collections::BTreeSet;
use std::path::Path;

const QUOTE_DIR: (&str, &str) = ("文风", "金句库");
const MAX_QUOTE_LEN: usize = 25;
const MAX_PER_CHAPTER: usize = 5;
const QUOTE_ENDS: [char; 3] = ['。', '！', '？'];
const SKIP_STARTS: [char; 4] = ['#', '*', '-', '>'];

const COUNTEREXAMPLES_FILE: (&str, &str) = ("文风", "审查反例库.md");
const MAX_COUNTEREXAMPLES: usize = 200;
const MAX_EVIDENCE_LEN: usize = 20;
const ABSORB_SEVERITIES: [&str; 2] = ["critical", "high"];
const SENT_END: &str = r"[。！？!?]";

fn counterexample_file(root: &Path) -> std::path::PathBuf {
    root.join(COUNTEREXAMPLES_FILE.0).join(COUNTEREXAMPLES_FILE.1)
}

/// 引擎 `_counterexample_lines`：非空且不以 # 开头的行（最旧在前，最新在后）。
fn counterexample_lines(root: &Path) -> Vec<String> {
    let p = counterexample_file(root);
    let Ok(text) = std::fs::read_to_string(&p) else {
        return Vec::new();
    };
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

/// 引擎 `_paragraphs`：按 `\n\s*\n` 切段，strip 后滤空。
fn paragraphs(text: &str) -> Vec<&str> {
    let re = Regex::new(r"\n\s*\n").expect("paragraph regex");
    re.split(text)
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect()
}

/// 引擎 `_quote_candidates`：独立成段短句（≤25 字、句末标点、无 他说/她道、
/// 不以 #/*/-/> 开头）。
fn quote_candidates(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    for p in paragraphs(text) {
        if p.chars().count() > MAX_QUOTE_LEN || p.starts_with(SKIP_STARTS) {
            continue;
        }
        if !p.ends_with(QUOTE_ENDS) || p.contains("他说") || p.contains("她道") {
            continue;
        }
        out.push(p);
    }
    out
}

/// 收割金句到 文风/金句库/{ch:03}.md（引擎 `harvest_quotes`），返回新增条数。
pub fn harvest_quotes(root: &Path, ch: i64) -> Result<usize, AppError> {
    let Some(p) = chapter_path(root, ch) else {
        return Err(AppError::new(
            ErrorCode::BookNotFound,
            format!(
                "缺第{ch}章正文: {}",
                root.join("定稿").join("正文").display()
            ),
            false,
        ));
    };
    let cands: Vec<String> = quote_candidates(&body(&p)?)
        .into_iter()
        .take(MAX_PER_CHAPTER)
        .map(str::to_owned)
        .collect();
    let dir = root.join(QUOTE_DIR.0).join(QUOTE_DIR.1);
    std::fs::create_dir_all(&dir).map_err(|e| {
        AppError::new(
            ErrorCode::DiskWriteFailed,
            format!("创建金句库目录失败: {e}"),
            false,
        )
    })?;
    let target = dir.join(format!("{ch:03}.md"));
    // 既有金句 = 金句库全部 .md 的非 # 行
    let mut existing = BTreeSet::new();
    for f in std::fs::read_dir(&dir).map_err(|e| {
        AppError::new(ErrorCode::DiskWriteFailed, format!("读金句库失败: {e}"), false)
    })? {
        let f = f.map_err(|e| {
            AppError::new(ErrorCode::DiskWriteFailed, format!("读金句库失败: {e}"), false)
        })?;
        if f.path().extension().is_some_and(|e| e == "md") {
            let Ok(text) = std::fs::read_to_string(f.path()) else {
                continue;
            };
            for line in text.lines().map(str::trim) {
                if !line.is_empty() && !line.starts_with('#') {
                    existing.insert(line.to_owned());
                }
            }
        }
    }
    let new: Vec<&str> = cands.iter().filter(|q| !existing.contains(*q)).map(String::as_str).collect();
    if new.is_empty() {
        return Ok(0);
    }
    let mut content = String::new();
    if target.exists() {
        content = std::fs::read_to_string(&target).unwrap_or_default();
        content = content.trim_end().to_owned();
    }
    let mut out = String::new();
    if !content.is_empty() {
        out.push_str(&content);
        out.push('\n');
    } else {
        out.push_str(&format!("# 第{ch}章 金句\n\n"));
    }
    out.push_str(&new.join("\n"));
    out.push('\n');
    if crate::tx::is_managed(root) {
        // P3 #20：权威追加走事务门（期望哈希 CAS 检测外部修改）
        let expected = file_hash_if_exists(&target)?;
        let rel = format!("{}/{}/{:03}.md", QUOTE_DIR.0, QUOTE_DIR.1, ch);
        crate::tx::write_authoritative(root, &rel, out.as_bytes(), expected.as_deref(), None)?;
    } else {
        std::fs::write(&target, out).map_err(|e| {
            AppError::new(ErrorCode::DiskWriteFailed, format!("写金句库失败: {e}"), false)
        })?;
    }
    Ok(new.len())
}

/// 吸收第 ch 章评审反例（引擎 `absorb_review`），返回新增条数。
pub fn absorb_review(root: &Path, ch: i64) -> Result<usize, AppError> {
    let (issues, errors) = load_review(&review_path(root, ch))?;
    if !errors.is_empty() {
        return Err(AppError::new(
            ErrorCode::BookCorrupted,
            format!("评审报告损坏,无法吸收: {}", errors.join("; ")),
            false,
        ));
    }
    let existing = counterexample_lines(root);
    let mut seen: BTreeSet<String> = existing.iter().cloned().collect();
    let mut new: Vec<String> = Vec::new();
    for i in &issues {
        if !ABSORB_SEVERITIES.contains(&i.severity.as_str()) || i.category != "ai_flavor" {
            continue;
        }
        let ev = i.evidence.trim();
        if !ev.is_empty()
            && !ev.contains('\n')
            && ev.chars().count() <= MAX_EVIDENCE_LEN
            && !seen.contains(ev)
        {
            seen.insert(ev.to_owned());
            new.push(ev.to_owned());
        }
    }
    if new.is_empty() {
        return Ok(0);
    }
    let p = counterexample_file(root);
    std::fs::create_dir_all(p.parent().expect("parent")).map_err(|e| {
        AppError::new(ErrorCode::DiskWriteFailed, format!("创建文风目录失败: {e}"), false)
    })?;
    let headers: Vec<String> = if p.exists() {
        // 引擎保留原始行（仅用 strip 判定 # 开头，行首空白原样写回）
        std::fs::read_to_string(&p)
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.trim().is_empty() && l.trim().starts_with('#'))
            .map(str::to_owned)
            .collect()
    } else {
        Vec::new()
    };
    let mut entries = existing;
    entries.extend(new.iter().cloned());
    if entries.len() > MAX_COUNTEREXAMPLES {
        entries.drain(..entries.len() - MAX_COUNTEREXAMPLES);
    }
    let mut out = String::new();
    if !headers.is_empty() {
        out.push_str(&headers.join("\n"));
    } else {
        out.push_str("# 审查反例库(AI 味反例,机检动态合并)");
    }
    out.push_str("\n\n");
    out.push_str(&entries.join("\n"));
    out.push('\n');
    if crate::tx::is_managed(root) {
        // P3 #20：权威追加走事务门（期望哈希 CAS 检测外部修改）
        let expected = file_hash_if_exists(&p)?;
        let rel = format!("{}/{}", COUNTEREXAMPLES_FILE.0, COUNTEREXAMPLES_FILE.1);
        crate::tx::write_authoritative(root, &rel, out.as_bytes(), expected.as_deref(), None)?;
    } else {
        std::fs::write(&p, out).map_err(|e| {
            AppError::new(ErrorCode::DiskWriteFailed, format!("写反例库失败: {e}"), false)
        })?;
    }
    Ok(new.len())
}

/// 文件当前内容哈希（不存在 → None）——权威追加的 CAS 期望值。
fn file_hash_if_exists(path: &std::path::Path) -> Result<Option<String>, AppError> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(path).map_err(|e| {
        AppError::new(
            ErrorCode::DiskWriteFailed,
            format!("读权威文件失败: {e}"),
            false,
        )
    })?;
    Ok(Some(lingbi_storage::sha256_hex(&bytes)))
}

/// 每章文风统计（引擎 `style_stats`）。
#[derive(Debug, Clone, PartialEq)]
pub struct ChapterStyle {
    pub hanzi: usize,
    pub paragraphs: usize,
    pub dialogue_ratio: f64,
    pub redline_hits: usize,
}

/// 引擎 `style_stats`：每章 {hanzi, paragraphs, dialogue_ratio, redline_hits}。
/// 对话占比 = 含 “” 或 「」 的段落数 / 总段数；红线命中按 checks.redlines(root)。
pub fn style_stats(root: &Path) -> Result<Vec<(i64, ChapterStyle)>, AppError> {
    let redlines = redlines(Some(root))?;
    // 正则编译提出循环（引擎 re.findall 每次调用编译，语义等价）
    let compiled: Vec<Regex> = redlines
        .iter()
        .filter_map(|(pat, _)| Regex::new(pat).ok())
        .collect();
    let mut out = Vec::new();
    for f in chapter_files(root) {
        let n = chapter_num(&f);
        let text = body(&root.join(BODY_DIR).join(&f))?;
        let paras = paragraphs(&text);
        let total = paras.len();
        let dial = paras
            .iter()
            .filter(|p| p.chars().any(|c| c == '“' || c == '”' || c == '「' || c == '」'))
            .count();
        let hits: usize = compiled
            .iter()
            .map(|re| re.find_iter(&text).count())
            .sum();
        out.push((
            n,
            ChapterStyle {
                hanzi: hanzi(&text),
                paragraphs: total,
                dialogue_ratio: if total > 0 { dial as f64 / total as f64 } else { 0.0 },
                redline_hits: hits,
            },
        ));
    }
    out.sort_by_key(|(n, _)| *n);
    Ok(out)
}

/// 引擎 `_avg_sentence_len`：平均句长 = 汉字数 / 句数（。！？!? 切分），round(x,1)。
fn avg_sentence_len(text: &str) -> f64 {
    let re = Regex::new(SENT_END).expect("sentence regex");
    let sents: Vec<&str> = re.split(text).filter(|s| !s.trim().is_empty()).collect();
    if sents.is_empty() {
        return 0.0;
    }
    py_round1(hanzi(text) as f64 / sents.len() as f64)
}

/// 近 n 章文风趋势（引擎 `trend`）：按章号升序取最后 n 章。
#[derive(Debug, Clone, PartialEq)]
pub struct TrendEntry {
    pub chapter: i64,
    pub dialogue_ratio: f64,
    pub redline_hits: usize,
    pub avg_sentence_len: f64,
    pub words: usize,
}

pub fn trend(root: &Path, n: usize) -> Result<Vec<TrendEntry>, AppError> {
    let stats = style_stats(root)?;
    let mut out = Vec::new();
    for (ch, s) in stats.iter().rev().take(n).rev() {
        let text = chapter_path(root, *ch)
            .map(|p| body(&p).unwrap_or_default())
            .unwrap_or_default();
        out.push(TrendEntry {
            chapter: *ch,
            dialogue_ratio: s.dialogue_ratio,
            redline_hits: s.redline_hits,
            avg_sentence_len: avg_sentence_len(&text),
            words: s.hanzi,
        });
    }
    Ok(out)
}

fn chapter_path(root: &Path, ch: i64) -> Option<std::path::PathBuf> {
    for f in chapter_files(root) {
        if chapter_num(&f) == ch {
            return Some(root.join(BODY_DIR).join(&f));
        }
    }
    None
}

/// Python dict repr（引擎 print 输出格式）：`{'k': v, ...}` 键序即插入序。
fn py_dict_repr(entries: &[(&str, String)]) -> String {
    let items: Vec<String> = entries
        .iter()
        .map(|(k, v)| format!("'{}': {v}", k.replace('\'', "\\'")))
        .collect();
    format!("{{{}}}", items.join(", "))
}

/// Python float repr（本域值 = 比率 ∈ [0,1] 与 round(x,1) 结果；`{:?}` 与 repr 最短
/// 往返一致；指数格式差异（1e-05 vs 1e-5）需比率 <1e-4 才触发，实际书不可达）。
fn py_float_repr(x: f64) -> String {
    if x == 0.0 {
        return "0.0".to_owned();
    }
    format!("{x:?}")
}

/// 引擎 cli.cmd_learn 默认输出（harvest + style_stats，含尾换行）。
pub fn learn_stdout(root: &Path, ch: i64) -> Result<String, AppError> {
    let n = harvest_quotes(root, ch)?;
    let sts = style_stats(root)?;
    let mut out = format!("金句收割 {n} 条(ch{ch});全书风格统计:");
    // 引擎 cli.py: `list(sts.items())[:10]`——最多打印 10 章
    for (ch, s) in sts.iter().take(10) {
        out.push_str(&format!(
            "\n  ch{ch}: {}",
            py_dict_repr(&[
                ("hanzi", s.hanzi.to_string()),
                ("paragraphs", s.paragraphs.to_string()),
                ("dialogue_ratio", py_float_repr(s.dialogue_ratio)),
                ("redline_hits", s.redline_hits.to_string()),
            ])
        ));
    }
    out.push('\n');
    Ok(out)
}

/// 引擎 cli.cmd_learn --absorb 输出（含尾换行）。
pub fn absorb_stdout(n: usize, ch: i64) -> String {
    format!("✅ 吸收反例 {n} 条(ch{ch})→ 文风/审查反例库.md(机检红线已动态生效)\n")
}

/// 引擎 cli.cmd_learn --trend 输出（逐行 "  chN: {...}"，含尾换行）。
pub fn trend_stdout(entries: &[TrendEntry]) -> String {
    let mut out = entries
        .iter()
        .map(|d| {
            format!(
                "  ch{}: {}",
                d.chapter,
                py_dict_repr(&[
                    ("dialogue_ratio", py_float_repr(d.dialogue_ratio)),
                    ("redline_hits", d.redline_hits.to_string()),
                    ("avg_sentence_len", py_float_repr(d.avg_sentence_len)),
                    ("words", d.words.to_string()),
                ])
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    out.push('\n');
    out
}
