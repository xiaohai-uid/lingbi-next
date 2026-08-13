//! book.yaml 配置解析 —— 与 storyrepo 引擎 `config.py` 行为等价（P2-01）。
//!
//! 引擎语义（黑盒基准）：
//! - 平铺 KV（front-matter 风格），`---` 分节时取首节后的内容；
//! - 数值字段按正则 `-?\d+` 转 int，否则保留字符串；
//! - 缺失字段合并默认值（name=未命名 / total=330 / min=3000 / max=3800 / target=990000）；
//! - `max < min` 时修正为 `min + 800`；
//! - 缺失文件 → FileNotFoundError。
//!
//! 本实现与引擎的差异（有意，fail-closed 纪律）：
//! - 数值键收到非整数字符串时返回 `BookCorrupted`（引擎宽容地存字符串）。

use lingbi_contracts::{AppError, ErrorCode};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub const BOOK_YAML: &str = "book.yaml";

/// 引擎 config.py DEFAULTS 的 Rust 映射。
pub const DEFAULT_NAME: &str = "未命名";
pub const DEFAULT_TOTAL_CHAPTERS: i64 = 330;
pub const DEFAULT_MIN_WORDS: i64 = 3000;
pub const DEFAULT_MAX_WORDS: i64 = 3800;
pub const DEFAULT_TARGET_WORDS: i64 = 990000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookConfig {
    pub name: String,
    /// 引擎 DEFAULTS 无 status；缺失时缺席（None）。
    pub status: Option<String>,
    /// Book V3 扩展：书 id（uuid，迁移时生成）。
    pub id: Option<String>,
    pub total_chapters: i64,
    pub min_words_per_chapter: i64,
    pub max_words_per_chapter: i64,
    pub target_words: i64,
    /// 未知键保留（引擎 dict 全量返回）；数值键被字符串覆盖时 fail-closed。
    #[serde(default)]
    pub extras: BTreeMap<String, serde_json::Value>,
}

impl Default for BookConfig {
    fn default() -> Self {
        Self {
            name: DEFAULT_NAME.to_owned(),
            status: None,
            id: None,
            total_chapters: DEFAULT_TOTAL_CHAPTERS,
            min_words_per_chapter: DEFAULT_MIN_WORDS,
            max_words_per_chapter: DEFAULT_MAX_WORDS,
            target_words: DEFAULT_TARGET_WORDS,
            extras: BTreeMap::new(),
        }
    }
}

fn is_engine_int(v: &str) -> bool {
    // 引擎正则 fullmatch(r"-?\d+")：可选负号 + 至少一个 ASCII 数字
    let digits = v.strip_prefix('-').unwrap_or(v);
    !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
}

/// 加载书配置。`root` 为书仓根（含 book.yaml）。
pub fn load_config(root: &Path) -> Result<BookConfig, AppError> {
    let path = root.join(BOOK_YAML);
    let text = std::fs::read_to_string(&path).map_err(|_| {
        AppError::new(
            ErrorCode::BookNotFound,
            format!("缺 book.yaml: {}", path.display()),
            false,
        )
    })?;
    // 引擎: t.split("---", 2)[1] —— 第一与第二个 --- 之间的段（引擎用 starts_with，无 trim）
    let body = if text.starts_with("---") {
        text.split("---").nth(1).unwrap_or("")
    } else {
        text.as_str()
    };
    let mut cfg = BookConfig::default();
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() || !line.contains(':') {
            continue;
        }
        let (k, v) = line.split_once(':').expect("含冒号");
        let k = k.trim();
        if k.is_empty() {
            continue;
        }
        let v = v.trim().trim_matches(|c| c == '"' || c == '\'');
        if is_engine_int(v) {
            // 引擎 Python int 任意精度；Rust i64 溢出时 fail-closed（不 panic）
            let n: i64 = v.parse().map_err(|_| {
                AppError::new(
                    ErrorCode::BookCorrupted,
                    format!("book.yaml 字段 {k} 超出 i64 范围: {v}"),
                    false,
                )
            })?;
            apply_int(&mut cfg, k, n);
        } else {
            apply_str(&mut cfg, k, v)?;
        }
    }
    if cfg.max_words_per_chapter < cfg.min_words_per_chapter {
        cfg.max_words_per_chapter = cfg.min_words_per_chapter + 800;
    }
    Ok(cfg)
}

fn apply_int(cfg: &mut BookConfig, key: &str, value: i64) {
    match key {
        "total_chapters" => cfg.total_chapters = value,
        "min_words_per_chapter" => cfg.min_words_per_chapter = value,
        "max_words_per_chapter" => cfg.max_words_per_chapter = value,
        "target_words" => cfg.target_words = value,
        _ => {
            cfg.extras.insert(key.to_owned(), serde_json::json!(value));
        }
    }
}

fn apply_str(cfg: &mut BookConfig, key: &str, value: &str) -> Result<(), AppError> {
    match key {
        "name" => cfg.name = value.to_owned(),
        "status" => cfg.status = Some(value.to_owned()),
        "id" => cfg.id = Some(value.to_owned()),
        // fail-closed：数值键收到非整数（引擎宽容存字符串，本实现拒绝）
        "total_chapters" | "min_words_per_chapter" | "max_words_per_chapter" | "target_words" => {
            return Err(AppError::new(
                ErrorCode::BookCorrupted,
                format!("book.yaml 字段 {key} 期望整数，实为字符串: {value}"),
                false,
            ));
        }
        _ => {
            cfg.extras.insert(key.to_owned(), serde_json::json!(value));
        }
    }
    Ok(())
}
