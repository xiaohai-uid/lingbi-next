//! P2-10 revision 等价测试：与引擎 revision.py 的 cascade_revision + cli.cmd_revision
//! 输出逐字节一致（json.dumps indent=1 + print 尾换行）。
//!
//! 两层基准：
//! 1. corpus 快照：在 018-resume-3 的 book 上跑 cascade(ch=1, expected=3)，输出
//!    stdout 与结果状态/视图必须与 019-revision-2 快照逐字节一致。注意 `_replay`
//!    用 new_state(root) 取根 basename 作 book 字段——测试根必须命名为
//!    sr-oracle-live（位于临时目录下，不触碰引擎实跑目录）。
//! 2. 3 个引擎 oracle 场景（生成脚本 /tmp/lingbi-revision-oracle/
//!    gen_revision_oracle.py——改测试场景必须先重跑引擎）。

use lingbi_book::revision::cascade_revision;
use lingbi_book::tracking::{load_state, TrackingState};
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

/// 引擎实跑目录同名根（book 字段 = basename，字节比较前提）。
fn engine_named_root(dir: &tempfile::TempDir) -> PathBuf {
    dir.path().join("sr-oracle-live")
}

fn cascade(root: &Path, ch: i64, expected: i64) -> (String, TrackingState) {
    let mut state = load_state(root).expect("load_state");
    let r = cascade_revision(root, &mut state, ch, expected).expect("cascade_revision");
    (r.to_json() + "\n", state)
}

/// 逐字节比较 追踪/ 目录（状态 + 派生视图）：文件集合对等 + 逐文件内容一致。
fn assert_tracking_tree_equal(root: &Path, corpus_step: &str, what: &str) {
    let want_dir = corpus(corpus_step).join("book").join("追踪");
    let got_dir = root.join("追踪");
    let mut got_files: Vec<PathBuf> = Vec::new();
    collect_files(&got_dir, &mut got_files);
    let mut want_files: Vec<PathBuf> = Vec::new();
    collect_files(&want_dir, &mut want_files);
    let rel = |p: &Path, base: &Path| p.strip_prefix(base).unwrap().to_owned();
    let mut got_rels: Vec<PathBuf> = got_files.iter().map(|p| rel(p, &got_dir)).collect();
    let mut want_rels: Vec<PathBuf> = want_files.iter().map(|p| rel(p, &want_dir)).collect();
    got_rels.sort();
    want_rels.sort();
    assert_eq!(
        got_rels, want_rels,
        "{what}: 追踪/ 文件集合与 corpus {corpus_step} 不一致（漏写/多写文件）"
    );
    for f in &got_files {
        let r = rel(f, &got_dir);
        let got = fs::read(f).unwrap();
        let want_b = fs::read(want_dir.join(&r)).unwrap();
        assert_eq!(got, want_b, "{what}: 追踪/{} 与 corpus {corpus_step} 不一致", r.display());
    }
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            collect_files(&entry.path(), out);
        } else {
            out.push(entry.path());
        }
    }
}

// ── 1) corpus 018 → 019 重放复现 ──

#[test]
fn corpus_revision_replay_matches_019_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let root = engine_named_root(&dir);
    copy_dir_all(&corpus("018-resume-3").join("book"), &root);
    let (stdout, _state) = cascade(&root, 1, 3);
    let want = fs::read_to_string(corpus("019-revision-2").join("stdout.txt")).unwrap();
    assert_eq!(stdout, want, "revision stdout 与 corpus 019 不一致");
    assert_tracking_tree_equal(&root, "019-revision-2", "重放后");
}

// ── 2) 场景（引擎 oracle） ──

const CONFLICT: &str = concat!(
    "{\n",
    " \"ok\": false,\n",
    " \"conflict\": true,\n",
    " \"impact\": [],\n",
    " \"notes\": [\n",
    "  \"期望修订号 2 与当前 3 不符,拒绝级联(请重读最新状态)\"\n",
    " ]\n",
    "}\n",
);

#[test]
fn expected_revision_mismatch_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let root = engine_named_root(&dir);
    copy_dir_all(&corpus("018-resume-3").join("book"), &root);
    let state_before = fs::read(root.join("追踪").join("_tracking-state.json")).unwrap();
    let card_before = fs::read(root.join("追踪").join("上下文.md")).unwrap();
    let (stdout, _state) = cascade(&root, 1, 2);
    assert_eq!(stdout, CONFLICT);
    // 冲突路径不写盘：状态与视图保持 018 原样
    let state_after = fs::read(root.join("追踪").join("_tracking-state.json")).unwrap();
    assert_eq!(state_after, state_before, "冲突被拒后状态不应变化");
    let card_after = fs::read(root.join("追踪").join("上下文.md")).unwrap();
    assert_eq!(card_after, card_before, "冲突被拒后视图不应重建");
}

const CHANGED: &str = concat!(
    "{\n",
    " \"ok\": true,\n",
    " \"conflict\": false,\n",
    " \"impact\": [\n",
    "  \"第2章 引用 P-099(状态已变),需同步检查\"\n",
    " ],\n",
    " \"notes\": [\n",
    "  \"重放范围 1..2 章,状态由章节文件重算(幂等)\",\n",
    "  \"受影响的 P/S/实体 3 项: P-099、S-099、赵六\",\n",
    "  \"派生视图已重建\",\n",
    "  \"级联完成;续写前请再跑 checks.gate(postcommit) 确认全绿\"\n",
    " ]\n",
    "}\n",
);

#[test]
fn changed_facts_reported_with_impact() {
    let dir = tempfile::tempdir().unwrap();
    let root = engine_named_root(&dir);
    copy_dir_all(&corpus("018-resume-3").join("book"), &root);
    let p1 = root.join("定稿").join("正文").join("001-开局.md");
    let t = fs::read_to_string(&p1).unwrap();
    write(&root, "定稿/正文/001-开局.md", &t
        .replace("promises_new: P-001", "promises_new: P-001,P-099")
        .replace("info_gaps_new: S-001", "info_gaps_new: S-001,S-099")
        .replace("entities_new: 阿米,米粒", "entities_new: 阿米,米粒,赵六"));
    let p2 = root.join("定稿").join("正文").join("002-夜访.md");
    let t2 = fs::read_to_string(&p2).unwrap();
    write(&root, "定稿/正文/002-夜访.md", &t2.replace(
        "promises_advanced: P-001",
        "promises_advanced: P-001,P-099",
    ));
    let (stdout, state) = cascade(&root, 1, 3);
    assert_eq!(stdout, CHANGED);
    // 状态确已重算：P-099 成为活跃承诺、赵六 进入实体、S-099 未曝光
    assert!(state.promises.contains_key("P-099"));
    assert!(state.entity_states.contains_key("赵六"));
    assert_eq!(state.info_gaps.get("S-099").map(|g| g.revealed), Some(false));
    assert_eq!(state.state_revision, 4);
}

const TIMELINE_PRESERVED: &str = concat!(
    "{\n",
    " \"ok\": true,\n",
    " \"conflict\": false,\n",
    " \"impact\": [],\n",
    " \"notes\": [\n",
    "  \"重放范围 1..2 章,状态由章节文件重算(幂等)\",\n",
    "  \"timeline 保留非 fact 事件 2 条\",\n",
    "  \"重放结果与旧状态一致(无事实变更)\",\n",
    "  \"派生视图已重建\",\n",
    "  \"级联完成;续写前请再跑 checks.gate(postcommit) 确认全绿\"\n",
    " ]\n",
    "}\n",
);

#[test]
fn non_fact_timeline_events_preserved_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let root = engine_named_root(&dir);
    copy_dir_all(&corpus("018-resume-3").join("book"), &root);
    // 拷贝 corpus 状态、仅替换 timeline（与 oracle 脚本一致——其余事实必须与
    // 正文 front matter 重放结果吻合，否则 changed 非空、notes 不符）
    let sp = root.join("追踪").join("_tracking-state.json");
    let mut s: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&sp).unwrap()).unwrap();
    s["timeline"] = serde_json::json!([
        {"ch": 1, "event": "开局", "kind": "fact"},
        {"ch": 2, "event": "夜访", "kind": "fact"},
        {"ch": 1, "event": "揭示 S-001", "kind": "reveal"},
        {"ch": 2, "event": "私记", "kind": "private"},
    ]);
    fs::write(&sp, s.to_string()).unwrap();
    let (stdout, state) = cascade(&root, 1, 3);
    assert_eq!(stdout, TIMELINE_PRESERVED);
    // 三轨插回：fact1, reveal(ch1), fact2, private(ch2)（引擎实跑确认的顺序）
    let kinds: Vec<&str> = state.timeline.iter().map(|e| e.kind.as_str()).collect();
    assert_eq!(kinds, ["fact", "reveal", "fact", "private"]);
}
