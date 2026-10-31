//! End-to-end through the real binary: grep-style output grouped by file,
//! per-language errors for queries a grammar cannot compile, and a clear
//! error exit when the query compiled against nothing.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tree")
}

fn run(query: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tg"))
        .arg(query)
        .arg(fixture_root())
        .output()
        .expect("tg runs")
}

fn join(lines: &[&str]) -> String {
    let root = fixture_root();
    let r = |name: &str| root.join(name).display().to_string();
    lines
        .iter()
        .map(|l| {
            let rendered = l
                .replace("README.md", &r("README.md"))
                .replace("sub/note.md", &r("sub/note.md"))
                .replace("main.rs", &r("main.rs"))
                .replace("app.ts", &r("app.ts"))
                .replace("Program.cs", &r("Program.cs"));
            rendered + "\n"
        })
        .collect()
}

#[test]
fn markdown_query_prints_grouped_matches() {
    let out = run("(paragraph) @p");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8(out.stdout),
        Ok(join(&[
            "README.md",
            "  3: Some *markdown*.",
            "  5: Second paragraph.",
            "",
            "sub/note.md",
            "  1: second note",
        ]))
    );
    // The query is invalid for the other three languages, which all have
    // candidate files: each reports its own error on stderr, markdown does
    // not (its block grammar compiled the query and ran it).
    let stderr = String::from_utf8(out.stderr).unwrap();
    for lang in ["rust", "typescript", "csharp"] {
        assert!(
            stderr.contains(&format!("tg: {lang}: query error:")),
            "missing per-language error for {lang} in:\n{stderr}"
        );
    }
    assert!(
        !stderr.contains("markdown: query error"),
        "markdown ran the query and must not be reported:\n{stderr}"
    );
}

#[test]
fn rust_query_prints_grouped_matches() {
    let out = run("(function_item name: (identifier) @n)");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8(out.stdout),
        Ok(join(&["main.rs", "  1: main"]))
    );
}

#[test]
fn typescript_query_prints_grouped_matches() {
    let out = run("(lexical_declaration) @x");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8(out.stdout),
        Ok(join(&["app.ts", "  1: const x = 1;"]))
    );
}

#[test]
fn csharp_query_prints_grouped_matches() {
    let out = run("(method_declaration) @m");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8(out.stdout),
        Ok(join(&["Program.cs", "  1: static void Main() {}"]))
    );
}

/// `(inline)` compiles in markdown's block grammar *and* the inline grammar,
/// so both run over each markdown file; the block grammar sees the heading's
/// inline and the paragraphs, the inline grammar re-reports the whole file as
/// one inline node. The exact output pins the union: hits from the two
/// grammars dedupe and sort by line then text.
#[test]
fn markdown_dual_grammar_union_is_deduped_and_sorted() {
    let out = run("(inline) @i");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8(out.stdout),
        Ok(join(&[
            "README.md",
            "  1: # Fixture",
            "  1: Fixture",
            "  3: Some *markdown*.",
            "  5: Second paragraph.",
            "",
            "sub/note.md",
            "  1: second note",
        ]))
    );
}

#[test]
fn invalid_query_exits_with_a_clear_error() {
    let out = run("(nope) @x");
    assert!(!out.status.success());
    assert_eq!(String::from_utf8(out.stdout), Ok(String::new()));
    let stderr = String::from_utf8(out.stderr).unwrap();
    for lang in ["markdown", "rust", "typescript", "csharp"] {
        assert!(
            stderr.contains(&format!("tg: {lang}: query error:")),
            "missing per-language error for {lang} in:\n{stderr}"
        );
    }
    assert!(
        stderr.contains("tg: the query compiled against none of the languages"),
        "missing overall error in:\n{stderr}"
    );
}
