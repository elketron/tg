//! End-to-end through the real binary with `--json`: NDJSON, one object per
//! match, `file` and `line` plus one field per named capture; a query with
//! no captures reports each matched node's text as `text`. The default
//! grep-style output is unchanged (covered by tests/output.rs). Every line
//! of stdout is parsed as JSON here — that's the contract.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tree")
}

fn run(query: &str, json: bool) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_tg"));
    cmd.arg(query);
    if json {
        cmd.arg("--json");
    }
    cmd.arg(fixture_root()).output().expect("tg runs")
}

/// Parse stdout as NDJSON: every non-empty line must be one JSON object.
fn ndjson(out: &Output) -> Vec<serde_json::Value> {
    let stdout = String::from_utf8(out.stdout.clone()).expect("utf8 stdout");
    stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str(l).expect("valid JSON per line"))
        .collect()
}

fn p(name: &str) -> String {
    fixture_root().join(name).display().to_string()
}

fn ok(out: &Output) {
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn rust_query_with_named_captures_emits_json() {
    let out = run("(function_item name: (identifier) @name) @fn", true);
    ok(&out);
    assert_eq!(
        ndjson(&out),
        vec![serde_json::json!({
            "file": p("main.rs"),
            "line": 1,
            "fn": "fn main() {}",
            "name": "main",
        })]
    );
}

#[test]
fn typescript_query_with_named_captures_emits_json() {
    let out = run(
        "(lexical_declaration (variable_declarator name: (identifier) @name) @decl)",
        true,
    );
    ok(&out);
    assert_eq!(
        ndjson(&out),
        vec![serde_json::json!({
            "file": p("app.ts"),
            "line": 1,
            "decl": "x = 1",
            "name": "x",
        })]
    );
}

#[test]
fn csharp_query_with_named_captures_emits_json() {
    let out = run("(class_declaration name: (identifier) @class) @decl", true);
    ok(&out);
    assert_eq!(
        ndjson(&out),
        vec![serde_json::json!({
            "file": p("Program.cs"),
            "line": 1,
            "decl": "class Program { static void Main() {} }",
            "class": "Program",
        })]
    );
}

/// A query with no captures: tg appends a `text` capture per top-level
/// pattern, so each match reports the matched node's full text. Only
/// markdown's block grammar compiles `(paragraph)`; the other languages
/// report per-language errors on stderr and the run succeeds.
#[test]
fn json_without_captures_reports_node_text() {
    let out = run("(paragraph)", true);
    ok(&out);
    assert_eq!(
        ndjson(&out),
        vec![
            serde_json::json!({"file": p("README.md"), "line": 3, "text": "Some *markdown*.\n"}),
            serde_json::json!({"file": p("README.md"), "line": 5, "text": "Second paragraph.\n"}),
            serde_json::json!({"file": p("sub/note.md"), "line": 1, "text": "second note\n"}),
        ]
    );
}

/// The grep-style shape is unchanged for capture-bearing queries; for
/// capture-LESS ones this is new behavior: on master such queries printed
/// nothing (matches have no captures to report), and the ` @text` rewrite
/// makes them print now. `(paragraph)` compiles in markdown's block
/// grammar only, so nothing double-reports here — unlike the `(inline)`
/// case in tests/output.rs.
#[test]
fn no_capture_query_prints_grep_style_matches() {
    let out = run("(paragraph)", false);
    ok(&out);
    let stdout = String::from_utf8(out.stdout).expect("utf8 stdout");
    assert_eq!(
        stdout,
        format!(
            "{p}\n  3: Some *markdown*.\n  5: Second paragraph.\n\n{q}\n  1: second note\n",
            p = p("README.md"),
            q = p("sub/note.md")
        )
    );
}
/// `file` and `line` are contract fields: a capture stealing either name
/// is overridden by them (inserted last in main's JSON print), so the
/// capture's own value is lost — documented in `--help`.
#[test]
fn capture_named_file_or_line_loses_to_the_contract_fields() {
    let out = run("(paragraph) @file", true);
    ok(&out);
    assert_eq!(
        ndjson(&out),
        vec![
            serde_json::json!({"file": p("README.md"), "line": 3}),
            serde_json::json!({"file": p("README.md"), "line": 5}),
            serde_json::json!({"file": p("sub/note.md"), "line": 1}),
        ]
    );
}

/// A stray top-level `)` is invalid for every grammar; the rewriter must
/// not panic on it (debug builds used to overflow the depth counter) and
/// the query must fail like any other invalid one.
#[test]
fn stray_paren_reports_compile_errors_instead_of_panicking() {
    let out = run("foo)", true);
    assert!(!out.status.success());
    assert_eq!(String::from_utf8(out.stdout).expect("utf8"), String::new());
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        stderr.contains("query error"),
        "expected per-language compile errors in:\n{stderr}"
    );
    assert!(!stderr.contains("panicked"), "rewriter panicked:\n{stderr}");
}
