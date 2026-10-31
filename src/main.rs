use std::io::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use tg::lang;
use tg::report::Report;

#[derive(Parser)]
#[command(
    name = "tg",
    about = "ripgrep-style search where the pattern is a tree-sitter query"
)]
struct Args {
    /// The tree-sitter query (S-expression, with at least one capture).
    query: String,
    /// Directory to search.
    #[arg(default_value = ".")]
    dir: PathBuf,
}

fn main() -> ExitCode {
    let args = Args::parse();
    let report = tg::run(&args.dir, &args.query);
    print(&report);
    if report.fatal {
        eprintln!(
            "tg: the query compiled against none of the languages with files here; \
             see the errors above, or restrict the query to a language that matches"
        );
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// Matches go to stdout (the tool's interface): grouped by file, each hit
/// indented with its line number and the node's first line, a blank line
/// between files. Everything else — query and parse errors, and the
/// per-language parse counts — goes to stderr.
fn print(report: &Report) {
    for error in &report.query_errors {
        eprintln!("tg: {}: query error: {}", error.name, error.error);
    }
    for path in &report.parse_errors {
        eprintln!("tg: {}: parse failed", path.display());
    }
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    for (i, group) in report.groups.iter().enumerate() {
        if i > 0 {
            let _ = writeln!(stdout);
        }
        let _ = writeln!(stdout, "{}", group.path.display());
        for hit in &group.hits {
            let _ = writeln!(stdout, "  {}: {}", hit.line, hit.text);
        }
    }
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
