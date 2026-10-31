use std::path::PathBuf;

use clap::Parser;
use tg::lang;

#[derive(Parser)]
#[command(
    name = "tg",
    about = "ripgrep-style search where the pattern is a tree-sitter query"
)]
struct Args {
    /// The tree-sitter query (S-expression); accepted but not matched yet.
    query: String,
    /// Directory to search.
    #[arg(default_value = ".")]
    dir: PathBuf,
}

fn main() {
    let args = Args::parse();
    let _query = args.query; // matching lands in the next stage
    let report = tg::run(&args.dir);
    for &language in &lang::LANGUAGES {
        let c = report
            .per_language
            .get(&language)
            .copied()
            .unwrap_or_default();
        eprintln!(
            "tg: {:<10} parsed ok {} failed {}",
            language.name(),
            c.ok,
            c.failed
        );
    }
    eprintln!("tg: {:<10} skipped {}", "other", report.other_skipped);
}
