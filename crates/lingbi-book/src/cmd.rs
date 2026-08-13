//! 命令层（P2-15）—— 引擎 `cli.py` 命令的等价实现（stdout + 退出码）。
//!
//! 供 P2-15 回放门禁（零 Python/WSL 依赖）与未来 Tauri command 层复用。
//! 各命令输出与引擎 print 序列逐字节一致（含尾换行）。
//!
//! 与引擎的差异（有意，fail-closed 纪律）：
//! - 引擎命令在异常时抛 traceback 崩溃（多数命令未捕获）；本实现返回
//!   `AppError`，回放 harness 将其视为失败（语料不可达）。
//! - `cmd_check` 的缺状态分支打印本实现错误文案（引擎 FileNotFoundError str）；
//!   语料状态恒存在，不可达。

use crate::batch::{create_batch_plan, create_plan_stdout};
use crate::brief::build_brief;
use crate::config::load_config;
use crate::contract::build_contract;
use crate::context::{assemble_context, context_stdout};
use crate::gates::{exit_code, gate, GateResult};
use crate::json_engine::{obj, render as render_json, Frag};
use crate::learn::{absorb_stdout, learn_stdout, trend, trend_stdout};
use crate::py_num::py_fmt_2f;
use crate::report::author_report;
use crate::resume::diagnose;
use crate::review::{has_blocking, load_review, save_review, write_report};
use crate::revision::cascade_revision;
use crate::state::{
    body, chapter_files, chapter_num, frontmatter, hanzi, latest_chapter, BODY_DIR, TRACKING_DIR,
};
use crate::tracking::{active_promises, bump_revision, load_state, merge_chapter_facts, save_state};
use crate::views::{card_size_ok, write_views};
use lingbi_contracts::{AppError, ErrorCode};
use std::path::Path;

/// 命令结果（stdout 含尾换行，与引擎 print 序列一致）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CmdOut {
    pub stdout: String,
    pub exit: i32,
}

/// 引擎 cli.cmd_doctor 的输出格式（"  ✔/✘ …" + "gate <stage>: PASS|FAIL"，尾换行）。
pub fn doctor_stdout(g: &GateResult, stage: &str) -> String {
    let mut lines = Vec::new();
    for s in &g.oks {
        lines.push(format!("  \u{2714} {s}"));
    }
    for s in &g.fails {
        lines.push(format!("  \u{2718} {s}"));
    }
    lines.push(format!(
        "gate {stage}: {}",
        if g.passed { "PASS" } else { "FAIL" }
    ));
    lines.join("\n") + "\n"
}

/// 引擎 `cmd_doctor`。
pub fn cmd_doctor(root: &Path, stage: &str, chapter: Option<i64>) -> Result<CmdOut, AppError> {
    let g = gate(root, stage, chapter)?;
    Ok(CmdOut {
        stdout: doctor_stdout(&g, stage),
        exit: exit_code(&g),
    })
}

/// 引擎 `cmd_contract`。
pub fn cmd_contract(root: &Path, ch: i64) -> Result<CmdOut, AppError> {
    let s = load_state(root)?;
    Ok(CmdOut {
        stdout: build_contract(root, &s, ch).to_json() + "\n",
        exit: 0,
    })
}

/// 引擎 `cmd_context`。
pub fn cmd_context(root: &Path, ch: i64) -> Result<CmdOut, AppError> {
    let s = load_state(root)?;
    Ok(CmdOut {
        stdout: context_stdout(&assemble_context(root, &s, ch)),
        exit: 0,
    })
}

/// 引擎 `cmd_brief`。
pub fn cmd_brief(root: &Path, ch: i64) -> Result<CmdOut, AppError> {
    let s = load_state(root)?;
    Ok(CmdOut {
        stdout: build_brief(root, &s, ch, None, None)? + "\n",
        exit: 0,
    })
}

/// 引擎 `cmd_review`。
pub fn cmd_review(root: &Path, ch: i64, json_path: &Path) -> Result<CmdOut, AppError> {
    let mut out = String::new();
    if !json_path.exists() {
        out.push_str(&format!("✘ 评审 JSON 不存在: {}\n", json_path.display()));
        return Ok(CmdOut { stdout: out, exit: 1 });
    }
    let (issues, errors) = load_review(json_path)?;
    for e in &errors {
        out.push_str(&format!("  ⚠ {e}\n"));
    }
    if !has_blocking(&issues) {
        save_review(root, ch, &issues)?;
        let rp = write_report(root, ch, &issues)?;
        out.push_str(&format!("✅ 评审落库(0 blocking),报告: {}\n", rp.display()));
        Ok(CmdOut { stdout: out, exit: 0 })
    } else {
        out.push_str(&format!(
            "✘ 存在 blocking({}):\n",
            issues.iter().filter(|i| i.blocking).count()
        ));
        for i in issues.iter().filter(|i| i.blocking) {
            out.push_str(&format!(
                "   - [{}/{}] {}\n",
                i.severity, i.category, i.description
            ));
        }
        out.push_str("  请定点修复或用户裁决后再 review。\n");
        Ok(CmdOut { stdout: out, exit: 1 })
    }
}

/// 引擎 `cmd_commit`。
pub fn cmd_commit(root: &Path, ch: i64, force: bool) -> Result<CmdOut, AppError> {
    let mut out = String::new();
    // 引擎 `args.chapter or latest_chapter`：ch 假值（0）回落最新章
    let ch = if ch == 0 { latest_chapter(root) } else { ch };
    let fs: Vec<String> = chapter_files(root)
        .into_iter()
        .filter(|f| chapter_num(f) == ch)
        .collect();
    if fs.is_empty() {
        out.push_str(&format!("✘ 章 {ch} 无正文\n"));
        return Ok(CmdOut { stdout: out, exit: 1 });
    }
    // L11 提交前提示（不强拦，流程纪律靠 author）；gate 意外错误按 fail-closed 传播
    // （引擎仅捕 FileNotFoundError——gate 内部已把缺状态映射为检查失败，不可达）
    match gate(root, "precommit", Some(ch)) {
        Ok(g) if !g.passed => {
            out.push_str(&format!(
                "⚠ precommit gate 未过({} 项失败),提交将带着问题入账。用 --force 显式确认。\n",
                g.fails.len()
            ));
            if !force {
                out.push_str("  (阻断:先修 gate 或加 --force)\n");
                return Ok(CmdOut { stdout: out, exit: 1 });
            }
        }
        Ok(_) => {}
        Err(e) => return Err(e),
    }
    let mut s = load_state(root)?;
    // M1 幂等守卫：该章已提交则拒绝重复合并（防止时间线污染）
    if s.chapters.contains_key(&ch.to_string()) {
        out.push_str(&format!(
            "✘ 章 {ch} 已提交过(rev {}).重写走 storyrepo revision --chapter {ch}(重放式级联);新章节请另建批次计划。\n",
            s.state_revision
        ));
        return Ok(CmdOut { stdout: out, exit: 1 });
    }
    let fm = frontmatter(&root.join(BODY_DIR).join(&fs[0]))?;
    let warns = merge_chapter_facts(
        &mut s,
        ch,
        &fm,
        fm.get("summary").map(String::as_str).unwrap_or_default(),
    )?;
    bump_revision(&mut s);
    save_state(root, &s)?;
    write_views(root, &s)?;
    // P3 #20：托管书仓在结算时快照 Final Revision（尽力而为，失败不阻断结算）
    if crate::tx::is_managed(root) {
        match std::fs::read(root.join(BODY_DIR).join(&fs[0])) {
            Ok(body) => {
                if let Err(e) = crate::tx::write_version_snapshot(root, ch, s.state_revision, &body)
                {
                    out.push_str(&format!("  ⚠ Final Revision 快照失败: {e}\n"));
                }
            }
            Err(e) => out.push_str(&format!("  ⚠ 正文快照读取失败: {e}\n")),
        }
    }
    out.push_str(&format!(
        "✅ commit ch{ch} (rev {}),派生视图已重建\n",
        s.state_revision
    ));
    for w in &warns {
        out.push_str(&format!("  ⚠ {w}\n"));
    }
    Ok(CmdOut { stdout: out, exit: 0 })
}

/// 引擎 `cmd_check`。
pub fn cmd_check(root: &Path) -> Result<CmdOut, AppError> {
    let s = load_state(root)?;
    let mut fails: Vec<&str> = Vec::new();
    if !root.join(TRACKING_DIR).join("上下文.md").exists() {
        fails.push("缺派生 上下文.md");
    }
    if !card_size_ok(root) {
        fails.push("状态卡超 12KB");
    }
    let mut chs: Vec<i64> = s
        .chapters
        .keys()
        .filter_map(|k| k.parse::<i64>().ok())
        .collect();
    chs.sort_unstable();
    if !chs.is_empty() && chs != (1..=*chs.last().expect("非空")).collect::<Vec<i64>>() {
        fails.push("章节记录不连续");
    }
    if s.imported_through_chapter != *chs.last().unwrap_or(&0) {
        fails.push("imported_through_chapter 与最新章不符");
    }
    let mut out = String::from("==== tracking check ====\n");
    for f in &fails {
        out.push_str(&format!("  ✘ {f}\n"));
    }
    if fails.is_empty() {
        out.push_str("  ✔ 状态有效 / 派生一致 / 状态卡 ≤12KB\n");
    }
    Ok(CmdOut {
        stdout: out,
        exit: if fails.is_empty() { 0 } else { 1 },
    })
}

/// 引擎 `cmd_status`。
pub fn cmd_status(root: &Path) -> Result<CmdOut, AppError> {
    let s = load_state(root).ok();
    let book = s.as_ref().map(|s| Frag::Str(s.book.clone()));
    let rev = s.as_ref().map(|s| Frag::Int(s.state_revision));
    let through = s.as_ref().map(|s| Frag::Int(s.imported_through_chapter));
    let chapters = s.as_ref().map(|s| s.chapters.len()).unwrap_or(0);
    let words: usize = chapter_files(root)
        .iter()
        .map(|f| hanzi(&body(&root.join(BODY_DIR).join(f)).unwrap_or_default()))
        .sum();
    // 引擎 len(active_promises(s))：滤除 status=done 的承诺
    let promises_open = s
        .as_ref()
        .map(|s| Frag::Int(active_promises(s).len() as i64));
    let gaps_unrevealed = s
        .as_ref()
        .map(|s| Frag::Int(s.info_gaps.values().filter(|g| !g.revealed).count() as i64));
    let out = render_json(&obj(vec![
        ("book".to_owned(), book.unwrap_or(Frag::Null)),
        ("rev".to_owned(), rev.unwrap_or(Frag::Null)),
        ("through".to_owned(), through.unwrap_or(Frag::Null)),
        ("chapters".to_owned(), Frag::Int(chapters as i64)),
        (
            "latest_chapter_file".to_owned(),
            Frag::Int(latest_chapter(root)),
        ),
        ("words".to_owned(), Frag::Int(words as i64)),
        (
            "promises_open".to_owned(),
            promises_open.unwrap_or(Frag::Null),
        ),
        (
            "gaps_unrevealed".to_owned(),
            gaps_unrevealed.unwrap_or(Frag::Null),
        ),
    ]));
    Ok(CmdOut {
        stdout: out + "\n",
        exit: 0,
    })
}

/// 引擎 `cmd_stats`。
pub fn cmd_stats(root: &Path) -> Result<CmdOut, AppError> {
    let files = chapter_files(root);
    let mut out = String::new();
    let mut tot = 0usize;
    for f in &files {
        let n = hanzi(&body(&root.join(BODY_DIR).join(f))?);
        tot += n;
        out.push_str(&format!("{f} {n:7} 字\n"));
    }
    out.push_str(&"-".repeat(30));
    out.push('\n');
    // 引擎仅捕 FileNotFoundError（配置损坏引擎崩溃）；本实现同——只对缺配置容错
    let target = match load_config(root) {
        Ok(c) => c.target_words,
        Err(e) if e.code == ErrorCode::BookNotFound => 0,
        Err(e) => return Err(e),
    };
    out.push_str(&format!(
        "正文合计: {tot} 字 ({} 万)",
        py_fmt_2f(tot as f64 / 10000.0)
    ));
    if target > 0 {
        out.push_str(&format!(
            " / 目标 {} 万 ({}%)",
            target / 10000,
            py_fmt_2f(tot as f64 / target as f64 * 100.0)
        ));
    }
    out.push('\n');
    Ok(CmdOut { stdout: out, exit: 0 })
}

/// 引擎 `cmd_resume`。
pub fn cmd_resume(root: &Path, ch: Option<i64>) -> Result<CmdOut, AppError> {
    Ok(CmdOut {
        stdout: diagnose(root, ch).to_json() + "\n",
        exit: 0,
    })
}

/// 引擎 `cmd_revision`。
pub fn cmd_revision(root: &Path, ch: i64, expected: Option<i64>) -> Result<CmdOut, AppError> {
    let mut s = load_state(root)?;
    let expected = expected.unwrap_or(s.state_revision);
    let r = cascade_revision(root, &mut s, ch, expected)?;
    Ok(CmdOut {
        stdout: r.to_json() + "\n",
        exit: if r.ok { 0 } else { 1 },
    })
}

/// 引擎 `cmd_learn` 默认（harvest + style_stats）。
pub fn cmd_learn(root: &Path, ch: i64) -> Result<CmdOut, AppError> {
    Ok(CmdOut {
        stdout: learn_stdout(root, ch)?,
        exit: 0,
    })
}

/// 引擎 `cmd_learn --absorb`。
pub fn cmd_learn_absorb(root: &Path, ch: i64) -> Result<CmdOut, AppError> {
    let n = crate::learn::absorb_review(root, ch)?;
    Ok(CmdOut {
        stdout: absorb_stdout(n, ch),
        exit: 0,
    })
}

/// 引擎 `cmd_learn --trend`。
pub fn cmd_learn_trend(root: &Path, n: usize) -> Result<CmdOut, AppError> {
    Ok(CmdOut {
        stdout: trend_stdout(&trend(root, n)?),
        exit: 0,
    })
}

/// 引擎 `_parse_chapters`：单章/区间/列表 → 章号列表（升序）。
pub fn parse_chapters(spec: &str) -> Result<Vec<i64>, String> {
    let spec = spec.trim();
    if spec.is_empty() {
        return Err("--chapters 不能为空(例:31 / 31-35 / 31,33,34)".to_owned());
    }
    let mut out = Vec::new();
    for part in spec.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((a, b)) = part.split_once('-') {
            if !a.chars().all(|c| c.is_ascii_digit()) || !b.chars().all(|c| c.is_ascii_digit()) {
                return Err(format!("无法解析章节: '{part}'(例:31 / 31-35 / 31,33,34)"));
            }
            let a: i64 = a.parse().map_err(|_| format!("无法解析章节: '{part}'"))?;
            let b: i64 = b.parse().map_err(|_| format!("无法解析章节: '{part}'"))?;
            if a > b {
                return Err(format!("区间反序: {a}-{b}"));
            }
            out.extend(a..=b);
        } else if part.chars().all(|c| c.is_ascii_digit()) {
            out.push(part.parse().map_err(|_| format!("无法解析章节: '{part}'"))?);
        } else {
            return Err(format!("无法解析章节: '{part}'(例:31 / 31-35 / 31,33,34)"));
        }
    }
    if out.is_empty() {
        return Err("--chapters 解析为空".to_owned());
    }
    Ok(out)
}

/// 引擎 `cmd_batch create`。
pub fn cmd_batch_create(root: &Path, chapters: &[i64]) -> Result<CmdOut, AppError> {
    let plan = create_batch_plan(root, chapters)?;
    // 引擎 cli 落盘 工作区/批次-<%Y%m%d-%H%M%S>.json
    let fname = format!(
        "批次-{}.json",
        plan.created.replace(['-', ':'], "").replace('T', "-")
    );
    let path = root.join("工作区").join(fname);
    std::fs::create_dir_all(path.parent().expect("parent")).map_err(|e| {
        AppError::new(
            ErrorCode::DiskWriteFailed,
            format!("创建工作区目录失败: {e}"),
            false,
        )
    })?;
    std::fs::write(&path, plan.to_json()).map_err(|e| {
        AppError::new(
            ErrorCode::DiskWriteFailed,
            format!("写批次计划失败: {e}"),
            false,
        )
    })?;
    Ok(CmdOut {
        stdout: create_plan_stdout(&plan, &path),
        exit: 0,
    })
}

/// 引擎 `cmd_report`。
pub fn cmd_report(root: &Path, stage: &str, ch: Option<i64>) -> Result<CmdOut, AppError> {
    Ok(CmdOut {
        stdout: author_report(root, stage, ch)? + "\n",
        exit: 0,
    })
}
