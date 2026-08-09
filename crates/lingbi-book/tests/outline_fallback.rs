//! P12 #54 大纲读取回退测试（产品缺陷修复验收）：
//! 真实《零界道种》无 大纲/细纲蓝图.md，计划在 大纲/章纲.md——load_outline
//! 细纲蓝图缺失时回退章纲.md（storyrepo 书仓惯例）；细纲蓝图存在时仍优先。
//! 非引擎等价测试（引擎无回退行为，属产品扩展），纯产品语义单元测试。

use lingbi_book::contract::load_outline;
use std::fs;
use std::path::Path;

fn write(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

#[test]
fn outline_falls_back_to_chapter_plan_when_blueprint_missing() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    // 真实《零界道种》形态：无细纲蓝图.md，计划在章纲.md（含范围行）
    write(
        &root.join("大纲/章纲.md"),
        "# 章纲\n\n| 章 | 标题 | 要发生什么 | 状态 |\n|---|---|---|---|\n\
         | 001 | 灰烬有籽 | 下零界取灯芯 | ✅ 定稿 |\n\
         | 002 | 灰里有苗 | 回镇见阿雀 | ✅ 定稿 |\n\
         | 005-010 | (待写) | 周旋肃界卫 | ☐ |\n",
    );
    let rows = load_outline(root);
    let chs: Vec<i64> = rows.iter().map(|r| r.ch).collect();
    assert_eq!(
        chs,
        vec![1, 2, 5],
        "细纲蓝图缺失 → 回退章纲.md（范围行取起始编号）"
    );
    assert_eq!(rows[0].title.as_deref(), Some("灰烬有籽"), "标题列");
    assert_eq!(rows[0].goal.as_deref(), Some("下零界取灯芯"), "要发生什么列");
}

#[test]
fn blueprint_preferred_when_present() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    write(
        &root.join("大纲/章纲.md"),
        "| 章 | 标题 | 要发生什么 |\n|---|---|---|\n| 001 | 章纲标题 | 章纲目标 |\n",
    );
    write(
        &root.join("大纲/细纲蓝图.md"),
        "| 章 | 标题 | 要发生什么 |\n|---|---|---|\n| 001 | 蓝本标题 | 蓝图目标 |\n",
    );
    let rows = load_outline(root);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].title.as_deref(), Some("蓝本标题"), "细纲蓝图优先");
    assert_eq!(rows[0].goal.as_deref(), Some("蓝图目标"));
}

#[test]
fn no_outline_files_returns_empty() {
    let dir = tempfile::TempDir::new().unwrap();
    assert!(load_outline(dir.path()).is_empty(), "两文件均无 → []");
}
