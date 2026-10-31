//! End-to-end: walk the fixture tree and check per-language parse counts.

use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tg::{Counts, Report, lang};

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tree")
}

#[test]
fn fixture_tree_parse_counts() {
    let report = tg::run(&fixture_root());
    let mut want = HashMap::new();
    want.insert(lang::Language::Markdown, Counts { ok: 2, failed: 0 });
    want.insert(lang::Language::Rust, Counts { ok: 1, failed: 0 });
    want.insert(lang::Language::TypeScript, Counts { ok: 2, failed: 0 });
    want.insert(lang::Language::CSharp, Counts { ok: 1, failed: 0 });
    assert_eq!(
        report,
        Report {
            per_language: want,
            other_skipped: 1
        }
    );
}

/// Unparseable content is not enough to make tree-sitter fail (it reports an
/// ERROR-tree root and parses "fine"), and git can't commit an unreadable
/// file, so this test builds a temp tree: one readable and one unreadable
/// `.rs`. The unreadable one must land in `failed`, not vanish from counts.
#[test]
fn unreadable_supported_file_counts_as_failed() {
    let dir = std::env::temp_dir().join(format!("tg-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let unreadable = dir.join("broken.rs");
    std::fs::write(&unreadable, "fn main() {}\n").unwrap();
    std::fs::write(dir.join("ok.rs"), "fn main() {}\n").unwrap();
    std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read(&unreadable).is_ok() {
        // Running as root: permissions don't block reads, nothing to assert.
        eprintln!("skipping: running as root, chmod 000 still readable");
        return;
    }
    let report = tg::run(&dir);
    assert_eq!(
        report.per_language.get(&lang::Language::Rust),
        Some(&Counts { ok: 1, failed: 1 })
    );
    std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o644)).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
}
