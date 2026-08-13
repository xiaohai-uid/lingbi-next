//! P2-08 context 等价测试：与引擎 context.py 的 assemble_context + cli.cmd_context
//! 输出逐字节一致。
//!
//! 两层基准：
//! 1. corpus 快照 005-context-1 / 006-context-2（同一 book，context --chapter 1/2）。
//!    context stdout 不含绝对路径，直接字节比较。
//! 2. 5 个边界场景：引擎实跑 oracle（生成脚本 /tmp/lingbi-context-oracle/
//!    gen_context_oracle.py——改测试场景必须先重跑引擎）。仅 missing_card 含绝对
//!    根路径（角色卡缺失占位），以 @ROOT@ 占位、测试替换回临时根。

use lingbi_book::context::{assemble_context, budget_check, context_stdout, Context};
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

fn context_stdout_for(root: &Path, ch: i64) -> String {
    let state = load_state(root).expect("load_state");
    context_stdout(&assemble_context(root, &state, ch))
}

// ── 1) corpus 005-context-1 / 006-context-2 ──

#[test]
fn corpus_context_1_and_2_stdout_match() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    copy_dir_all(&corpus("005-context-1").join("book"), &root);
    for (step, ch) in [("005-context-1", 1), ("006-context-2", 2)] {
        let want = fs::read_to_string(corpus(step).join("stdout.txt")).unwrap();
        assert_eq!(context_stdout_for(&root, ch), want, "corpus {step}");
    }
}

// ── 2) 场景基建（与 oracle 生成脚本逐项对齐） ──

fn scenario(name: &str, ch: i64, modify: impl Fn(&Path), oracle: String) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    copy_dir_all(&corpus("005-context-1").join("book"), &root);
    modify(&root);
    let expected = oracle.replace("@ROOT@", &root.display().to_string());
    assert_eq!(context_stdout_for(&root, ch), expected, "{name}");
}

/// 无细纲：实体卡回退 state.entity_states（corpus 状态为空 → 无卡）；注入为空。
/// 注：行首两空格必须写在字符串字面量内（concat! 不吞行首空白，`\` 续行会吞）。
const NO_OUTLINE: &str = concat!(
    "budget: 0.1KB OK\n",
    "\n",
    "[recent_endings]\n",
    "  远处有火光一闪而逝。\n",
    "\n",
    "[summary_chain]\n",
    "  阿米夜归,与米粒重逢,远山火光一闪而逝。\n",
    "\n",
    "[entity_cards]\n",
    "\n",
    "[next_chapter_injection]\n",
    "  \n",
);

#[test]
fn scenario_no_outline() {
    scenario("no_outline", 2, |root| {
        fs::remove_file(root.join("大纲").join("细纲蓝图.md")).unwrap();
        // P12 #54：细纲蓝图缺失 → 产品回退 章纲.md；「无大纲」语义 = 两文件均无
        fs::remove_file(root.join("大纲").join("章纲.md")).unwrap();
    }, NO_OUTLINE.to_owned());
}

/// 角色卡缺失：占位内嵌绝对根路径（@ROOT@ 替换）。
const MISSING_CARD: &str = concat!(
    "budget: 0.3KB OK\n",
    "\n",
    "[recent_endings]\n",
    "  远处有火光一闪而逝。\n",
    "\n",
    "[summary_chain]\n",
    "  阿米夜归,与米粒重逢,远山火光一闪而逝。\n",
    "\n",
    "[entity_cards]\n",
    "  【阿米】## 身份\n",
    "主角,沉默寡言。\n",
    "## 现状\n",
    "第一章与米粒重逢。\n",
    "\n",
    "  【赵五】(角色卡缺失:@ROOT@\\定稿\\设定\\角色\\赵五.md)\n",
    "\n",
    "[next_chapter_injection]\n",
    "  \n",
);

const OUTLINE_MISSING_CARD: &str = concat!(
    "| ch | 标题 | 要发生什么 | 硬约束 | 禁区 | 角色 | suspense | foreshadow | twist |\n",
    "|----|------|-----------|--------|------|------|----------|------------|-------|\n",
    "| 1 | 开局 | 重逢 | 无 | 无 | 阿米,赵五 | 火光 | P-001 | 无 |\n",
    "| 2 | 夜访 | 夜访 | 无 | 无 | 阿米,赵五 | S-001 | 无 | 无 |\n",
);

#[test]
fn scenario_missing_card_embeds_absolute_path() {
    scenario("missing_card", 2, |root| {
        write(root, "大纲/细纲蓝图.md", OUTLINE_MISSING_CARD);
    }, MISSING_CARD.to_owned());
}

/// 正文末段为 —— 横幅 → 跳过取前段。
const BANNER_ENDING: &str = concat!(
    "budget: 0.3KB OK\n",
    "\n",
    "[recent_endings]\n",
    "  远处有火光一闪而逝。\n",
    "\n",
    "[summary_chain]\n",
    "  阿米夜归,与米粒重逢,远山火光一闪而逝。\n",
    "\n",
    "[entity_cards]\n",
    "  【阿米】## 身份\n",
    "主角,沉默寡言。\n",
    "## 现状\n",
    "第一章与米粒重逢。\n",
    "\n",
    "  【米粒】## 身份\n",
    "灯下之人,写作者。\n",
    "## 现状\n",
    "第二章失联。\n",
    "\n",
    "\n",
    "[next_chapter_injection]\n",
    "  城中惊变,米粒现身；无；P-002；S-001 揭示\n",
);

#[test]
fn scenario_banner_ending_skipped() {
    scenario("banner_ending", 2, |root| {
        let p = root.join("定稿").join("正文").join("001-开局.md");
        let t = fs::read_to_string(&p).unwrap();
        fs::write(p, t.trim_end_matches('\n').to_owned() + "\n\n—— 本章完 ——\n").unwrap();
    }, BANNER_ENDING.to_owned());
}

/// 总字节 1280（280B 结尾 + 1000B 摘要）→ 1.25 → Python 银行家舍入 → 1.2KB
/// （Rust round() 的 half-away 会给 1.3，此场景锁定差异）。
fn budget_125_oracle() -> String {
    format!(
        "budget: 1.2KB OK\n\n[recent_endings]\n  {}\n\n[summary_chain]\n  {}\n\n[entity_cards]\n\n[next_chapter_injection]\n  \n",
        "x".repeat(200),
        "x".repeat(200)
    )
}

#[test]
fn scenario_budget_rounds_half_even_at_display() {
    scenario("budget_125", 2, |root| {
        fs::remove_file(root.join("大纲").join("细纲蓝图.md")).unwrap();
        // P12 #54：细纲蓝图缺失 → 产品回退 章纲.md；「无大纲」语义 = 两文件均无
        fs::remove_file(root.join("大纲").join("章纲.md")).unwrap();
        write(
            root,
            "定稿/正文/001-开局.md",
            &format!("---\ntitle: 开局\nchapter: 001\nvolume: 1\nwords: 280\nstatus: 定稿\n---\n\n{}", "x".repeat(280)),
        );
        write(root, "定稿/记忆/章摘要/001.md", &"x".repeat(1000));
        write(root, "追踪/_tracking-state.json", r#"{"entity_states": {}}"#);
    }, budget_125_oracle());
}

/// 总字节 975（摘要 975B）→ round2=0.95 → "%.1f" dtoa 正确舍入 → 0.9KB
/// （f64(0.95)<0.95；朴素 (x*10.0) 路径会吸到 9.5 → "1.0"，此场景锁定差异）。
fn budget_display_975_oracle() -> String {
    format!(
        "budget: 0.9KB OK\n\n[recent_endings]\n\n[summary_chain]\n  {}\n\n[entity_cards]\n\n[next_chapter_injection]\n  \n",
        "x".repeat(200)
    )
}

#[test]
fn scenario_budget_display_975_rounds_correctly() {
    scenario("budget_display_975", 2, |root| {
        fs::remove_file(root.join("大纲").join("细纲蓝图.md")).unwrap();
        // P12 #54：细纲蓝图缺失 → 产品回退 章纲.md；「无大纲」语义 = 两文件均无
        fs::remove_file(root.join("大纲").join("章纲.md")).unwrap();
        fs::remove_file(root.join("定稿").join("正文").join("001-开局.md")).unwrap();
        write(root, "定稿/记忆/章摘要/001.md", &"x".repeat(975));
        write(root, "追踪/_tracking-state.json", r#"{"entity_states": {}}"#);
    }, budget_display_975_oracle());
}

/// 总字节 9486（800 字结尾 + 1000 字摘要 + 3×400 字卡 + 150 字注入）→ 9.3KB 超限!
fn budget_over_oracle() -> String {
    let mut s = format!("budget: 9.3KB 超限!\n\n[recent_endings]\n  {}\n", "夜".repeat(200));
    s.push_str(&format!("\n[summary_chain]\n  {}\n", "夜".repeat(200)));
    s.push_str("\n[entity_cards]\n");
    for name in ["阿米", "米粒", "赵五"] {
        s.push_str(&format!("  【{name}】{}\n", "夜".repeat(196)));
    }
    s.push_str(&format!("\n[next_chapter_injection]\n  {}\n", "变".repeat(150)));
    s
}

const OUTLINE_OVER_BUDGET: &str = "| ch | 标题 | 要发生什么 | 硬约束 | 禁区 | 角色 | suspense | foreshadow | twist |\n\
|----|------|-----------|--------|------|------|----------|------------|-------|\n\
| 1 | 开局 | 重逢 | 无 | 无 | 阿米,米粒,赵五 | 火光 | P-001 | 无 |\n\
| 2 | 夜访 | 夜访 | 无 | 无 | 阿米,米粒,赵五 | S-001 | 无 | 无 |\n";

#[test]
fn scenario_budget_over_prints_over_limit() {
    scenario("budget_over", 2, |root| {
        write(
            root,
            "定稿/正文/001-开局.md",
            &format!("---\ntitle: 开局\nchapter: 001\nvolume: 1\nwords: 800\nstatus: 定稿\n---\n\n{}", "夜".repeat(800)),
        );
        write(root, "定稿/记忆/章摘要/001.md", &"夜".repeat(1000));
        for n in ["阿米", "米粒", "赵五"] {
            write(root, &format!("定稿/设定/角色/{n}.md"), &"夜".repeat(400));
        }
        let outline = OUTLINE_OVER_BUDGET.to_owned()
            + &format!("| 3 | 惊变 | {} | 无 | 无 | 米粒 | 无 | 无 | 无 |\n", "变".repeat(150));
        write(root, "大纲/细纲蓝图.md", &outline);
    }, budget_over_oracle());
}

// ── 3) 纯函数单元测试 ──

#[test]
fn budget_check_threshold_is_8kb() {
    let base = Context {
        recent_endings: Vec::new(),
        summary_chain: String::new(),
        entity_cards: Vec::new(),
        next_chapter_injection: String::new(),
        budget_kb: 0.0,
    };
    let mut at = base.clone();
    at.budget_kb = 8.0;
    assert!(budget_check(&at));
    let mut over = base;
    over.budget_kb = 8.01;
    assert!(!budget_check(&over));
}
