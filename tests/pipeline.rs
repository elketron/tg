//! End-to-end: walk the fixture tree and check per-language parse counts.

use std::collections::HashMap;
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
