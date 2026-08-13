//! P2-04 derived views 等价测试：与引擎 `views.py` 逐文件字节一致。
//!
//! 基准：corpus 快照 `steps/012-commit-1 .. 022-report-write-2` 的 `追踪/` 派生视图
//! （引擎 commit/revision 等步骤写入的产物）。测试对每步：拷贝书仓 → 载入该步状态 →
//! `write_views` 重建 → `追踪/` 下全部文件逐字节比较（含 _tracking-state.json，
//! views 不得触碰它）。
//!
//! 单测场景的期望值均为引擎实跑产物（oracle，见各测试注释）。

use lingbi_book::state::MAX_CARD_BYTES;
use lingbi_book::tracking::{init_state, load_state, new_state};
use lingbi_book::views::{card7, card_size_ok, write_views};
use std::fs;
use std::path::{Path, PathBuf};

fn corpus(root: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/storyrepo-oracle/153eb63/steps")
        .join(root)
        .join("book")
}

/// corpus 中带完整派生视图产物的全部步骤（012=commit-1, 015=commit-2, 019=revision-2 …）。
const STEPS_WITH_VIEWS: [&str; 11] = [
    "012-commit-1",
    "013-check",
    "014-status",
    "015-commit-2",
    "016-doctor-postcommit-2",
    "017-stats",
    "018-resume-3",
    "019-revision-2",
    "020-learn-2",
    "021-batch-create-3-4",
    "022-report-write-2",
];

fn copy_dir_all(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let target = dst.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir_all(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn collect_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in fs::read_dir(&d).unwrap() {
            let entry = entry.unwrap();
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// 追踪/ 下全部文件逐字节比较（文件集一致 + 内容一致）。
fn assert_tracking_identical(expected: &Path, actual: &Path) {
    let exp = collect_files(expected);
    let act = collect_files(actual);
    assert_eq!(exp.len(), act.len(), "文件数不一致: 期望 {} 实得 {}", exp.len(), act.len());
    for (e, a) in exp.iter().zip(act.iter()) {
        let rel = e.strip_prefix(expected).unwrap();
        assert_eq!(rel, a.strip_prefix(actual).unwrap(), "文件集不一致");
        assert_eq!(
            fs::read(a).unwrap(),
            fs::read(e).unwrap(),
            "字节不一致: {}",
            rel.display()
        );
    }
}

// ── 引擎 oracle（实跑产物） ──

/// 空书仓状态卡：无 book.yaml / 无细纲蓝图 / 无正文 → DEFAULTS(330) + 占位合同 + 无正文提示。
const ORACLE_EMPTY_CARD: &str = "\
# 续写状态卡(7 栏)

## 1. 当前进度
最新章: 0 / 计划: 330 | 活跃承诺: 0 | 未曝光信息差: 0

## 2. 下一章合同
（无未勾选细纲行,续写依总纲自由发挥）

## 3. 活跃伏笔
（无）

## 4. 核心角色状态
（无）

## 5. 最近章节结尾
（无正文,待首章落笔）

## 6. 未曝光信息差
（无）

## 7. 风格锚点
一、拒绝抽象升华与万能比喻,写具体动作与可感细节。
二、每章必有推进:情节、人物或伏笔至少一项向前。
三、章末留钩:悬念、转折或新伏笔,让读者想翻下一页。
";

/// 空状态 + 完整书仓（000-initial）状态卡：最新章=2（读文件）、计划=4（读 book.yaml）。
const ORACLE_INITIAL_CARD: &str = "\
# 续写状态卡(7 栏)

## 1. 当前进度
最新章: 2 / 计划: 4 | 活跃承诺: 0 | 未曝光信息差: 0

## 2. 下一章合同
| ch | 标题 | 要发生什么 | 硬约束 | 禁区 | 角色 | suspense | foreshadow | twist |

## 3. 活跃伏笔
（无）

## 4. 核心角色状态
（无）

## 5. 最近章节结尾
- 第1章: 远处有火光一闪而逝。
- 第2章: 门内没有回应。

## 6. 未曝光信息差
（无）

## 7. 风格锚点
一、拒绝抽象升华与万能比喻,写具体动作与可感细节。
二、每章必有推进:情节、人物或伏笔至少一项向前。
三、章末留钩:悬念、转折或新伏笔,让读者想翻下一页。
";

/// 空书仓其余视图（引擎 oracle）。
const ORACLE_EMPTY_VAULT: &str = "\
# 伏笔与信息差台账

## 伏笔
| ID | 状态 | 埋设章 | 最近推进 | 完成章 | 备注 |
| --- | --- | --- | --- | --- | --- |

## 信息差
| ID | 状态 | 埋设章 | 最近推进 | 完成章 | 备注 |
| --- | --- | --- | --- | --- | --- |";

const ORACLE_EMPTY_TRUTH: &str = "\
# 作者真相(仅作者可见)

## 时间线(全量)
（空）

## 未曝光信息差(作者才知,读者未见)
（无）";

const ORACLE_EMPTY_KNOWN: &str = "\
# 读者已知(读者视角)

## 时间线(读者已见)
（空）

## 已曝光信息差
（无）";

const ORACLE_EMPTY_TIMELINE: &str = "\
# 时间线(append-only,只增不改)

| 章 | 类型 | 事件 |
| --- | --- | --- |
|  |  | （空） |";

// ── 等价测试 ──

#[test]
fn views_byte_identical_to_engine_every_step() {
    // 验收标准 1：commit 步骤后全部派生视图与 corpus 快照逐文件一致（012-022 共 11 步）
    for step in STEPS_WITH_VIEWS {
        let src = corpus(step);
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("book");
        copy_dir_all(&src, &root);
        let s = load_state(&root).unwrap_or_else(|e| panic!("{step}: {e}"));
        write_views(&root, &s).unwrap_or_else(|e| panic!("{step}: {e}"));
        assert_tracking_identical(&src.join("追踪"), &root.join("追踪"));
        // 验收标准 2：状态卡 ≤12KB
        let card = card7(&root, &s).unwrap();
        assert!(card.len() <= MAX_CARD_BYTES, "{step} 状态卡超限: {}B", card.len());
        assert!(card_size_ok(&root), "{step} card_size_ok 应为 true");
    }
}

#[test]
fn card7_empty_book_matches_engine() {
    // 合成空书仓：无 book.yaml/大纲/正文（引擎 oracle 实跑核验）
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    init_state(&root).unwrap();
    let s = load_state(&root).unwrap();
    assert_eq!(card7(&root, &s).unwrap(), ORACLE_EMPTY_CARD);
    write_views(&root, &s).unwrap();
    for (name, oracle) in [
        ("伏笔.md", ORACLE_EMPTY_VAULT),
        ("作者真相.md", ORACLE_EMPTY_TRUTH),
        ("读者已知.md", ORACLE_EMPTY_KNOWN),
        ("时间线.md", ORACLE_EMPTY_TIMELINE),
    ] {
        let p = root.join("追踪").join(name);
        assert_eq!(
            fs::read_to_string(&p).unwrap(),
            oracle,
            "{}",
            p.display()
        );
    }
    // 空状态无实体/无章节 → 两个子目录无文件
    assert_eq!(collect_files(&root.join("追踪/角色状态")).len(), 0);
    assert_eq!(collect_files(&root.join("追踪/逐章记录")).len(), 0);
}

#[test]
fn card7_initial_corpus_matches_engine() {
    // 000-initial：空状态 + 完整书仓（引擎 oracle）
    let root = corpus("000-initial");
    let s = load_state(&root).unwrap();
    assert_eq!(card7(&root, &s).unwrap(), ORACLE_INITIAL_CARD);
}

#[test]
fn card_endings_skip_banner_paragraphs_like_engine() {
    // —— 横幅段从最近结尾候选剔除（引擎 oracle：最后一段是横幅 → 取上一段）
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    fs::create_dir_all(root.join("定稿/正文")).unwrap();
    fs::write(
        root.join("定稿/正文/001-开局.md"),
        "---\ntitle: 开局\n---\n\n第一段正文。\n\n中间段。\n\n—— 横幅段(应跳过)\n",
    )
    .unwrap();
    let s = new_state(&root);
    let card = card7(&root, &s).unwrap();
    assert!(card.contains("## 5. 最近章节结尾\n- 第1章: 中间段。"), "{card}");
}

#[test]
fn card_contract_skips_checked_lines_like_engine() {
    // 细纲蓝图首条未勾选行作下一章合同（引擎 oracle：跳过 - [x] 已完成行）
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    fs::create_dir_all(root.join("大纲")).unwrap();
    fs::write(
        root.join("大纲/细纲蓝图.md"),
        "# 细纲蓝图\n\n- [x] 已完成行\n- [ ] 下一章任务 A\n普通行应被忽略?不——首条未勾选即返回\n",
    )
    .unwrap();
    let s = new_state(&root);
    let card = card7(&root, &s).unwrap();
    assert!(card.contains("## 2. 下一章合同\n下一章任务 A"), "{card}");
}

#[test]
fn card_contract_empty_unchecked_line_falls_back_to_placeholder() {
    // "- [ ] " 空勾选行 → 引擎 contract=""（falsy）→ 卡显示占位（oracle 核验）
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    fs::create_dir_all(root.join("大纲")).unwrap();
    fs::write(root.join("大纲/细纲蓝图.md"), "- [ ] \n").unwrap();
    let s = new_state(&root);
    let card = card7(&root, &s).unwrap();
    assert!(card.contains(
        "## 2. 下一章合同\n（无未勾选细纲行,续写依总纲自由发挥）"
    ));
}

#[test]
fn card7_truncates_at_12kb_utf8_safe_like_engine() {
    // 超限卡：巨量 note → 引擎 _cut 按字节截断、丢弃不完整尾字节（oracle: 12287B, 尾=…待首章落）
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    fs::create_dir_all(root.join("大纲")).unwrap();
    fs::write(root.join("大纲/细纲蓝图.md"), "| ch | 标题 | 要发生什么 |\n").unwrap();
    let mut s = new_state(&root);
    s.promises.insert(
        "P-001".to_owned(),
        lingbi_book::tracking::PromiseEntry {
            status: "open".to_owned(),
            open_ch: Some(1),
            adv_ch: Some(1),
            done_ch: None,
            note: "长".repeat(4000), // 12000B 的 note → 卡必然超 12KB
        },
    );
    let card = card7(&root, &s).unwrap();
    assert_eq!(card.len(), 12287, "截断应止于 12288B 前的完整字符边界");
    assert!(card.ends_with("## 5. 最近章节结尾\n（无正文,待首章落"));
    assert!(card.starts_with("# 续写状态卡(7 栏)"));
    // 截断发生在 笔 字中部（原串 12288B 处）→ 丢弃 1 字节；String 天然保证无残字节
}

#[test]
fn write_views_removes_stale_entity_files_and_logs() {
    // 引擎先清 角色状态/逐章记录 下旧 .md 再写当前（实体/章节被移除时不留残卡）
    let src = corpus("015-commit-2");
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    copy_dir_all(&src, &root);
    let mut s = load_state(&root).unwrap();
    s.entity_states.shift_remove("米粒");
    s.chapters.shift_remove("2");
    write_views(&root, &s).unwrap();
    assert!(!root.join("追踪/角色状态/米粒.md").exists(), "残卡未清理");
    assert!(root.join("追踪/角色状态/阿米.md").exists());
    assert!(!root.join("追踪/逐章记录/ch002.md").exists(), "残日志未清理");
    assert!(root.join("追踪/逐章记录/ch001.md").exists());
}

#[test]
fn note_separators_match_engine() {
    // 引擎: 状态卡 §3/§6 用 "- ID 备注"（空格），作者真相/读者已知用 "- ID: 备注"（冒号）
    // （引擎 oracle 核验：views.py `"- %s %s"` vs `"- %s: %s"`）
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    let mut s = new_state(&root);
    s.promises.insert(
        "P-001".to_owned(),
        lingbi_book::tracking::PromiseEntry {
            status: "open".to_owned(),
            open_ch: Some(1),
            adv_ch: Some(1),
            done_ch: None,
            note: "备注甲".to_owned(),
        },
    );
    s.info_gaps.insert(
        "S-001".to_owned(),
        lingbi_book::tracking::InfoGapEntry {
            revealed: true,
            plant_ch: Some(1),
            reveal_ch: None,
            note: "备注乙".to_owned(),
        },
    );
    s.info_gaps.insert(
        "S-002".to_owned(),
        lingbi_book::tracking::InfoGapEntry {
            revealed: false,
            plant_ch: Some(1),
            reveal_ch: None,
            note: "备注丙".to_owned(),
        },
    );
    let card = card7(&root, &s).unwrap();
    assert!(card.contains("- P-001 备注甲"), "状态卡应空格分隔: {card}");
    assert!(!card.contains("- P-001: "));
    write_views(&root, &s).unwrap();
    let truth = fs::read_to_string(root.join("追踪/作者真相.md")).unwrap();
    let known = fs::read_to_string(root.join("追踪/读者已知.md")).unwrap();
    assert!(truth.contains("- S-002: 备注丙"), "作者真相应冒号分隔: {truth}");
    assert!(known.contains("- S-001: 备注乙"), "读者已知应冒号分隔: {known}");
}

#[test]
fn card_size_ok_missing_file_is_false() {
    let dir = tempfile::tempdir().unwrap();
    assert!(!card_size_ok(dir.path()));
    fs::create_dir_all(dir.path().join("追踪")).unwrap();
    assert!(!card_size_ok(dir.path()));
}
