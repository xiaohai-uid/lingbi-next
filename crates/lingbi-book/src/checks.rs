//! 机检模块（P2-05）—— 与引擎 `checks.py` 行为等价。
//!
//! 覆盖：AI 红线表（redlines）、11 项确定性机检、`collect_checks` 聚合。
//! 每项检查返回 `(ok, detail)`，序号 1..11 固定（与 gate 对应，见 `gates` 模块）。
//! 约定：纯确定性输出；state 缺字段容错；错误以 `AppError` 传播（引擎 raise）。
//!
//! 与引擎的差异（有意，fail-closed 纪律）：
//! - 引擎 `_check_leaks` 捕获 load_state 的 FileNotFoundError/ValueError 并把异常
//!   文本拼进 detail；本实现取 `AppError.message`——实测引擎 `str(e)` 即自定义
//!   消息本体（非 Errno 前缀），缺失状态路径格式与引擎一致；损坏状态路径的 JSON
//!   解析错误文本来自 serde（与 Python json 不同，文档化）。
//! - 引擎嵌套 entry 字段缺失宽容取值（如 info_gap 缺 revealed → 视为未曝光、
//!   promise 缺 status → "open"）；本实现 serde 拒收缺失必填字段（`BookCorrupted`，
//!   corpus 恒有，同 views/tracking 既有纪律）。
//! - `redlines()` 签名收 `Option<&Path>`（None = 仅内置表），与引擎 Optional 一致；
//!   反例库文件在 `_check_redlines` 只读一次（引擎逐章重读，行为等价）。
//! - 引擎 `collect_checks` 的 chapter 参数从 CLI 字符串强转 int；本实现直接收
//!   `Option<i64>`，非法值由命令层负责（Rust 类型即校验）。

use crate::config::load_config;
use crate::state::{
    body, chapter_files, chapter_num, fm_list, frontmatter, hanzi, BODY_DIR, MAX_CARD_BYTES,
    STATE_FILE, TRACKING_DIR,
};
use crate::tracking::load_state;
use lingbi_contracts::{AppError, ErrorCode};
use regex::Regex;
use std::path::{Path, PathBuf};

/// 引擎 `checks.STAGES`：合法 stage 集合。
pub const STAGES: [&str; 3] = ["prewrite", "precommit", "postcommit"];
/// 引擎 `checks.COUNTEREXAMPLES_FILE`：本书反例库相对路径。
pub const COUNTEREXAMPLES_FILE: &str = "文风/审查反例库.md";

/// 引擎 `checks.REDLINES`：AI 红线正则表（(pattern, name)，顺序固定）。
pub const REDLINES: [(&str, &str); 9] = [
    (r"他终于明白", "章末总结体·终于明白"),
    (r"这一夜[^。]*注定", "章末总结体·这一夜注定"),
    (r"无人入眠", "章末总结体·无人入眠"),
    (
        r"(?:本章|这一章|本章节|这一回).{0,12}(?:讲述|讲了|写到|写了|展现|描写)",
        "章末总结体",
    ),
    (r"他不知道的是", "预告体"),
    (
        r"(?:下一章|下一回|且听下回分解|欲知后事|敬请期待|未完待续)",
        "预告体",
    ),
    (r"像潮水般|如闪电般|仿佛春风", "万能比喻"),
    (
        r"(?:宛如|犹如|恰如|仿佛)(?:一幅|一场|一首)?(?:画卷|史诗|乐章|诗篇|梦境)",
        "万能比喻",
    ),
    (
        r"(?:或许|也许|可能).{0,4}(?:这|那)?就?是(?:人生|命运|生活|成长|一切)",
        "抽象升华",
    ),
];

/// 引擎 `checks.redlines(root)`：root 给定并入本书反例库短语（re.escape → 字面正则）。
pub fn redlines(root: Option<&Path>) -> Result<Vec<(String, String)>, AppError> {
    let mut out: Vec<(String, String)> = REDLINES
        .iter()
        .map(|(p, n)| ((*p).to_owned(), (*n).to_owned()))
        .collect();
    if let Some(root) = root {
        let p = root.join(COUNTEREXAMPLES_FILE);
        if p.exists() {
            let text = std::fs::read_to_string(&p).map_err(|e| {
                AppError::new(
                    ErrorCode::BookNotFound,
                    format!("读审查反例库失败: {}", e),
                    false,
                )
            })?;
            for ln in text.lines() {
                let ln = ln.trim();
                if !ln.is_empty() && !ln.starts_with('#') {
                    out.push((regex::escape(ln), format!("反例库·{ln}")));
                }
            }
        }
    }
    Ok(out)
}

/// 占位符正则（P7 章事务引擎草稿机检与 check_placeholder 同一正则）。
pub const PLACEHOLDER_RE: &str = r"TODO|占位|待补|待写|XXX|\{[A-Za-z_]+\}";

/// 草稿文本占位符命中（P7 machine-check 节点用；`check_placeholder` 同正则）。
pub fn placeholder_hits(text: &str) -> Vec<String> {
    let re = Regex::new(PLACEHOLDER_RE).expect("占位符正则静态合法");
    re.find_iter(text).map(|m| m.as_str().to_owned()).collect()
}

/// 草稿文本红线命中（P7 machine-check 节点用；`check_redlines` 同表），
/// 返回 (红线名, 命中片段)。root 给定并入本书反例库（同 `redlines`）。
pub fn redline_hits(text: &str, root: Option<&Path>) -> Result<Vec<(String, String)>, AppError> {
    let rl = redlines(root)?;
    let mut hits = Vec::new();
    for (pat, name) in &rl {
        let re = Regex::new(pat).expect("红线正则来自内置表或 re.escape 产物");
        if let Some(m) = re.find(text) {
            let (s, e) = (char_index(text, m.start()), char_index(text, m.end()));
            hits.push((name.clone(), snippet(text, s, e, 6)));
        }
    }
    Ok(hits)
}

/// 引擎 `collect_checks` 的单项结果。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CheckResult {
    pub ok: bool,
    pub name: String,
    pub detail: String,
}

/// 引擎 `_target_chapters`：按 chapter_num 排序返回 [(num, 正文全路径)]。
/// chapter 为 None → 全部；Some(n) → 仅匹配章（无则空）。
fn target_chapters(root: &Path, chapter: Option<i64>) -> Vec<(i64, PathBuf)> {
    let mut fs = chapter_files(root);
    fs.sort_by_key(|f| chapter_num(f));
    match chapter {
        None => fs
            .iter()
            .map(|f| (chapter_num(f), root.join(BODY_DIR).join(f)))
            .collect(),
        Some(n) => fs
            .iter()
            .find(|f| chapter_num(f) == n)
            .map(|f| vec![(n, root.join(BODY_DIR).join(f))])
            .unwrap_or_default(),
    }
}

/// 引擎 `_per_chapter` 空目标聚合：无章节 / 指定章不存在。
fn no_targets(chapter: Option<i64>) -> (bool, String) {
    match chapter {
        None => (true, "无章节".to_owned()),
        Some(n) => (false, format!("第{n}章 不存在")),
    }
}

/// 引擎 `_per_chapter`：逐章跑 `fn(num, path)` 聚合（全过 + "; " 拼接 detail）。
fn per_chapter<F>(
    root: &Path,
    chapter: Option<i64>,
    f: F,
) -> Result<(bool, String), AppError>
where
    F: Fn(i64, &Path) -> Result<(bool, String), AppError>,
{
    let targets = target_chapters(root, chapter);
    if targets.is_empty() {
        return Ok(no_targets(chapter));
    }
    let mut all_ok = true;
    let mut details = Vec::new();
    for (n, p) in &targets {
        let (ok, d) = f(*n, p)?;
        all_ok &= ok;
        details.push(d);
    }
    Ok((all_ok, details.join("; ")))
}

/// 引擎 `_snippet(text, start, end, pad)`：字符偏移窗口 + 首尾 … 标记。
fn snippet(text: &str, start: usize, end: usize, pad: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    let a = start.saturating_sub(pad);
    let b = (end + pad).min(chars.len());
    let mut s = String::new();
    if a > 0 {
        s.push('…');
    }
    s.extend(chars[a..b].iter());
    if b < chars.len() {
        s.push('…');
    }
    s
}

/// 字节偏移 → 字符偏移（引擎 re 的 start/end 是字符索引）。
fn char_index(text: &str, byte: usize) -> usize {
    text[..byte].chars().count()
}

/// 引擎 `os.path.relpath`（strip root 前缀；不满足时原样返回）。
fn relpath(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .map(|r| r.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.display().to_string())
}

fn mtime(p: &Path) -> Result<std::time::SystemTime, AppError> {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .map_err(|_| {
            AppError::new(
                ErrorCode::BookNotFound,
                format!("读 mtime 失败: {}", p.display()),
                false,
            )
        })
}

// ── 11 项检查（每项 (ok, detail)，序号 1..11 与 gate 对应） ──

/// 1. 编号连续且唯一：chapter_files 按 chapter_num 排序 == 1..N。
fn check_numbering(
    root: &Path,
    _chapter: Option<i64>,
    _stage: &str,
) -> Result<(bool, String), AppError> {
    let mut nums: Vec<i64> = chapter_files(root).iter().map(|f| chapter_num(f)).collect();
    nums.sort_unstable();
    if nums.is_empty() {
        return Ok((true, "无章节(空书)".to_owned()));
    }
    let expect: Vec<i64> = (1..=nums.len() as i64).collect();
    if nums != expect {
        return Ok((
            false,
            format!("编号不连续或重复: 现有 {nums:?},应为 {expect:?}"),
        ));
    }
    Ok((true, format!("编号 1..{} 连续唯一", nums.len())))
}

/// 2. 字数窗口：hanzi(body) in [min_words-400, max_words+200]。
fn check_words(
    root: &Path,
    chapter: Option<i64>,
    _stage: &str,
) -> Result<(bool, String), AppError> {
    let cfg = load_config(root)?;
    let lo = (cfg.min_words_per_chapter - 400).max(0);
    let hi = cfg.max_words_per_chapter + 200;
    per_chapter(root, chapter, |n, p| {
        let c = hanzi(&body(p)?) as i64;
        if c < lo {
            return Ok((false, format!("第{n}章 {c} 字,低于下限 {lo}")));
        }
        if c > hi {
            return Ok((false, format!("第{n}章 {c} 字,超出上限 {hi}")));
        }
        Ok((true, format!("第{n}章 {c} 字")))
    })
}

/// 3. 占位符：正文含 TODO|占位|待补|待写|XXX|{标识} → fail。
fn check_placeholder(
    root: &Path,
    chapter: Option<i64>,
    _stage: &str,
) -> Result<(bool, String), AppError> {
    let re = Regex::new(r"TODO|占位|待补|待写|XXX|\{[A-Za-z_]+\}").expect("占位符正则静态合法");
    per_chapter(root, chapter, |n, p| {
        let text = body(p)?;
        if let Some(m) = re.find(&text) {
            return Ok((false, format!("第{n}章 含占位符「{}」", m.as_str())));
        }
        Ok((true, format!("第{n}章 无占位符")))
    })
}

/// 4. 泄密扫描：未曝光信息差 S-id 出现在正文（仅扫描 n >= plant_ch 的章节）。
fn check_leaks(
    root: &Path,
    chapter: Option<i64>,
    _stage: &str,
) -> Result<(bool, String), AppError> {
    let s = match load_state(root) {
        Ok(s) => s,
        Err(e) => return Ok((false, format!("缺追踪状态,无法扫描泄密({})", e.message))),
    };
    // 引擎: v.get("plant_ch", 1) or 1 —— None/0 → 1；负数原样保留（truthy）
    let gaps: Vec<(String, i64)> = s
        .info_gaps
        .iter()
        .filter(|(_, v)| !v.revealed)
        .map(|(g, v)| {
            let plant = match v.plant_ch {
                Some(0) | None => 1,
                Some(n) => n,
            };
            (g.clone(), plant)
        })
        .collect();
    if gaps.is_empty() {
        return Ok((true, "无未曝光信息差".to_owned()));
    }
    let targets = target_chapters(root, chapter);
    if targets.is_empty() {
        return Ok(no_targets(chapter));
    }
    let mut bad = Vec::new();
    for (n, p) in &targets {
        let text = body(p)?;
        let hits: Vec<&str> = gaps
            .iter()
            .filter(|(g, plant)| *n >= *plant && text.contains(g.as_str()))
            .map(|(g, _)| g.as_str())
            .collect();
        if !hits.is_empty() {
            bad.push(format!("第{n}章 正文出现未曝光信息差 {}", hits.join("、")));
        }
    }
    if bad.is_empty() {
        return Ok((true, "正文未泄密".to_owned()));
    }
    Ok((false, bad.join("; ")))
}

/// 5. 实体登记：entities_new 每项须出现在 定稿/设定/名册.md。
fn check_entities(
    root: &Path,
    chapter: Option<i64>,
    _stage: &str,
) -> Result<(bool, String), AppError> {
    let roster = root.join("定稿").join("设定").join("名册.md");
    per_chapter(root, chapter, |n, p| {
        let ents = fm_list(&frontmatter(p)?, "entities_new");
        if ents.is_empty() {
            return Ok((true, format!("第{n}章 无新实体")));
        }
        let content = match std::fs::read_to_string(&roster) {
            Ok(c) => c,
            Err(_) => {
                return Ok((
                    false,
                    format!("第{n}章 实体 {} 未登记(缺 定稿/设定/名册.md)", ents.join("、")),
                ));
            }
        };
        let missing: Vec<&str> = ents
            .iter()
            .filter(|e| !content.contains(e.as_str()))
            .map(|e| e.as_str())
            .collect();
        if !missing.is_empty() {
            return Ok((
                false,
                format!("第{n}章 实体未登记于名册: {}", missing.join("、")),
            ));
        }
        Ok((true, format!("第{n}章 实体均已登记")))
    })
}

/// 6. 章文件齐全：每章 front matter 存在 + 定稿/记忆/章摘要/{NNN}.md 存在。
fn check_files(
    root: &Path,
    chapter: Option<i64>,
    _stage: &str,
) -> Result<(bool, String), AppError> {
    per_chapter(root, chapter, |n, p| {
        if frontmatter(p)?.is_empty() {
            return Ok((false, format!("第{n}章 缺 front matter")));
        }
        let sp = root
            .join("定稿")
            .join("记忆")
            .join("章摘要")
            .join(format!("{n:03}.md"));
        if !sp.exists() {
            return Ok((false, format!("第{n}章 缺章摘要 {}", relpath(&sp, root))));
        }
        Ok((true, format!("第{n}章 文件齐全")))
    })
}

/// 7. AI 红线：redlines() 每 pattern 扫正文，命中 → fail（注明位置片段）。
fn check_redlines(
    root: &Path,
    chapter: Option<i64>,
    _stage: &str,
) -> Result<(bool, String), AppError> {
    let rl = redlines(Some(root))?;
    per_chapter(root, chapter, |n, p| {
        let text = body(p)?;
        for (pat, name) in &rl {
            let re = Regex::new(pat).expect("红线正则来自内置表或 re.escape 产物");
            if let Some(m) = re.find(&text) {
                let (s, e) = (char_index(&text, m.start()), char_index(&text, m.end()));
                return Ok((
                    false,
                    format!(
                        "第{n}章 红线「{name}」命中: 片段 {}",
                        snippet(&text, s, e, 6)
                    ),
                ));
            }
        }
        Ok((true, format!("第{n}章 无红线命中")))
    })
}

/// 引擎 `_in_quotes`：frag 是否完全位于一对中文引号/直引号内。
fn in_quotes(text: &str, frag: &str) -> bool {
    for (open, close) in [('"', '"'), ('“', '”'), ('「', '」')] {
        let mut depth = 0u32;
        for (i, ch) in text.char_indices() {
            if ch == open {
                depth += 1;
            } else if ch == close && depth > 0 {
                depth -= 1;
            } else if depth > 0 && text[i..].starts_with(frag) {
                return true;
            }
        }
    }
    false
}

/// 引擎 `_common5`：两段去空白后 ≥5 字公共子串（全在引号内不算复读）。
fn common5(a: &str, b: &str) -> Option<String> {
    let (la, lb) = (a.chars().count(), b.chars().count());
    if la < 5 || lb < 5 {
        return None;
    }
    let (shorter, longer) = if la > lb { (b, a) } else { (a, b) };
    let lchars: Vec<char> = longer.chars().collect();
    let grams: std::collections::HashSet<String> = lchars
        .windows(5)
        .map(|w| w.iter().collect())
        .collect();
    let schars: Vec<char> = shorter.chars().collect();
    for w in schars.windows(5) {
        let g: String = w.iter().collect();
        if grams.contains(&g) {
            if in_quotes(a, &g) && in_quotes(b, &g) {
                continue;
            }
            return Some(g);
        }
    }
    None
}

/// 8. 复读检测：相邻两段 ≥5 字公共子串（去空白）→ fail；或同一 4-gram ≥6 次 → fail。
fn check_repeat(
    root: &Path,
    chapter: Option<i64>,
    _stage: &str,
) -> Result<(bool, String), AppError> {
    per_chapter(root, chapter, |n, p| {
        let text = body(p)?;
        // 引擎: body.split("\n\n") 精确分隔 + seg.strip() 过滤 + 去全部空白
        let paras: Vec<String> = text
            .split("\n\n")
            .filter(|seg| !seg.trim().is_empty())
            .map(|seg| seg.split_whitespace().collect())
            .collect();
        for (a, b) in paras.iter().zip(paras.iter().skip(1)) {
            if let Some(g) = common5(a, b) {
                return Ok((false, format!("第{n}章 相邻段落存在重复片段「{g}」")));
            }
        }
        let flat: String = paras.join("");
        if flat.chars().count() >= 4 {
            // 引擎 Counter + 过滤句读/引号 4-gram + max 按首现序平局取先
            // （IndexMap 保插入序 = 引擎 dict 序；严格 > 保留首个达最大者）
            let punc = ['。', '！', '？', '；', '，', '、', '”', '」', ')', '}'];
            let quotes = ['"', '“', '”', '「', '」'];
            let chars: Vec<char> = flat.chars().collect();
            let mut cnt: indexmap::IndexMap<String, usize> = indexmap::IndexMap::new();
            for w in chars.windows(4) {
                let g: String = w.iter().collect();
                *cnt.entry(g).or_insert(0) += 1;
            }
            let mut best: Option<(String, usize)> = None;
            for (g, k) in &cnt {
                let (first, last) = (g.chars().next().unwrap(), g.chars().last().unwrap());
                if punc.contains(&first) || punc.contains(&last) {
                    continue;
                }
                if g.chars().any(|c| quotes.contains(&c)) {
                    continue;
                }
                if best.as_ref().is_none_or(|(_, b)| *k > *b) {
                    best = Some((g.clone(), *k));
                }
            }
            if let Some((g, k)) = best
                && k >= 6
            {
                return Ok((false, format!("第{n}章 4字片段「{g}」出现 {k} 次")));
            }
        }
        Ok((true, format!("第{n}章 无复读")))
    })
}

/// 9. 合同断言：must_cover 词正文须全部包含；forbidden 词正文不得含。
fn check_contract(
    root: &Path,
    chapter: Option<i64>,
    _stage: &str,
) -> Result<(bool, String), AppError> {
    per_chapter(root, chapter, |n, p| {
        let fm = frontmatter(p)?;
        let text = body(p)?;
        let miss: Vec<String> = fm_list(&fm, "must_cover")
            .into_iter()
            .filter(|w| !text.contains(w.as_str()))
            .collect();
        let hit: Vec<String> = fm_list(&fm, "forbidden")
            .into_iter()
            .filter(|w| text.contains(w.as_str()))
            .collect();
        let mut bad = Vec::new();
        if !miss.is_empty() {
            bad.push(format!("未覆盖必含词 {}", miss.join("、")));
        }
        if !hit.is_empty() {
            bad.push(format!("命中禁区词 {}", hit.join("、")));
        }
        if bad.is_empty() {
            return Ok((true, format!("第{n}章 合同满足")));
        }
        Ok((false, format!("第{n}章 {}", bad.join("; "))))
    })
}

/// 10. 追踪新鲜：state mtime ≥ 最新正文 mtime（prewrite 跳过，目标章除外）。
///
/// 状态卡存在且 ≤12KB 且不早于状态文件。
fn check_fresh(
    root: &Path,
    chapter: Option<i64>,
    stage: &str,
) -> Result<(bool, String), AppError> {
    if stage == "prewrite" {
        return Ok((true, "跳过(prewrite 不查状态新鲜)".to_owned()));
    }
    let sp = root.join(TRACKING_DIR).join(STATE_FILE);
    if !sp.exists() {
        return Ok((false, format!("缺追踪状态({})", sp.display())));
    }
    let fs = chapter_files(root);
    if !fs.is_empty() {
        // 正在提交的目标章除外（该章的 commit 本身就是刷新动作）
        let cand: Vec<&String> = fs
            .iter()
            .filter(|f| chapter_num(f) != chapter.unwrap_or(-1))
            .collect();
        if !cand.is_empty() {
            let latest = cand
                .iter()
                .map(|f| mtime(&root.join(BODY_DIR).join(f)))
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .max()
                .expect("cand 非空");
            if mtime(&sp)? < latest {
                return Ok((
                    false,
                    "追踪状态早于最新正文(先重跑 state 合并/保存)".to_owned(),
                ));
            }
        }
    }
    let card = root.join(TRACKING_DIR).join("上下文.md");
    if !card.exists() {
        return Ok((false, format!("缺状态卡 {}", relpath(&card, root))));
    }
    let size = std::fs::metadata(&card)
        .map_err(|e| {
            AppError::new(
                ErrorCode::BookNotFound,
                format!("读状态卡元数据失败: {}", e),
                false,
            )
        })?
        .len();
    if size > MAX_CARD_BYTES as u64 {
        return Ok((
            false,
            format!("状态卡 {} {size} 字节,超过上限 {MAX_CARD_BYTES}", relpath(&card, root)),
        ));
    }
    // L4：状态卡必须不早于状态文件（save 与 views 之间崩溃会留下“状态新、卡旧”中间态）
    if mtime(&card)? < mtime(&sp)? {
        return Ok((
            false,
            "状态卡早于状态文件(派生视图未重建,重跑 commit 或 views 重建)".to_owned(),
        ));
    }
    Ok((true, format!("追踪新鲜,状态卡 {size} 字节")))
}

/// 11. 摘要非空：章节 front matter summary 非空。
fn check_summary(
    root: &Path,
    chapter: Option<i64>,
    _stage: &str,
) -> Result<(bool, String), AppError> {
    per_chapter(root, chapter, |n, p| {
        let fm = frontmatter(p)?;
        let s = fm.get("summary").map(|s| s.trim()).unwrap_or("");
        if s.is_empty() {
            return Ok((false, format!("第{n}章 summary 为空")));
        }
        Ok((true, format!("第{n}章 summary 非空")))
    })
}

/// 引擎 `checks._CHECKS`：序号即 gate 的检查项编号（1..11），顺序固定。
type CheckFn = fn(&Path, Option<i64>, &str) -> Result<(bool, String), AppError>;

const CHECKS: [(&str, CheckFn); 11] = [
    ("编号连续且唯一", check_numbering),
    ("字数窗口", check_words),
    ("占位符", check_placeholder),
    ("泄密扫描", check_leaks),
    ("实体登记", check_entities),
    ("章文件齐全", check_files),
    ("AI红线", check_redlines),
    ("复读检测", check_repeat),
    ("合同断言", check_contract),
    ("追踪新鲜", check_fresh),
    ("摘要非空", check_summary),
];

/// 引擎 `collect_checks`：返回全部 11 项 check（不过滤 stage，由 gates 决定只看哪些）。
///
/// 每项 `{ok, name, detail}`；stage 非法 → 错误；chapter 为 None → 逐章全查。
pub fn collect_checks(
    root: &Path,
    stage: &str,
    chapter: Option<i64>,
) -> Result<Vec<CheckResult>, AppError> {
    if !STAGES.contains(&stage) {
        return Err(AppError::new(
            ErrorCode::BookCorrupted,
            format!("stage 非法: {stage}"),
            false,
        ));
    }
    let mut out = Vec::with_capacity(CHECKS.len());
    for (name, f) in CHECKS {
        let (ok, detail) = f(root, chapter, stage)?;
        out.push(CheckResult {
            ok,
            name: name.to_owned(),
            detail,
        });
    }
    Ok(out)
}
