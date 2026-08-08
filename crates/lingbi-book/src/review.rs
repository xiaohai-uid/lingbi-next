//! review 读侧模块（P2-11 子集）—— 与引擎 `review.py` 读侧等价（resume 依赖）。
//!
//! 覆盖：review_path / load_review / has_blocking / ReviewIssue。
//! 写侧（save_review / report_path / cmd_review）在 P3 review 票据补全。
//! 解析语义与引擎 models.ReviewIssue.from_dict + __post_init__ 一致（必填键缺失 →
//! 缺字段错误；severity/category 枚举校验；critical 强制 blocking）。
//!
//! 与引擎的差异（有意）：错误文案中的非字符串值展示与 Python str() 不完全一致、
//! JSON 非法文案用 serde 错误文本——这些错误串不进入任何 stdout（resume 只取
//! corrupt 布尔），不构成等价差异。

use lingbi_contracts::{AppError, ErrorCode};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub const REVIEW_DIR: &str = "工作区/评审报告";

const SEVERITIES: [&str; 4] = ["critical", "high", "medium", "low"];
const CATEGORIES: [&str; 8] = [
    "continuity",
    "setting",
    "character",
    "timeline",
    "ai_flavor",
    "logic",
    "pacing",
    "other",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewIssue {
    pub severity: String,
    pub category: String,
    pub location: String,
    pub description: String,
    pub evidence: String,
    pub fix_hint: String,
    pub blocking: bool,
}

/// 引擎 `review_path`：工作区/评审报告/第NNN章.json。
pub fn review_path(root: &Path, ch: i64) -> PathBuf {
    root.join(REVIEW_DIR).join(format!("第{ch:03}章.json"))
}

/// 引擎 `bool()` 真值（blocking 字段非布尔时的 Python 真值语义）。
fn py_bool(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Null => false,
        Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(true),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// 引擎 `_parse_issues` 单条：from_dict（必填键）+ __post_init__（枚举校验、critical 强制）。
/// 返回裸错误消息，由 parse_issues 统一加 "第 N 条 issue " 前缀（引擎两处 except 同构）。
fn parse_issue(d: &serde_json::Map<String, Value>) -> Result<ReviewIssue, String> {
    let missing = |k: &str| format!("缺字段 '{k}'");
    let severity = d.get("severity").ok_or_else(|| missing("severity"))?;
    let category = d.get("category").ok_or_else(|| missing("category"))?;
    let location = d.get("location").ok_or_else(|| missing("location"))?;
    let description = d.get("description").ok_or_else(|| missing("description"))?;
    let str_of = |v: &Value| v.as_str().map(str::to_owned);
    // 引擎 __post_init__：非字符串/不在枚举内 → ValueError
    let severity = str_of(severity).ok_or_else(|| format!("severity 非法: {severity}"))?;
    if !SEVERITIES.contains(&severity.as_str()) {
        return Err(format!("severity 非法: {severity}"));
    }
    let category = str_of(category).ok_or_else(|| format!("category 非法: {category}"))?;
    if !CATEGORIES.contains(&category.as_str()) {
        return Err(format!("category 非法: {category}"));
    }
    // 引擎 blocking：bool(d.get("blocking", severity=="critical"))，critical 强制 True
    let blocking = severity == "critical"
        || d.get("blocking").map(py_bool).unwrap_or(false);
    Ok(ReviewIssue {
        severity,
        category,
        location: str_of(location).unwrap_or_default(),
        description: str_of(description).unwrap_or_default(),
        evidence: d
            .get("evidence")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned(),
        fix_hint: d
            .get("fix_hint")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned(),
        blocking,
    })
}

/// 引擎 `_parse_issues`：issues 字段缺失/非数组 → errors；逐条解析。
fn parse_issues(raw: Option<&Value>) -> (Vec<ReviewIssue>, Vec<String>) {
    let Some(raw) = raw else {
        return (Vec::new(), vec!["缺 issues 字段".to_owned()]);
    };
    let Some(arr) = raw.as_array() else {
        return (Vec::new(), vec!["issues 字段不是数组".to_owned()]);
    };
    let mut issues = Vec::new();
    let mut errors = Vec::new();
    for (i, item) in arr.iter().enumerate() {
        let idx = i + 1;
        let Some(d) = item.as_object() else {
            errors.push(format!("第 {idx} 条 issue 不是对象"));
            continue;
        };
        match parse_issue(d) {
            Ok(issue) => issues.push(issue),
            Err(e) => errors.push(format!("第 {idx} 条 issue {e}")),
        }
    }
    (issues, errors)
}

/// 引擎 `load_review`：读评审 JSON；文件缺失抛错，非法 JSON/顶层非对象/条目错误 → errors。
pub fn load_review(path: &Path) -> Result<(Vec<ReviewIssue>, Vec<String>), AppError> {
    if !path.exists() {
        return Err(AppError::new(
            ErrorCode::BookNotFound,
            format!("缺评审报告: {}", path.display()),
            false,
        ));
    }
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            return Ok((Vec::new(), vec![format!("评审 JSON 非法: {e}")]));
        }
    };
    let data: Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            return Ok((Vec::new(), vec![format!("评审 JSON 非法: {e}")]));
        }
    };
    if !data.is_object() {
        return Ok((Vec::new(), vec!["评审 JSON 顶层不是对象".to_owned()]));
    }
    Ok(parse_issues(data.get("issues")))
}

/// 引擎 `has_blocking`。
pub fn has_blocking(issues: &[ReviewIssue]) -> bool {
    issues.iter().any(|i| i.blocking)
}
