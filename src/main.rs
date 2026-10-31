use std::io::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use tg::Options;
use tg::lang;
use tg::report::Report;

#[derive(Parser)]
#[command(
    name = "tg",
    about = "ripgrep-style search where the pattern is a tree-sitter query"
)]
struct Args {
    /// The tree-sitter query (S-expression); exactly one of this or
    /// `--file` is required, and with `--file` every positional is a
    /// directory. With `--json`, named captures define the output fields.
    query: Option<String>,
    /// Read the query from this file instead of the argument; exactly one
    /// of the two is required. With `--file`, a lone positional is the
    /// directory (as in `rg -f query.scm dir`).
    #[arg(short = 'f', long, value_name = "PATH")]
    file: Option<PathBuf>,
    /// Directories to search.
    #[arg(default_value = ".")]
    dir: Vec<PathBuf>,
    /// Comma-separated list of languages to restrict the run to; files of
    /// every other supported language are skipped.
    #[arg(long, value_delimiter = ',', value_name = "LANGS")]
    langs: Vec<lang::Language>,
    /// Do not respect ignore files (.gitignore, .ignore, ...) when walking.
    #[arg(long)]
    no_ignore: bool,
    /// Include hidden files and directories.
    #[arg(long)]
    hidden: bool,
    /// Print one JSON object per match, one per line (NDJSON): `file`,
    /// `line`, and each named capture as a field. A query with no captures
    /// reports the matched node's text as `text`. `file` and `line` always
    /// keep their contract values: a capture named `file` or `line` is
    /// overridden by them.
    #[arg(long)]
    json: bool,
}

fn main() -> ExitCode {
    let mut args = Args::parse();
    // Exactly one of the query argument or `--file` must be given; with
    // `--file`, a lone positional is the directory (same interface as
    // ripgrep's `rg -f query.scm dir`), so move it out of the query slot
    // (clap routes the first free positional there).
    if args.file.is_some()
        && let Some(query) = args.query.take()
    {
        args.dir.insert(0, PathBuf::from(query));
    }
    if args.query.is_none() && args.file.is_none() {
        eprintln!("tg: a query is required: give one as an argument or with --file");
        return ExitCode::FAILURE;
    }
    let Some(query) = query_text(&args) else {
        return ExitCode::FAILURE;
    };
    let opts = Options {
        langs: args.langs.clone(),
        no_ignore: args.no_ignore,
        hidden: args.hidden,
    };
    let report = tg::run(&args.dir[0], &query, &opts);
    print(&report, &opts, args.json);
    if report.fatal {
        eprintln!(
            "tg: the query compiled against none of the languages with files here; \
             see the errors above, or restrict the query to a language that matches"
        );
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// The query text from the argument or from `--file` (`main` has checked
/// that exactly one is present); a failed read prints its own error and is
/// fatal.
fn query_text(args: &Args) -> Option<String> {
    if let Some(query) = &args.query {
        return Some(query.clone());
    }
    let path = args.file.as_ref()?;
    match std::fs::read_to_string(path) {
        Ok(text) => Some(text),
        Err(e) => {
            eprintln!("tg: {}: {e}", path.display());
            None
        }
    }
}

/// Matches go to stdout (the tool's interface): with `--json`, one NDJSON
/// line per match (`file`, `line`, each named capture); otherwise grouped
/// by file, each hit indented with its line number and the node's first
/// line, a blank line between files. Everything else — query and parse
/// errors, and the per-language parse counts — goes to stderr.
fn print(report: &Report, opts: &Options, json: bool) {
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
    let tallied: Vec<lang::Language> = if opts.langs.is_empty() {
        lang::LANGUAGES.to_vec()
    } else {
        opts.langs.clone()
    };
    for &language in &tallied {
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
