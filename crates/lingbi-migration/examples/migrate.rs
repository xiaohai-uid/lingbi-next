//! 迁移验收 CLI：`cargo run -p lingbi-migration --example migrate <source> <destination>`
//!
//! 运行五件套管线并打印六字段回执 JSON；退出码 0 = 迁移成功且校验通过。
//! 用于本地验收（#37 契约校验 / #39 真实《零界道种》迁移）。

use lingbi_migration::migrate_book;
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: migrate <source> <destination>");
        std::process::exit(2);
    }
    match migrate_book(Path::new(&args[1]), Path::new(&args[2])) {
        Ok(receipt) => {
            println!("{}", receipt.to_json().expect("receipt json"));
        }
        Err(error) => {
            eprintln!("migration failed: {error}");
            std::process::exit(1);
        }
    }
}
