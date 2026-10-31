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
    /// The tree-sitter query (S-expression). With `--json`, named captures
    /// define the output fields.
    query: String,
    /// Directory to search.
    #[arg(default_value = ".")]
    dir: PathBuf,
    /// Print one JSON object per match, one per line (NDJSON): `file`,
    /// `line`, and each named capture as a field. A query with no captures
    /// reports the matched node's text as `text`. `file` and `line` always
    /// keep their contract values: a capture named `file` or `line` is
    /// overridden by them.
    #[arg(long)]
    json: bool,
}

fn main() -> ExitCode {
    let args = Args::parse();
    let report = tg::run(&args.dir, &args.query);
    print(&report, args.json);
    if report.fatal {
        eprintln!(
            "tg: the query compiled against none of the languages with files here; \
             see the errors above, or restrict the query to a language that matches"
        );
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// Matches go to stdout (the tool's interface): with `--json`, one NDJSON
/// line per match (`file`, `line`, each named capture); otherwise grouped
/// by file, each hit indented with its line number and the node's first
/// line, a blank line between files. Everything else — query and parse
/// errors, and the per-language parse counts — goes to stderr.
fn print(report: &Report, json: bool) {
    for error in &report.query_errors {
        eprintln!("tg: {}: query error: {}", error.name, error.error);
    }
    for path in &report.parse_errors {
        eprintln!("tg: {}: parse failed", path.display());
    }
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    if json {
        for group in &report.groups {
            for hit in &group.hits {
                let mut obj = serde_json::Map::new();
                for (name, text) in &hit.captures {
                    obj.insert(name.clone(), text.clone().into());
                }
                // Inserted last so the contract fields win a name clash.
                obj.insert("file".into(), group.path.display().to_string().into());
                obj.insert("line".into(), (hit.line as u64).into());
                if let Ok(line) = serde_json::to_string(&obj) {
                    let _ = writeln!(stdout, "{line}");
                }
            }
        }
    } else {
        for (i, group) in report.groups.iter().enumerate() {
            if i > 0 {
                let _ = writeln!(stdout);
            }
            let _ = writeln!(stdout, "{}", group.path.display());
            for hit in &group.hits {
                let Some((_, text)) = hit.captures.first() else {
                    continue;
                };
                let _ = writeln!(
                    stdout,
                    "  {}: {}",
                    hit.line,
                    text.lines().next().unwrap_or_default()
                );
            }
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
