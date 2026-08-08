//! revision 模块（P2-10）—— 与引擎 `revision.py` 等价：回炉级联。
//!
//! 重放式重算（1..latest 由章节文件重建状态，天然幂等）+ expected_state_revision
//! 并发控制 + 三轨合并（timeline 非 fact 事件按 (ch, 原序) 插回）+ 影响扫描 +
//! bump+save+视图重建。输出 {"ok", "conflict", "impact", "notes"}。
//!
//! 与引擎的差异（有意，fail-closed 纪律）：
//! - 引擎 `_replay` 保留旧状态中 new_state 未定义的键（dict 任意键）；本实现
//!   TrackingState 字段固定，引擎写出的状态无额外键（语料全部一致），差异不可达。
//! - 非 fact timeline 事件仅保留 ch/event/kind 三字段（serde 丢未知字段）；引擎
//!   dict 原样保留全部字段——当前语料事件均为三字段，差异不可达。
//! - 视图重建失败时引擎在 notes 记录 Python 异常文本；本实现记录 AppError 文本
//!   （文案差异，等价范围内不可达——语料视图均成功重建）。

use crate::json_engine::{obj, render as render_json, Frag};
use crate::state::{chapter_files, chapter_num, fm_list, frontmatter, latest_chapter, BODY_DIR};
use crate::tracking::{
    bump_revision, merge_chapter_facts, new_state, save_state, TimelineEvent, TrackingState,
};
use crate::views::write_views;
use lingbi_contracts::AppError;
use std::collections::BTreeSet;
use std::path::Path;

/// 回炉级联的结果（键序与引擎返回 dict 一致，to_json 逐字节等价）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevisionResult {
    pub ok: bool,
    pub conflict: bool,
    pub impact: Vec<String>,
    pub notes: Vec<String>,
}

impl RevisionResult {
    /// 引擎 `json.dumps(r, ensure_ascii=False, indent=1)` 的逐字节等价输出。
    pub fn to_json(&self) -> String {
        render_json(&obj(vec![
            ("ok".to_owned(), Frag::Bool(self.ok)),
            ("conflict".to_owned(), Frag::Bool(self.conflict)),
            (
                "impact".to_owned(),
                Frag::Arr(
                    self.impact
                        .iter()
                        .map(|s| Frag::Str(s.clone()))
                        .collect(),
                ),
            ),
            (
                "notes".to_owned(),
                Frag::Arr(
                    self.notes
                        .iter()
                        .map(|s| Frag::Str(s.clone()))
                        .collect(),
                ),
            ),
        ]))
    }
}

/// 非 fact 时间线事件（三轨保留的对象；引擎 `kind not in ("fact", None)`）。
fn is_non_fact(ev: &TimelineEvent) -> bool {
    ev.kind != "fact"
}

/// 引擎 `_replay`：从头（1..latest）重放全部章节 front matter 与摘要，幂等正确。
fn replay(root: &Path, state: &TrackingState) -> Result<TrackingState, AppError> {
    let mut fresh = new_state(root);
    for f in chapter_files(root) {
        let n = chapter_num(&f);
        if n < 1 {
            continue;
        }
        let fm = frontmatter(&root.join(BODY_DIR).join(&f))?;
        merge_chapter_facts(
            &mut fresh,
            n,
            &fm,
            fm.get("summary").map(String::as_str).unwrap_or_default(),
        )?;
    }
    // 三轨保留：merge 只产生 fact 事件，reveal/private 事件按 (ch, 原序) 插回，
    // 保持时间线交错顺序（插到最后一个 ch 不大于本事件的 fact 之后）
    let mut non_fact: Vec<(usize, TimelineEvent)> = state
        .timeline
        .iter()
        .enumerate()
        .filter(|(_, ev)| is_non_fact(ev))
        .map(|(i, ev)| (i, ev.clone()))
        .collect();
    non_fact.sort_by_key(|(i, ev)| (ev.ch, *i));
    for (_, ev) in non_fact {
        let ch = ev.ch;
        let mut pos = 0;
        for (j, e) in fresh.timeline.iter().enumerate() {
            if e.ch <= ch {
                pos = j + 1;
            }
        }
        fresh.timeline.insert(pos, ev);
    }
    Ok(fresh)
}

/// 引擎 `_changed_ids`：重放前后 P/S/实体 id 集合差异。
fn changed_ids(old: &TrackingState, fresh: &TrackingState) -> BTreeSet<String> {
    let mut changed = BTreeSet::new();
    for k in old.promises.keys().chain(fresh.promises.keys()) {
        if old.promises.get(k) != fresh.promises.get(k) {
            changed.insert(k.clone());
        }
    }
    for k in old.info_gaps.keys().chain(fresh.info_gaps.keys()) {
        if old.info_gaps.get(k) != fresh.info_gaps.get(k) {
            changed.insert(k.clone());
        }
    }
    for k in old.entity_states.keys().chain(fresh.entity_states.keys()) {
        if old.entity_states.get(k) != fresh.entity_states.get(k) {
            changed.insert(k.clone());
        }
    }
    changed
}

/// 引擎 `_scan_impact`：扫描其他章节 front matter 引用已变 P/S/实体。
fn scan_impact(root: &Path, ch: i64, changed: &BTreeSet<String>) -> Result<Vec<String>, AppError> {
    let mut impact = Vec::new();
    for f in chapter_files(root) {
        let n = chapter_num(&f);
        if n == ch || n < 1 {
            continue;
        }
        let fm = frontmatter(&root.join(BODY_DIR).join(&f))?;
        let mut refs: Vec<String> = Vec::new();
        for key in [
            "promises_new",
            "promises_advanced",
            "promises_done",
            "info_gaps_new",
            "info_gaps_revealed",
            "entities_new",
        ] {
            refs.extend(fm_list(&fm, key));
        }
        let mut hit: Vec<&String> = refs.iter().filter(|r| changed.contains(*r)).collect();
        hit.sort();
        hit.dedup();
        if !hit.is_empty() {
            impact.push(format!(
                "第{n}章 引用 {}(状态已变),需同步检查",
                hit.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("、")
            ));
        }
    }
    Ok(impact)
}

/// 回炉级联（引擎 `cascade_revision`）：重放 → 冲突检测 → 影响扫描 → bump+save → 重建视图。
/// `state` 会被更新为级联后的新状态（引擎 state.clear/update 语义）。
pub fn cascade_revision(
    root: &Path,
    state: &mut TrackingState,
    ch: i64,
    expected_revision: i64,
) -> Result<RevisionResult, AppError> {
    if expected_revision != state.state_revision {
        return Ok(RevisionResult {
            ok: false,
            conflict: true,
            impact: Vec::new(),
            notes: vec![format!(
                "期望修订号 {expected_revision} 与当前 {} 不符,拒绝级联(请重读最新状态)",
                state.state_revision
            )],
        });
    }
    let latest = latest_chapter(root);
    let mut fresh = replay(root, state)?;
    fresh.state_revision = state.state_revision; // 保留并发号，随后 bump
    let changed = changed_ids(state, &fresh);
    let impact = scan_impact(root, ch, &changed)?;
    bump_revision(&mut fresh);
    save_state(root, &fresh)?;
    *state = fresh;

    let mut notes = vec![format!("重放范围 1..{latest} 章,状态由章节文件重算(幂等)")];
    let preserved = state.timeline.iter().filter(|ev| is_non_fact(ev)).count();
    if preserved > 0 {
        notes.push(format!("timeline 保留非 fact 事件 {preserved} 条"));
    }
    if changed.is_empty() {
        notes.push("重放结果与旧状态一致(无事实变更)".to_owned());
    } else {
        notes.push(format!(
            "受影响的 P/S/实体 {} 项: {}",
            changed.len(),
            changed.iter().map(String::as_str).collect::<Vec<_>>().join("、")
        ));
    }
    match write_views(root, state) {
        Ok(()) => notes.push("派生视图已重建".to_owned()),
        Err(e) => notes.push(format!("派生视图重建失败: {e}")),
    }
    notes.push("级联完成;续写前请再跑 checks.gate(postcommit) 确认全绿".to_owned());

    Ok(RevisionResult {
        ok: true,
        conflict: false,
        impact,
        notes,
    })
}
