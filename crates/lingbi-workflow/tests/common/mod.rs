//! 测试设施统一从 `lingbi_workflow::testing` 复用（feature test-support）。

pub use lingbi_workflow::testing::{
    init_book, standard_script, DRAFT, POLISH, REVIEW_BLOCKING, REVIEW_OK, ScriptedProvider,
};
