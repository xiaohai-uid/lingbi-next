//! report 模块（P2-14）—— 与引擎 `report.py` 等价：作者报告（三段式）。
//!
//! 状态（最新章/字数/活跃承诺/未曝光信息差/计划章数）→ 本次做了什么
//! （gate 机检 / 审查 / 已结算章）→ 下一步建议。纯文本面向作者，无 JSON/无路径。
//!
//! 与引擎的差异（有意，fail-closed 纪律）：
//! - 引擎 `load_state` 只捕 FileNotFoundError（损坏状态崩溃）；本实现按无状态容错
//!   （同 resume 模块，语料不可达）。
//! - 引擎 gate 调用失败（除 ImportError 外）崩溃；本实现按「gates 模块未就绪」
//!   行容错（语料 gate 恒成功，不可达）。
//! - 非数字章节键引擎 int() 崩溃；本实现跳过（语料不可达）。
//! - `load_config` 只捕 FileNotFoundError（book.yaml 损坏/字段类型错引擎崩溃）；
//!   本实现按 plan=0 容错（语料不可达）。
//! - 正文读取失败引擎 raise；本实现按 0 字计（文件刚被 chapter_files 列出，
//!   不可达——容错仅为防御）。
//! - 审查损坏分支展示的错误文本为引擎/本实现各自的解析器文案（review.rs 已声明），
//!   该行括号内允许差异（见 report_equivalence 测试）。

use crate::config::load_config;
use crate::gates::gate;
use crate::review::{has_blocking, load_review, review_path};
use crate::state::{body, chapter_files, chapter_num, hanzi, BODY_DIR};
use crate::tracking::{active_promises, load_state, unrevealed_gaps};
use lingbi_contracts::AppError;
use std::path::Path;

const GATE_STAGES: [&str; 3] = ["prewrite", "precommit", "postcommit"];

/// 三段式作者报告（引擎 `author_report`，含尾换行——CLI print 再补一个）。
pub fn author_report(root: &Path, stage: &str, ch: Option<i64>) -> Result<String, AppError> {
    let s = load_state(root).ok();
    let files = chapter_files(root);
    let latest = files.iter().map(|f| chapter_num(f)).max().unwrap_or(0);
    let total_words: usize = files
        .iter()
        .map(|f| hanzi(&body(&root.join(BODY_DIR).join(f)).unwrap_or_default()))
        .sum();
    let active = s.as_ref().map(|s| active_promises(s).len()).unwrap_or(0);
    let gaps = s.as_ref().map(|s| unrevealed_gaps(s).len()).unwrap_or(0);
    // 引擎 config.load_config 缺失 → FileNotFoundError 被捕获 → plan=0
    let plan = load_config(root).map(|c| c.total_chapters).unwrap_or(0);
    // 引擎 `ch or latest or 1`：ch 假值（None/0）回落 latest
    let target = match ch {
        Some(c) if c != 0 => c,
        _ => {
            if latest > 0 {
                latest
            } else {
                1
            }
        }
    };

    let mut lines = vec![
        format!("# 作者报告({stage})"),
        String::new(),
        "## 一、状态".to_owned(),
        format!("- 最新章: {latest} / 计划 {plan} 章"),
        format!("- 正文总字数: {total_words} 字"),
        format!("- 活跃承诺: {active}  未曝光信息差: {gaps}"),
        String::new(),
        format!("## 二、本次做了什么({stage})"),
    ];
    let mut gate_ok: Option<bool> = None;
    if GATE_STAGES.contains(&stage) {
        match gate(root, stage, ch) {
            Ok(g) => {
                gate_ok = Some(g.passed);
                lines.push(format!(
                    "- 机检({stage}): 通过 {} 项,问题 {} 项 → {}",
                    g.oks.len(),
                    g.fails.len(),
                    if g.passed { "放行" } else { "未放行" }
                ));
            }
            // 引擎 ImportError 分支（本实现 gate 恒可用，Err 仅防御）
            Err(_) => lines.push("- 机检: gates 模块未就绪(待模块 1 落地)".to_owned()),
        }
    } else {
        lines.push("- 机检: 本阶段不涉及".to_owned());
    }
    let mut blocking = false;
    let rp = review_path(root, target);
    if rp.exists() {
        let (issues, errors) = match load_review(&rp) {
            Ok(x) => x,
            Err(_) => (Vec::new(), vec!["评审报告无法解析".to_owned()]),
        };
        if !errors.is_empty() {
            lines.push(format!(
                "- 审查: 第{target}章评审报告损坏({})",
                errors.join("、")
            ));
            blocking = true;
        } else {
            blocking = has_blocking(&issues);
            lines.push(format!(
                "- 审查: 第{target}章 问题 {} 条,blocking {} 条",
                issues.len(),
                issues.iter().filter(|i| i.blocking).count()
            ));
        }
    } else {
        lines.push(format!("- 审查: 第{target}章尚无评审报告"));
    }
    // 引擎 `sorted(int(k) for k in chapters)`；非数字键本实现跳过（引擎崩溃）
    let mut committed: Vec<i64> = s
        .as_ref()
        .map(|s| {
            s.chapters
                .keys()
                .filter_map(|k| k.parse::<i64>().ok())
                .collect()
        })
        .unwrap_or_default();
    committed.sort_unstable();
    lines.push(format!(
        "- 已结算章: {} 章(最新 {})",
        committed.len(),
        if committed.is_empty() {
            "无".to_owned()
        } else {
            format!("第{}章", committed.last().expect("非空"))
        }
    ));
    lines.push(String::new());
    lines.push("## 三、下一步建议".to_owned());
    let mut nexts: Vec<String> = Vec::new();
    if s.is_none() {
        nexts.push("缺追踪状态:先跑 init_state".to_owned());
    }
    if gate_ok == Some(false) {
        nexts.push("机检未放行:定点修复后重跑 gate(失败只补跑失败步骤)".to_owned());
    }
    if blocking {
        nexts.push(format!("第{target}章存在 blocking:定点修复后进润色,勿重跑评审"));
    }
    if stage == "postcommit" && nexts.is_empty() {
        // 引擎字节锁定文本（P10 #27：仅建议文案，非功能依赖——应用运行时
        // 零 git 调用，干净 Windows 无 Git 全功能正常）
        nexts.push("本批次可结转:git add -A && git commit(或先重建派生视图)".to_owned());
    }
    if nexts.is_empty() {
        nexts.push(format!("续写第{}章:合同→任务书→起草→机检→评审→结算", target + 1));
    }
    lines.extend(nexts.into_iter().map(|n| format!("- {n}")));

    Ok(lines.join("\n") + "\n")
}
