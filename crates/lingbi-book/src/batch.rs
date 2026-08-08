//! batch 模块（P2-12）—— 与引擎 `batch.py` 等价：批次计划与顺序合并。
//!
//! 引擎只负责计划 JSON 结构与顺序合并；子代理编排（并行起草）发生在技能层。
//! - create_batch_plan(root, chapters)：生成计划（每章：章号/合同/正文输出 glob/状态）。
//! - merge_batch(root, plan, chapters_done)：按章序逐章走既有 commit 路径
//!   （读 front matter → merge_chapter_facts → bump_revision → save_state → write_views）；
//!   单章机检失败标记 rejected 不阻塞同批；已提交章由幂等守卫整批拒绝（报错，零写入）。
//!
//! 与引擎的差异（有意，fail-closed 纪律）：
//! - 引擎 plan 为任意 dict（非法计划 raise ValueError，CLI 未捕获即崩溃）；本实现
//!   以类型化 BatchPlan 承载（非法结构在反序列化层报错），非法计划错误不可达。
//! - 引擎 plan 无 expected_revision 键时跳过并发守卫；本实现类型化计划必有该字段
//!   （create_batch_plan 恒写入），不可达。
//! - `created` 时间戳用 chrono::Local（引擎 datetime.now() 本地时区，ISO seconds）。

use crate::contract::{build_contract, Contract};
use crate::gates::gate;
use crate::json_engine::{frag_strs, obj, render as render_json, Frag};
use crate::state::frontmatter;
use crate::tracking::{bump_revision, load_state, merge_chapter_facts, save_state};
use crate::views::write_views;
use indexmap::IndexMap;
use lingbi_contracts::{AppError, ErrorCode};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub const MODE: &str = "subagent-parallel";

/// 批次计划（键序与引擎 dict 一致，to_json 逐字节等价）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchPlan {
    pub book: String,
    pub mode: String,
    pub created: String,
    pub expected_revision: i64,
    pub chapters: Vec<BatchPlanChapter>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchPlanChapter {
    pub chapter: i64,
    pub contract: Contract,
    pub output_path: String,
    pub status: String,
}

impl BatchPlan {
    /// 引擎 `json.dump(plan, ensure_ascii=False, indent=1)` 的逐字节等价输出。
    pub fn to_json(&self) -> String {
        render_json(&obj(vec![
            ("book".to_owned(), Frag::Str(self.book.clone())),
            ("mode".to_owned(), Frag::Str(self.mode.clone())),
            ("created".to_owned(), Frag::Str(self.created.clone())),
            ("expected_revision".to_owned(), Frag::Int(self.expected_revision)),
            (
                "chapters".to_owned(),
                Frag::Arr(
                    self.chapters
                        .iter()
                        .map(|c| {
                            obj(vec![
                                ("chapter".to_owned(), Frag::Int(c.chapter)),
                                ("contract".to_owned(), c.contract.to_frag()),
                                ("output_path".to_owned(), Frag::Str(c.output_path.clone())),
                                ("status".to_owned(), Frag::Str(c.status.clone())),
                            ])
                        })
                        .collect(),
                ),
            ),
        ]))
    }
}

/// 被拒条目（引擎 rejected 元素：fails 或 reason 二选一）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RejectedEntry {
    pub chapter: i64,
    pub fails: Option<Vec<String>>,
    pub reason: Option<String>,
}

/// 批次合并结果（键序与引擎 dict 一致，to_json 逐字节等价）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchMergeResult {
    pub merged: Vec<i64>,
    pub rejected: Vec<RejectedEntry>,
    pub skipped: Vec<i64>,
    pub warns: IndexMap<i64, Vec<String>>,
    pub revision: i64,
    pub ok: bool,
}

impl BatchMergeResult {
    /// 引擎 `json.dumps(r, ensure_ascii=False, indent=1)` 的逐字节等价输出。
    pub fn to_json(&self) -> String {
        render_json(&obj(vec![
            (
                "merged".to_owned(),
                Frag::Arr(self.merged.iter().map(|n| Frag::Int(*n)).collect()),
            ),
            (
                "rejected".to_owned(),
                Frag::Arr(
                    self.rejected
                        .iter()
                        .map(|r| {
                            let mut entries = vec![("chapter".to_owned(), Frag::Int(r.chapter))];
                            if let Some(fails) = &r.fails {
                                entries.push(("fails".to_owned(), frag_strs(fails)));
                            }
                            if let Some(reason) = &r.reason {
                                entries.push(("reason".to_owned(), Frag::Str(reason.clone())));
                            }
                            obj(entries)
                        })
                        .collect(),
                ),
            ),
            (
                "skipped".to_owned(),
                Frag::Arr(self.skipped.iter().map(|n| Frag::Int(*n)).collect()),
            ),
            (
                "warns".to_owned(),
                obj(
                    self.warns
                        .iter()
                        .map(|(k, v)| (k.to_string(), frag_strs(v)))
                        .collect(),
                ),
            ),
            ("revision".to_owned(), Frag::Int(self.revision)),
            ("ok".to_owned(), Frag::Bool(self.ok)),
        ]))
    }
}

/// 第 ch 章正文输出路径（glob 模板）：标题由 worker 在 front matter 自定，故以 * 匹配。
fn body_glob(root: &Path, ch: i64) -> String {
    root.join("定稿")
        .join("正文")
        .join(format!("{ch:03}-*.md"))
        .display()
        .to_string()
}

/// 生成批次计划（引擎 `create_batch_plan`）。chapters 自动升序去重。
pub fn create_batch_plan(root: &Path, chapters: &[i64]) -> Result<BatchPlan, AppError> {
    let s = load_state(root)?;
    let mut chs: Vec<i64> = chapters.to_vec();
    chs.sort_unstable();
    chs.dedup();
    let items = chs
        .iter()
        .map(|&ch| BatchPlanChapter {
            chapter: ch,
            contract: build_contract(root, &s, ch),
            output_path: body_glob(root, ch),
            status: "pending".to_owned(),
        })
        .collect();
    Ok(BatchPlan {
        book: s.book.clone(),
        mode: MODE.to_owned(),
        created: chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
        expected_revision: s.state_revision,
        chapters: items,
    })
}

/// 引擎 cli.cmd_batch create 的两行输出（含尾换行，逐字节一致）。
pub fn create_plan_stdout(plan: &BatchPlan, path: &Path) -> String {
    format!(
        "✅ 批次计划已生成: {}({} 章, mode={})\n   worker 协议:每个子代理读 状态卡+合同(计划内) → 写正文+front matter → doctor precommit 通过 → 返回\n",
        path.display(),
        plan.chapters.len(),
        plan.mode
    )
}

/// 按章序合并批次计划（引擎 `merge_batch`）。chapters_done 为已返回章集合
/// （引擎 dict 值仅做成员判定）；None = 全部视为已返回。
pub fn merge_batch(
    root: &Path,
    plan: &BatchPlan,
    chapters_done: Option<&BTreeSet<i64>>,
) -> Result<BatchMergeResult, AppError> {
    let mut s = load_state(root)?;
    // 并发守卫：计划固化的修订号与当前不符 → 拒绝（防止并发 merge 交错污染）
    if plan.expected_revision != s.state_revision {
        return Err(AppError::new(
            ErrorCode::BookCorrupted,
            format!(
                "批次计划期望 rev {},当前 {}——状态已变化,请重建计划",
                plan.expected_revision, s.state_revision
            ),
            false,
        ));
    }
    // M1 幂等守卫：计划中任何章已提交 → 整批拒绝（重复 merge 同一计划在此被拦下）
    let mut dup_chapters: Vec<i64> = plan
        .chapters
        .iter()
        .map(|c| c.chapter)
        .filter(|ch| s.chapters.contains_key(&ch.to_string()))
        .collect();
    dup_chapters.sort_unstable();
    if !dup_chapters.is_empty() {
        return Err(AppError::new(
            ErrorCode::BookCorrupted,
            format!(
                "批次含已提交章 {}(rev {})。重写走 storyrepo revision --chapter N(重放式级联);或为剩余章另建计划。",
                dup_chapters.iter().map(i64::to_string).collect::<Vec<_>>().join("、"),
                s.state_revision
            ),
            false,
        ));
    }
    // 预刷新（无 bump 保存 + 派生视图重建）：批次正文全部由 worker 预写，
    // 该刷新既满足机检「追踪新鲜」检查，也修复中断遗留的「状态新、卡旧」中间态
    save_state(root, &s)?;
    write_views(root, &s)?;

    let returned: BTreeSet<i64> = match chapters_done {
        Some(set) => set.clone(),
        None => plan.chapters.iter().map(|c| c.chapter).collect(),
    };
    // 引擎 items.sort(key=chapter)：章序合并（手改乱序计划文件时写盘顺序一致）
    let mut ordered: Vec<&BatchPlanChapter> = plan.chapters.iter().collect();
    ordered.sort_by_key(|c| c.chapter);
    let mut merged: Vec<i64> = Vec::new();
    let mut rejected: Vec<RejectedEntry> = Vec::new();
    let mut skipped: Vec<i64> = Vec::new();
    let mut warns: IndexMap<i64, Vec<String>> = IndexMap::new();
    for item in ordered {
        let ch = item.chapter;
        if ch < 1 {
            // 引擎校验：章号须 ≥1（类型化 plan 下为防御）
            return Err(AppError::new(
                ErrorCode::BookCorrupted,
                format!("批次计划非法:章号须 ≥1,收到 {ch}"),
                false,
            ));
        }
        if !returned.contains(&ch) {
            skipped.push(ch);
            continue;
        }
        let raw_pattern = &item.output_path;
        let pattern = if Path::new(raw_pattern).is_absolute() {
            raw_pattern.clone()
        } else {
            root.join(raw_pattern).display().to_string()
        };
        // 引擎 glob 永不抛错：坏 pattern → 空集 → rejected 缺正文
        let mut files: Vec<PathBuf> = match glob::glob(&pattern) {
            Ok(it) => it.filter_map(Result::ok).collect(),
            Err(_) => Vec::new(),
        };
        files.sort();
        if files.is_empty() {
            rejected.push(RejectedEntry {
                chapter: ch,
                fails: None,
                reason: Some(format!("缺正文(worker 未产出文件): {pattern}")),
            });
            continue;
        }
        if files.len() > 1 {
            rejected.push(RejectedEntry {
                chapter: ch,
                fails: None,
                reason: Some(format!(
                    "正文文件不唯一: {}",
                    files
                        .iter()
                        .map(|f| f.display().to_string())
                        .collect::<Vec<_>>()
                        .join("、")
                )),
            });
            continue;
        }
        let g = gate(root, "precommit", Some(ch))?;
        if !g.passed {
            rejected.push(RejectedEntry {
                chapter: ch,
                fails: Some(g.fails),
                reason: None,
            });
            continue;
        }
        let fm = frontmatter(&files[0])?;
        warns.insert(
            ch,
            merge_chapter_facts(
                &mut s,
                ch,
                &fm,
                fm.get("summary").map(String::as_str).unwrap_or_default(),
            )?,
        );
        bump_revision(&mut s);
        save_state(root, &s)?;
        write_views(root, &s)?;
        merged.push(ch);
    }
    let ok = !merged.is_empty() && rejected.is_empty() && skipped.is_empty();
    Ok(BatchMergeResult {
        merged,
        rejected,
        skipped,
        warns,
        revision: s.state_revision,
        ok,
    })
}
