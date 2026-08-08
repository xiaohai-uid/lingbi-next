//! 门禁模块（P2-06 前置）—— 与引擎 `gates.py` 行为等价。
//!
//! 覆盖：`gate()` 按 stage 过滤 `collect_checks` 结果，输出通过/失败清单。
//! - prewrite：只跑 1(编号)/2(字数,允许弱提示)/3(占位符)/11(摘要)；其余记"跳过"。
//! - precommit：全量，passed = 无 fail。
//! - postcommit：全量且第 10 项（追踪新鲜）必查（不得跳过，计入 fail）。
//!
//! 注意：本模块随 P2-05 checks 落地（doctor stdout 等价测试依赖聚合）；
//! `exit_code`（引擎 `cli.cmd_doctor` 的 0/1 映射）随 P2-06 落在库层，
//! 待命令层（lingbi-application/ffi）落地时由 doctor 命令调用。
//!
//! 与引擎的差异（有意）：`gate()` 本身纯聚合、无输出/退出码（引擎同）；
//! 引擎 cmd_doctor 的 FileNotFoundError → exit 1 路径属 CLI 层，不在本模块。

use crate::checks::collect_checks;
use lingbi_contracts::AppError;
use std::collections::HashSet;
use std::path::Path;

/// 引擎 `gate()` 返回的聚合结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateResult {
    pub passed: bool,
    pub fails: Vec<String>,
    pub oks: Vec<String>,
}

/// 引擎 `cli.cmd_doctor` 的 exit code：`return 0 if r["passed"] else 1`。
pub fn exit_code(g: &GateResult) -> i32 {
    if g.passed {
        0
    } else {
        1
    }
}

/// 引擎 `gate(root, stage, chapter)`：`{"passed", "fails", "oks"}`。
pub fn gate(
    root: &Path,
    stage: &str,
    chapter: Option<i64>,
) -> Result<GateResult, AppError> {
    let checks = collect_checks(root, stage, chapter)?;
    let active: HashSet<usize> = if stage == "prewrite" {
        HashSet::from([1, 2, 3, 11])
    } else {
        (1..=checks.len()).collect()
    };
    let mut fails = Vec::new();
    let mut oks = Vec::new();
    for (i, c) in checks.iter().enumerate() {
        let i = i + 1;
        if !active.contains(&i) {
            oks.push(format!("{}(跳过)", c.name));
            continue;
        }
        if i == 2 && stage == "prewrite" && !c.ok {
            // 第 2 项在 prewrite 仅弱提示，不计 fail
            oks.push(format!("{}(弱提示: {})", c.name, c.detail));
            continue;
        }
        if c.ok {
            oks.push(c.name.clone());
        } else {
            fails.push(format!("{}: {}", c.name, c.detail));
        }
    }
    Ok(GateResult {
        passed: fails.is_empty(),
        fails,
        oks,
    })
}
