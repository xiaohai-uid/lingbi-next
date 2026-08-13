//! contract 模块（P2-07）—— 与引擎 `contract.py` 等价：细纲蓝图 → 第 ch 章写作合同。
//!
//! 从 大纲/细纲蓝图.md 提取合同 {goal, hard_constraints, forbidden, foreshadow_todo,
//! ending_requirement, sources}；只读不写；无细纲文件不抛错（goal 降级占位）。
//! 蓝图行格式两种（均以 | 分隔，第 1 列 == ch）：
//! 1) Markdown 表格：首行表头须含 "要发生什么"（或首列为 ch/章/章号/章节）；表头列名
//!    按子串识别（ch/标题/要发生什么/硬约束/禁区/角色/suspense/foreshadow/twist）。
//! 2) 无表头管道行（简易，列序同表头）：ch|标题|要发生什么|硬约束|禁区|角色|suspense|foreshadow|twist。
//!
//! 与引擎的差异（有意，fail-closed 纪律）：
//! - 引擎 `_to_int` 用 Python `\d`（Unicode 十进制数字）；本实现仅 ASCII 数字——
//!   蓝图章号与语料均 ASCII，不构成等价差异。
//! - 引擎 `int()` 无界；本实现超 i64 的章号按无数字处理（同 `_to_int` 的 None 路径）。
//! - 引擎 `text.splitlines()` 认裸 `\r` 等 Unicode 换行；本实现仅按 `\n` 切行（行尾
//!   `\r` 仍被 trim 掉）——蓝图文件均为 `\n`/`\r\n` 行尾，不构成等价差异。
//! - 细纲文件非 UTF-8 时引擎 `f.read()` 抛 UnicodeDecodeError 崩溃；本实现按
//!   「无细纲」降级（占位 goal + 总纲 source）——语料与蓝图均由引擎 UTF-8 写出，
//!   该路径在等价范围内不可达。

use crate::json_engine::{frag_strs, obj, render as render_json, Frag};
use crate::tracking::{active_promises, state_path, unrevealed_gaps, TrackingState};
use std::path::Path;

pub const OUTLINE_DIR: &str = "大纲";
pub const OUTLINE_FILE: &str = "细纲蓝图.md";
pub const SUMMARY_DIR: &str = "大纲";
pub const SUMMARY_FILE: &str = "总纲.md";

/// 细纲蓝图的一行（规范键与引擎 load_outline 行一致）。
/// `Option` 区分「列缺失」（表头无此列，引擎 get→None→str(None)="None"）与
/// 「列存在但单元格为空」（引擎赋 ""）——两者在 split_list 中行为不同。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OutlineRow {
    pub ch: i64,
    pub title: Option<String>,
    pub goal: Option<String>,
    pub hard: Option<String>,
    pub forbidden: Option<String>,
    pub roles: Option<String>,
    pub suspense: Option<String>,
    pub foreshadow: Option<String>,
    pub twist: Option<String>,
}

/// 写作合同（键序与引擎 build_contract 返回 dict 一致，to_json 逐字节等价）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contract {
    pub goal: String,
    pub hard_constraints: Vec<String>,
    pub forbidden: Vec<String>,
    pub foreshadow_todo: Vec<String>,
    pub ending_requirement: String,
    /// 有序 sources（引擎 dict 插入序：追踪状态 → 细纲蓝图/总纲）。
    pub sources: Vec<(String, String)>,
}

impl Contract {
    /// 引擎 `json.dumps(c, ensure_ascii=False, indent=1)` 的逐字节等价输出。
    pub fn to_json(&self) -> String {
        render_json(&self.to_frag())
    }

    pub(crate) fn to_frag(&self) -> Frag {
        obj(vec![
            ("goal".to_owned(), Frag::Str(self.goal.clone())),
            (
                "hard_constraints".to_owned(),
                frag_strs(&self.hard_constraints),
            ),
            ("forbidden".to_owned(), frag_strs(&self.forbidden)),
            ("foreshadow_todo".to_owned(), frag_strs(&self.foreshadow_todo)),
            (
                "ending_requirement".to_owned(),
                Frag::Str(self.ending_requirement.clone()),
            ),
            (
                "sources".to_owned(),
                obj(
                    self.sources
                        .iter()
                        .map(|(k, v)| (k.clone(), Frag::Str(v.clone())))
                        .collect(),
                ),
            ),
        ])
    }
}

/// 逗号/顿号/分号分隔的列表。
pub fn split_list(s: &str) -> Vec<String> {
    s.split([',', '，', '、', ';', '；'])
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .map(str::to_owned)
        .collect()
}

/// 引擎 `_to_int`：首个数字串（r"\d+" 的 ASCII 子集）。
fn to_int(s: &str) -> Option<i64> {
    let start = s.find(|c: char| c.is_ascii_digit())?;
    let rest = &s[start..];
    let end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

/// 表头列名 → 规范键。
fn norm_key(header: &str) -> Option<&'static str> {
    let h = header.trim().to_lowercase();
    if matches!(h.as_str(), "ch" | "章" | "章号" | "章节" | "章序") {
        return Some("ch");
    }
    if h.contains("标题") || h.contains("章名") || h.contains("title") {
        return Some("title");
    }
    if h.contains("要发生什么") || h.contains("goal") || h == "发生" {
        return Some("goal");
    }
    if h.contains("硬约束") || h.contains("约束") || h.contains("hard") {
        return Some("hard");
    }
    if h.contains("禁区") || h.contains("forbidden") {
        return Some("forbidden");
    }
    if h.contains("角色") || h.contains("人物") || h.contains("roles") || h.contains("chars") {
        return Some("roles");
    }
    if h.contains("suspense") || h.contains("悬念") {
        return Some("suspense");
    }
    if h.contains("foreshadow") || h.contains("伏笔") {
        return Some("foreshadow");
    }
    if h.contains("twist") || h.contains("反转") {
        return Some("twist");
    }
    None
}

/// 首行是否表头：任一单元格含 "要发生什么"，或整格 ∈ {要发生什么, ch, 章, 章号, 章节}。
fn looks_like_header(cells: &[String]) -> bool {
    cells.iter().any(|c| {
        c.contains("要发生什么")
            || matches!(
                c.trim().to_lowercase().as_str(),
                "要发生什么" | "ch" | "章" | "章号" | "章节"
            )
    })
}

/// 分隔行（`|---|---|` 等）：全空，或字符集 ⊆ {-, :, 空格}。
fn is_separator(cells: &[String]) -> bool {
    cells.iter().all(|c| c.is_empty())
        || cells
            .concat()
            .chars()
            .all(|c| matches!(c, '-' | ':' | ' '))
}

/// 简易行（无表头）：列 1..9 顺次映射 title/goal/hard/forbidden/roles/suspense/foreshadow/twist。
/// 短行缺列取空串（引擎 `cells[i] if i < len(cells) else ""`，键必存在）。
fn positional_row(cells: &[String]) -> OutlineRow {
    OutlineRow {
        ch: to_int(&cells[0]).unwrap_or(0),
        title: Some(cells.get(1).cloned().unwrap_or_default()),
        goal: Some(cells.get(2).cloned().unwrap_or_default()),
        hard: Some(cells.get(3).cloned().unwrap_or_default()),
        forbidden: Some(cells.get(4).cloned().unwrap_or_default()),
        roles: Some(cells.get(5).cloned().unwrap_or_default()),
        suspense: Some(cells.get(6).cloned().unwrap_or_default()),
        foreshadow: Some(cells.get(7).cloned().unwrap_or_default()),
        twist: Some(cells.get(8).cloned().unwrap_or_default()),
    }
}

fn set_field(row: &mut OutlineRow, key: &str, val: String) {
    match key {
        "title" => row.title = Some(val),
        "goal" => row.goal = Some(val),
        "hard" => row.hard = Some(val),
        "forbidden" => row.forbidden = Some(val),
        "roles" => row.roles = Some(val),
        "suspense" => row.suspense = Some(val),
        "foreshadow" => row.foreshadow = Some(val),
        "twist" => row.twist = Some(val),
        _ => {}
    }
}

/// 读 大纲/细纲蓝图.md（缺失回退 大纲/章纲.md——storyrepo 真实书仓惯例，
/// P12 #54 验收暴露：真实《零界道种》无细纲蓝图、计划在章纲.md），
/// 返回规范行列表（按 ch 升序）；两文件均无返回 []。
pub fn load_outline(root: &Path) -> Vec<OutlineRow> {
    let outline_path = if root.join(OUTLINE_DIR).join(OUTLINE_FILE).is_file() {
        root.join(OUTLINE_DIR).join(OUTLINE_FILE)
    } else {
        root.join(OUTLINE_DIR).join("章纲.md")
    };
    let Ok(text) = std::fs::read_to_string(&outline_path) else {
        return Vec::new();
    };
    let mut rows: Vec<OutlineRow> = Vec::new();
    let mut headers: Option<Vec<String>> = None;
    for raw in text.split('\n') {
        let line = raw.trim();
        if !line.contains('|') {
            continue;
        }
        let cells: Vec<String> = line
            .trim_matches('|')
            .split('|')
            .map(|c| c.trim().to_owned())
            .collect();
        if headers.is_none() {
            if looks_like_header(&cells) {
                headers = Some(cells);
            } else if is_separator(&cells) {
                continue;
            } else if to_int(&cells[0]).is_some() {
                rows.push(positional_row(&cells));
            }
            continue;
        }
        let hs = headers.as_ref().expect("headers checked");
        if is_separator(&cells) {
            continue;
        }
        let ch_idx = hs
            .iter()
            .position(|h| norm_key(h) == Some("ch"))
            .unwrap_or(0);
        if ch_idx >= cells.len() {
            continue;
        }
        let Some(chnum) = to_int(&cells[ch_idx]) else {
            continue;
        };
        let mut row = OutlineRow {
            ch: chnum,
            ..OutlineRow::default()
        };
        for (i, h) in hs.iter().enumerate() {
            if let Some(k) = norm_key(h).filter(|k| *k != "ch") {
                set_field(&mut row, k, cells.get(i).cloned().unwrap_or_default());
            }
        }
        rows.push(row);
    }
    rows.sort_by_key(|r| r.ch);
    rows
}

/// suspense/foreshadow/twist 非空合并（"；" 连接；列缺失视为空，引擎 `or ""`）。
fn merge_cells(row: &OutlineRow) -> String {
    [
        row.suspense.as_deref().unwrap_or_default().trim(),
        row.foreshadow.as_deref().unwrap_or_default().trim(),
        row.twist.as_deref().unwrap_or_default().trim(),
    ]
    .into_iter()
    .filter(|s| !s.is_empty())
    .collect::<Vec<_>>()
    .join("；")
}

/// 引擎 `_foreshadow_todo` 的 limit 默认值（活跃伏笔最多取 5 条）。
const FORESHADOW_TODO_LIMIT: usize = 5;

/// 引擎 `_foreshadow_todo`：活跃伏笔中 note 非空或 adv_ch 距当前 ≥3 的 id
/// （≤limit 条，note 优先、按埋设章序）。
fn foreshadow_todo(state: &TrackingState, ch: i64, limit: usize) -> Vec<String> {
    // (pid, note 优先旗标, adv_ch)——与引擎 cands 三元组对应；旗标 0=有 note 排前
    let mut cands: Vec<(String, bool, i64)> = Vec::new();
    for (pid, v) in active_promises(state) {
        let adv = v.adv_ch.unwrap_or(0);
        let note = v.note.trim();
        if !note.is_empty() || (adv != 0 && ch - adv >= 3) {
            cands.push((pid.clone(), !note.is_empty(), adv));
        }
    }
    // 引擎稳定排序：note 优先（flag 0 前），同 flag 按 adv 升序，同键保插入序
    cands.sort_by_key(|t| (if t.1 { 0 } else { 1 }, t.2));
    cands.into_iter().take(limit).map(|t| t.0).collect()
}

/// 构建第 ch 章写作合同（引擎 `build_contract`）。
///
/// - goal/hard/ending：细纲蓝图第 ch 行；ending_requirement 取 ch+1 行
///   suspense/foreshadow/twist 合并（本章结尾须为其埋伏笔）。
/// - forbidden：未曝光信息差 id + 细纲行 "禁区" 列。
/// - 无细纲文件 → goal="(无细纲,依 大纲/总纲.md 一句话)"，不抛错。
pub fn build_contract(root: &Path, state: &TrackingState, ch: i64) -> Contract {
    let outline = load_outline(root);
    let has_outline = !outline.is_empty();
    let row = outline.iter().find(|r| r.ch == ch);
    let next_row = outline.iter().find(|r| r.ch == ch + 1);
    let gaps: Vec<String> = unrevealed_gaps(state)
        .into_iter()
        .map(|(g, _)| g.clone())
        .collect();

    let mut sources = vec![("追踪状态".to_owned(), state_path(root).display().to_string())];
    let (goal, hard_constraints, forbidden, ending_requirement) = if !has_outline {
        sources.push((
            "总纲".to_owned(),
            root.join(SUMMARY_DIR).join(SUMMARY_FILE).display().to_string(),
        ));
        (
            "(无细纲,依 大纲/总纲.md 一句话)".to_owned(),
            Vec::new(),
            gaps,
            String::new(),
        )
    } else {
        sources.push((
            "细纲蓝图".to_owned(),
            root.join(OUTLINE_DIR).join(OUTLINE_FILE).display().to_string(),
        ));
        match row {
            None => (format!("(细纲蓝图无第{ch}章行)"), Vec::new(), gaps, String::new()),
            Some(r) => {
                let mut forbidden = gaps;
                // 引擎 `split_list(row.get("forbidden"))`：列缺失时 str(None)="None" → ["None"]
                forbidden.extend(split_list(r.forbidden.as_deref().unwrap_or("None")));
                let ending_requirement = match next_row {
                    Some(nr) => merge_cells(nr),
                    None => String::new(),
                };
                (
                    r.goal.clone().unwrap_or_default(),
                    split_list(r.hard.as_deref().unwrap_or("None")),
                    forbidden,
                    ending_requirement,
                )
            }
        }
    };

    Contract {
        goal,
        hard_constraints,
        forbidden,
        foreshadow_todo: foreshadow_todo(state, ch, FORESHADOW_TODO_LIMIT),
        ending_requirement,
        sources,
    }
}
