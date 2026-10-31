//! End-to-end through the library: walk the fixture tree and check
//! per-language parse counts. Queries here compile against every grammar
//! (so nothing is reported on stderr) but match nothing.

use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tg::lang;
use tg::report::{Counts, Report};

/// A query every grammar understands; matches nothing in the fixtures.
const NEUTRAL_QUERY: &str = "(ERROR) @x";

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tree")
}

#[test]
fn fixture_tree_parse_counts() {
    let report = tg::run(&fixture_root(), NEUTRAL_QUERY, &tg::Options::default());
    let mut want = HashMap::new();
    want.insert(lang::Language::Markdown, Counts { ok: 2, failed: 0 });
    want.insert(lang::Language::Rust, Counts { ok: 1, failed: 0 });
    want.insert(lang::Language::TypeScript, Counts { ok: 2, failed: 0 });
    want.insert(lang::Language::CSharp, Counts { ok: 1, failed: 0 });
    assert_eq!(
        report,
        Report {
            groups: Vec::new(),
            query_errors: Vec::new(),
            parse_errors: Vec::new(),
            per_language: want,
            other_skipped: 1,
            fatal: false,
        }
    );
}

/// Unparseable content is not enough to make tree-sitter fail (it reports an
/// ERROR-tree root and parses "fine"), and git can't commit an unreadable
/// file, so this test builds a temp tree: one readable and one unreadable
/// `.rs`. The unreadable one must land in `failed` (and `parse_errors`), not
/// vanish from counts.
#[test]
fn unreadable_supported_file_counts_as_failed() {
    let dir = std::env::temp_dir().join(format!("tg-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let unreadable = dir.join("broken.rs");
    std::fs::write(&unreadable, "fn main() {}\n").unwrap();
    let ok_file = dir.join("ok.rs");
    std::fs::write(&ok_file, "fn main() {}\n").unwrap();
    std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read(&unreadable).is_ok() {
        // Running as root: permissions don't block reads, nothing to assert.
        eprintln!("skipping: running as root, chmod 000 still readable");
        return;
    }
    let report = tg::run(&dir, NEUTRAL_QUERY, &tg::Options::default());
    assert_eq!(
        report.per_language.get(&lang::Language::Rust),
        Some(&Counts { ok: 1, failed: 1 })
    );
    assert_eq!(report.parse_errors, vec![unreadable.clone()]);
    std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o644)).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A query that compiles in markdown's block grammar only: the language ran
/// the query (and found matches), so its inline-grammar failure is not
/// reported; the other languages each report once.
#[test]
fn partial_compile_reports_only_fully_failed_languages() {
    let report = tg::run(&fixture_root(), "(paragraph) @p", &tg::Options::default());
    let names: Vec<&str> = report
        .query_errors
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(names, ["csharp", "rust", "typescript"]);
    assert!(!report.fatal);
    assert_eq!(report.groups.len(), 2, "both markdown fixtures match");
}
