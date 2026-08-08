//! lingbi-book — Book Project V3 书仓核心（P2 逐模块等价移植 storyrepo 引擎）。
//!
//! 行为基准：`fixtures/storyrepo-oracle/<engine_sha>/`（Python 引擎黑盒 golden corpus）。
//! 契约基准：`contracts/book/book-v3.schema.json` + `book-v3.tree.json`。
//!
//! 模块序（P2 票据）：config → book state → tracking state → derived views →
//! checks → gates → contract → context → brief → revision → resume → batch → learn → report。

pub mod batch;
pub mod brief;
pub mod checks;
pub mod config;
pub mod contract;
pub mod context;
pub mod gates;
pub mod learn;
pub mod report;
pub mod resume;
pub mod review;
pub mod revision;
pub mod state;
pub mod tracking;
pub mod views;

mod json_engine;
mod py_num;

pub use batch::{create_batch_plan, create_plan_stdout, merge_batch, BatchMergeResult, BatchPlan};
pub use brief::build_brief;
pub use checks::{collect_checks, redlines, CheckResult};
pub use config::{load_config, BookConfig};
pub use contract::{build_contract, load_outline, split_list, Contract};
pub use context::{assemble_context, budget_check, context_stdout, Context};
pub use gates::{exit_code, gate, GateResult};
pub use learn::{absorb_review, absorb_stdout, harvest_quotes, learn_stdout, style_stats, trend, trend_stdout};
pub use report::author_report;
pub use resume::{diagnose, DiagnoseResult};
pub use review::{has_blocking, load_review, review_path, ReviewIssue};
pub use revision::{cascade_revision, RevisionResult};
pub use state::{body, chapter_files, chapter_num, fm_list, frontmatter, hanzi, latest_chapter};
pub use tracking::{
    dump_state, init_state, load_state, merge_chapter_facts, new_state, save_state, TrackingState,
};
pub use views::{card7, card_size_ok, write_views};