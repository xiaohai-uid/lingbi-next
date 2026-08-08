//! P2-09 brief 等价测试：与引擎 brief.py 的 build_brief + cli.cmd_brief 输出逐字节一致。
//!
//! 两层基准：
//! 1. corpus 快照 007-brief-1 / 008-brief-2（同一 book，brief --chapter 1/2）。
//!    brief 为纯文本（无绝对路径），直接字节比较（CLI print 补尾换行）。
//! 2. 5 个边界场景：引擎实跑 oracle（生成脚本 /tmp/lingbi-brief-oracle/
//!    gen_brief_oracle.py——改测试场景必须先重跑引擎）。

use lingbi_book::contract::Contract;
use lingbi_book::context::Context;
use lingbi_book::brief::build_brief;
use lingbi_book::tracking::load_state;
use std::fs;
use std::path::{Path, PathBuf};

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

fn write(root: &Path, rel: &str, content: &str) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, content).unwrap();
}

fn brief_for(root: &Path, ch: i64) -> String {
    let state = load_state(root).expect("load_state");
    build_brief(root, &state, ch, None, None)
        .expect("build_brief")
        + "\n"
}

// ── 1) corpus 007-brief-1 / 008-brief-2 ──

#[test]
fn corpus_brief_1_and_2_stdout_match() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    copy_dir_all(&corpus("007-brief-1").join("book"), &root);
    for (step, ch) in [("007-brief-1", 1), ("008-brief-2", 2)] {
        let want = fs::read_to_string(corpus(step).join("stdout.txt")).unwrap();
        assert_eq!(brief_for(&root, ch), want, "corpus {step}");
    }
}

// ── 2) 场景基建（与 oracle 生成脚本逐项对齐） ──

fn scenario(name: &str, ch: i64, modify: impl Fn(&Path), oracle: &str) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    copy_dir_all(&corpus("007-brief-1").join("book"), &root);
    modify(&root);
    assert_eq!(brief_for(&root, ch), oracle, "{name}");
}

/// 缺风格宪法 → "无风格宪法文件"。
const NO_STYLE: &str = concat!(
    "# 第1章 写作任务书(样板书)\n",
    "\n",
    "## 一、开篇委托\n",
    "- 书名:样板书\n",
    "- 章号:1\n",
    "- 标题:开局\n",
    "- 一句话目标:阿米与米粒重逢,火光预示\n",
    "\n",
    "## 二、这章的故事\n",
    "- 前文摘要:暂无前文摘要\n",
    "- 目标:阿米与米粒重逢,火光预示\n",
    "- 节点(细纲·要发生什么):阿米与米粒重逢,火光预示\n",
    "- 必覆盖:不写赵三\n",
    "- 禁区:赵三\n",
    "- 检索线索(实体卡):\n",
    "  - 【阿米】## 身份\n",
    "主角,沉默寡言。\n",
    "## 现状\n",
    "第一章与米粒重逢。\n",
    "\n",
    "  - 【米粒】## 身份\n",
    "灯下之人,写作者。\n",
    "## 现状\n",
    "第二章失联。\n",
    "\n",
    "\n",
    "## 三、这章的人物\n",
    "- 阿米: ## 身份\n",
    "主角,沉默寡言。\n",
    "## 现状\n",
    "第一章与米粒重逢。\n",
    "\n",
    "- 米粒: ## 身份\n",
    "灯下之人,写作者。\n",
    "## 现状\n",
    "第二章失联。\n",
    "\n",
    "\n",
    "## 四、怎么写更顺\n",
    "- 风格宪法(前 200 字):无风格宪法文件\n",
    "- 结尾埋伏笔要求:S-001；P-001；无\n",
    "\n",
    "## 五、收在哪里\n",
    "- 结尾要求:S-001；P-001；无\n",
    "- 上章结尾复述:尚无上章(本书开篇)\n",
    "\n",
);

#[test]
fn scenario_no_style_constitution() {
    scenario("no_style", 1, |root| {
        fs::remove_file(root.join("文风").join("风格宪法.md")).unwrap();
    }, NO_STYLE);
}

/// 风格宪法 >200 字 → 截前 200 字。
fn style_long_oracle() -> String {
    let mut s = concat!(
        "# 第1章 写作任务书(样板书)\n",
        "\n",
        "## 一、开篇委托\n",
        "- 书名:样板书\n",
        "- 章号:1\n",
        "- 标题:开局\n",
        "- 一句话目标:阿米与米粒重逢,火光预示\n",
        "\n",
        "## 二、这章的故事\n",
        "- 前文摘要:暂无前文摘要\n",
        "- 目标:阿米与米粒重逢,火光预示\n",
        "- 节点(细纲·要发生什么):阿米与米粒重逢,火光预示\n",
        "- 必覆盖:不写赵三\n",
        "- 禁区:赵三\n",
        "- 检索线索(实体卡):\n",
        "  - 【阿米】## 身份\n",
        "主角,沉默寡言。\n",
        "## 现状\n",
        "第一章与米粒重逢。\n",
        "\n",
        "  - 【米粒】## 身份\n",
        "灯下之人,写作者。\n",
        "## 现状\n",
        "第二章失联。\n",
        "\n",
        "\n",
        "## 三、这章的人物\n",
        "- 阿米: ## 身份\n",
        "主角,沉默寡言。\n",
        "## 现状\n",
        "第一章与米粒重逢。\n",
        "\n",
        "- 米粒: ## 身份\n",
        "灯下之人,写作者。\n",
        "## 现状\n",
        "第二章失联。\n",
        "\n",
        "\n",
        "## 四、怎么写更顺\n",
        "- 风格宪法(前 200 字):# 风格宪法\n",
        "\n",
    )
    .to_owned();
    // 风格宪法头 200 码点 = "# 风格宪法\n\n"（8）+ 风×192；其后直接接下一行（无空行）
    s.push_str(&"风".repeat(192));
    s.push_str("\n- 结尾埋伏笔要求:S-001；P-001；无\n\n## 五、收在哪里\n");
    s.push_str("- 结尾要求:S-001；P-001；无\n- 上章结尾复述:尚无上章(本书开篇)\n\n");
    s
}

#[test]
fn scenario_style_truncated_at_200() {
    scenario("style_long", 1, |root| {
        write(root, "文风/风格宪法.md", &format!("# 风格宪法\n\n{}\n", "风".repeat(250)));
    }, &style_long_oracle());
}

/// 无细纲/无正文/无摘要/空状态 → 占位标题 第3章 + 无卡片 + 必覆盖/禁区 无。
const MINIMAL: &str = concat!(
    "# 第3章 写作任务书(样板书)\n",
    "\n",
    "## 一、开篇委托\n",
    "- 书名:样板书\n",
    "- 章号:3\n",
    "- 标题:第3章\n",
    "- 一句话目标:(无细纲,依 大纲/总纲.md 一句话)\n",
    "\n",
    "## 二、这章的故事\n",
    "- 前文摘要:暂无前文摘要\n",
    "- 目标:(无细纲,依 大纲/总纲.md 一句话)\n",
    "- 节点(细纲·要发生什么):(无细纲,依 大纲/总纲.md 一句话)\n",
    "- 必覆盖:无\n",
    "- 禁区:无\n",
    "\n",
    "## 三、这章的人物\n",
    "- 无(未登记角色)\n",
    "\n",
    "## 四、怎么写更顺\n",
    "- 风格宪法(前 200 字):# 风格宪法\n",
    "\n",
    "短句,克制,白描。\n",
    "\n",
    "- 结尾埋伏笔要求:无明确要求\n",
    "\n",
    "## 五、收在哪里\n",
    "- 结尾要求:无明确要求\n",
    "- 上章结尾复述:尚无上章(本书开篇)\n",
    "\n",
);

#[test]
fn scenario_minimal_book_placeholder_title() {
    scenario("minimal", 3, |root| {
        fs::remove_file(root.join("大纲").join("细纲蓝图.md")).unwrap();
        for f in fs::read_dir(root.join("定稿").join("正文")).unwrap() {
            fs::remove_file(f.unwrap().path()).unwrap();
        }
        for f in fs::read_dir(root.join("定稿").join("记忆").join("章摘要")).unwrap() {
            fs::remove_file(f.unwrap().path()).unwrap();
        }
        write(root, "追踪/_tracking-state.json", r#"{"entity_states": {}}"#);
    }, MINIMAL);
}

/// 角色卡含 状态/驱动/作用/说话 关键词行 → 提炼（前 6 行）；无关键词卡 → 原文前 400。
fn distill_oracle() -> String {
    concat!(
        "# 第1章 写作任务书(样板书)\n",
        "\n",
        "## 一、开篇委托\n",
        "- 书名:样板书\n",
        "- 章号:1\n",
        "- 标题:开局\n",
        "- 一句话目标:阿米与米粒重逢,火光预示\n",
        "\n",
        "## 二、这章的故事\n",
        "- 前文摘要:暂无前文摘要\n",
        "- 目标:阿米与米粒重逢,火光预示\n",
        "- 节点(细纲·要发生什么):阿米与米粒重逢,火光预示\n",
        "- 必覆盖:不写赵三\n",
        "- 禁区:赵三\n",
        "- 检索线索(实体卡):\n",
        "  - 【阿米】## 状态\n",
        "静默。\n",
        "## 驱动\n",
        "寻人。\n",
        "## 作用\n",
        "线索担当。\n",
        "## 说话\n",
        "寡言。\n",
        "## 其他\n",
        "无。\n",
        "\n",
        "  - 【米粒】## 身份\n",
        "灯下之人。\n",
        "## 现状\n",
        "失联中。\n",
        "\n",
        "\n",
        "## 三、这章的人物\n",
        "- 阿米: ## 状态；## 驱动；## 作用；## 说话\n",
        "- 米粒: ## 身份\n",
        "灯下之人。\n",
        "## 现状\n",
        "失联中。\n",
        "\n",
        "\n",
        "## 四、怎么写更顺\n",
        "- 风格宪法(前 200 字):# 风格宪法\n",
        "\n",
        "短句,克制,白描。\n",
        "\n",
        "- 结尾埋伏笔要求:S-001；P-001；无\n",
        "\n",
        "## 五、收在哪里\n",
        "- 结尾要求:S-001；P-001；无\n",
        "- 上章结尾复述:尚无上章(本书开篇)\n",
        "\n",
    )
    .to_owned()
}

#[test]
fn scenario_distill_keyword_lines() {
    scenario("distill", 1, |root| {
        write(
            root,
            "定稿/设定/角色/阿米.md",
            "## 状态\n静默。\n## 驱动\n寻人。\n## 作用\n线索担当。\n## 说话\n寡言。\n## 其他\n无。\n",
        );
        write(root, "定稿/设定/角色/米粒.md", "## 身份\n灯下之人。\n## 现状\n失联中。\n");
    }, &distill_oracle());
}

/// 显式传入 contract/context：不复用内部装配。
const EXPLICIT: &str = concat!(
    "# 第1章 写作任务书(样板书)\n",
    "\n",
    "## 一、开篇委托\n",
    "- 书名:样板书\n",
    "- 章号:1\n",
    "- 标题:开局\n",
    "- 一句话目标:自定义目标\n",
    "\n",
    "## 二、这章的故事\n",
    "- 前文摘要:摘要链B\n",
    "- 目标:自定义目标\n",
    "- 节点(细纲·要发生什么):自定义目标\n",
    "- 必覆盖:硬甲\n",
    "- 禁区:禁乙\n",
    "- 检索线索(实体卡):\n",
    "  - 【客串】自定义内容\n",
    "\n",
    "## 三、这章的人物\n",
    "- 客串: 自定义内容\n",
    "\n",
    "## 四、怎么写更顺\n",
    "- 风格宪法(前 200 字):# 风格宪法\n",
    "\n",
    "短句,克制,白描。\n",
    "\n",
    "- 结尾埋伏笔要求:自定义结尾\n",
    "\n",
    "## 五、收在哪里\n",
    "- 结尾要求:自定义结尾\n",
    "- 上章结尾复述(供衔接):上章结尾A\n",
    "\n",
);

#[test]
fn scenario_explicit_contract_and_context() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    copy_dir_all(&corpus("007-brief-1").join("book"), &root);
    let state = load_state(&root).unwrap();
    let c = Contract {
        goal: "自定义目标".to_owned(),
        hard_constraints: vec!["硬甲".to_owned()],
        forbidden: vec!["禁乙".to_owned()],
        foreshadow_todo: Vec::new(),
        ending_requirement: "自定义结尾".to_owned(),
        sources: Vec::new(),
    };
    let ctx = Context {
        recent_endings: vec!["上章结尾A".to_owned()],
        summary_chain: "摘要链B".to_owned(),
        entity_cards: vec!["【客串】自定义内容".to_owned()],
        next_chapter_injection: String::new(),
        budget_kb: 0.0,
    };
    let brief = build_brief(&root, &state, 1, Some(&c), Some(&ctx)).unwrap();
    assert_eq!(brief + "\n", EXPLICIT);
}

/// book.yaml 显式 `name:`（空值）→ 空书名（引擎 DEFAULTS 仅缺键兜底为 未命名）。
const EMPTY_NAME: &str = concat!(
    "# 第1章 写作任务书()\n",
    "\n",
    "## 一、开篇委托\n",
    "- 书名:\n",
    "- 章号:1\n",
    "- 标题:开局\n",
    "- 一句话目标:阿米与米粒重逢,火光预示\n",
    "\n",
    "## 二、这章的故事\n",
    "- 前文摘要:暂无前文摘要\n",
    "- 目标:阿米与米粒重逢,火光预示\n",
    "- 节点(细纲·要发生什么):阿米与米粒重逢,火光预示\n",
    "- 必覆盖:不写赵三\n",
    "- 禁区:赵三\n",
    "- 检索线索(实体卡):\n",
    "  - 【阿米】## 身份\n",
    "主角,沉默寡言。\n",
    "## 现状\n",
    "第一章与米粒重逢。\n",
    "\n",
    "  - 【米粒】## 身份\n",
    "灯下之人,写作者。\n",
    "## 现状\n",
    "第二章失联。\n",
    "\n",
    "\n",
    "## 三、这章的人物\n",
    "- 阿米: ## 身份\n",
    "主角,沉默寡言。\n",
    "## 现状\n",
    "第一章与米粒重逢。\n",
    "\n",
    "- 米粒: ## 身份\n",
    "灯下之人,写作者。\n",
    "## 现状\n",
    "第二章失联。\n",
    "\n",
    "\n",
    "## 四、怎么写更顺\n",
    "- 风格宪法(前 200 字):# 风格宪法\n",
    "\n",
    "短句,克制,白描。\n",
    "\n",
    "- 结尾埋伏笔要求:S-001；P-001；无\n",
    "\n",
    "## 五、收在哪里\n",
    "- 结尾要求:S-001；P-001；无\n",
    "- 上章结尾复述:尚无上章(本书开篇)\n",
    "\n",
);

#[test]
fn scenario_empty_book_name_kept() {
    scenario("empty_name", 1, |root| {
        write(root, "book.yaml", "name:\nstatus: writing\n");
    }, EMPTY_NAME);
}
