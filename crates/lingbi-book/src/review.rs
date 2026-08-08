//! review 读侧 + 写侧（P2-11 读侧、P2-15 写侧落地）—— 与引擎 `review.py` 等价。
//!
//! 覆盖：review_path / report_path / load_review / has_blocking / ReviewIssue /
//! save_review / write_report。
//! 解析语义与引擎 models.ReviewIssue.from_dict + __post_init__ 一致（必填键缺失 →
//! 缺字段错误；severity/category 枚举校验；critical 强制 blocking）。
//!
//! 与引擎的差异（有意）：错误文案中的非字符串值展示与 Python str() 不完全一致、
//! JSON 非法文案用 serde 错误文本——resume 只取 corrupt 布尔不受影响；report 的
//! 审查损坏分支直接展示该文本，行内允许差异（report_equivalence 用前缀断言）。

use crate::json_engine::{obj, render as render_json, Frag};
use lingbi_contracts::{AppError, ErrorCode};
use serde_json::Value;
use std::path::{Path, PathBuf};

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

/// 评审目录（组件序 join——路径会出现在 stdout 中，须与引擎 os.path.join 反斜杠一致）。
fn review_dir(root: &Path) -> PathBuf {
    root.join("工作区").join("评审报告")
}

/// 引擎 `review_path`：工作区/评审报告/第NNN章.json。
pub fn review_path(root: &Path, ch: i64) -> PathBuf {
    review_dir(root).join(format!("第{ch:03}章.json"))
}

/// 引擎 `report_path`：工作区/评审报告/第NNN章.md。
pub fn report_path(root: &Path, ch: i64) -> PathBuf {
    review_dir(root).join(format!("第{ch:03}章.md"))
}

/// 引擎 `save_review`：写评审 json（issues 全字段 + 计数/blocking 汇总），返回路径。
pub fn save_review(root: &Path, ch: i64, issues: &[ReviewIssue]) -> Result<PathBuf, AppError> {
    let p = review_path(root, ch);
    std::fs::create_dir_all(p.parent().expect("parent")).map_err(|e| {
        AppError::new(
            ErrorCode::DiskWriteFailed,
            format!("创建评审目录失败: {e}"),
            false,
        )
    })?;
    let blocking_count = issues.iter().filter(|i| i.blocking).count();
    let payload = obj(vec![
        ("chapter".to_owned(), Frag::Int(ch)),
        (
            "issues".to_owned(),
            Frag::Arr(
                issues
                    .iter()
                    .map(|i| {
                        obj(vec![
                            ("severity".to_owned(), Frag::Str(i.severity.clone())),
                            ("category".to_owned(), Frag::Str(i.category.clone())),
                            ("location".to_owned(), Frag::Str(i.location.clone())),
                            ("description".to_owned(), Frag::Str(i.description.clone())),
                            ("evidence".to_owned(), Frag::Str(i.evidence.clone())),
                            ("fix_hint".to_owned(), Frag::Str(i.fix_hint.clone())),
                            ("blocking".to_owned(), Frag::Bool(i.blocking)),
                        ])
                    })
                    .collect(),
            ),
        ),
        ("issues_count".to_owned(), Frag::Int(issues.len() as i64)),
        ("blocking_count".to_owned(), Frag::Int(blocking_count as i64)),
        ("has_blocking".to_owned(), Frag::Bool(blocking_count > 0)),
    ]);
    std::fs::write(&p, render_json(&payload)).map_err(|e| {
        AppError::new(
            ErrorCode::DiskWriteFailed,
            format!("写评审报告失败: {e}"),
            false,
        )
    })?;
    Ok(p)
}

/// 引擎 `write_report`：人类可读审查报告 md（问题清单/位置/建议 + blocking 标记），
/// 返回路径。
pub fn write_report(root: &Path, ch: i64, issues: &[ReviewIssue]) -> Result<PathBuf, AppError> {
    let p = report_path(root, ch);
    std::fs::create_dir_all(p.parent().expect("parent")).map_err(|e| {
        AppError::new(
            ErrorCode::DiskWriteFailed,
            format!("创建评审目录失败: {e}"),
            false,
        )
    })?;
    let blocking_count = issues.iter().filter(|i| i.blocking).count();
    let mut l = vec![
        format!("# 第{ch}章 审查报告"),
        String::new(),
        format!("- 问题数: {},其中 blocking: {}", issues.len(), blocking_count),
        format!(
            "- 结论: {}",
            if blocking_count > 0 {
                "有 blocking,未放行,先定点修复"
            } else {
                "无 blocking,可进入润色/结算"
            }
        ),
        String::new(),
    ];
    if issues.is_empty() {
        l.push("未发现问题。".to_owned());
    }
    for (idx, i) in issues.iter().enumerate() {
        let tag = if i.blocking { "⛔ blocking" } else { "非 blocking" };
        l.push(format!(
            "### {}. [{}/{}] {}({})",
            idx + 1,
            i.severity,
            i.category,
            tag,
            i.location
        ));
        l.push(format!("- 问题: {}", i.description));
        if !i.evidence.is_empty() {
            l.push(format!("- 证据: {}", i.evidence));
        }
        if !i.fix_hint.is_empty() {
            l.push(format!("- 建议: {}", i.fix_hint));
        }
        l.push(String::new());
    }
    let text = l.join("\n").trim_end().to_owned() + "\n";
    std::fs::write(&p, text).map_err(|e| {
        AppError::new(
            ErrorCode::DiskWriteFailed,
            format!("写审查报告失败: {e}"),
            false,
        )
    })?;
    Ok(p)
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
