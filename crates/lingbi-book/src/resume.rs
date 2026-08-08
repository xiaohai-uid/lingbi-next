//! resume 模块（P2-11）—— 与引擎 `resume.py` 等价：断点续跑诊断。
//!
//! 依据 正文/评审 json/追踪 chapters/状态卡新鲜 判定进度，输出恢复建议
//! （只建议，不覆盖）。依赖 review 读侧（load_review/has_blocking）。
//!
//! 与引擎的差异（有意，fail-closed 纪律）：
//! - 引擎 `load_state` 只捕获 FileNotFoundError（状态损坏时 ValueError 传播导致
//!   diagnose 崩溃）；本实现任何加载失败均按无状态处理（"缺追踪状态" 后缀）——
//!   语料状态均有效，差异不可达。

use crate::json_engine::{obj, render as render_json, Frag};
use crate::review::{has_blocking, load_review, review_path};
use crate::state::{chapter_files, chapter_num, latest_chapter, BODY_DIR, TRACKING_DIR};
use crate::tracking::load_state;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// 断点诊断结果（键序与引擎 dict 一致，to_json 逐字节等价）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnoseResult {
    pub status: String,
    pub next_step: String,
    /// 有序 files：正文/评审报告/追踪已合并/状态卡新鲜。
    pub files: Vec<(String, bool)>,
}

impl DiagnoseResult {
    /// 引擎 `json.dumps(r, ensure_ascii=False, indent=1)` 的逐字节等价输出。
    pub fn to_json(&self) -> String {
        render_json(&obj(vec![
            ("status".to_owned(), Frag::Str(self.status.clone())),
            ("next_step".to_owned(), Frag::Str(self.next_step.clone())),
            (
                "files".to_owned(),
                obj(
                    self.files
                        .iter()
                        .map(|(k, v)| (k.clone(), Frag::Bool(*v)))
                        .collect(),
                ),
            ),
        ]))
    }
}

fn chapter_path(root: &Path, ch: i64) -> Option<PathBuf> {
    for f in chapter_files(root) {
        if chapter_num(&f) == ch {
            return Some(root.join(BODY_DIR).join(&f));
        }
    }
    None
}

/// stat 失败按远古时间处理（引擎 getmtime 会崩溃；文件刚由 chapter_files 列出，
/// 不可达——容错仅为防御，见模块头差异说明）。
fn mtime(p: &Path) -> SystemTime {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .unwrap_or(UNIX_EPOCH)
}

/// 引擎 `diagnose`：判定第 ch 章（None → latest+1）进度，输出恢复建议。
pub fn diagnose(root: &Path, ch: Option<i64>) -> DiagnoseResult {
    let latest = latest_chapter(root);
    let target = match ch {
        Some(c) => c,
        None if latest > 0 => latest + 1,
        None => 1,
    };
    let body_ok = chapter_path(root, target).is_some();
    let review_p = review_path(root, target);
    let review_ok = review_p.exists();
    // 引擎 except FileNotFoundError → None（损坏状态引擎崩溃，本实现容错，见模块头）
    let s = load_state(root).ok();
    let tracked_ok = s
        .as_ref()
        .map(|s| s.chapters.contains_key(&target.to_string()))
        .unwrap_or(false);
    let files = chapter_files(root);
    let card_ok = if files.is_empty() {
        true
    } else {
        let card_p = root.join(TRACKING_DIR).join("上下文.md");
        let newest = files
            .iter()
            .map(|f| mtime(&root.join(BODY_DIR).join(f)))
            .max()
            .unwrap_or(UNIX_EPOCH);
        card_p.exists() && mtime(&card_p) >= newest
    };
    let fm = vec![
        ("正文".to_owned(), body_ok),
        ("评审报告".to_owned(), review_ok),
        ("追踪已合并".to_owned(), tracked_ok),
        ("状态卡新鲜".to_owned(), card_ok),
    ];

    let (status, mut step) = if !body_ok {
        if review_ok {
            (
                "unknown",
                format!("异常:第{target}章有评审报告但无正文,请人工核查,勿直接续写覆盖"),
            )
        } else {
            (
                "not_started",
                format!("第{target}章尚未动笔:先跑机检 prewrite,再按 合同→任务书→起草 推进"),
            )
        }
    } else {
        let mut corrupt = false;
        let mut blocking = false;
        if review_ok {
            match load_review(&review_p) {
                Ok((issues, errors)) => {
                    corrupt = !errors.is_empty();
                    blocking = has_blocking(&issues) || corrupt;
                }
                Err(_) => {
                    corrupt = true;
                    blocking = true;
                }
            }
        }
        if !review_ok {
            (
                "draft",
                "正文已写未评审:跑机检 precommit,再评审(Step 5)".to_owned(),
            )
        } else if corrupt {
            (
                "draft",
                "评审报告损坏:重跑评审(Step 5)或用户裁决".to_owned(),
            )
        } else if blocking {
            (
                "draft",
                "评审有 blocking:定点修复后直接进润色,不重跑评审".to_owned(),
            )
        } else if tracked_ok {
            let mut step = format!("第{target}章已结算:续写第{}章或结转", target + 1);
            if !card_ok {
                step.push_str(";状态卡过期,先重建视图再续写");
            }
            ("committed", step)
        } else {
            (
                "reviewed",
                "评审通过未结算:跑追踪合并 + 重建派生视图(Step 7)".to_owned(),
            )
        }
    };
    if s.is_none() && body_ok {
        step.push_str("(缺追踪状态:先 init_state)");
    }

    DiagnoseResult {
        status: status.to_owned(),
        next_step: step,
        files: fm,
    }
}
