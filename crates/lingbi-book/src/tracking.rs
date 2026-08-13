//! tracking state 模块（P2-03）—— 与引擎 `state.py` 追踪状态部分行为等价。
//!
//! 覆盖：new/load/save/init/revision/bump/merge_chapter_facts/active_promises/unrevealed_gaps。
//!
//! 关键约束：`save_state` 的 JSON 输出必须与引擎 `json.dump(s, ensure_ascii=False, indent=1)`
//! **逐字节一致**（P2-15 回放门禁按结果树哈希验收）。因此：
//! - 键序 = 引擎插入序（`IndexMap`，serde 解析保持文件序，dump 按声明序）；
//! - 手写引擎式 JSON 序列化（1 空格缩进、`": "` 分隔、中文原样、JSON 标准转义）。
//!
//! 与引擎的差异（有意，fail-closed 纪律）：
//! - 引擎 `load_state` 对类型错误字段**重置为默认**；本实现返回 `BookCorrupted`（fail-closed）。
//! - 引擎 `save_state` 用 tmp+rename 原子写；本实现 legacy 书仓同；托管书仓
//!   （`.lingbi/` 存在）经事务门（P3 #20，见 `crate::tx`）——引擎无此概念，属扩展。`

use crate::json_engine::{obj, opt_int, render as render_json, Frag};
use crate::state::{fm_list, TRACKING_DIR};
use indexmap::IndexMap;
use lingbi_contracts::{AppError, ErrorCode};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const STATE_FILE: &str = "_tracking-state.json";
const INIT_CARD_PLACEHOLDER: &str = "# 续写状态卡(7 栏,待首章后由 views 重建)\n";
/// 引擎 `summary[:80]`：时间线事件码点上限。
const SUMMARY_LIMIT: usize = 80;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChapterEntry {
    pub status: String,
    pub title: String,
    pub words: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromiseEntry {
    pub status: String,
    #[serde(default)]
    pub open_ch: Option<i64>,
    #[serde(default)]
    pub adv_ch: Option<i64>,
    #[serde(default)]
    pub done_ch: Option<i64>,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfoGapEntry {
    pub revealed: bool,
    #[serde(default)]
    pub plant_ch: Option<i64>,
    #[serde(default)]
    pub reveal_ch: Option<i64>,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimelineEvent {
    pub ch: i64,
    pub event: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityState {
    pub first_ch: i64,
    #[serde(default, rename = "where")]
    pub where_: String,
    #[serde(default)]
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ReaderKnowledge {
    #[serde(default)]
    pub reveals: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackingState {
    #[serde(default = "default_spec")]
    pub spec: i64,
    #[serde(default)]
    pub book: String,
    #[serde(default)]
    pub imported_through_chapter: i64,
    #[serde(default = "default_revision")]
    pub state_revision: i64,
    #[serde(default)]
    pub chapters: IndexMap<String, ChapterEntry>,
    #[serde(default)]
    pub promises: IndexMap<String, PromiseEntry>,
    #[serde(default)]
    pub info_gaps: IndexMap<String, InfoGapEntry>,
    #[serde(default)]
    pub timeline: Vec<TimelineEvent>,
    #[serde(default)]
    pub entity_states: IndexMap<String, EntityState>,
    #[serde(default)]
    pub reader_knowledge: ReaderKnowledge,
}

fn default_spec() -> i64 {
    1
}

fn default_revision() -> i64 {
    1
}

pub(crate) fn state_path(root: &Path) -> std::path::PathBuf {
    root.join(TRACKING_DIR).join(STATE_FILE)
}

/// 引擎 `new_state`：初始权威状态。
pub fn new_state(root: &Path) -> TrackingState {
    TrackingState {
        spec: 1,
        book: root
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "未命名".to_owned()),
        imported_through_chapter: 0,
        state_revision: 1,
        chapters: IndexMap::new(),
        promises: IndexMap::new(),
        info_gaps: IndexMap::new(),
        timeline: Vec::new(),
        entity_states: IndexMap::new(),
        reader_knowledge: ReaderKnowledge { reveals: Vec::new() },
    }
}

/// 引擎 `load_state`：解析 + 最小 schema 补全（缺失字段取默认）。
/// 差异：类型错误 fail-closed（引擎重置）。
pub fn load_state(root: &Path) -> Result<TrackingState, AppError> {
    let path = state_path(root);
    let text = std::fs::read_to_string(&path).map_err(|_| {
        AppError::new(
            ErrorCode::BookNotFound,
            format!("缺追踪状态(先跑 storyrepo init): {}", path.display()),
            false,
        )
    })?;
    let mut s: TrackingState = serde_json::from_str(&text).map_err(|e| {
        AppError::new(
            ErrorCode::BookCorrupted,
            format!("追踪状态损坏: {}。救援:删除后重跑 init,或用 revision 重放重建", e),
            false,
        )
    })?;
    // 引擎最小 schema 补全:缺失键取 base 默认(serde default 已覆盖数值/集合);
    // book 缺失时补 basename(引擎 os.path.basename)。spec=0/空 book 原样保留(引擎同)。
    if s.book.is_empty() {
        s.book = new_state(root).book;
    }
    Ok(s)
}

/// 引擎 `save_state`：原子写（tmp + rename），JSON 输出与引擎 dump 逐字节一致。
/// P3 #20：托管书仓（`.lingbi/` 存在）经事务门（修订守卫 + CAS + 日志 + 回执）；
/// legacy 书仓保持引擎原路径（回放门禁字节锁定）。
pub fn save_state(root: &Path, s: &TrackingState) -> Result<(), AppError> {
    if crate::tx::is_managed(root) {
        crate::tx::write_authoritative_state(root, s)?;
        return Ok(());
    }
    let dir = root.join(TRACKING_DIR);
    std::fs::create_dir_all(&dir).map_err(|e| {
        AppError::new(
            ErrorCode::DiskWriteFailed,
            format!("创建追踪目录失败: {}", e),
            false,
        )
    })?;
    let path = state_path(root);
    let tmp = path.with_extension("json.tmp");
    let json = dump_state(s);
    std::fs::write(&tmp, json).map_err(|e| {
        AppError::new(
            ErrorCode::DiskWriteFailed,
            format!("写追踪状态失败: {}", e),
            false,
        )
    })?;
    std::fs::rename(&tmp, &path).map_err(|e| {
        AppError::new(
            ErrorCode::DiskWriteFailed,
            format!("提交追踪状态失败: {}", e),
            false,
        )
    })?;
    Ok(())
}

/// 引擎 `init_state`：幂等初始化；返回 true 表示本次新建。
pub fn init_state(root: &Path) -> Result<bool, AppError> {
    if state_path(root).exists() {
        return Ok(false);
    }
    let s = new_state(root);
    save_state(root, &s)?;
    for d in ["角色状态", "逐章记录"] {
        std::fs::create_dir_all(root.join(TRACKING_DIR).join(d)).map_err(|e| {
            AppError::new(
                ErrorCode::DiskWriteFailed,
                format!("创建追踪子目录失败: {}", e),
                false,
            )
        })?;
    }
    let ctx = root.join(TRACKING_DIR).join("上下文.md");
    if !ctx.exists() {
        std::fs::write(&ctx, INIT_CARD_PLACEHOLDER).map_err(|e| {
            AppError::new(
                ErrorCode::DiskWriteFailed,
                format!("写状态卡占位失败: {}", e),
                false,
            )
        })?;
    }
    Ok(true)
}

/// 引擎 `revision`：当前修订号（原样返回，引擎 `int(s.get("state_revision", 1))`）。
pub fn revision(s: &TrackingState) -> i64 {
    s.state_revision
}

/// 引擎 `bump_revision`：修订号 +1，返回新值。
pub fn bump_revision(s: &mut TrackingState) -> i64 {
    s.state_revision += 1;
    s.state_revision
}

/// 引擎 `merge_chapter_facts`：合并一章事实；返回警告列表。
/// 差异（fail-closed）：引擎对非数字章节键抛 ValueError；本实现返回 `BookCorrupted`。
#[allow(clippy::too_many_lines)]
pub fn merge_chapter_facts(
    s: &mut TrackingState,
    ch: i64,
    fm: &std::collections::HashMap<String, String>,
    summary: &str,
) -> Result<Vec<String>, AppError> {
    let mut warn = Vec::new();
    for p in fm_list(fm, "promises_new") {
        s.promises
            .entry(p)
            .or_insert(PromiseEntry {
                status: "open".to_owned(),
                open_ch: Some(ch),
                adv_ch: Some(ch),
                done_ch: None,
                note: String::new(),
            });
    }
    for p in fm_list(fm, "promises_advanced") {
        if let Some(entry) = s.promises.get_mut(&p) {
            entry.adv_ch = Some(ch);
            entry.status = "adv".to_owned();
        } else {
            warn.push(format!(
                "未知承诺 id 被忽略: {p}(请检查拼写或先在 promises_new 登记)"
            ));
        }
    }
    for p in fm_list(fm, "promises_done") {
        if let Some(entry) = s.promises.get_mut(&p) {
            entry.status = "done".to_owned();
            entry.done_ch = Some(ch);
        } else {
            warn.push(format!(
                "未知承诺 id 被忽略: {p}(请检查拼写或先在 promises_new 登记)"
            ));
        }
    }
    for g in fm_list(fm, "info_gaps_new") {
        s.info_gaps.entry(g).or_insert(InfoGapEntry {
            revealed: false,
            plant_ch: Some(ch),
            reveal_ch: None,
            note: String::new(),
        });
    }
    for g in fm_list(fm, "info_gaps_revealed") {
        if let Some(entry) = s.info_gaps.get_mut(&g) {
            entry.revealed = true;
            entry.reveal_ch = Some(ch);
        } else {
            warn.push(format!(
                "未知信息差 id 被忽略: {g}(请检查拼写或在 info_gaps_new 登记)"
            ));
        }
    }
    for e in fm_list(fm, "entities_new") {
        s.entity_states.entry(e).or_insert(EntityState {
            first_ch: ch,
            where_: String::new(),
            state: String::new(),
        });
    }
    if !summary.is_empty() {
        // 引擎 summary[:80]：码点截断（Rust truncate 按字节会 panic，不可用）
        let event: String = summary.chars().take(SUMMARY_LIMIT).collect();
        s.timeline.push(TimelineEvent {
            ch,
            event,
            kind: "fact".to_owned(),
        });
    }
    s.chapters.insert(
        ch.to_string(),
        ChapterEntry {
            status: "committed".to_owned(),
            title: fm.get("title").cloned().unwrap_or_default(),
            words: fm.get("words").cloned().unwrap_or_default(),
        },
    );
    // 引擎 int(k) 对非数字键抛 ValueError；本实现 fail-closed
    s.imported_through_chapter = s
        .chapters
        .keys()
        .map(|k| {
            k.parse::<i64>().map_err(|_| {
                AppError::new(
                    ErrorCode::BookCorrupted,
                    format!("追踪状态章节键非数字: {k}"),
                    false,
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .max()
        .unwrap_or(0);
    Ok(warn)
}

/// 引擎 `active_promises`。
pub fn active_promises(s: &TrackingState) -> Vec<(&String, &PromiseEntry)> {
    s.promises
        .iter()
        .filter(|(_, v)| v.status != "done")
        .collect()
}

/// 引擎 `unrevealed_gaps`。
pub fn unrevealed_gaps(s: &TrackingState) -> Vec<(&String, &InfoGapEntry)> {
    s.info_gaps
        .iter()
        .filter(|(_, v)| !v.revealed)
        .collect()
}

// ── 引擎式 JSON 序列化（json.dump ensure_ascii=False indent=1 逐字节等价） ──
// Frag/esc/dump/obj/opt_int 已上提共享模块 json_engine（P2-07 起 contract 共用）。

impl TrackingState {
    fn to_frag(&self) -> Frag {
        obj(vec![
            ("spec".to_owned(), Frag::Int(self.spec)),
            ("book".to_owned(), Frag::Str(self.book.clone())),
            (
                "imported_through_chapter".to_owned(),
                Frag::Int(self.imported_through_chapter),
            ),
            ("state_revision".to_owned(), Frag::Int(self.state_revision)),
            ("chapters".to_owned(), self.chapters_to_frag()),
            ("promises".to_owned(), self.promises_to_frag()),
            ("info_gaps".to_owned(), self.info_gaps_to_frag()),
            ("timeline".to_owned(), self.timeline_to_frag()),
            ("entity_states".to_owned(), self.entity_states_to_frag()),
            (
                "reader_knowledge".to_owned(),
                obj(vec![(
                    "reveals".to_owned(),
                    Frag::Arr(
                        self.reader_knowledge
                            .reveals
                            .iter()
                            .map(json_value_to_frag)
                            .collect(),
                    ),
                )]),
            ),
        ])
    }

    fn chapters_to_frag(&self) -> Frag {
        Frag::Obj(
            self.chapters
                .iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        obj(vec![
                            ("status".to_owned(), Frag::Str(v.status.clone())),
                            ("title".to_owned(), Frag::Str(v.title.clone())),
                            ("words".to_owned(), Frag::Str(v.words.clone())),
                        ]),
                    )
                })
                .collect(),
        )
    }

    fn promises_to_frag(&self) -> Frag {
        Frag::Obj(
            self.promises
                .iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        obj(vec![
                            ("status".to_owned(), Frag::Str(v.status.clone())),
                            (
                                "open_ch".to_owned(),
                                opt_int(v.open_ch),
                            ),
                            (
                                "adv_ch".to_owned(),
                                opt_int(v.adv_ch),
                            ),
                            (
                                "done_ch".to_owned(),
                                opt_int(v.done_ch),
                            ),
                            ("note".to_owned(), Frag::Str(v.note.clone())),
                        ]),
                    )
                })
                .collect(),
        )
    }

    fn info_gaps_to_frag(&self) -> Frag {
        Frag::Obj(
            self.info_gaps
                .iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        obj(vec![
                            ("revealed".to_owned(), Frag::Bool(v.revealed)),
                            (
                                "plant_ch".to_owned(),
                                opt_int(v.plant_ch),
                            ),
                            (
                                "reveal_ch".to_owned(),
                                opt_int(v.reveal_ch),
                            ),
                            ("note".to_owned(), Frag::Str(v.note.clone())),
                        ]),
                    )
                })
                .collect(),
        )
    }

    fn timeline_to_frag(&self) -> Frag {
        Frag::Arr(
            self.timeline
                .iter()
                .map(|e| {
                    obj(vec![
                        ("ch".to_owned(), Frag::Int(e.ch)),
                        ("event".to_owned(), Frag::Str(e.event.clone())),
                        ("kind".to_owned(), Frag::Str(e.kind.clone())),
                    ])
                })
                .collect(),
        )
    }

    fn entity_states_to_frag(&self) -> Frag {
        Frag::Obj(
            self.entity_states
                .iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        obj(vec![
                            ("first_ch".to_owned(), Frag::Int(v.first_ch)),
                            ("where".to_owned(), Frag::Str(v.where_.clone())),
                            ("state".to_owned(), Frag::Str(v.state.clone())),
                        ]),
                    )
                })
                .collect(),
        )
    }
}

/// serde_json::Value → 引擎式 Frag（reveals 内容兜底）。
/// 注：对象键序遵循 serde_json::Map（未开 preserve_order 时按字母序；
/// 引擎为插入序——当前 reveals 恒空，写入方（P7/P8）落地前需对齐）。
fn json_value_to_frag(v: &serde_json::Value) -> Frag {
    match v {
        serde_json::Value::Null => Frag::Null,
        serde_json::Value::Bool(b) => Frag::Bool(*b),
        serde_json::Value::Number(n) => Frag::Raw(n.to_string()),
        serde_json::Value::String(s) => Frag::Str(s.clone()),
        serde_json::Value::Array(items) => {
            Frag::Arr(items.iter().map(json_value_to_frag).collect())
        }
        serde_json::Value::Object(map) => Frag::Obj(
            map.iter()
                .map(|(k, v)| (k.clone(), json_value_to_frag(v)))
                .collect(),
        ),
    }
}

/// 引擎 `json.dump(s, ensure_ascii=False, indent=1)` 的逐字节等价输出。
pub fn dump_state(s: &TrackingState) -> String {
    render_json(&s.to_frag())
}
