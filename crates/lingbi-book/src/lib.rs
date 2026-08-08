//! lingbi-book — Book Project V3 书仓核心（P2 逐模块等价移植 storyrepo 引擎）。
//!
//! 行为基准：`fixtures/storyrepo-oracle/<engine_sha>/`（Python 引擎黑盒 golden corpus）。
//! 契约基准：`contracts/book/book-v3.schema.json` + `book-v3.tree.json`。
//!
//! 模块序（P2 票据）：config → book state → tracking state → derived views →
//! checks → gates → contract → context → brief → revision → resume → batch → learn → report。

pub mod config;
pub mod state;

pub use config::{load_config, BookConfig};
pub use state::{body, chapter_files, chapter_num, fm_list, frontmatter, hanzi, latest_chapter};