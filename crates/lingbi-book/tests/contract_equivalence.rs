//! P2-07 contract 等价测试：与引擎 contract.py 的 build_contract 输出逐字节一致
//! （json.dumps ensure_ascii=False indent=1；CLI print 补尾换行）。
//!
//! 两层基准：
//! 1. corpus 快照 003-contract-1 / 004-contract-2（contract --chapter 1/2）。corpus
//!    stdout 内嵌引擎运行根路径（sr-oracle-live），测试把书拷贝到临时根后做
//!    「引擎根 → 临时根」转义替换再比较——sources 中的绝对根路径是唯一差异
//!    （引擎 CLI 行为即内嵌绝对路径，非移植偏差）。
//! 2. 9 个边界场景：引擎实跑 oracle（根路径以 @ROOT@ 占位）。生成脚本
//!    /tmp/lingbi-contract-oracle/gen_contract_oracle.py——改测试场景必须先重跑引擎
//!    重新生成 oracle，不得手写期望值。

use lingbi_book::contract::{build_contract, split_list};
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

/// JSON 内转义后的路径（引擎 json.dumps 把反斜杠转义为 \\）。
fn escaped(path: &Path) -> String {
    path.display().to_string().replace('\\', "\\\\")
}

/// corpus stdout 的引擎运行根（以转义形式出现于 sources）。
const ENGINE_ROOT: &str = r"C:\Users\a1691\AppData\Local\Temp\sr-oracle-live";

fn contract_json(root: &Path, ch: i64) -> String {
    let state = load_state(root).expect("load_state");
    build_contract(root, &state, ch).to_json()
}

/// 期望 stdout（oracle 模板补 print 尾换行）。
fn expected_from_template(template: &str, root: &Path) -> String {
    template.replace("@ROOT@", &escaped(root)) + "\n"
}

fn assert_contract(root: &Path, ch: i64, expected: &str, what: &str) {
    let actual = contract_json(root, ch) + "\n";
    assert_eq!(actual, expected, "{what} 输出与引擎不一致");
}

// ── 1) corpus 003-contract-1 / 004-contract-2 ──

#[test]
fn corpus_contract_1_and_2_stdout_match() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    copy_dir_all(&corpus("003-contract-1").join("book"), &root);
    for (step, ch) in [("003-contract-1", 1), ("004-contract-2", 2)] {
        let want = fs::read_to_string(corpus(step).join("stdout.txt")).unwrap();
        let want = want
            .trim_end_matches('\n')
            .replace(&escaped(Path::new(ENGINE_ROOT)), &escaped(&root))
            + "\n";
        assert_contract(&root, ch, &want, &format!("corpus {step}"));
    }
}

// ── 2) 场景基建（与 oracle 生成脚本逐项对齐） ──

const OUTLINE_TABLE: &str = "| ch | 标题 | 要发生什么 | 硬约束 | 禁区 | 角色 | suspense | foreshadow | twist |\n\
|----|------|-----------|--------|------|------|----------|------------|-------|\n\
| 1 | 开局 | 阿米与米粒重逢,火光预示 | 不写赵三 | 赵三 | 阿米,米粒 | 火光 | P-001 | 无 |\n\
| 2 | 夜访 | 阿米夜访,米粒失联 | 不暴露 S-001 | 赵三 | 阿米,米粒 | S-001 | P-001 | 无 |\n";

/// 基础追踪状态（S-001 未曝光、S-002 已曝光；P-002 带 note）——与 oracle 脚本 state() 同构。
const STATE_TEMPLATE: &str = r#"{"spec":1,"book":"样板书","imported_through_chapter":2,"state_revision":3,"chapters":{"1":{"status":"committed","title":"开局","words":"1200"},"2":{"status":"committed","title":"夜访","words":"1500"}},"promises":{PROMISES_BODY},"info_gaps":{"S-001":{"revealed":false,"plant_ch":1,"reveal_ch":null,"note":""},"S-002":{"revealed":true,"plant_ch":1,"reveal_ch":2,"note":""}},"timeline":[{"ch":1,"event":"开局","kind":"fact"}],"entity_states":{"阿米":{"first_ch":1,"where":"","state":""}},"reader_knowledge":{"reveals":[]}}"#;

const DEFAULT_PROMISES: &str = r#""P-001":{"status":"open","open_ch":1,"adv_ch":1,"done_ch":null,"note":""},"P-002":{"status":"open","open_ch":2,"adv_ch":2,"done_ch":null,"note":"留给读者"}"#;

/// foreshadow_adv 场景：adv 落后 ≥3 章、note 优先、done 排除、limit 5。
const ADV_PROMISES: &str = r#""P-001":{"status":"open","open_ch":1,"adv_ch":1,"done_ch":null,"note":""},"P-002":{"status":"open","open_ch":1,"adv_ch":1,"done_ch":null,"note":""},"P-003":{"status":"done","open_ch":1,"adv_ch":1,"done_ch":2,"note":""},"P-004":{"status":"open","open_ch":2,"adv_ch":2,"done_ch":null,"note":"伏笔四"},"P-005":{"status":"open","open_ch":1,"adv_ch":1,"done_ch":null,"note":""},"P-006":{"status":"open","open_ch":1,"adv_ch":1,"done_ch":null,"note":""},"P-007":{"status":"open","open_ch":1,"adv_ch":1,"done_ch":null,"note":""}"#;

fn state_json(promises: &str) -> String {
    STATE_TEMPLATE.replace("PROMISES_BODY", promises)
}

fn scenario(name: &str, ch: i64, files: &[(&str, &str)], oracle: &str) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    for (rel, content) in files {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, content).unwrap();
    }
    let expected = expected_from_template(oracle, &root);
    assert_contract(&root, ch, &expected, name);
}

const NO_OUTLINE: &str = r#"{
 "goal": "(无细纲,依 大纲/总纲.md 一句话)",
 "hard_constraints": [],
 "forbidden": [
  "S-001"
 ],
 "foreshadow_todo": [
  "P-002"
 ],
 "ending_requirement": "",
 "sources": {
  "追踪状态": "@ROOT@\\追踪\\_tracking-state.json",
  "总纲": "@ROOT@\\大纲\\总纲.md"
 }
}"#;

#[test]
fn scenario_no_outline() {
    scenario(
        "no_outline",
        1,
        &[("追踪/_tracking-state.json", &state_json(DEFAULT_PROMISES))],
        NO_OUTLINE,
    );
}

const MISSING_ROW: &str = r#"{
 "goal": "(细纲蓝图无第9章行)",
 "hard_constraints": [],
 "forbidden": [
  "S-001"
 ],
 "foreshadow_todo": [
  "P-002",
  "P-001"
 ],
 "ending_requirement": "",
 "sources": {
  "追踪状态": "@ROOT@\\追踪\\_tracking-state.json",
  "细纲蓝图": "@ROOT@\\大纲\\细纲蓝图.md"
 }
}"#;

#[test]
fn scenario_missing_row() {
    scenario(
        "missing_row",
        9,
        &[
            ("大纲/细纲蓝图.md", OUTLINE_TABLE),
            ("追踪/_tracking-state.json", &state_json(DEFAULT_PROMISES)),
        ],
        MISSING_ROW,
    );
}

const FORESHADOW_ADV: &str = r#"{
 "goal": "(细纲蓝图无第5章行)",
 "hard_constraints": [],
 "forbidden": [
  "S-001"
 ],
 "foreshadow_todo": [
  "P-004",
  "P-001",
  "P-002",
  "P-005",
  "P-006"
 ],
 "ending_requirement": "",
 "sources": {
  "追踪状态": "@ROOT@\\追踪\\_tracking-state.json",
  "细纲蓝图": "@ROOT@\\大纲\\细纲蓝图.md"
 }
}"#;

#[test]
fn scenario_foreshadow_todo_ordering_and_limit() {
    scenario(
        "foreshadow_adv",
        5,
        &[
            ("大纲/细纲蓝图.md", OUTLINE_TABLE),
            ("追踪/_tracking-state.json", &state_json(ADV_PROMISES)),
        ],
        FORESHADOW_ADV,
    );
}

const SPLIT_LIST_ORACLE: &str = r#"{
 "goal": "重逢",
 "hard_constraints": [
  "不写赵三",
  "不写张三",
  "不写李四"
 ],
 "forbidden": [
  "S-001",
  "赵三",
  "赵四",
  "王五"
 ],
 "foreshadow_todo": [
  "P-002"
 ],
 "ending_requirement": "",
 "sources": {
  "追踪状态": "@ROOT@\\追踪\\_tracking-state.json",
  "细纲蓝图": "@ROOT@\\大纲\\细纲蓝图.md"
 }
}"#;

#[test]
fn scenario_split_list_mixed_separators() {
    scenario(
        "split_list",
        1,
        &[
            (
                "大纲/细纲蓝图.md",
                "| ch | 标题 | 要发生什么 | 硬约束 | 禁区 | 角色 | suspense | foreshadow | twist |\n\
                 |----|------|-----------|--------|------|------|----------|------------|-------|\n\
                 | 1 | 开局 | 重逢 | 不写赵三,不写张三;不写李四 | 赵三、赵四；王五 | 阿米 | 火光 | P-001 | 无 |\n",
            ),
            ("追踪/_tracking-state.json", &state_json(DEFAULT_PROMISES)),
        ],
        SPLIT_LIST_ORACLE,
    );
}

const POSITIONAL_ROWS_ORACLE: &str = r#"{
 "goal": "阿米与米粒重逢",
 "hard_constraints": [
  "无"
 ],
 "forbidden": [
  "S-001",
  "赵三"
 ],
 "foreshadow_todo": [
  "P-002"
 ],
 "ending_requirement": "S-001；P-001；无",
 "sources": {
  "追踪状态": "@ROOT@\\追踪\\_tracking-state.json",
  "细纲蓝图": "@ROOT@\\大纲\\细纲蓝图.md"
 }
}"#;

#[test]
fn scenario_positional_rows_no_header() {
    scenario(
        "positional_rows",
        1,
        &[
            (
                "大纲/细纲蓝图.md",
                "1|开局|阿米与米粒重逢|无|赵三|阿米,米粒|火光|P-001|无\n\
                 2|夜访|阿米夜访|无|赵三|阿米,米粒|S-001|P-001|无\n",
            ),
            ("追踪/_tracking-state.json", &state_json(DEFAULT_PROMISES)),
        ],
        POSITIONAL_ROWS_ORACLE,
    );
}

/// 未识别表头（章序 不在表头提示内）→ 表头行被跳过，数据行按简易行解析兜底
/// （列 2..9 顺次映射 title/goal/hard/forbidden/roles/suspense/foreshadow/twist）。
const UNRECOGNIZED_HEADER_ORACLE: &str = r#"{
 "goal": "重逢",
 "hard_constraints": [
  "不写赵三"
 ],
 "forbidden": [
  "S-001",
  "赵三"
 ],
 "foreshadow_todo": [
  "P-002"
 ],
 "ending_requirement": "S-001；无；无",
 "sources": {
  "追踪状态": "@ROOT@\\追踪\\_tracking-state.json",
  "细纲蓝图": "@ROOT@\\大纲\\细纲蓝图.md"
 }
}"#;

#[test]
fn scenario_unrecognized_header_falls_back_to_positional() {
    scenario(
        "header_variants",
        1,
        &[
            (
                "大纲/细纲蓝图.md",
                "| 章序 | 章名 | 剧情目标 | 硬性约束 | 禁止词 | 主要人物 | 悬念点 | 伏笔点 | 反转点 |\n\
                 |------|------|----------|----------|--------|----------|--------|--------|--------|\n\
                 | 1 | 开局 | 重逢 | 不写赵三 | 赵三 | 阿米,米粒 | 火光 | P-001 | 无 |\n\
                 | 2 | 夜访 | 夜访 | 无 | 无 | 阿米,米粒 | S-001 | 无 | 无 |\n",
            ),
            ("追踪/_tracking-state.json", &state_json(DEFAULT_PROMISES)),
        ],
        UNRECOGNIZED_HEADER_ORACLE,
    );
}

const FEWER_CELLS_ORACLE: &str = r#"{
 "goal": "",
 "hard_constraints": [],
 "forbidden": [
  "S-001"
 ],
 "foreshadow_todo": [
  "P-002"
 ],
 "ending_requirement": "S-001；P-001；无",
 "sources": {
  "追踪状态": "@ROOT@\\追踪\\_tracking-state.json",
  "细纲蓝图": "@ROOT@\\大纲\\细纲蓝图.md"
 }
}"#;

#[test]
fn scenario_row_shorter_than_header() {
    scenario(
        "fewer_cells",
        1,
        &[
            (
                "大纲/细纲蓝图.md",
                "| ch | 标题 | 要发生什么 | 硬约束 | 禁区 | 角色 | suspense | foreshadow | twist |\n\
                 |----|------|-----------|--------|------|------|----------|------------|-------|\n\
                 | 1 | 开局 |\n\
                 | 2 | 夜访 | 阿米夜访 | 不写赵三 | 赵三 | 阿米 | S-001 | P-001 | 无 |\n",
            ),
            ("追踪/_tracking-state.json", &state_json(DEFAULT_PROMISES)),
        ],
        FEWER_CELLS_ORACLE,
    );
}

/// 章号列内嵌数字（"第3章"→3）；非数字章号行跳过（"无" 行不产出）。
const TO_INT_CH_ORACLE: &str = r#"{
 "goal": "惊变",
 "hard_constraints": [
  "None"
 ],
 "forbidden": [
  "S-001",
  "None"
 ],
 "foreshadow_todo": [
  "P-002"
 ],
 "ending_requirement": "",
 "sources": {
  "追踪状态": "@ROOT@\\追踪\\_tracking-state.json",
  "细纲蓝图": "@ROOT@\\大纲\\细纲蓝图.md"
 }
}"#;

#[test]
fn scenario_chapter_cell_with_embedded_number() {
    scenario(
        "to_int_ch",
        3,
        &[
            (
                "大纲/细纲蓝图.md",
                "| ch | 要发生什么 |\n| 第3章 | 惊变 |\n| 无 | 跳过 |\n",
            ),
            ("追踪/_tracking-state.json", &state_json(DEFAULT_PROMISES)),
        ],
        TO_INT_CH_ORACLE,
    );
}

#[test]
fn scenario_empty_outline_file_counts_as_no_outline() {
    scenario(
        "empty_outline",
        1,
        &[
            ("大纲/细纲蓝图.md", ""),
            ("追踪/_tracking-state.json", &state_json(DEFAULT_PROMISES)),
        ],
        NO_OUTLINE,
    );
}

// ── 3) 纯函数单元测试 ──

#[test]
fn split_list_matches_engine_regex() {
    assert_eq!(
        split_list("不写赵三,不写张三;不写李四"),
        ["不写赵三", "不写张三", "不写李四"]
    );
    assert_eq!(
        split_list("赵三、赵四；王五"),
        ["赵三", "赵四", "王五"]
    );
    assert!(split_list("").is_empty());
    assert!(split_list("  ").is_empty());
}

#[test]
fn contract_json_has_no_trailing_newline() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    copy_dir_all(&corpus("003-contract-1").join("book"), &root);
    let state = load_state(&root).unwrap();
    let json = build_contract(&root, &state, 1).to_json();
    assert!(!json.ends_with('\n'));
    assert!(json.starts_with("{\n \"goal\""));
}
