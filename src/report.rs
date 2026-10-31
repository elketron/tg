//! Data collected by a search run; the binary turns this into output.
//! Every type here is plain data (derives only, per the project's rules).

use std::collections::HashMap;
use std::path::PathBuf;

use crate::lang::Language;

/// Files parsed ok / failed, per language.
#[derive(Default, Debug, PartialEq, Eq, Clone, Copy)]
pub struct Counts {
    pub ok: usize,
    pub failed: usize,
}

/// One match: 1-based line of the first captured node, and that node's
/// first line. Ordered by line, then text, so hits from two grammars of
/// the same file (markdown) dedupe and sort deterministically.
#[derive(Debug, PartialEq, Eq, Clone, PartialOrd, Ord)]
pub struct Hit {
    pub line: usize,
    pub text: String,
}

/// A file with at least one hit, and its hits sorted by line.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Group {
    pub path: PathBuf,
    pub hits: Vec<Hit>,
}

/// A message attributed to one language (the query failed to compile
/// against it) or grammar (markdown's split parser reports per grammar).
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct NamedError {
    pub name: String,
    pub error: String,
}

/// Everything a run found, in deterministic order.
#[derive(Debug, PartialEq, Eq, Default, Clone)]
pub struct Report {
    pub groups: Vec<Group>,
    /// Query compile errors, sorted by language name; only languages that
    /// have candidate files. A language with several grammars reports its
    /// first failing grammar.
    pub query_errors: Vec<NamedError>,
    /// Files whose read or parse failed, sorted.
    pub parse_errors: Vec<PathBuf>,
    pub per_language: HashMap<Language, Counts>,
    pub other_skipped: usize,
    /// True when languages with candidate files exist but the query failed
    /// to compile against every one of them.
    pub fatal: bool,
}
