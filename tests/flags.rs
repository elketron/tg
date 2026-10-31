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

/// With `--file`, a lone positional is the directory (same interface as
/// ripgrep's `rg -f query.scm dir`), so the query comes from the file and
/// the walk starts at WALK; nothing else is walked.
#[test]
fn file_flag_with_a_dir_walks_only_that_dir() {
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

/// With `--file`, even a second positional is a directory (ripgrep
/// semantics): the run walks the fixture through it, not whatever the
/// stray positional names.
#[test]
fn with_file_positionals_are_all_directories() {
    let out = run(&["--file", "tests/fixtures/walk.scm", WALK, WALK]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Both positionals are the same dir, so visible.md appears once per walk.
    assert_eq!(
        String::from_utf8(out.stdout),
        Ok(join(&[&p("visible.md"), "  3: Visible paragraph."]))
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
