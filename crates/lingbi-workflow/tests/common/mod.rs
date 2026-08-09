//! 测试设施统一从 `lingbi_workflow::testing` 复用（feature test-support）。

pub use lingbi_workflow::testing::{
    init_book, standard_script, DRAFT, POLISH, REVIEW_OK, ScriptedProvider,
};
// run_persistence 二进制不使用（Gate 返工测试在 output_gate.rs）——按二进制
// 编译时该 re-export 在部分目标中 unused，显式豁免。
#[allow(unused_imports)]
pub use lingbi_workflow::testing::REVIEW_BLOCKING;
