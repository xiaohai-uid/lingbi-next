//! 书仓状态层（P2-02 book state）—— 与引擎 `state.py` 书仓部分行为等价。
//!
//! 覆盖：章节文件枚举/编号、正文读取、front matter 解析、汉字计数。
//! tracking state（load/save/init/merge）见 `tracking` 模块（P2-03）。
//!
//! 与引擎的差异（有意，fail-closed 纪律）：
//! - 引擎读文件失败 raise FileNotFoundError；本实现返回 `BookNotFound` 错误（不静默空值）。
//! - 引擎正则 `\d` 匹配 Unicode 数字；本实现仅 ASCII（合理简化，文档化）。

use lingbi_contracts::{AppError, ErrorCode};
use std::collections::HashMap;
use std::path::Path;

pub const TRACKING_DIR: &str = "追踪";
pub const STATE_FILE: &str = "_tracking-state.json";
pub const BODY_DIR: &str = "定稿/正文";
/// 引擎 views.MAX_CARD_BYTES（状态卡预算，派生视图模块使用）。
pub const MAX_CARD_BYTES: usize = 12288;

/// 章节文件列表（引擎：`定稿/正文` 下匹配 `\d{3,4}-` 且 `.md`，排序）。
pub fn chapter_files(root: &Path) -> Vec<String> {
    let dir = root.join(BODY_DIR);
    let mut files: Vec<String> = match std::fs::read_dir(&dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|f| f.ends_with(".md") && chapter_num(f) >= 0)
            .collect(),
        Err(_) => Vec::new(),
    };
    files.sort();
    files
}

/// 章节编号（引擎：`re.match(r"(\d{3,4})-", fname)`，无匹配 -1）。
pub fn chapter_num(fname: &str) -> i64 {
    let bytes = fname.as_bytes();
    let mut i = 0;
    let mut n: i64 = 0;
    let mut digits = 0;
    while i < bytes.len() && digits < 4 {
        let c = bytes[i];
        if c.is_ascii_digit() {
            n = n * 10 + (c - b'0') as i64;
            digits += 1;
            i += 1;
        } else {
            break;
        }
    }
    if digits >= 3 && i < bytes.len() && bytes[i] == b'-' {
        n
    } else {
        -1
    }
}

/// 最新章节编号（无章节为 0，与引擎 default 一致）。
pub fn latest_chapter(root: &Path) -> i64 {
    chapter_files(root)
        .iter()
        .map(|f| chapter_num(f))
        .max()
        .unwrap_or(0)
}

/// 解析章节 front matter（引擎：`---` 开头 + 首分节段含 `:` 才解析；平铺 KV）。
/// 文件缺失 → `BookNotFound`；格式不合法（非 ---/无冒号段）→ 空 map（与引擎宽容一致）。
pub fn frontmatter(path: &Path) -> Result<HashMap<String, String>, AppError> {
    let text = read_text(path)?;
    if !text.starts_with("---") {
        return Ok(HashMap::new());
    }
    let Some(seg) = text.split("---").nth(1) else {
        return Ok(HashMap::new());
    };
    if !seg.contains(':') {
        return Ok(HashMap::new());
    }
    let mut fm = HashMap::new();
    for line in seg.lines() {
        if let Some((k, v)) = line.split_once(':') {
            fm.insert(k.trim().to_owned(), v.trim().to_owned());
        }
    }
    Ok(fm)
}

/// 正文（引擎：`re.sub(r"^---.*?---\n", "", t, re.S)` 非贪婪剥第一个 front matter 块）。
/// 文件缺失 → `BookNotFound`。
pub fn body(path: &Path) -> Result<String, AppError> {
    let text = read_text(path)?;
    if !text.starts_with("---") {
        return Ok(text);
    }
    // 从首个 "---" 之后找第一个 "---\n"（非贪婪等价）
    let after_first = &text[3..];
    match after_first.find("---\n") {
        Some(p) => Ok(text[p + 3 + 4..].to_owned()),
        None => Ok(text),
    }
}

fn read_text(path: &Path) -> Result<String, AppError> {
    std::fs::read_to_string(path).map_err(|_| {
        AppError::new(
            ErrorCode::BookNotFound,
            format!("缺文件: {}", path.display()),
            false,
        )
    })
}

/// front matter 逗号列表（引擎：去空 + 剔除 `""`/`''` 占位）。
pub fn fm_list(fm: &HashMap<String, String>, key: &str) -> Vec<String> {
    fm.get(key)
        .map(|v| {
            v.split(',')
                .map(|e| e.trim().to_owned())
                .filter(|e| !e.is_empty() && e != "\"\"" && e != "''")
                .collect()
        })
        .unwrap_or_default()
}

/// 汉字计数（引擎：`[\u4e00-\u9fff]`）。
pub fn hanzi(text: &str) -> usize {
    text.chars()
        .filter(|c| ('\u{4e00}'..='\u{9fff}').contains(c))
        .count()
}
