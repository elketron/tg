//! End-to-end through the real binary for `-r <template>`: `%name` renders
//! the named capture, `%match` the whole matched node, a capture-less query
//! replaces whole nodes, a dry run prints a unified diff and touches
//! nothing, `--write` rewrites fixture copies in a temp dir, an unknown
//! placeholder errors without writing, and a file whose match regions
//! overlap is skipped with an error. Diff blocks are pinned exactly.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A fresh temp dir per test seeded with fixture copies (a rust file and a
/// two-paragraph markdown file) so writes can never touch the originals.
fn fixture_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tg-rewrite-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.rs"), "fn main() {}\n").unwrap();
    std::fs::write(dir.join("b.md"), "hello world.\n\nsecond para.\n").unwrap();
    dir
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tg"))
        .args(args)
        .output()
        .expect("tg runs")
}

fn ok(out: &Output) {
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn stdout(out: &Output) -> String {
    String::from_utf8(out.stdout.clone()).expect("utf8 stdout")
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

fn read(dir: &Path, name: &str) -> String {
    std::fs::read_to_string(dir.join(name)).unwrap()
}

fn diff(path: &str, body: &str) -> String {
    format!("--- {path}\n+++ {path}\n{body}")
}

#[test]
fn write_applies_a_template_with_a_capture_placeholder() {
    let dir = fixture_dir("write-capture");
    let path = dir.join("a.rs").display().to_string();
    let out = run(&[
        "(function_item name: (identifier) @name)",
        "-r",
        "// renamed %name",
        "--write",
        &dir.display().to_string(),
    ]);
    ok(&out);
    assert_eq!(stdout(&out), String::new(), "writes report to stderr");
    assert!(
        stderr(&out).contains(&diff(
            &path,
            "@@ -1 +1 @@\n-fn main() {}\n+// renamed main\n"
        )),
        "{}",
        stderr(&out)
    );
    assert_eq!(read(&dir, "a.rs"), "// renamed main\n");
    assert_eq!(read(&dir, "b.md"), "hello world.\n\nsecond para.\n");
}

#[test]
fn dry_run_prints_a_unified_diff_and_leaves_files_untouched() {
    let dir = fixture_dir("dry-run");
    let path = dir.join("a.rs").display().to_string();
    let out = run(&[
        "(function_item name: (identifier) @name)",
        "-r",
        "%name",
        &dir.display().to_string(),
    ]);
    ok(&out);
    assert_eq!(
        stdout(&out),
        diff(&path, "@@ -1 +1 @@\n-fn main() {}\n+main\n")
    );
    assert!(!stderr(&out).contains("+++"), "the diff went to stdout");
    assert_eq!(
        read(&dir, "a.rs"),
        "fn main() {}\n",
        "dry run wrote nothing"
    );
}

#[test]
fn match_placeholder_expands_to_the_whole_matched_node() {
    let dir = fixture_dir("match-node");
    let path = dir.join("a.rs").display().to_string();
    let out = run(&[
        "(function_item name: (identifier) @name)",
        "-r",
        "%match;",
        "--write",
        &dir.display().to_string(),
    ]);
    ok(&out);
    assert!(
        stderr(&out).contains(&diff(&path, "@@ -1 +1 @@\n-fn main() {}\n+fn main() {};\n")),
        "{}",
        stderr(&out)
    );
    assert_eq!(read(&dir, "a.rs"), "fn main() {};\n");
}

#[test]
fn a_capture_less_query_replaces_whole_nodes() {
    let dir = fixture_dir("capture-less");
    let path = dir.join("b.md").display().to_string();
    let out = run(&[
        "(paragraph)",
        "-r",
        "REPLACED",
        "--write",
        &dir.display().to_string(),
    ]);
    ok(&out);
    assert!(
        stderr(&out).contains(&diff(
            &path,
            "@@ -1,3 +1,2 @@\n-hello world.\n-\n-second para.\n+REPLACED\n+REPLACED\n\\ No newline at end of file\n"
        )),
        "{}",
        stderr(&out)
    );
    assert_eq!(read(&dir, "b.md"), "REPLACED\nREPLACED");
    assert_eq!(read(&dir, "a.rs"), "fn main() {}\n", "no match, no change");
}

#[test]
fn an_unknown_placeholder_errors_without_writing() {
    let dir = fixture_dir("unknown-placeholder");
    let out = run(&[
        "(function_item name: (identifier) @name)",
        "-r",
        "%nope x",
        "--write",
        &dir.display().to_string(),
    ]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("placeholder %nope is not defined"), "{}", err);
    assert_eq!(
        err.lines().filter(|l| l.contains("is not defined")).count(),
        1,
        "only the language the query actually ran on is blamed:\n{err}"
    );
    assert_eq!(stdout(&out), String::new());
    assert_eq!(read(&dir, "a.rs"), "fn main() {}\n", "nothing was written");
    assert_eq!(read(&dir, "b.md"), "hello world.\n\nsecond para.\n");
}

#[test]
fn a_file_with_overlapping_matches_is_skipped_with_an_error() {
    let dir = fixture_dir("overlap");
    std::fs::write(dir.join("c.rs"), "fn a() {\n    // inner comment\n}\n").unwrap();
    let path = dir.join("c.rs").display().to_string();
    let out = run(&[
        "(function_item) @f (line_comment) @c",
        "-r",
        "X",
        "--write",
        &dir.display().to_string(),
    ]);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains(&format!(
            "{path}: skipped: match regions 0..31 and 13..29 overlap"
        )),
        "{}",
        stderr(&out)
    );
    assert_eq!(
        read(&dir, "c.rs"),
        "fn a() {\n    // inner comment\n}\n",
        "the overlapping file was skipped, not rewritten"
    );
    // The other rust file has a single clean match and is still rewritten.
    assert_eq!(read(&dir, "a.rs"), "X\n");
}

#[test]
fn replace_composes_with_langs() {
    let dir = fixture_dir("langs");
    let path = dir.join("b.md").display().to_string();
    let out = run(&[
        "(paragraph)",
        "-r",
        "HI",
        "--langs",
        "markdown",
        "--write",
        &dir.display().to_string(),
    ]);
    ok(&out);
    assert!(
        stderr(&out).contains(&diff(
            &path,
            "@@ -1,3 +1,2 @@\n-hello world.\n-\n-second para.\n+HI\n+HI\n\\ No newline at end of file\n"
        )),
        "{}",
        stderr(&out)
    );
    assert_eq!(read(&dir, "b.md"), "HI\nHI");
    assert_eq!(
        read(&dir, "a.rs"),
        "fn main() {}\n",
        "rust was filtered out and stayed untouched"
    );
}

/// Markdown runs both of its split grammars; on a plain-text file both
/// report the same text but their regions differ by the trailing newline,
/// so the regions overlap and -r must skip the file rather than guess.
#[test]
fn markdown_dual_grammar_overlap_skips_the_file() {
    let dir = fixture_dir("md-overlap");
    std::fs::write(dir.join("note.md"), "second note\n").unwrap();
    let path = dir.join("note.md").display().to_string();
    let out = run(&[
        "(inline) @i",
        "-r",
        "X",
        "--write",
        &dir.display().to_string(),
    ]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(
        err.contains(&format!(
            "{path}: skipped: match regions 0..11 and 0..12 overlap"
        )),
        "{}",
        err
    );
    assert_eq!(
        read(&dir, "note.md"),
        "second note\n",
        "skipped, not rewritten"
    );
}

// json-plus-r documents that the JSON output is suppressed and the diff/
// writes happen instead; a lone --write without -r is an error.
#[test]
fn json_with_replace_still_diffs_and_write_alone_is_an_error() {
    let dir = fixture_dir("json-plus-r");
    let path = dir.join("a.rs").display().to_string();
    let out = run(&[
        "(function_item name: (identifier) @name)",
        "-r",
        "%name",
        "--json",
        &dir.display().to_string(),
    ]);
    ok(&out);
    assert_eq!(
        stdout(&out),
        diff(&path, "@@ -1 +1 @@\n-fn main() {}\n+main\n"),
        "no NDJSON lines: the diff replaced them"
    );
    let out = run(&["(paragraph) @p", "--write", &dir.display().to_string()]);
    assert!(!out.status.success());
    assert_eq!(stdout(&out), String::new());
    assert!(
        stderr(&out).contains("--write does nothing without -r"),
        "{}",
        stderr(&out)
    );
}
