#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Book Project V3 契约校验器(纯标准库,零第三方依赖)。

三种模式:
  python scripts/validate-book-v3.py <book_root>            # 单书仓校验
  python scripts/validate-book-v3.py --corpus <corpus_dir>  # golden corpus 全量(23 快照 + 棘轮跨步)
  python scripts/validate-book-v3.py --self-test            # 反例集构造并断言全拒

契约来源:
  contracts/book/book-v3.schema.json  (book.yaml + _tracking-state.json 数据契约)
  contracts/book/book-v3.tree.json    (目录树类别/可变性契约)
"""
import argparse
import json
import os
import re
import shutil
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SCHEMA_PATH = os.path.join(ROOT, "contracts", "book", "book-v3.schema.json")
TREE_PATH = os.path.join(ROOT, "contracts", "book", "book-v3.tree.json")

REGEX_HINT = set("^$\\*+?[](){}|")
FRONTMATTER_REQUIRED = ["title", "chapter", "volume", "words", "status", "summary"]


# ── 迷你 JSON Schema 引擎(覆盖本契约用到的子集) ──────────────────────────
def _resolve(ref, defs):
    if ref.startswith("#/$defs/"):
        return defs[ref[len("#/$defs/"):]]
    raise ValueError("不支持的 $ref: %s" % ref)


def _check(instance, schema, defs, path, errors):
    if isinstance(schema, bool):
        if not schema:
            errors.append("%s: schema=false" % path)
        return
    if "$ref" in schema:
        _check(instance, _resolve(schema["$ref"], defs), defs, path, errors)
        return
    if "const" in schema:
        if instance != schema["const"]:
            errors.append("%s: 期望 const=%r,实为 %r" % (path, schema["const"], instance))
        return
    if "enum" in schema:
        if instance not in schema["enum"]:
            errors.append("%s: 不在 enum %s 内" % (path, schema["enum"]))
        return
    t = schema.get("type")
    if isinstance(t, list):
        if not any(_type_ok(instance, x) for x in t):
            errors.append("%s: 类型期望 %s,实为 %s" % (path, t, type(instance).__name__))
            return
    elif t and not _type_ok(instance, t):
        errors.append("%s: 类型期望 %s,实为 %s" % (path, t, type(instance).__name__))
        return
    if t == "object" or (isinstance(t, list) and "object" in t):
        if not isinstance(instance, dict):
            return
        for k in schema.get("required", []):
            if k not in instance:
                errors.append("%s: 缺必填字段 %s" % (path, k))
        for k, sub in schema.get("properties", {}).items():
            if k in instance:
                _check(instance[k], sub, defs, "%s.%s" % (path, k), errors)
        for pat, sub in schema.get("patternProperties", {}).items():
            for k, v in instance.items():
                if re.fullmatch(pat, str(k)):
                    _check(v, sub, defs, "%s.%s" % (path, k), errors)
        if schema.get("additionalProperties") is False:
            known = set(schema.get("properties", {})) | {
                k for pat in schema.get("patternProperties", {})
                for k in instance if re.fullmatch(pat, str(k))}
            for k in instance:
                if k not in known:
                    errors.append("%s: 未知字段 %s" % (path, k))
    elif t == "array":
        if isinstance(instance, list):
            items = schema.get("items")
            for i, v in enumerate(instance):
                _check(v, items, defs, "%s[%d]" % (path, i), errors)
    if "minimum" in schema and isinstance(instance, (int, float)) and instance < schema["minimum"]:
        errors.append("%s: %r < minimum %r" % (path, instance, schema["minimum"]))


def _type_ok(instance, t):
    if t == "string":
        return isinstance(instance, str)
    if t == "integer":
        return isinstance(instance, int) and not isinstance(instance, bool)
    if t == "boolean":
        return isinstance(instance, bool)
    if t == "object":
        return isinstance(instance, dict)
    if t == "array":
        return isinstance(instance, list)
    if t == "null":
        return instance is None
    return True


def validate_schema(instance, schema, defs):
    errors = []
    _check(instance, schema, defs, "$", errors)
    return errors


# ── 书仓解析(storyrepo 语义) ────────────────────────────────────────────
def parse_book_yaml(root):
    """平铺 KV(front-matter 风格),数值转 int——与 storyrepo config.py 一致。"""
    p = os.path.join(root, "book.yaml")
    with open(p, encoding="utf-8") as f:
        t = f.read()
    if t.startswith("---"):
        t = t.split("---", 2)[1]
    cfg = {}
    for line in t.splitlines():
        line = line.strip()
        if not line or ":" not in line:
            continue
        k, v = line.split(":", 1)
        k, v = k.strip(), v.strip().strip("\"'")
        if not k:
            continue
        cfg[k] = int(v) if re.fullmatch(r"-?\d+", v) else v
    return cfg


def parse_frontmatter(text):
    """章节 front matter:首个 --- 块内平铺 KV。"""
    if not text.startswith("---"):
        return {}
    body = text.split("---", 2)
    if len(body) < 3:
        return {}
    fm = {}
    for line in body[1].splitlines():
        line = line.strip()
        if not line or ":" not in line:
            continue
        k, v = line.split(":", 1)
        fm[k.strip()] = v.strip()
    return fm


# ── 目录树匹配 ──────────────────────────────────────────────────────────
def _is_regex_key(key):
    return any(c in key for c in REGEX_HINT)


def match_path(rel, paths):
    """精确优先,再正则全匹配。"""
    if rel in paths:
        return paths[rel]
    for key, entry in paths.items():
        if _is_regex_key(key):
            try:
                if re.fullmatch(key, rel):
                    return entry
            except re.error:
                continue
    return None


def walk_files(root):
    for base, _dirs, files in os.walk(root):
        for fn in sorted(files):
            yield os.path.relpath(os.path.join(base, fn), root).replace("\\", "/")


# ── 校验 ────────────────────────────────────────────────────────────────
def validate_book(root, tree, defs):
    errors = []
    for d in tree.get("required_dirs", []):
        if not os.path.isdir(os.path.join(root, d)):
            errors.append("缺必需目录: %s" % d)
    for rel, entry in tree["paths"].items():
        if entry.get("required") and not os.path.exists(os.path.join(root, rel)):
            errors.append("缺必需文件: %s" % rel)
    chapter_nums = []
    for rel in walk_files(root):
        entry = match_path(rel, tree["paths"])
        if entry is None:
            errors.append("未知文件类别: %s" % rel)
            continue
        if rel == "book.yaml":
            try:
                cfg = parse_book_yaml(root)
                errors += ["book.yaml: " + e for e in
                           validate_schema(cfg, defs["book_config"], defs)]
            except Exception as e:
                errors.append("book.yaml 解析失败: %s" % e)
        elif rel == "追踪/_tracking-state.json":
            try:
                with open(os.path.join(root, rel), encoding="utf-8") as f:
                    st = json.load(f)
                errors += ["tracking: " + e for e in
                           validate_schema(st, defs["tracking_state"], defs)]
            except Exception as e:
                errors.append("tracking 解析失败: %s" % e)
        if entry.get("role") == "chapter":
            m = re.fullmatch(r"(\d{3})-.+\.md", os.path.basename(rel))
            if not m:
                errors.append("章节文件名非法: %s" % rel)
                continue
            chapter_nums.append(int(m.group(1)))
            fm = parse_frontmatter(open(os.path.join(root, rel), encoding="utf-8").read())
            for k in FRONTMATTER_REQUIRED:
                if k not in fm:
                    errors.append("%s: front matter 缺 %s" % (rel, k))
            if fm.get("chapter") and str(fm["chapter"]) != m.group(1):
                errors.append("%s: front matter chapter=%s 与文件名 %s 不符"
                              % (rel, fm["chapter"], m.group(1)))
    # 正文编号连续唯一
    if chapter_nums:
        nums = sorted(set(chapter_nums))
        if len(nums) != len(chapter_nums):
            errors.append("章节编号重复: %s" % chapter_nums)
        if nums != list(range(1, max(nums) + 1)):
            errors.append("章节编号不连续: %s" % nums)
    return errors


def ratchet_ok(prev_root, next_root, allow_change_dirs=()):
    """棘轮:定稿/ 下文件跨步字节不变(allow_change_dirs 例外,如 revision 步骤)。"""
    errors = []
    prev_files = {os.path.relpath(os.path.join(b, f), prev_root).replace("\\", "/")
                  for b, _d, fs in os.walk(prev_root) for f in fs}
    next_files = {os.path.relpath(os.path.join(b, f), next_root).replace("\\", "/")
                  for b, _d, fs in os.walk(next_root) for f in fs}
    for rel in sorted(prev_files | next_files):
        if not rel.startswith("定稿/"):
            continue
        if rel in allow_change_dirs or rel.startswith(tuple(allow_change_dirs)):
            continue
        p, n = os.path.join(prev_root, rel), os.path.join(next_root, rel)
        if not os.path.exists(n):
            errors.append("棘轮违例: 定稿文件消失 %s" % rel)
        elif os.path.exists(p) and not os.path.exists(n):
            errors.append("棘轮违例: 定稿文件消失 %s" % rel)
        elif os.path.exists(p) and os.path.exists(n):
            with open(p, "rb") as f1, open(n, "rb") as f2:
                if f1.read() != f2.read():
                    errors.append("棘轮违例: 定稿文件被修改 %s" % rel)
    return errors


# ── 反例集 ──────────────────────────────────────────────────────────────
def build_negatives(base):
    """构造 8 个非法书仓,返回 {名: 路径}。"""
    cases = {}

    def make(name, mutate):
        root = os.path.join(base, name)
        shutil.copytree(os.path.join(base, "good"), root)
        mutate(root)
        return root

    good = os.path.join(base, "good")
    # 合法基底:最小书仓(从 corpus 000-initial 复制)
    corpus_init = os.path.join(ROOT, "fixtures", "storyrepo-oracle")
    src = None
    for sha in sorted(os.listdir(corpus_init)):
        cand = os.path.join(corpus_init, sha, "steps", "000-initial", "book")
        if os.path.isdir(cand):
            src = cand
            break
    if src is None:
        raise SystemExit("corpus 000-initial 不存在,无法构造反例")
    shutil.copytree(src, good)

    cases["n1-no-book-yaml"] = make("n1", lambda r: os.remove(os.path.join(r, "book.yaml")))
    cases["n2-no-tracking"] = make("n2", lambda r: os.remove(
        os.path.join(r, "追踪", "_tracking-state.json")))
    cases["n3-noncontiguous"] = make("n3", lambda r: os.rename(
        os.path.join(r, "定稿", "正文", "002-夜访.md"),
        os.path.join(r, "定稿", "正文", "003-夜访.md")))
    cases["n4-bad-frontmatter"] = make("n4", lambda r: _strip_fm_key(
        os.path.join(r, "定稿", "正文", "001-开局.md"), "summary"))
    cases["n5-bad-revision"] = make("n5", lambda r: _set_json(
        os.path.join(r, "追踪", "_tracking-state.json"), "state_revision", "abc"))
    cases["n6-stray-file"] = make("n6", lambda r: open(
        os.path.join(r, "root-junk.txt"), "w").write("junk"))
    cases["n7-bad-chapter-name"] = make("n7", lambda r: os.rename(
        os.path.join(r, "定稿", "正文", "001-开局.md"),
        os.path.join(r, "定稿", "正文", "开局.md")))
    cases["n8-workspace-writes-final"] = make("n8", lambda r: open(
        os.path.join(r, "定稿", "正文", "099-越权.md"), "w").write("# 越权\n"))
    return cases


def _strip_fm_key(path, key):
    t = open(path, encoding="utf-8").read()
    lines = t.split("\n")
    out = [ln for ln in lines if not ln.startswith(key + ":")]
    open(path, "w", encoding="utf-8", newline="\n").write("\n".join(out))


def _set_json(path, key, value):
    d = json.load(open(path, encoding="utf-8"))
    d[key] = value
    open(path, "w", encoding="utf-8", newline="\n").write(
        json.dumps(d, ensure_ascii=False, indent=1))


def main():
    ap = argparse.ArgumentParser(description="Book V3 契约校验器")
    ap.add_argument("book_root", nargs="?", help="书仓根(单仓模式)")
    ap.add_argument("--corpus", help="corpus 根:验证全部快照 + 棘轮跨步")
    ap.add_argument("--self-test", action="store_true", help="反例集断言全拒")
    args = ap.parse_args()

    tree = json.load(open(TREE_PATH, encoding="utf-8"))
    schema = json.load(open(SCHEMA_PATH, encoding="utf-8"))
    defs = schema["$defs"]
    total_err = 0

    def report(name, errors):
        nonlocal total_err
        if errors:
            total_err += len(errors)
            print("✘ %s (%d):" % (name, len(errors)))
            for e in errors[:8]:
                print("    - %s" % e)
        else:
            print("✔ %s" % name)

    if args.corpus:
        steps = os.path.join(args.corpus, "steps")
        ids = sorted(os.listdir(steps))
        for sid in ids:
            root = os.path.join(steps, sid, "book")
            report("%s tree" % sid, validate_book(root, tree, defs))
        # 棘轮跨步(允许 revision 步骤)
        allow = {"0019-revision-2"}
        for a, b in zip(ids, ids[1:]):
            errs = ratchet_ok(os.path.join(steps, a, "book"),
                              os.path.join(steps, b, "book"),
                              allow_change_dirs=())
            if b in allow:
                errs = [e for e in errs if "0019-revision" not in a]
            report("ratchet %s→%s" % (a, b), errs)
        print("corpus total errors:", total_err)
        return 1 if total_err else 0

    if args.self_test:
        tmp = tempfile.mkdtemp(prefix="bv3-neg-")
        try:
            cases = build_negatives(tmp)
            for name, root in sorted(cases.items()):
                errs = validate_book(root, tree, defs)
                if errs:
                    print("✔ %s 被拒 (%d 项)" % (name, len(errs)))
                else:
                    total_err += 1
                    print("✘ %s 未被拒!" % name)
            # 正例确认:good 基底必须通过
            errs = validate_book(os.path.join(tmp, "good"), tree, defs)
            if errs:
                total_err += len(errs)
                print("✘ good 基底意外失败:", errs)
            else:
                print("✔ good 基底通过")
        finally:
            shutil.rmtree(tmp, ignore_errors=True)
        print("self-test errors:", total_err)
        return 1 if total_err else 0

    if not args.book_root:
        ap.error("需要 book_root / --corpus / --self-test 之一")
    report(args.book_root, validate_book(args.book_root, tree, defs))
    return 1 if total_err else 0


if __name__ == "__main__":
    sys.exit(main())
