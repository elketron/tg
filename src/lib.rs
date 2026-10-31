//! tg: walk a tree with the `ignore` crate (ripgrep's rules) and parse every
//! file whose extension maps to a supported grammar.

pub mod lang;

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use ignore::{WalkBuilder, WalkState};
use lang::Language;

/// Files parsed ok / failed, per language.
#[derive(Default, Debug, PartialEq, Eq, Clone, Copy)]
pub struct Counts {
    pub ok: usize,
    pub failed: usize,
}

/// Parse counts per language, plus one skipped total for extensions with no
/// grammar.
#[derive(Default, Debug, PartialEq, Eq)]
pub struct Report {
    pub per_language: HashMap<Language, Counts>,
    pub other_skipped: usize,
}

/// Walk `dir` (gitignore/ignore-file rules and hidden-file rules on, matching
/// ripgrep's defaults) and parse every candidate file with its grammar.
pub fn run(dir: &Path) -> Report {
    let per_language: Arc<Mutex<HashMap<Language, Counts>>> = Arc::new(Mutex::new(HashMap::new()));
    let other_skipped: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));
    let (pl, sk) = (Arc::clone(&per_language), Arc::clone(&other_skipped));
    WalkBuilder::new(dir).build_parallel().run(move || {
        let (pl, sk) = (Arc::clone(&pl), Arc::clone(&sk));
        let mut parsers: HashMap<Language, Option<tree_sitter::Parser>> = HashMap::new();
        Box::new(move |result| {
            let Ok(entry) = result else {
                return WalkState::Continue;
            };
            if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                return WalkState::Continue;
            }
            let path = entry.path();
            let Some(language) = lang::language_for(path) else {
                *sk.lock().unwrap() += 1;
                return WalkState::Continue;
            };
            let parser = parsers.entry(language).or_insert_with(|| {
                let mut p = tree_sitter::Parser::new();
                p.set_language(&language.grammar()).ok().map(|()| p)
            });
            let ok = match parser {
                Some(p) => match fs::read(path) {
                    Ok(bytes) => p.parse(&bytes, None).is_some(),
                    Err(_) => false,
                },
                None => false,
            };
            let mut guard = pl.lock().unwrap();
            let c = guard.entry(language).or_default();
            if ok {
                c.ok += 1;
            } else {
                c.failed += 1;
            }
            WalkState::Continue
        })
    });
    let per_language = Arc::try_unwrap(per_language).unwrap().into_inner().unwrap();
    let other_skipped = Arc::try_unwrap(other_skipped)
        .unwrap()
        .into_inner()
        .unwrap();
    Report {
        per_language,
        other_skipped,
    }
}
