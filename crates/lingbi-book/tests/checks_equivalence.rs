//! P2-05 checks 等价测试：与引擎 checks.py/gates.py 的 doctor stdout 逐字节一致。
//!
//! 两层基准：
//! 1. corpus 快照 5 个 doctor 步（001/002 prewrite、009/010 precommit、016 postcommit）
//!    的 stdout.txt（引擎实跑产物，含尾换行）。
//! 2. 20 个失败路径/边界场景：引擎实跑 oracle（见 ORACLE_* 常量，生成脚本与场景
//!    基建逐项对齐——改测试场景必须先重跑引擎生成 oracle）。
//!
//! 平台注：`缺章摘要` detail 含 `os.path.relpath` 产物（Windows 反斜杠）；oracle 在
//! Windows 生成，测试亦在 Windows 跑（仓库 Windows 目标）。

use lingbi_book::checks::{collect_checks, redlines};
use lingbi_book::gates::{gate, GateResult};
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

fn corpus(step: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/storyrepo-oracle/153eb63/steps")
        .join(step)
}

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

/// 引擎 cli.cmd_doctor 的输出格式（"  ✔/✘ …" + "gate <stage>: PASS|FAIL"，尾换行）。
fn doctor_stdout(g: &GateResult, stage: &str) -> String {
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

// ── 场景基建（与 oracle 生成脚本逐项对齐） ──

/// oracle 脚本的章节 front matter + 正文（正文 hanzi = 30）。
const FM: &str = "---\ntitle: 开局\nchapter: 001\nvolume: 1\nwords: 66\nstatus: 定稿\n\
promises_new: P-001\npromises_advanced: \npromises_done: \n\
info_gaps_new: S-001\ninfo_gaps_revealed: \nentities_new: 阿米,米粒\n\
must_cover: 阿米,米粒\nforbidden: 赵三\nsummary: 阿米夜归。\n---\n\n";
const BODY_OK: &str = "山门之外，夜色如墨。\n\n阿米推开木门，米粒在灯下抬头。\n\n远处有火光一闪而逝。";

/// oracle 脚本的最小追踪状态（与 merge 产物同构）。
const STATE: &str = r#"{"spec": 1, "book": "样板书", "imported_through_chapter": 1, "state_revision": 2, "chapters": {"1": {"status": "committed", "title": "开局", "words": "66"}}, "promises": {"P-001": {"status": "open", "open_ch": 1, "adv_ch": 1, "done_ch": null, "note": ""}}, "info_gaps": {"S-001": {"revealed": false, "plant_ch": 1, "reveal_ch": null, "note": ""}}, "timeline": [{"ch": 1, "event": "阿米夜归。", "kind": "fact"}], "entity_states": {"阿米": {"first_ch": 1, "where": "", "state": ""}, "米粒": {"first_ch": 1, "where": "", "state": ""}}, "reader_knowledge": {"reveals": []}}"#;

/// 大字数下限/上限用 book.yaml。
const BOOK_YAML_STRICT: &str =
    "---\nname: 测试书\nstatus: writing\ntotal_chapters: 4\nmin_words_per_chapter: 3000\n\
     max_words_per_chapter: 3800\ntarget_words: 200000\n---\n";

struct Scenario {
    _dir: tempfile::TempDir,
    root: PathBuf,
}

impl Scenario {
    /// 从 corpus 000-initial 拷贝空书仓（书仓内容随场景改写）。
    fn from_initial() -> Scenario {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("book");
        copy_dir_all(&corpus("000-initial").join("book"), &root);
        Scenario { _dir: dir, root }
    }

    fn write(&self, rel: &str, content: &str) {
        let p = self.root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, content).unwrap();
    }

    fn remove(&self, rel: &str) {
        fs::remove_file(self.root.join(rel)).unwrap();
    }

    fn rename(&self, from: &str, to: &str) {
        fs::rename(self.root.join(from), self.root.join(to)).unwrap();
    }

    fn write_state(&self) {
        self.write("追踪/_tracking-state.json", STATE);
    }

    /// 引擎场景的 settle：状态卡时间刷新为 now（改写后 mtime 最新）。
    fn settle(&self) {
        self.write("追踪/上下文.md", "# 续写状态卡\n");
    }

    fn gate(&self, stage: &str, chapter: Option<i64>) -> GateResult {
        gate(&self.root, stage, chapter).unwrap()
    }

    fn assert_stdout(&self, stage: &str, chapter: Option<i64>, expected: &str) {
        let stdout = doctor_stdout(&self.gate(stage, chapter), stage);
        assert_eq!(stdout, expected, "场景输出与引擎 oracle 不一致");
    }
}

/// 全 PASS 基线场景（words_high: max=10 → hi=210，正文 30 字在内）。
fn scenario_words_high() -> Scenario {
    let b = Scenario::from_initial();
    b.write(
        "book.yaml",
        "---\nname: 测试书\nstatus: writing\ntotal_chapters: 4\n\
             min_words_per_chapter: 100\nmax_words_per_chapter: 10\ntarget_words: 200000\n---\n",
    );
    b.write("定稿/正文/001-开局.md", &format!("{FM}{BODY_OK}"));
    b.write_state();
    b.settle();
    b
}

// ── 引擎 oracle（实跑产物；每一段以 \n 结尾，与 print 一致） ──

const ORACLE_PLACEHOLDER: &str = "  \u{2714} 编号连续且唯一
  \u{2714} 字数窗口
  \u{2714} 泄密扫描
  \u{2714} 实体登记
  \u{2714} 章文件齐全
  \u{2714} AI红线
  \u{2714} 复读检测
  \u{2714} 合同断言
  \u{2714} 追踪新鲜
  \u{2714} 摘要非空
  \u{2718} 占位符: 第1章 含占位符「待补」
gate precommit: FAIL
";

const ORACLE_NUMBERING_GAP: &str = "  \u{2714} 字数窗口
  \u{2714} 占位符
  \u{2714} 泄密扫描
  \u{2714} 实体登记
  \u{2714} 章文件齐全
  \u{2714} AI红线
  \u{2714} 复读检测
  \u{2714} 合同断言
  \u{2714} 追踪新鲜
  \u{2714} 摘要非空
  \u{2718} 编号连续且唯一: 编号不连续或重复: 现有 [1, 3],应为 [1, 2]
gate precommit: FAIL
";

const ORACLE_WORDS_LOW: &str = "  \u{2714} 编号连续且唯一
  \u{2714} 占位符
  \u{2714} 泄密扫描
  \u{2714} 实体登记
  \u{2714} 章文件齐全
  \u{2714} AI红线
  \u{2714} 复读检测
  \u{2714} 合同断言
  \u{2714} 追踪新鲜
  \u{2714} 摘要非空
  \u{2718} 字数窗口: 第1章 30 字,低于下限 2600
gate precommit: FAIL
";

const ORACLE_WORDS_HIGH: &str = "  \u{2714} 编号连续且唯一
  \u{2714} 字数窗口
  \u{2714} 占位符
  \u{2714} 泄密扫描
  \u{2714} 实体登记
  \u{2714} 章文件齐全
  \u{2714} AI红线
  \u{2714} 复读检测
  \u{2714} 合同断言
  \u{2714} 追踪新鲜
  \u{2714} 摘要非空
gate precommit: PASS
";

const ORACLE_LEAK: &str = "  \u{2714} 编号连续且唯一
  \u{2714} 字数窗口
  \u{2714} 占位符
  \u{2714} 实体登记
  \u{2714} 章文件齐全
  \u{2714} AI红线
  \u{2714} 复读检测
  \u{2714} 合同断言
  \u{2714} 追踪新鲜
  \u{2714} 摘要非空
  \u{2718} 泄密扫描: 第1章 正文出现未曝光信息差 S-001
gate precommit: FAIL
";

const ORACLE_LEAK_AFTER_PLANT: &str = "  \u{2714} 编号连续且唯一
  \u{2714} 字数窗口
  \u{2714} 占位符
  \u{2714} 泄密扫描
  \u{2714} 实体登记
  \u{2714} 章文件齐全
  \u{2714} AI红线
  \u{2714} 复读检测
  \u{2714} 合同断言
  \u{2714} 追踪新鲜
  \u{2714} 摘要非空
gate precommit: PASS
";

const ORACLE_ENTITIES_MISSING_ROSTER: &str = "  \u{2714} 编号连续且唯一
  \u{2714} 字数窗口
  \u{2714} 占位符
  \u{2714} 泄密扫描
  \u{2714} 章文件齐全
  \u{2714} AI红线
  \u{2714} 复读检测
  \u{2714} 合同断言
  \u{2714} 追踪新鲜
  \u{2714} 摘要非空
  \u{2718} 实体登记: 第1章 实体 阿米、米粒 未登记(缺 定稿/设定/名册.md)
gate precommit: FAIL
";

const ORACLE_ENTITIES_UNREGISTERED: &str = "  \u{2714} 编号连续且唯一
  \u{2714} 字数窗口
  \u{2714} 占位符
  \u{2714} 泄密扫描
  \u{2714} 章文件齐全
  \u{2714} AI红线
  \u{2714} 复读检测
  \u{2714} 合同断言
  \u{2714} 追踪新鲜
  \u{2714} 摘要非空
  \u{2718} 实体登记: 第1章 实体未登记于名册: 阿米
gate precommit: FAIL
";

const ORACLE_NO_FRONTMATTER: &str = "  \u{2714} 编号连续且唯一
  \u{2714} 字数窗口
  \u{2714} 占位符
  \u{2714} 泄密扫描
  \u{2714} 实体登记
  \u{2714} AI红线
  \u{2714} 复读检测
  \u{2714} 合同断言
  \u{2714} 追踪新鲜
  \u{2718} 章文件齐全: 第1章 缺 front matter
  \u{2718} 摘要非空: 第1章 summary 为空
gate precommit: FAIL
";

const ORACLE_MISSING_SUMMARY_FILE: &str = "  \u{2714} 编号连续且唯一
  \u{2714} 字数窗口
  \u{2714} 占位符
  \u{2714} 泄密扫描
  \u{2714} 实体登记
  \u{2714} AI红线
  \u{2714} 复读检测
  \u{2714} 合同断言
  \u{2714} 追踪新鲜
  \u{2714} 摘要非空
  \u{2718} 章文件齐全: 第1章 缺章摘要 定稿\\记忆\\章摘要\\001.md
gate precommit: FAIL
";

const ORACLE_REDLINE: &str = "  \u{2714} 编号连续且唯一
  \u{2714} 字数窗口
  \u{2714} 占位符
  \u{2714} 泄密扫描
  \u{2714} 实体登记
  \u{2714} 章文件齐全
  \u{2714} 复读检测
  \u{2714} 合同断言
  \u{2714} 追踪新鲜
  \u{2714} 摘要非空
  \u{2718} AI红线: 第1章 红线「章末总结体·终于明白」命中: 片段 …闪而逝。

他终于明白，一切都已注…
gate precommit: FAIL
";

const ORACLE_REDLINE_COUNTEREXAMPLE: &str = "  \u{2714} 编号连续且唯一
  \u{2714} 字数窗口
  \u{2714} 占位符
  \u{2714} 泄密扫描
  \u{2714} 实体登记
  \u{2714} 章文件齐全
  \u{2714} 复读检测
  \u{2714} 合同断言
  \u{2714} 追踪新鲜
  \u{2714} 摘要非空
  \u{2718} AI红线: 第1章 红线「反例库·山外青山」命中: 片段 …闪而逝。

山外青山楼外楼。
gate precommit: FAIL
";

const ORACLE_REPEAT_ADJACENT: &str = "  \u{2714} 编号连续且唯一
  \u{2714} 字数窗口
  \u{2714} 占位符
  \u{2714} 泄密扫描
  \u{2714} 实体登记
  \u{2714} 章文件齐全
  \u{2714} AI红线
  \u{2714} 追踪新鲜
  \u{2714} 摘要非空
  \u{2718} 复读检测: 第1章 相邻段落存在重复片段「他推开木门」
  \u{2718} 合同断言: 第1章 未覆盖必含词 阿米、米粒
gate precommit: FAIL
";

const ORACLE_REPEAT_4GRAM: &str = "  \u{2714} 编号连续且唯一
  \u{2714} 字数窗口
  \u{2714} 占位符
  \u{2714} 泄密扫描
  \u{2714} 实体登记
  \u{2714} 章文件齐全
  \u{2714} AI红线
  \u{2714} 追踪新鲜
  \u{2714} 摘要非空
  \u{2718} 复读检测: 第1章 4字片段「山中灯火」出现 6 次
  \u{2718} 合同断言: 第1章 未覆盖必含词 阿米、米粒
gate precommit: FAIL
";

const ORACLE_CONTRACT: &str = "  \u{2714} 编号连续且唯一
  \u{2714} 字数窗口
  \u{2714} 占位符
  \u{2714} 泄密扫描
  \u{2714} 实体登记
  \u{2714} 章文件齐全
  \u{2714} AI红线
  \u{2714} 复读检测
  \u{2714} 追踪新鲜
  \u{2714} 摘要非空
  \u{2718} 合同断言: 第1章 未覆盖必含词 赵四; 命中禁区词 赵三
gate precommit: FAIL
";

const ORACLE_FRESH_STALE_BODY: &str = "  \u{2714} 编号连续且唯一
  \u{2714} 字数窗口
  \u{2714} 占位符
  \u{2714} 泄密扫描
  \u{2714} 实体登记
  \u{2714} 章文件齐全
  \u{2714} AI红线
  \u{2714} 复读检测
  \u{2714} 合同断言
  \u{2714} 摘要非空
  \u{2718} 追踪新鲜: 追踪状态早于最新正文(先重跑 state 合并/保存)
gate precommit: FAIL
";

const ORACLE_FRESH_STALE_CARD: &str = "  \u{2714} 编号连续且唯一
  \u{2714} 字数窗口
  \u{2714} 占位符
  \u{2714} 泄密扫描
  \u{2714} 实体登记
  \u{2714} 章文件齐全
  \u{2714} AI红线
  \u{2714} 复读检测
  \u{2714} 合同断言
  \u{2714} 摘要非空
  \u{2718} 追踪新鲜: 状态卡早于状态文件(派生视图未重建,重跑 commit 或 views 重建)
gate precommit: FAIL
";

const ORACLE_SUMMARY_EMPTY: &str = "  \u{2714} 编号连续且唯一
  \u{2714} 字数窗口
  \u{2714} 占位符
  \u{2714} 泄密扫描
  \u{2714} 实体登记
  \u{2714} 章文件齐全
  \u{2714} AI红线
  \u{2714} 复读检测
  \u{2714} 合同断言
  \u{2714} 追踪新鲜
  \u{2718} 摘要非空: 第1章 summary 为空
gate precommit: FAIL
";

const ORACLE_WEAK_HINT: &str = "  \u{2714} 编号连续且唯一
  \u{2714} 字数窗口(弱提示: 第1章 30 字,低于下限 2600)
  \u{2714} 占位符
  \u{2714} 泄密扫描(跳过)
  \u{2714} 实体登记(跳过)
  \u{2714} 章文件齐全(跳过)
  \u{2714} AI红线(跳过)
  \u{2714} 复读检测(跳过)
  \u{2714} 合同断言(跳过)
  \u{2714} 追踪新鲜(跳过)
  \u{2714} 摘要非空
gate prewrite: PASS
";

const ORACLE_CHAPTER_NOT_EXIST: &str = "  \u{2714} 编号连续且唯一
  \u{2714} 追踪新鲜
  \u{2718} 字数窗口: 第99章 不存在
  \u{2718} 占位符: 第99章 不存在
  \u{2718} 泄密扫描: 第99章 不存在
  \u{2718} 实体登记: 第99章 不存在
  \u{2718} 章文件齐全: 第99章 不存在
  \u{2718} AI红线: 第99章 不存在
  \u{2718} 复读检测: 第99章 不存在
  \u{2718} 合同断言: 第99章 不存在
  \u{2718} 摘要非空: 第99章 不存在
gate precommit: FAIL
";

const ORACLE_PLANT_ZERO: &str = "  \u{2714} 编号连续且唯一
  \u{2714} 字数窗口
  \u{2714} 占位符
  \u{2714} 实体登记
  \u{2714} 章文件齐全
  \u{2714} AI红线
  \u{2714} 复读检测
  \u{2714} 合同断言
  \u{2714} 追踪新鲜
  \u{2714} 摘要非空
  \u{2718} 泄密扫描: 第1章 正文出现未曝光信息差 S-001
gate precommit: FAIL
";

const ORACLE_REPEAT_TIE: &str = "  \u{2714} 编号连续且唯一
  \u{2714} 字数窗口
  \u{2714} 占位符
  \u{2714} 泄密扫描
  \u{2714} 实体登记
  \u{2714} 章文件齐全
  \u{2714} AI红线
  \u{2714} 追踪新鲜
  \u{2714} 摘要非空
  \u{2718} 复读检测: 第1章 4字片段「山风夜雨」出现 6 次
  \u{2718} 合同断言: 第1章 未覆盖必含词 阿米、米粒
gate precommit: FAIL
";

// ── corpus 步：doctor stdout 等价 ──

#[test]
fn doctor_stdout_matches_corpus_every_step() {
    // 验收标准：doctor 步骤 stdout 与 corpus 一致（5 个 corpus 步全 PASS）
    for (step, stage, chapter) in [
        ("001-doctor-prewrite-1", "prewrite", Some(1)),
        ("002-doctor-prewrite-2", "prewrite", Some(2)),
        ("009-doctor-precommit-1", "precommit", Some(1)),
        ("010-doctor-precommit-2", "precommit", Some(2)),
        ("016-doctor-postcommit-2", "postcommit", Some(2)),
    ] {
        let src = corpus(step);
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("book");
        copy_dir_all(&src.join("book"), &root);
        // Windows fs::copy 保留 mtime（追踪新鲜依赖）；Linux 行为差异文档化
        let g = gate(&root, stage, chapter).unwrap_or_else(|e| panic!("{step}: {e}"));
        let stdout = doctor_stdout(&g, stage);
        let expected = fs::read_to_string(src.join("stdout.txt")).unwrap();
        assert_eq!(stdout, expected, "{step} stdout 与 corpus 不一致");
        assert_eq!(
            g.passed,
            expected.contains(": PASS"),
            "{step} gate passed 与 exit code 不一致"
        );
    }
}

// ── 场景 oracle：失败路径/边界 ──

#[test]
fn placeholder_fails_like_engine() {
    let b = Scenario::from_initial();
    b.write("定稿/正文/001-开局.md", &format!("{FM}{BODY_OK}\n\n此处待补细节。"));
    b.write_state();
    b.settle();
    b.assert_stdout("precommit", Some(1), ORACLE_PLACEHOLDER);
}

#[test]
fn numbering_gap_fails_like_engine() {
    let b = Scenario::from_initial();
    b.rename("定稿/正文/002-夜访.md", "定稿/正文/003-夜访.md");
    b.write("定稿/记忆/章摘要/003.md", "# 章摘要 003\n");
    b.write_state();
    b.settle();
    b.assert_stdout("precommit", None, ORACLE_NUMBERING_GAP);
}

#[test]
fn words_low_fails_like_engine() {
    let b = Scenario::from_initial();
    b.write("book.yaml", BOOK_YAML_STRICT);
    b.write("定稿/正文/001-开局.md", &format!("{FM}{BODY_OK}"));
    b.write_state();
    b.settle();
    b.assert_stdout("precommit", Some(1), ORACLE_WORDS_LOW);
}

#[test]
fn words_high_passes_like_engine() {
    // 正文 30 字在 [max(0,100-400), 10+200] 窗口内 → 全 PASS 基线
    let b = scenario_words_high();
    b.assert_stdout("precommit", Some(1), ORACLE_WORDS_HIGH);
}

#[test]
fn leak_fails_like_engine() {
    let b = Scenario::from_initial();
    b.write("定稿/正文/001-开局.md", &format!("{FM}{BODY_OK}\n\nS-001 的秘密被提及。"));
    b.write_state();
    b.settle();
    b.assert_stdout("precommit", Some(1), ORACLE_LEAK);
}

#[test]
fn leak_after_plant_chapter_passes_like_engine() {
    // 埋设章 2 > 目标章 1 → 时序豁免不报
    let b = Scenario::from_initial();
    b.write("定稿/正文/001-开局.md", &format!("{FM}{BODY_OK}\n\nS-001 的秘密。"));
    b.write(
        "追踪/_tracking-state.json",
        &STATE.replace("\"plant_ch\": 1", "\"plant_ch\": 2"),
    );
    b.settle();
    b.assert_stdout("precommit", Some(1), ORACLE_LEAK_AFTER_PLANT);
}

#[test]
fn entities_missing_roster_fails_like_engine() {
    let b = Scenario::from_initial();
    b.remove("定稿/设定/名册.md");
    b.write("定稿/正文/001-开局.md", &format!("{FM}{BODY_OK}"));
    b.write_state();
    b.settle();
    b.assert_stdout("precommit", Some(1), ORACLE_ENTITIES_MISSING_ROSTER);
}

#[test]
fn entities_unregistered_fails_like_engine() {
    let b = Scenario::from_initial();
    b.write("定稿/设定/名册.md", "# 名册\n\n米粒\n");
    b.write("定稿/正文/001-开局.md", &format!("{FM}{BODY_OK}"));
    b.write_state();
    b.settle();
    b.assert_stdout("precommit", Some(1), ORACLE_ENTITIES_UNREGISTERED);
}

#[test]
fn no_frontmatter_fails_like_engine() {
    let b = Scenario::from_initial();
    b.write("定稿/正文/001-开局.md", "正文无 front matter。");
    b.write_state();
    b.settle();
    b.assert_stdout("precommit", Some(1), ORACLE_NO_FRONTMATTER);
}

#[test]
fn missing_summary_file_fails_like_engine() {
    let b = Scenario::from_initial();
    b.remove("定稿/记忆/章摘要/001.md");
    b.write("定稿/正文/001-开局.md", &format!("{FM}{BODY_OK}"));
    b.write_state();
    b.settle();
    b.assert_stdout("precommit", Some(1), ORACLE_MISSING_SUMMARY_FILE);
}

#[test]
fn redline_fails_like_engine() {
    let b = Scenario::from_initial();
    b.write(
        "定稿/正文/001-开局.md",
        &format!("{FM}{BODY_OK}\n\n他终于明白，一切都已注定。"),
    );
    b.write_state();
    b.settle();
    b.assert_stdout("precommit", Some(1), ORACLE_REDLINE);
}

#[test]
fn redline_counterexample_merged_like_engine() {
    let b = Scenario::from_initial();
    b.write("文风/审查反例库.md", "# 反例库\n\n山外青山\n");
    b.write(
        "定稿/正文/001-开局.md",
        &format!("{FM}{BODY_OK}\n\n山外青山楼外楼。"),
    );
    b.write_state();
    b.settle();
    b.assert_stdout("precommit", Some(1), ORACLE_REDLINE_COUNTEREXAMPLE);
}

#[test]
fn repeat_adjacent_fails_like_engine() {
    let b = Scenario::from_initial();
    b.write(
        "定稿/正文/001-开局.md",
        &format!("{FM}他推开木门走进来。\n\n他推开木门走出去。"),
    );
    b.write_state();
    b.settle();
    b.assert_stdout("precommit", Some(1), ORACLE_REPEAT_ADJACENT);
}

#[test]
fn repeat_4gram_fails_like_engine() {
    let b = Scenario::from_initial();
    b.write(
        "定稿/正文/001-开局.md",
        &format!("{FM}{}", "山中灯火".repeat(6)),
    );
    b.write_state();
    b.settle();
    b.assert_stdout("precommit", Some(1), ORACLE_REPEAT_4GRAM);
}

#[test]
fn contract_fails_like_engine() {
    let b = Scenario::from_initial();
    b.write(
        "定稿/正文/001-开局.md",
        &format!(
            "{}{BODY_OK}\n\n赵三出现了。",
            FM.replace("must_cover: 阿米,米粒", "must_cover: 阿米,赵四")
        ),
    );
    b.write_state();
    b.settle();
    b.assert_stdout("precommit", Some(1), ORACLE_CONTRACT);
}

#[test]
fn fresh_stale_body_fails_like_engine() {
    // 目标章 1 除外 → 重写非目标章 2 使其新于状态文件
    let b = Scenario::from_initial();
    b.write_state();
    b.settle();
    thread::sleep(Duration::from_millis(5)); // 防 mtime 同 tick 抖动
    let body2 = fs::read_to_string(b.root.join("定稿/正文/002-夜访.md")).unwrap();
    b.write("定稿/正文/002-夜访.md", &body2);
    b.assert_stdout("precommit", Some(1), ORACLE_FRESH_STALE_BODY);
}

#[test]
fn fresh_stale_card_fails_like_engine() {
    // 状态卡先于状态文件写入 → 卡旧于状态 → L4 中间态失败
    let b = Scenario::from_initial();
    b.write("追踪/上下文.md", "# 续写状态卡\n");
    thread::sleep(Duration::from_millis(5)); // 防 mtime 同 tick 抖动
    b.write_state();
    b.assert_stdout("precommit", Some(1), ORACLE_FRESH_STALE_CARD);
}

#[test]
fn summary_empty_fails_like_engine() {
    let b = Scenario::from_initial();
    b.write(
        "定稿/正文/001-开局.md",
        &format!(
            "{}{BODY_OK}",
            FM.replace("summary: 阿米夜归。", "summary: ")
        ),
    );
    b.write_state();
    b.settle();
    b.assert_stdout("precommit", Some(1), ORACLE_SUMMARY_EMPTY);
}

#[test]
fn prewrite_weak_hint_like_engine() {
    // prewrite: 第 2 项字数仅弱提示不计 fail，4-10 项记跳过
    let b = Scenario::from_initial();
    b.write("book.yaml", BOOK_YAML_STRICT);
    b.write("定稿/正文/001-开局.md", &format!("{FM}{BODY_OK}"));
    b.write_state();
    b.settle();
    b.assert_stdout("prewrite", Some(1), ORACLE_WEAK_HINT);
}

#[test]
fn chapter_not_exist_fails_like_engine() {
    let b = Scenario::from_initial();
    b.write_state();
    b.settle();
    b.assert_stdout("precommit", Some(99), ORACLE_CHAPTER_NOT_EXIST);
}

#[test]
fn leak_plant_ch_zero_treated_as_one_like_engine() {
    // 引擎 v.get("plant_ch", 1) or 1: plant_ch=0 → 1 → 第1章 ≥1 命中
    let b = Scenario::from_initial();
    b.write(
        "定稿/正文/001-开局.md",
        &format!("{FM}{BODY_OK}\n\nS-001 的秘密。"),
    );
    b.write(
        "追踪/_tracking-state.json",
        &STATE.replace("\"plant_ch\": 1", "\"plant_ch\": 0"),
    );
    b.settle();
    b.assert_stdout("precommit", Some(1), ORACLE_PLANT_ZERO);
}

#[test]
fn repeat_4gram_tie_prefers_first_occurrence_like_engine() {
    // A=山风夜雨 首现于 0；B=灯火阑珊 后到但先达 6 次 → 引擎 max 按首现序取 A
    let b = Scenario::from_initial();
    b.write(
        "定稿/正文/001-开局.md",
        &format!(
            "{FM}{}{}",
            "山风夜雨",
            "灯火阑珊".repeat(6) + &"山风夜雨".repeat(5)
        ),
    );
    b.write_state();
    b.settle();
    b.assert_stdout("precommit", Some(1), ORACLE_REPEAT_TIE);
}

#[test]
fn leak_missing_state_detail_matches_engine_prefix() {
    // 缺追踪状态：泄密扫描 fail detail = 引擎 FileNotFoundError str
    // （实测 str(e) 无 Errno 前缀，即自定义消息本体；路径部分不做全等比较）
    let b = Scenario::from_initial();
    b.remove("追踪/_tracking-state.json");
    let g = b.gate("precommit", Some(1));
    let leak = g
        .fails
        .iter()
        .find(|f| f.starts_with("泄密扫描"))
        .expect("泄密扫描应 fail");
    assert!(
        leak.starts_with("泄密扫描: 缺追踪状态,无法扫描泄密(缺追踪状态(先跑 storyrepo init): "),
        "{leak}"
    );
    assert!(leak.ends_with("\\_tracking-state.json)"), "{leak}");
    assert!(
        g.fails
            .iter()
            .any(|f| f.starts_with("追踪新鲜: 缺追踪状态(")),
        "{:?}",
        g.fails
    );
}

// ── 模块级行为 ──

#[test]
fn collect_checks_shape_on_corpus() {
    // 009 步 precommit 全查:11 项固定顺序 + 名称 + 全 ok
    let src = corpus("009-doctor-precommit-1");
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    copy_dir_all(&src.join("book"), &root);
    let checks = collect_checks(&root, "precommit", Some(1)).unwrap();
    assert_eq!(checks.len(), 11);
    let names: Vec<&str> = checks.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "编号连续且唯一",
            "字数窗口",
            "占位符",
            "泄密扫描",
            "实体登记",
            "章文件齐全",
            "AI红线",
            "复读检测",
            "合同断言",
            "追踪新鲜",
            "摘要非空",
        ]
    );
    assert!(checks.iter().all(|c| c.ok), "009 步 11 项应全过");
}

#[test]
fn collect_checks_rejects_bad_stage() {
    let b = Scenario::from_initial();
    let err = collect_checks(&b.root, "precommit2", None).unwrap_err();
    assert_eq!(err.code, lingbi_contracts::ErrorCode::BookCorrupted);
    assert!(err.message.contains("stage 非法: precommit2"));
}

#[test]
fn redlines_merges_counterexamples_like_engine() {
    // 无反例库 → 仅内置 9 条
    let b = Scenario::from_initial();
    let rl = redlines(Some(&b.root)).unwrap();
    assert_eq!(rl.len(), 9);
    assert!(rl.iter().any(|(p, n)| n == "章末总结体·终于明白" && p == "他终于明白"));
    // 有反例库 → 并入 (re.escape 产物, "反例库·<短语>" 命名)
    b.write("文风/审查反例库.md", "# 反例库\n\n山外青山\n");
    let rl2 = redlines(Some(&b.root)).unwrap();
    assert_eq!(rl2.len(), 10);
    assert!(rl2.iter().any(|(p, n)| n == "反例库·山外青山" && p == "山外青山"));
    // None → 仅内置
    assert_eq!(redlines(None).unwrap().len(), 9);
}
