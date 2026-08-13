//! 引擎式 JSON 序列化（json.dump/dumps ensure_ascii=False indent=1 逐字节等价）。
//!
//! P2-03 由 tracking 落地（dump_state），P2-07 起 contract 共用（to_json）。
//! 规则：1 空格缩进、`": "` 分隔、中文原样、JSON 标准转义；空数组/空对象内联。

pub(crate) enum Frag {
    Str(String),
    Int(i64),
    Raw(String),
    Bool(bool),
    Null,
    Arr(Vec<Frag>),
    Obj(Vec<(String, Frag)>),
}

pub(crate) fn esc(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

pub(crate) fn dump(frag: &Frag, indent: usize, out: &mut String) {
    match frag {
        Frag::Str(s) => esc(s, out),
        Frag::Int(n) => out.push_str(&n.to_string()),
        Frag::Raw(r) => out.push_str(r),
        Frag::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Frag::Null => out.push_str("null"),
        Frag::Arr(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push('\n');
                out.push_str(&" ".repeat(indent + 1));
                dump(item, indent + 1, out);
            }
            out.push('\n');
            out.push_str(&" ".repeat(indent));
            out.push(']');
        }
        Frag::Obj(entries) => {
            if entries.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push('{');
            for (i, (k, v)) in entries.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push('\n');
                out.push_str(&" ".repeat(indent + 1));
                esc(k, out);
                out.push_str(": ");
                dump(v, indent + 1, out);
            }
            out.push('\n');
            out.push_str(&" ".repeat(indent));
            out.push('}');
        }
    }
}

pub(crate) fn obj(entries: Vec<(String, Frag)>) -> Frag {
    Frag::Obj(entries)
}

/// 字符串数组 → 引擎式 Frag 数组。
pub(crate) fn frag_strs(items: &[String]) -> Frag {
    Frag::Arr(items.iter().map(|s| Frag::Str(s.clone())).collect())
}

pub(crate) fn opt_int(v: Option<i64>) -> Frag {
    match v {
        Some(n) => Frag::Int(n),
        None => Frag::Null,
    }
}

/// 渲染为字符串（引擎 dump 顶层无缩进）。
pub(crate) fn render(frag: &Frag) -> String {
    let mut out = String::new();
    dump(frag, 0, &mut out);
    out
}
