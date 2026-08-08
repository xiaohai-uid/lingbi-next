//! Python 数值语义（舍入/格式化）—— 与 CPython 的 IEEE754 运算路径逐位一致。
//!
//! P2-08 由 context 落地（round2/%.1f），P2-13 起 learn 复用（round(x,1)）。

/// Python `round(x, 2)`：银行家舍入（half-even）。与引擎相同的 IEEE754 运算路径。
pub(crate) fn py_round2(x: f64) -> f64 {
    (x * 100.0).round_ties_even() / 100.0
}

/// `x·10` 的正确舍入（ties-even），不经中间 f64 乘法。
///
/// 引擎 `"%.1f"` / `round(x, 1)` 由 dtoa 对真实值正确舍入；朴素 `(x*10.0).round_ties_even()`
/// 会先把真实值 ×10 舍入到 f64——当真实值贴近 .5 边界时被吸到整半点，随后
/// ties-even 走错方向（如 budget_kb=0.95：真实值 9.4999…96 应舍为 9，朴素路径
/// 得 9.5→10 显示 "1.0"，引擎 "0.9"；budget_kb=0.05 同理反向）。本实现按
/// x = m·2^(exp-52) 的位表示精确计算 m·10·2^-s 与 2^(s-1) 的大小/相等关系（u128
/// 整数运算，无二次舍入）。调用方数值 ∈ [0, 16)，m·10 ≤ 2^57，u128 足够。
pub(crate) fn round10_ties_even(x: f64) -> i64 {
    if x == 0.0 {
        return 0;
    }
    let bits = x.to_bits();
    let exp = ((bits >> 52) & 0x7ff) as i32 - 1023; // 归一化指数（值 = m·2^(exp-52)）
    let mant = (bits & ((1u64 << 52) - 1)) | (1u64 << 52);
    let s = 52 - exp; // 值 = m·2^-s
    let n = (mant as u128) * 10;
    if s <= 0 {
        // x·10 为整数（exp ≥ 52；调用方数值不可达，保守路径）
        return (n << (-s) as u32) as i64;
    }
    let q = n >> s;
    let r = n & ((1u128 << s) - 1);
    let half = 1u128 << (s - 1);
    let rounded = if r > half {
        q + 1
    } else if r < half {
        q
    } else if q & 1 == 1 {
        q + 1
    } else {
        q
    };
    rounded as i64
}

/// Python `"%.1f"`：一位小数（ties-even，见 round10_ties_even）。
pub(crate) fn py_fmt_1f(x: f64) -> String {
    let tenths = round10_ties_even(x);
    format!("{}.{}", tenths.div_euclid(10), tenths.rem_euclid(10))
}

/// Python `round(x, 1)`：一位小数（ties-even）。
pub(crate) fn py_round1(x: f64) -> f64 {
    round10_ties_even(x) as f64 / 10.0
}
