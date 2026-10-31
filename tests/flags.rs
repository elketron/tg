//! End-to-end through the real binary for the walk/query-source flags:
//! `--file`, `--langs`, `--no-ignore` and `--hidden`. Exact stdout pins
//! which files were walked and which languages ran.

use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tg"))
        .args(args)
        .output()
        .expect("tg runs")
}

fn join(lines: &[&str]) -> String {
    lines.iter().map(|l| l.to_string() + "\n").collect()
}

/// The fixture walk dir: `visible.md` always counts; `ignored.md` is
/// gitignored and `.hidden.md` is hidden, so by default only visible.md is
/// walked.
const WALK: &str = "tests/fixtures/walk";
const TREE: &str = "tests/fixtures/tree";

fn p(name: &str) -> String {
    format!("{WALK}/{name}")
}

#[test]
fn file_flag_reads_the_query_from_a_scm_file() {
    let out = run(&["--file", "tests/fixtures/walk.scm", WALK]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8(out.stdout),
        Ok(join(&[&p("visible.md"), "  3: Visible paragraph."]))
    );
}

/// Given both a query argument and `--file`, the positional is the
/// directory (ripgrep semantics): the query comes from the file. One
/// positional, one directory, so the run walks WALK normally.
#[test]
fn query_and_file_together_search_the_positional_as_dir() {
    let out = run(&["--file", "tests/fixtures/walk.scm", "(paragraph) @p"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // The cwd is the crate root; "(paragraph) @p" names no directory, so
    // the walk finds nothing and stdout stays empty.
    assert_eq!(String::from_utf8(out.stdout), Ok(String::new()));
}

/// tg searches one directory; with `--file`, every positional is a
/// directory, so a second one is an error rather than a silent drop.
#[test]
fn a_second_directory_positional_is_an_error() {
    let out = run(&["--file", "tests/fixtures/walk.scm", WALK, TREE]);
    assert!(!out.status.success());
    assert_eq!(String::from_utf8(out.stdout), Ok(String::new()));
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        stderr.contains("search one directory at a time (2 given)"),
        "expected the two-directories error in:\n{stderr}"
    );
}

#[test]
fn a_query_is_required_as_argument_or_file() {
    let out = run(&[]);
    assert!(!out.status.success());
    assert_eq!(String::from_utf8(out.stdout), Ok(String::new()));
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        stderr.contains("a query is required"),
        "expected the required-query error in:\n{stderr}"
    );
}

#[test]
fn langs_restricts_the_run_to_the_listed_languages() {
    // markdown is filtered out, so its file is skipped: nothing matches and
    // the run exits clean (no language with candidate files failed).
    let out = run(&["(paragraph) @p", "--langs", "rust", WALK]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8(out.stdout), Ok(String::new()));
    // The same query without the filter does match (see no_ignore/hidden
    // tests for the full default walk); with markdown listed, it matches
    // again.
    let out = run(&["(paragraph) @p", "--langs", "markdown", WALK]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8(out.stdout),
        Ok(join(&[&p("visible.md"), "  3: Visible paragraph."]))
    );
}

#[test]
fn no_ignore_walks_ignored_files() {
    let out = run(&["(paragraph) @p", WALK]);
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8(out.stdout),
        Ok(join(&[&p("visible.md"), "  3: Visible paragraph."]))
    );
    let out = run(&["--no-ignore", "(paragraph) @p", WALK]);
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8(out.stdout),
        Ok(join(&[
            &p("ignored.md"),
            "  1: Ignored paragraph.",
            "",
            &p("visible.md"),
            "  3: Visible paragraph.",
        ]))
    );
}

#[test]
fn hidden_flag_includes_hidden_files() {
    let out = run(&["--hidden", "(paragraph) @p", WALK]);
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8(out.stdout),
        Ok(join(&[
            &p(".hidden.md"),
            "  1: Hidden paragraph.",
            "",
            &p("visible.md"),
            "  3: Visible paragraph.",
        ]))
    );
}

/// The `--langs` values the README documents must be exactly these
/// spellings on the command line (`typescript` and `csharp` need explicit
/// pins; clap kebab-cases multi-word variants by default).
#[test]
fn langs_accepts_the_documented_spellings() {
    let out = run(&[
        "(paragraph) @p",
        "--langs",
        "rust,markdown,typescript,csharp",
        WALK,
    ]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8(out.stdout),
        Ok(join(&[&p("visible.md"), "  3: Visible paragraph."]))
    );
}
