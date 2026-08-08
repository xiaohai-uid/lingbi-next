#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""P0: 生成 storyrepo 引擎 golden oracle corpus（冻结引擎 SHA）。

把 Python 引擎当黑盒:构造标准样本书仓,按固定脚本逐条执行 CLI 命令,
每步记录 stdout / exit code + 步后整仓快照(含追踪状态/派生视图等副作用),
产出 fixtures/storyrepo-oracle/<engine_sha>/。

Rust Book Core 的 equivalence tests 只读本 corpus(Windows 门禁零 Python/WSL 依赖);
Python 引擎仅在本生成器运行期被调用一次(或 parity job 中)。

用法:
  python scripts/gen-storyrepo-oracle.py <engine_dir> <out_dir>
"""
import argparse
import glob
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile

PROSE_CH1 = (
    "山门之外，夜色如墨。\n\n"
    "阿米推开木门，米粒在灯下抬头。\n\n"
    "他笑了笑，说:今夜有客。\n\n"
    "风起，檐角铜铃轻响。\n\n"
    "米粒放下笔，望向窗外。\n\n"
    "远处有火光一闪而逝。\n"
)
PROSE_CH2 = (
    "子时，巷中无人。\n\n"
    "阿米独自走过长街，脚步无声。\n\n"
    "米粒的窗还亮着。\n\n"
    "夜色如墨，万籁俱寂。\n\n"
    "他停下，抬手敲门。\n\n"
    "门内没有回应。\n"
)


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def normalize_tree(root):
    """整目录 CRLF→LF(幂等)。引擎在 Windows 写 CRLF,统一 LF 使 corpus 跨平台字节一致。"""
    for base, _dirs, files in os.walk(root):
        for fn in files:
            p = os.path.join(base, fn)
            with open(p, "rb") as f:
                data = f.read()
            if b"\r\n" in data:
                with open(p, "wb") as f:
                    f.write(data.replace(b"\r\n", b"\n"))


def dir_hashes(root):
    """整目录 {relpath: sha256}。"""
    hashes = {}
    for base, _dirs, files in os.walk(root):
        for fn in files:
            p = os.path.join(base, fn)
            rel = os.path.relpath(p, root).replace("\\", "/")
            hashes[rel] = sha256_file(p)
    return hashes


def snapshot(root, dst):
    """整仓快照(复制到 dst)+ 返回 {relpath: sha256}。"""
    if os.path.exists(dst):
        shutil.rmtree(dst)
    shutil.copytree(root, dst)
    return dir_hashes(dst)


def write(path, content):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(content)


def build_sample_book(root):
    """构造标准样本书仓(与引擎测试 conftest 同构,min 字宽调低使短正文可过 gate)。"""
    for d in ("大纲/承诺", "大纲/信息差", "大纲/卷纲", "定稿/正文", "定稿/设定",
              "定稿/设定/角色", "定稿/设定/信息差", "定稿/记忆/章摘要", "文风/金句库",
              "工作区/评审报告", "追踪"):
        os.makedirs(os.path.join(root, d), exist_ok=True)
    write(os.path.join(root, "book.yaml"),
          "---\nname: 样板书\nstatus: writing\ntotal_chapters: 4\n"
          "min_words_per_chapter: 100\nmax_words_per_chapter: 300\n"
          "target_words: 200000\n---\n")
    write(os.path.join(root, "大纲", "总纲.md"),
          "# 总纲\n\n一本用于引擎 oracle 的样板书:主角阿米与米粒,伏笔 P-001,信息差 S-001。\n")
    write(os.path.join(root, "大纲", "细纲蓝图.md"),
          "| ch | 标题 | 要发生什么 | 硬约束 | 禁区 | 角色 | suspense | foreshadow | twist |\n"
          "|----|------|-----------|--------|------|------|----------|------------|-------|\n"
          "| 1 | 开局 | 阿米与米粒重逢,火光预示 | 不写赵三 | 赵三 | 阿米,米粒 | 火光 | P-001 | 无 |\n"
          "| 2 | 夜访 | 阿米夜访,米粒失联 | 不暴露 S-001 | 赵三 | 阿米,米粒 | S-001 | P-001 | 无 |\n"
          "| 3 | 惊变 | 城中惊变,米粒现身 | 无 | 无 | 米粒 | 无 | P-002 | S-001 揭示 |\n"
          "| 4 | 余波 | 主角抉择 | 无 | 无 | 阿米 | 无 | 无 | 无 |\n")
    write(os.path.join(root, "大纲", "章纲.md"),
          "| 章 | 标题 | 要发生什么 | 状态 |\n"
          "|----|------|-----------|------|\n"
          "| 1 | 开局 | 重逢 | ✅ |\n"
          "| 2 | 夜访 | 夜访 | ✅ |\n"
          "| 3 | 惊变 | 惊变 | ☐ |\n"
          "| 4 | 余波 | 抉择 | ☐ |\n")
    write(os.path.join(root, "定稿", "设定", "名册.md"),
          "| 名 | 身份 | 首次出场 |\n"
          "|----|------|----------|\n"
          "| 阿米 | 主角 | 001 |\n"
          "| 米粒 | 灯下之人 | 001 |\n")
    write(os.path.join(root, "定稿", "设定", "角色", "阿米.md"),
          "## 身份\n主角,沉默寡言。\n## 现状\n第一章与米粒重逢。\n")
    write(os.path.join(root, "定稿", "设定", "角色", "米粒.md"),
          "## 身份\n灯下之人,写作者。\n## 现状\n第二章失联。\n")
    write(os.path.join(root, "定稿", "设定", "信息差", "S-总表.md"),
          "| 编号 | 内容 | 埋设章 | 揭示章 | 状态 |\n"
          "|------|------|--------|--------|------|\n"
          "| S-001 | 火光真相 | 1 | 3 | open |\n")
    write(os.path.join(root, "大纲", "承诺", "总表.md"),
          "| 编号 | 承诺 | 开启章 | 推进章 | 完成章 | 状态 |\n"
          "|------|------|--------|--------|--------|------|\n"
          "| P-001 | 火光必有真相 | 1 | | | open |\n")
    write(os.path.join(root, "文风", "风格宪法.md"),
          "# 风格宪法\n\n短句,克制,白描。\n")
    # 章 1:完整 front matter,正文含 must_cover 实体与信息差埋设
    write(os.path.join(root, "定稿", "正文", "001-开局.md"),
          "---\n"
          "title: 开局\nchapter: 001\nvolume: 1\n"
          "words: 66\nstatus: 定稿\n"
          "promises_new: P-001\npromises_advanced: \npromises_done: \n"
          "info_gaps_new: S-001\ninfo_gaps_revealed: \n"
          "entities_new: 阿米,米粒\nmust_cover: 阿米,米粒\nforbidden: 赵三\n"
          "summary: 阿米夜归,与米粒重逢,远山火光一闪而逝。\n"
          "---\n\n" + PROSE_CH1)
    # 章 2:草稿态,front matter 齐备,正文含 must_cover 与可收割金句
    write(os.path.join(root, "定稿", "正文", "002-夜访.md"),
          "---\n"
          "title: 夜访\nchapter: 002\nvolume: 1\n"
          "words: 63\nstatus: 草稿\n"
          "promises_new: \npromises_advanced: P-001\npromises_done: \n"
          "info_gaps_new: \ninfo_gaps_revealed: \n"
          "entities_new: \nmust_cover: 米粒\nforbidden: 赵三\n"
          "summary: 子时阿米夜访,米粒的窗亮着却无人回应。\n"
          "---\n\n" + PROSE_CH2)
    for n, body, summary in ((1, PROSE_CH1, "阿米夜归,与米粒重逢,远山火光一闪而逝。"),
                             (2, PROSE_CH2, "子时阿米夜访,米粒的窗亮着却无人回应。")):
        write(os.path.join(root, "定稿", "记忆", "章摘要", "%03d.md" % n),
              "# 章摘要 %03d\n%s\n" % (n, summary))
    # 评审输入(供 review 命令消费)
    write(os.path.join(root, "工作区", "评审报告", "第002章.json"),
          json.dumps({"issues": [
              {"severity": "high", "category": "continuity", "location": "002-夜访.md:12",
               "description": "时间线矛盾:门内没有回应", "evidence": "前文米粒在灯下",
               "fix_hint": "改为门内有声"},
              {"severity": "low", "category": "ai_flavor", "location": "002-夜访.md:16",
               "description": "句式重复", "evidence": "夜色如墨,万籁俱寂",
               "fix_hint": "删减"},
          ]}, ensure_ascii=False, indent=1))
    # 追踪状态初始化(单一权威)
    sys.path.insert(0, engine_dir)
    from storyrepo import state as st  # noqa: E402
    st.init_state(root)
    normalize_tree(root)


STEPS = [
    ("doctor-prewrite-1", ["doctor", "--stage", "prewrite", "--chapter", "1"]),
    ("doctor-prewrite-2", ["doctor", "--stage", "prewrite", "--chapter", "2"]),
    ("contract-1", ["contract", "--chapter", "1"]),
    ("contract-2", ["contract", "--chapter", "2"]),
    ("context-1", ["context", "--chapter", "1"]),
    ("context-2", ["context", "--chapter", "2"]),
    ("brief-1", ["brief", "--chapter", "1"]),
    ("brief-2", ["brief", "--chapter", "2"]),
    ("doctor-precommit-1", ["doctor", "--stage", "precommit", "--chapter", "1"]),
    ("doctor-precommit-2", ["doctor", "--stage", "precommit", "--chapter", "2"]),
    ("review-2", ["review", "--chapter", "2",
                  "--json", os.path.join("工作区", "评审报告", "第002章.json")]),
    ("commit-1", ["commit", "--chapter", "1"]),
    ("check", ["check"]),
    ("status", ["status"]),
    ("commit-2", ["commit", "--chapter", "2"]),
    ("doctor-postcommit-2", ["doctor", "--stage", "postcommit", "--chapter", "2"]),
    ("stats", ["stats"]),
    ("resume-3", ["resume", "--chapter", "3"]),
    ("revision-2", ["revision", "--chapter", "2"]),
    ("learn-2", ["learn", "--chapter", "2"]),
    ("batch-create-3-4", ["batch", "create", "--chapters", "3-4"]),
    ("report-write-2", ["report", "--stage", "write", "--chapter", "2"]),
]


def run_step(engine_dir, book_root, argv, env):
    proc = subprocess.run(
        [sys.executable, "-m", "storyrepo.cli"] + argv + ["--root", book_root],
        cwd=book_root, env=env, capture_output=True, timeout=120)
    # Windows 管道下 print 会转 CRLF;corpus 统一 LF(与仓库 fixtures 约定一致)
    out = proc.stdout.decode("utf-8", errors="replace").replace("\r\n", "\n")
    return proc.returncode, out


def main():
    ap = argparse.ArgumentParser(description="生成 storyrepo golden oracle corpus")
    ap.add_argument("engine_dir", help="storyrepo engine 目录(冻结 SHA worktree)")
    ap.add_argument("out_dir", help="corpus 输出目录(fixtures/storyrepo-oracle/<sha>)")
    args = ap.parse_args()
    global engine_dir
    engine_dir = os.path.abspath(args.engine_dir)
    out = os.path.abspath(args.out_dir)
    if os.path.exists(out):
        shutil.rmtree(out)
    os.makedirs(out)
    env = dict(os.environ, PYTHONUTF8="1", PYTHONIOENCODING="utf-8",
               PYTHONPATH=engine_dir)

    steps_dir = os.path.join(out, "steps")
    os.makedirs(steps_dir)
    manifest = {
        "engine_sha": None,
        "generator": os.path.basename(__file__),
        "python": sys.version.split()[0],
        "steps": [],
    }
    # 引擎 SHA
    try:
        r = subprocess.run(["git", "-C", engine_dir, "rev-parse", "HEAD"],
                           capture_output=True, text=True)
        manifest["engine_sha"] = r.stdout.strip() or None
    except Exception:
        pass

    # 活书仓放临时目录;corpus 只保留不可变快照 steps/000-initial + steps/NNN-*
    live = tempfile.mkdtemp(prefix="sr-oracle-live-")
    try:
        build_sample_book(live)
        init_snap = os.path.join(steps_dir, "000-initial")
        os.makedirs(init_snap)
        initial_hashes = snapshot(live, os.path.join(init_snap, "book"))
        manifest["initial_book"] = {"files": len(initial_hashes),
                                    "sha256": initial_hashes}
        for i, (name, argv) in enumerate(STEPS, 1):
            step_dir = os.path.join(steps_dir, "%03d-%s" % (i, name))
            os.makedirs(step_dir)
            code, stdout = run_step(engine_dir, live, argv, env)
            normalize_tree(live)  # 引擎副作用统一 LF,保证跨步/跨平台确定性
            with open(os.path.join(step_dir, "stdout.txt"), "w", encoding="utf-8",
                      newline="\n") as f:
                f.write(stdout)
            with open(os.path.join(step_dir, "exitcode"), "w", encoding="utf-8",
                      newline="\n") as f:
                f.write(str(code))
            hashes = snapshot(live, os.path.join(step_dir, "book"))
            manifest["steps"].append({
                "id": "%03d-%s" % (i, name),
                "cmd": argv,
                "exit": code,
                "stdout_sha256": sha256_file(os.path.join(step_dir, "stdout.txt")),
                "book_files": len(hashes),
                "book_sha256": hashes,
            })
            print("[%02d/%02d] %-22s exit=%d" % (i, len(STEPS), name, code))
    finally:
        shutil.rmtree(live, ignore_errors=True)
    with open(os.path.join(out, "MANIFEST.json"), "w", encoding="utf-8",
              newline="\n") as f:
        json.dump(manifest, f, ensure_ascii=False, indent=1)
    print("corpus → %s (steps=%d)" % (out, len(STEPS)))


if __name__ == "__main__":
    main()
