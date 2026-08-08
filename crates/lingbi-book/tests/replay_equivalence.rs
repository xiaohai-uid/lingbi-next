//! P2-15 全 22 步 corpus 回放（P2 最终验收门禁，issue #19）。
//!
//! 从 000-initial 快照起按 MANIFEST 顺序执行全部 22 步命令（库层 cmd 实现，
//! 零 Python/WSL 依赖），每步断言：退出码、stdout、结果树（文件集合 + 内容）
//! 与 corpus 快照一致。测试根命名 sr-oracle-live 以匹配状态 book 字段与路径。
//!
//! 规范化例外（时间/路径依赖数据，逐项注明）：
//! - contract 的 sources 与 batch 计划内的绝对路径：按「引擎根 → 临时根」
//!   转义替换（引擎 CLI 行为即内嵌绝对路径，非移植偏差）。
//! - review 报告路径（011）与 batch 计划文件名（021）：路径归一 + 时间戳
//!   文件名归一（批次-\d{8}-\d{6}.json → 批次-TIMESTAMP.json）。
//! - batch 计划 created 字段（引擎 datetime.now()）：JSON 解析后置空比较。
//!
//! 平台注：corpus 由 Windows 引擎生成（stdout 内嵌反斜杠路径、引擎根
//! C:\Users\a1691\AppData\Local\Temp\sr-oracle-live 硬编码于本测试）——门禁面向
//! Windows 目标（仓库既定目标，同 checks_equivalence 平台注）。

use lingbi_book::cmd::{
    cmd_batch_create, cmd_brief, cmd_check, cmd_commit, cmd_context, cmd_contract, cmd_doctor,
    cmd_learn, cmd_report, cmd_resume, cmd_review, cmd_revision, cmd_stats, cmd_status, CmdOut,
};
use lingbi_contracts::{AppError, ErrorCode};
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};

fn corpus(step: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/storyrepo-oracle/153eb63/steps")
        .join(step)
}

fn manifest() -> serde_json::Value {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/storyrepo-oracle/153eb63/MANIFEST.json");
    serde_json::from_str(&fs::read_to_string(p).unwrap()).unwrap()
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

fn escaped(path: &Path) -> String {
    path.display().to_string().replace('\\', "\\\\")
}

/// stdout 归一：引擎根 → 临时根（转义与原始两形式）+ 批次时间戳文件名。
fn normalize_stdout(text: &str, corpus_root: &Path, our_root: &Path) -> String {
    let t = text
        .replace(&escaped(corpus_root), &escaped(our_root))
        .replace(
            &corpus_root.display().to_string(),
            &our_root.display().to_string(),
        );
    let re = Regex::new(r"批次-\d{8}-\d{6}\.json").expect("plan name regex");
    re.replace_all(&t, "批次-TIMESTAMP.json").into_owned()
}

/// JSON 值内路径归一（batch 计划字符串字段）。
fn normalize_value_paths(v: &mut serde_json::Value, corpus_root: &Path, our_root: &Path) {
    match v {
        serde_json::Value::String(s) => {
            *s = s
                .replace(&escaped(corpus_root), &escaped(our_root))
                .replace(
                    &corpus_root.display().to_string(),
                    &our_root.display().to_string(),
                );
        }
        serde_json::Value::Array(a) => {
            for item in a {
                normalize_value_paths(item, corpus_root, our_root);
            }
        }
        serde_json::Value::Object(m) => {
            for item in m.values_mut() {
                normalize_value_paths(item, corpus_root, our_root);
            }
        }
        _ => {}
    }
}

fn collect_rels(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in fs::read_dir(&d).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                stack.push(entry.path());
            } else {
                out.push(
                    entry
                        .path()
                        .strip_prefix(dir)
                        .unwrap()
                        .display()
                        .to_string()
                        .replace('\\', "/"),
                );
            }
        }
    }
    out
}

/// 结果树对照：文件集合对等（批次文件名时间戳归一）+ 内容逐字节；
/// 批次计划文件按 JSON 解析（created 置空 + 路径归一）比较。
fn assert_book_tree_equal(root: &Path, step_id: &str, corpus_root: &Path) {
    let want_dir = corpus(step_id).join("book");
    let re = Regex::new(r"批次-\d{8}-\d{6}\.json").expect("plan name regex");
    // (归一化 rel, 实际路径) 配对——批次计划文件名时间戳不同
    let norm = |rel: String| re.replace_all(&rel, "批次-TIMESTAMP.json").into_owned();
    let mut got_files: Vec<(String, PathBuf)> = collect_rels(root)
        .into_iter()
        .map(|r| (norm(r.clone()), root.join(&r)))
        .collect();
    let mut want_files: Vec<(String, PathBuf)> = collect_rels(&want_dir)
        .into_iter()
        .map(|r| (norm(r.clone()), want_dir.join(&r)))
        .collect();
    got_files.sort_by(|a, b| a.0.cmp(&b.0));
    want_files.sort_by(|a, b| a.0.cmp(&b.0));
    let got_rels: Vec<&str> = got_files.iter().map(|(r, _)| r.as_str()).collect();
    let want_rels: Vec<&str> = want_files.iter().map(|(r, _)| r.as_str()).collect();
    assert_eq!(got_rels, want_rels, "{step_id} 结果树文件集合不一致");
    for (rel, got_path) in &got_files {
        let got = fs::read(got_path).unwrap();
        let want_path = want_files
            .iter()
            .find(|(r, _)| r == rel)
            .map(|(_, p)| p)
            .expect("want 侧对应文件");
        if rel.contains("批次-") && rel.ends_with(".json") {
            let mut g: serde_json::Value = serde_json::from_slice(&got).unwrap();
            let mut w: serde_json::Value = serde_json::from_slice(&fs::read(want_path).unwrap()).unwrap();
            g["created"] = serde_json::Value::String(String::new());
            w["created"] = serde_json::Value::String(String::new());
            normalize_value_paths(&mut g, corpus_root, root);
            normalize_value_paths(&mut w, corpus_root, root);
            assert_eq!(g, w, "{step_id} 批次计划内容不一致(created 除外)");
        } else {
            let want = fs::read(want_path).unwrap();
            assert_eq!(got, want, "{step_id} 结果树 {rel} 不一致");
        }
    }
}

/// 按 MANIFEST 命令分派（argv 与引擎 argparse 位置一致）。
fn run_cmd(root: &Path, cmd: &[&str]) -> Result<CmdOut, AppError> {
    match cmd[0] {
        "doctor" => cmd_doctor(root, cmd[2], cmd.get(4).and_then(|c| c.parse().ok())),
        "contract" => cmd_contract(root, cmd[2].parse().unwrap()),
        "context" => cmd_context(root, cmd[2].parse().unwrap()),
        "brief" => cmd_brief(root, cmd[2].parse().unwrap()),
        "review" => {
            let ch: i64 = cmd[2].parse().unwrap();
            let json_path = root.join(cmd[4].replace('\\', "/"));
            cmd_review(root, ch, &json_path)
        }
        "commit" => cmd_commit(root, cmd[2].parse().unwrap(), false),
        "check" => cmd_check(root),
        "status" => cmd_status(root),
        "stats" => cmd_stats(root),
        "resume" => cmd_resume(root, cmd.get(2).and_then(|c| c.parse().ok())),
        "revision" => cmd_revision(root, cmd[2].parse().unwrap(), None),
        "learn" => cmd_learn(root, cmd[2].parse().unwrap()),
        "batch" => {
            let chapters = lingbi_book::cmd::parse_chapters(cmd[3])
                .map_err(|e| AppError::new(ErrorCode::BookCorrupted, e, false))?;
            cmd_batch_create(root, &chapters)
        }
        "report" => cmd_report(root, cmd[2], cmd.get(4).and_then(|c| c.parse().ok())),
        other => panic!("未知命令: {other}"),
    }
}

#[test]
fn replay_all_22_steps_match_corpus() {
    let manifest = manifest();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("sr-oracle-live");
    copy_dir_all(&corpus("000-initial").join("book"), &root);
    let corpus_root = Path::new(r"C:\Users\a1691\AppData\Local\Temp\sr-oracle-live");
    let steps = manifest["steps"].as_array().expect("steps");
    for step in steps {
        let id = step["id"].as_str().unwrap();
        let cmds: Vec<&str> = step["cmd"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.as_str().unwrap())
            .collect();
        let exit_want = step["exit"].as_i64().unwrap() as i32;
        let out = run_cmd(&root, &cmds).unwrap_or_else(|e| panic!("{id} 执行失败: {e}"));
        assert_eq!(out.exit, exit_want, "{id} 退出码不一致");
        let want = fs::read_to_string(corpus(id).join("stdout.txt")).unwrap();
        assert_eq!(
            normalize_stdout(&out.stdout, corpus_root, &root),
            normalize_stdout(&want, corpus_root, &root),
            "{id} stdout 不一致"
        );
        assert_book_tree_equal(&root, id, corpus_root);
    }
}
