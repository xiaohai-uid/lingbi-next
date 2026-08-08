//! 派生视图模块（P2-04）—— 与引擎 `views.py` 行为等价。
//!
//! 覆盖：续写状态卡（7 栏，≤12KB）/ 伏笔台账 / 作者真相 / 读者已知 / 时间线 /
//! 角色状态逐卡 / 逐章记录。全部写入 `追踪/` 下派生文件；状态本体只经 `tracking`
//! 模块写入（`_tracking-state.json`）。幂等：重复调用不炸；state 缺字段容错。
//!
//! 字节级契约：`write_views` 产物必须与引擎 `views.py` 逐字节一致
//! （P2-15 回放门禁按结果树哈希验收）——注意无尾换行（引擎 `"\n".join(...)`）：
//! 仅状态卡以 `\n` 结尾（7 栏末附加空串），其余视图文件均无尾换行。
//!
//! 与引擎的差异（有意，fail-closed 纪律）：
//! - 引擎 `_cut` 用 Python UTF-8 decode(ignore) 丢尾残字节；本实现 `valid_up_to` 等价。
//! - 引擎 state 缺字段宽容取值（如 promise 缺 status → "open"、timeline 缺 kind →
//!   "fact"、entity 缺 first_ch → "首现章未知"）；本实现 serde 拒收缺失必填字段
//!   （`BookCorrupted`，corpus 恒有）。
//! - 章节键过滤仅 ASCII 数字：引擎 `str(x).isdigit()` 会接受 Unicode 数字键、随后
//!   `int()` 抛 ValueError（崩溃）；本实现跳过非 ASCII 数字键，超 i64 范围返回
//!   `BookCorrupted`（见 write_chapter_logs）。
//! - 引擎 views.py 声明未用的 `CHAPTER_LOG_TARGET` 不移植。

use crate::config::{load_config, BookConfig};
use crate::state::{
    body, chapter_files, chapter_num, fm_list, frontmatter, latest_chapter, BODY_DIR,
    MAX_CARD_BYTES, TRACKING_DIR,
};
use crate::tracking::{active_promises, unrevealed_gaps, InfoGapEntry, TimelineEvent, TrackingState};
use lingbi_contracts::{AppError, ErrorCode};
use std::path::Path;

const CARD_TITLE: &str = "# 续写状态卡(7 栏)";
const NEXT_PLACEHOLDER: &str = "（无未勾选细纲行,续写依总纲自由发挥）";
const STYLE_ANCHOR: &str = "\
一、拒绝抽象升华与万能比喻,写具体动作与可感细节。
二、每章必有推进:情节、人物或伏笔至少一项向前。
三、章末留钩:悬念、转折或新伏笔,让读者想翻下一页。";
/// 引擎 `views.CHAPTER_LOG_MAX`：逐章记录截断上限（字节）。
const CHAPTER_LOG_MAX: usize = 3072;

// ── 通用工具 ──

/// 引擎 `_cut(text, limit)`：按字节截断，UTF-8 安全（丢弃尾部不完整序列）。
fn cut(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_owned();
    }
    let b = &text.as_bytes()[..limit];
    match std::str::from_utf8(b) {
        Ok(s) => s.to_owned(),
        // 源为合法 UTF-8，仅尾部可能不完整 → 丢弃不完整尾字节（引擎 decode ignore 等价）
        Err(e) => std::str::from_utf8(&b[..e.valid_up_to()])
            .expect("valid_up_to 边界必为合法 UTF-8")
            .to_owned(),
    }
}

fn write_file(path: &Path, content: &str) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            AppError::new(
                ErrorCode::DiskWriteFailed,
                format!("创建目录失败: {}", e),
                false,
            )
        })?;
    }
    std::fs::write(path, content).map_err(|_| {
        AppError::new(
            ErrorCode::DiskWriteFailed,
            format!("写文件失败: {}", path.display()),
            false,
        )
    })
}

/// 引擎 `_num(v)`：None → 空串。
fn opt_num(v: Option<i64>) -> String {
    v.map(|n| n.to_string()).unwrap_or_default()
}

/// 引擎 `- %s{sep}%s` / `- %s` 两态列表行（note 空则省略）。
/// sep：状态卡用 " "（`- P-001 备注`），作者真相/读者已知用 ": "（`- S-001: 备注`）。
fn note_line(prefix: &str, note: &str, sep: &str) -> String {
    let note = note.trim();
    if note.is_empty() {
        format!("- {prefix}")
    } else {
        format!("- {prefix}{sep}{note}")
    }
}

/// 引擎 `- %s. %s` 时间线行（作者真相/读者已知共用，仅过滤条件不同）。
fn timeline_lines<'a>(tl: impl Iterator<Item = &'a TimelineEvent>) -> Vec<String> {
    tl.map(|ev| format!("- {}. {}", ev.ch, ev.event)).collect()
}

/// 引擎 `sorted(id for id, v in m if pred(v))`：过滤 + 按键排序（逐章记录五类字段共用）。
fn chapter_ids<'a, T: 'a>(
    iter: impl Iterator<Item = (&'a String, &'a T)>,
    f: impl Fn(&T) -> bool,
) -> Vec<&'a str> {
    let mut v: Vec<&str> = iter.filter(|(_, e)| f(e)).map(|(k, _)| k.as_str()).collect();
    v.sort_unstable();
    v
}

/// 引擎 `re.split(r"\n\s*\n", body)`：连续空白行 = 一个段分隔符，段内单换行保留。
/// 引擎 `\s` 与 Rust `char::is_whitespace` 同为 Unicode White_Space，行为一致。
fn split_paragraphs(body: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut in_sep = false;
    for line in body.split('\n') {
        if line.trim().is_empty() {
            if !in_sep {
                out.push(std::mem::take(&mut cur));
                in_sep = true;
            }
        } else {
            in_sep = false;
            if !cur.is_empty() {
                cur.push('\n');
            }
            cur.push_str(line);
        }
    }
    out.push(cur);
    out
}

/// 引擎 `_next_contract_line`：细纲蓝图.md 首个未勾选行（无则 None）。
fn next_contract_line(root: &Path) -> Result<Option<String>, AppError> {
    let p = root.join("大纲").join("细纲蓝图.md");
    let text = match std::fs::read_to_string(&p) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(AppError::new(
                ErrorCode::BookNotFound,
                format!("读细纲蓝图失败: {}", e),
                false,
            ));
        }
    };
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        if let Some((checked, rest)) = checkbox_rest(t) {
            if !checked {
                return Ok(Some(rest));
            }
        } else if !t.contains('[') {
            return Ok(Some(t.to_owned()));
        }
    }
    Ok(None)
}

/// 引擎正则 `^[-*]?\s*\[([ xX])\]\s*(.*)$` 的等价手工解析。
/// 返回 `(是否已勾选, ']' 后内容)`；不匹配返回 None。
fn checkbox_rest(t: &str) -> Option<(bool, String)> {
    let mut chars = t.chars().peekable();
    // [-*]?
    if matches!(chars.peek(), Some('-') | Some('*')) {
        chars.next();
    }
    // \s*（贪婪）
    while chars.peek().is_some_and(|c| c.is_whitespace()) {
        chars.next();
    }
    // \[
    if chars.next() != Some('[') {
        return None;
    }
    // ([ xX])
    let mark = chars.next()?;
    if !matches!(mark, ' ' | 'x' | 'X') {
        return None;
    }
    // \]
    if chars.next() != Some(']') {
        return None;
    }
    // \s* 贪婪吞掉 ']' 后全部空白，(.*) 取余下内容
    let rest: String = chars.skip_while(|c| c.is_whitespace()).collect();
    Some((mark != ' ', rest))
}

/// 引擎 `_last_endings(root, n=3, limit=150)`：最近 n 章正文末段（跳过 —— 横幅段）。
fn last_endings(root: &Path, n: usize, limit: usize) -> Result<Vec<(i64, String)>, AppError> {
    let files = chapter_files(root);
    let mut nums: Vec<i64> = files.iter().map(|f| chapter_num(f)).collect();
    nums.sort_unstable();
    let start = nums.len().saturating_sub(n);
    let mut out = Vec::new();
    for num in &nums[start..] {
        let f = files
            .iter()
            .find(|f| chapter_num(f) == *num)
            .expect("chapter_files 与 chapter_num 同源");
        let text = body(&root.join(BODY_DIR).join(f))?;
        let paras: Vec<String> = split_paragraphs(&text)
            .into_iter()
            .map(|p| p.trim().to_owned())
            .filter(|p| !p.is_empty())
            .collect();
        let mut para = String::new();
        for p in paras.iter().rev() {
            if p.starts_with("——") {
                continue;
            }
            para = p.chars().take(limit).collect(); // 引擎 para[:limit] 码点截断
            break;
        }
        out.push((*num, para));
    }
    Ok(out)
}

// ── 续写状态卡（7 栏） ──

/// 引擎 `card7(root, state)`：7 栏固定顺序/固定标题，≤12KB 超限截断末尾。
pub fn card7(root: &Path, s: &TrackingState) -> Result<String, AppError> {
    // 引擎: 缺 book.yaml 用 DEFAULTS（仅 FileNotFoundError 被捕获）
    let cfg = match load_config(root) {
        Ok(c) => c,
        Err(e) if e.code == ErrorCode::BookNotFound => BookConfig::default(),
        Err(e) => return Err(e),
    };
    let latest = latest_chapter(root);
    let planned = cfg.total_chapters; // 引擎 int(cfg.get("total_chapters", 330))
    let actives = active_promises(s);
    let gaps = unrevealed_gaps(s);
    let contract = next_contract_line(root)?;
    let entities: Vec<(&String, &crate::tracking::EntityState)> =
        s.entity_states.iter().take(6).collect();
    let endings = last_endings(root, 3, 150)?;

    let mut lines: Vec<String> = vec![CARD_TITLE.to_owned(), String::new()];
    // 1 当前进度
    lines.push("## 1. 当前进度".to_owned());
    lines.push(format!(
        "最新章: {latest} / 计划: {planned} | 活跃承诺: {} | 未曝光信息差: {}",
        actives.len(),
        gaps.len()
    ));
    lines.push(String::new());
    // 2 下一章合同（空串视为无 → 占位，引擎 falsy 语义）
    lines.push("## 2. 下一章合同".to_owned());
    lines.push(match contract.as_deref() {
        Some(c) if !c.is_empty() => c.chars().take(120).collect(), // 引擎 contract[:120] 码点截断
        _ => NEXT_PLACEHOLDER.to_owned(),
    });
    lines.push(String::new());
    // 3 活跃伏笔
    lines.push("## 3. 活跃伏笔".to_owned());
    if actives.is_empty() {
        lines.push("（无）".to_owned());
    } else {
        for (pid, v) in actives.iter().take(10) {
            lines.push(note_line(pid, &v.note, " "));
        }
    }
    lines.push(String::new());
    // 4 核心角色状态（引擎 first_ch 缺失 → "首现章未知"；本实现 serde 拒收缺字段，分支省略）
    lines.push("## 4. 核心角色状态".to_owned());
    if entities.is_empty() {
        lines.push("（无）".to_owned());
    } else {
        for (name, v) in &entities {
            let where_ = v.where_.trim();
            lines.push(if where_.is_empty() {
                format!("- {name}: 首现第{}章", v.first_ch)
            } else {
                format!("- {name}: 首现第{}章 {where_}", v.first_ch)
            });
        }
    }
    lines.push(String::new());
    // 5 最近章节结尾
    lines.push("## 5. 最近章节结尾".to_owned());
    if endings.is_empty() {
        lines.push("（无正文,待首章落笔）".to_owned());
    } else {
        for (num, para) in &endings {
            lines.push(format!("- 第{num}章: {para}"));
        }
    }
    lines.push(String::new());
    // 6 未曝光信息差
    lines.push("## 6. 未曝光信息差".to_owned());
    if gaps.is_empty() {
        lines.push("（无）".to_owned());
    } else {
        for (gid, v) in gaps.iter().take(8) {
            lines.push(note_line(gid, &v.note, " "));
        }
    }
    lines.push(String::new());
    // 7 风格锚点（末尾空串 → 状态卡以 \n 结尾，与引擎一致）
    lines.push("## 7. 风格锚点".to_owned());
    lines.push(STYLE_ANCHOR.to_owned());
    lines.push(String::new());
    Ok(cut(&lines.join("\n"), MAX_CARD_BYTES))
}

/// 引擎 `card_size_ok`：上下文卡 ≤12KB；缺文件视为不通过。
pub fn card_size_ok(root: &Path) -> bool {
    let p = root.join(TRACKING_DIR).join("上下文.md");
    std::fs::metadata(&p)
        .map(|m| m.len() <= MAX_CARD_BYTES as u64)
        .unwrap_or(false)
}

// ── 视图重建 ──

/// 引擎 `write_views`：重建全部派生视图（幂等；state 缺字段容错）。
pub fn write_views(root: &Path, s: &TrackingState) -> Result<(), AppError> {
    let tk = root.join(TRACKING_DIR);
    write_file(&tk.join("上下文.md"), &card7(root, s)?)?;
    write_file(&tk.join("伏笔.md"), &vault_table(s))?;
    write_file(&tk.join("作者真相.md"), &author_truth(s))?;
    write_file(&tk.join("读者已知.md"), &reader_known(s))?;
    write_file(&tk.join("时间线.md"), &timeline_table(s))?;
    write_entity_files(&tk, s)?;
    write_chapter_logs(root, &tk, s)?;
    Ok(())
}

/// 引擎 `_vault_table`：伏笔与信息差台账（键序 = 引擎 sorted(items) 按键排序）。
fn vault_table(s: &TrackingState) -> String {
    let mut lines = vec![
        "# 伏笔与信息差台账".to_owned(),
        String::new(),
        "## 伏笔".to_owned(),
        "| ID | 状态 | 埋设章 | 最近推进 | 完成章 | 备注 |".to_owned(),
        "| --- | --- | --- | --- | --- | --- |".to_owned(),
    ];
    let mut prom_keys: Vec<&String> = s.promises.keys().collect();
    prom_keys.sort();
    for pid in prom_keys {
        let v = &s.promises[pid];
        lines.push(format!(
            "| {pid} | {} | {} | {} | {} | {} |",
            v.status,
            opt_num(v.open_ch),
            opt_num(v.adv_ch),
            opt_num(v.done_ch),
            v.note.replace('|', "\\|")
        ));
    }
    lines.push(String::new());
    lines.push("## 信息差".to_owned());
    lines.push("| ID | 状态 | 埋设章 | 最近推进 | 完成章 | 备注 |".to_owned());
    lines.push("| --- | --- | --- | --- | --- | --- |".to_owned());
    let mut gap_keys: Vec<&String> = s.info_gaps.keys().collect();
    gap_keys.sort();
    for gid in gap_keys {
        let v = &s.info_gaps[gid];
        let status = if v.revealed { "已曝光" } else { "未曝光" };
        lines.push(format!(
            "| {gid} | {status} | {} |  | {} | {} |",
            opt_num(v.plant_ch),
            opt_num(v.reveal_ch),
            v.note.replace('|', "\\|")
        ));
    }
    lines.join("\n")
}

/// 引擎 `_author_truth`：作者真相（全量时间线 + 未曝光信息差）。
fn author_truth(s: &TrackingState) -> String {
    let mut lines = vec![
        "# 作者真相(仅作者可见)".to_owned(),
        String::new(),
        "## 时间线(全量)".to_owned(),
    ];
    if s.timeline.is_empty() {
        lines.push("（空）".to_owned());
    } else {
        lines.extend(timeline_lines(s.timeline.iter()));
    }
    lines.push(String::new());
    lines.push("## 未曝光信息差(作者才知,读者未见)".to_owned());
    let gaps = unrevealed_gaps(s);
    if gaps.is_empty() {
        lines.push("（无）".to_owned());
    } else {
        for (gid, v) in gaps {
            lines.push(note_line(gid, &v.note, ": "));
        }
    }
    lines.join("\n")
}

/// 引擎 `_reader_known`：读者已知（fact/reveal 时间线 + 已曝光信息差）。
fn reader_known(s: &TrackingState) -> String {
    let mut lines = vec![
        "# 读者已知(读者视角)".to_owned(),
        String::new(),
        "## 时间线(读者已见)".to_owned(),
    ];
    let tl: Vec<&TimelineEvent> = s
        .timeline
        .iter()
        .filter(|ev| matches!(ev.kind.as_str(), "fact" | "reveal"))
        .collect();
    if tl.is_empty() {
        lines.push("（空）".to_owned());
    } else {
        lines.extend(timeline_lines(tl.into_iter()));
    }
    lines.push(String::new());
    lines.push("## 已曝光信息差".to_owned());
    let revealed: Vec<(&String, &InfoGapEntry)> =
        s.info_gaps.iter().filter(|(_, v)| v.revealed).collect();
    if revealed.is_empty() {
        lines.push("（无）".to_owned());
    } else {
        for (gid, v) in revealed {
            lines.push(note_line(gid, &v.note, ": "));
        }
    }
    lines.join("\n")
}

/// 引擎 `_timeline_table`：时间线（append-only 观感）。
fn timeline_table(s: &TrackingState) -> String {
    let mut lines = vec![
        "# 时间线(append-only,只增不改)".to_owned(),
        String::new(),
        "| 章 | 类型 | 事件 |".to_owned(),
        "| --- | --- | --- |".to_owned(),
    ];
    if s.timeline.is_empty() {
        lines.push("|  |  | （空） |".to_owned());
    }
    for ev in &s.timeline {
        lines.push(format!(
            "| {} | {} | {} |",
            ev.ch,
            ev.kind,
            ev.event.replace('|', "\\|")
        ));
    }
    lines.join("\n")
}

/// 引擎 `_write_entity_files`：先清 角色状态/ 下旧 .md 残卡，再写当前实体。
fn write_entity_files(tk: &Path, s: &TrackingState) -> Result<(), AppError> {
    let edir = tk.join("角色状态");
    remove_stale_md(&edir)?;
    for (name, v) in &s.entity_states {
        if name.trim().is_empty() {
            continue;
        }
        // 引擎 safe 名：/ \ : * ? " < > | → _
        let safe: String = name
            .chars()
            .map(|c| match c {
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
                c => c,
            })
            .collect();
        let content = format!(
            "# 角色状态:{name}\n\n- 首现章: {}\n- 位置: {}\n- 状态: {}",
            v.first_ch,
            v.where_.trim(),
            v.state.trim()
        );
        write_file(&edir.join(format!("{safe}.md")), &content)?;
    }
    Ok(())
}

/// 引擎 `_write_chapter_logs`：先清 逐章记录/ 下旧 .md，再写当前章节（ch%03d.md，≤3072B）。
fn write_chapter_logs(root: &Path, tk: &Path, s: &TrackingState) -> Result<(), AppError> {
    let ldir = tk.join("逐章记录");
    remove_stale_md(&ldir)?;
    // 引擎: sorted(int(x) for x in chapters if x.isdigit()) —— 仅纯数字键；
    // 差异：Unicode 数字键引擎 int() 崩溃，本实现跳过；超 i64 范围 fail-closed。
    let mut keys: Vec<i64> = s
        .chapters
        .keys()
        .filter(|k| !k.is_empty() && k.chars().all(|c| c.is_ascii_digit()))
        .map(|k| {
            k.parse::<i64>().map_err(|_| {
                AppError::new(
                    ErrorCode::BookCorrupted,
                    format!("追踪状态章节键超出 i64 范围: {k}"),
                    false,
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    keys.sort_unstable();
    for k in keys {
        let content = chapter_log(root, s, k)?;
        write_file(&ldir.join(format!("ch{k:03}.md")), &cut(&content, CHAPTER_LOG_MAX))?;
    }
    Ok(())
}

/// 清空目录下全部 .md（引擎 L1 清理陈旧文件；目录不存在时跳过）。
fn remove_stale_md(dir: &Path) -> Result<(), AppError> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir).map_err(|e| {
        AppError::new(
            ErrorCode::DiskWriteFailed,
            format!("读目录失败: {}", e),
            false,
        )
    })? {
        let entry = entry.map_err(|e| {
            AppError::new(
                ErrorCode::DiskWriteFailed,
                format!("读目录条目失败: {}", e),
                false,
            )
        })?;
        let name = entry.file_name();
        if name.to_string_lossy().ends_with(".md") {
            std::fs::remove_file(entry.path()).map_err(|e| {
                AppError::new(
                    ErrorCode::DiskWriteFailed,
                    format!("清理陈旧视图失败: {}", e),
                    false,
                )
            })?;
        }
    }
    Ok(())
}

/// 引擎 `_chapter_constraints`：从章节 front matter 回读必覆盖/禁区。
fn chapter_constraints(root: &Path, ch: i64) -> Result<String, AppError> {
    for f in chapter_files(root) {
        if chapter_num(&f) == ch {
            let fm = frontmatter(&root.join(BODY_DIR).join(&f))?;
            let mut parts = Vec::new();
            let must = fm_list(&fm, "must_cover");
            let forb = fm_list(&fm, "forbidden");
            if !must.is_empty() {
                parts.push(format!("必覆盖:{}", must.join(",")));
            }
            if !forb.is_empty() {
                parts.push(format!("禁区:{}", forb.join(",")));
            }
            return Ok(if parts.is_empty() {
                "无".to_owned()
            } else {
                parts.join("; ")
            });
        }
    }
    Ok("无".to_owned())
}

/// 引擎 `_chapter_log`：一章的六类字段记录（结果/角色变化/伏笔变化/时间与揭示/约束/下一章承诺）。
fn chapter_log(root: &Path, s: &TrackingState, ch: i64) -> Result<String, AppError> {
    let rec = s.chapters.get(&ch.to_string());
    let title = rec.map(|r| r.title.trim()).unwrap_or("");
    let status = rec.map(|r| r.status.trim()).unwrap_or("");
    let words = rec.map(|r| r.words.trim()).unwrap_or("");
    // 结果
    let events: Vec<&str> = s
        .timeline
        .iter()
        .filter(|ev| ev.ch == ch)
        .map(|ev| ev.event.as_str())
        .collect();
    let mut result = if events.is_empty() {
        "(无摘要)".to_owned()
    } else {
        events.join("; ")
    };
    let mut meta = Vec::new();
    if !status.is_empty() {
        meta.push(format!("状态:{status}"));
    }
    if !words.is_empty() {
        meta.push(format!("字数:{words}"));
    }
    if !meta.is_empty() {
        result.push_str(&format!("（{}）", meta.join(", ")));
    }
    // 角色变化（本章首现实体，附位置/状态）
    let mut ent = Vec::new();
    for (name, v) in &s.entity_states {
        if v.first_ch == ch {
            let mut base = format!("首现:{name}");
            let mut extra = Vec::new();
            let w = v.where_.trim();
            let stt = v.state.trim();
            if !w.is_empty() {
                extra.push(format!("位置:{w}"));
            }
            if !stt.is_empty() {
                extra.push(format!("状态:{stt}"));
            }
            if !extra.is_empty() {
                base.push_str(&format!("({})", extra.join(",")));
            }
            ent.push(base);
        }
    }
    // 伏笔变化（引擎 sorted(...)：过滤 + 按键排序）
    let p_new = chapter_ids(s.promises.iter(), |v| v.open_ch == Some(ch));
    let p_adv = chapter_ids(s.promises.iter(), |v| v.adv_ch == Some(ch));
    let p_done = chapter_ids(s.promises.iter(), |v| v.done_ch == Some(ch));
    let g_new = chapter_ids(s.info_gaps.iter(), |v| v.plant_ch == Some(ch));
    let g_done = chapter_ids(s.info_gaps.iter(), |v| v.reveal_ch == Some(ch));
    let mut pv = Vec::new();
    if !p_new.is_empty() {
        pv.push(format!("新埋:{}", p_new.join(",")));
    }
    if !p_adv.is_empty() {
        pv.push(format!("推进:{}", p_adv.join(",")));
    }
    if !p_done.is_empty() {
        pv.push(format!("回收:{}", p_done.join(",")));
    }
    if !g_new.is_empty() {
        pv.push(format!("埋设:{}", g_new.join(",")));
    }
    if !g_done.is_empty() {
        pv.push(format!("揭示:{}", g_done.join(",")));
    }
    // 时间与揭示
    let mut ts = Vec::new();
    if !g_done.is_empty() {
        ts.push(format!("揭示信息差:{}", g_done.join(",")));
    }
    for ev in &s.timeline {
        if ev.ch == ch && matches!(ev.kind.as_str(), "reveal" | "private") {
            ts.push(format!("事件:{}", ev.event));
        }
    }
    // 下一章承诺（引擎 sorted(set(p_new) | set(p_adv) | set(g_new))）
    let mut nxt: Vec<&str> = p_new
        .iter()
        .chain(p_adv.iter())
        .chain(g_new.iter())
        .copied()
        .collect();
    nxt.sort();
    nxt.dedup();

    let mut head = format!("# 逐章记录:第{ch}章");
    if !title.is_empty() {
        head.push_str(&format!("《{title}》"));
    }
    Ok([
        head,
        String::new(),
        format!("- 结果: {result}"),
        format!(
            "- 角色变化: {}",
            if ent.is_empty() { "无".to_owned() } else { ent.join("; ") }
        ),
        format!(
            "- 伏笔变化: {}",
            if pv.is_empty() { "无".to_owned() } else { pv.join("; ") }
        ),
        format!(
            "- 时间与揭示: {}",
            if ts.is_empty() { "无".to_owned() } else { ts.join("; ") }
        ),
        format!("- 约束: {}", chapter_constraints(root, ch)?),
        format!(
            "- 下一章承诺: {}",
            if nxt.is_empty() { "无".to_owned() } else { nxt.join(", ") }
        ),
    ]
    .join("\n"))
}
