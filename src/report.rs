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

/// One match: 1-based line of the first captured node, and every capture
/// as (name, captured text) in capture order. Ordered by line, then
/// captures, so hits from two grammars of the same file (markdown)
/// dedupe and sort deterministically.
#[derive(Debug, PartialEq, Eq, Clone, PartialOrd, Ord)]
pub struct Hit {
    pub line: usize,
    pub captures: Vec<(String, String)>,
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

/// One region of a file the `-r` pass will replace: byte range and the
/// rendered replacement. Ordered by range so a file's edits apply in place;
/// exact duplicates (the same region rendering to the same text) collapse.
/// An overlapping region with a different text cannot collapse and makes
/// the run skip the file (it must not guess).
#[derive(Debug, PartialEq, Eq, Clone, PartialOrd, Ord)]
pub struct Edit {
    pub start: usize,
    pub end: usize,
    pub replacement: String,
}

/// A file the `-r` pass changes: content before and after, sorted by path.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Change {
    pub path: PathBuf,
    pub old: String,
    pub new: String,
}

/// A segment of a parsed `-r` template (the `Template` type that owns
/// these lives in `template.rs`; this one is plain data).
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum TemplatePart {
    Literal(String),
    Capture(String),
}

/// Everything a run found, in deterministic order.
#[derive(Debug, PartialEq, Eq, Default, Clone)]
pub struct Report {
    pub groups: Vec<Group>,
    /// Files the `-r` pass changes (sorted by path), content before and
    /// after. Populated only when a template ran; files whose matches
    /// produced no net change are left out.
    pub changes: Vec<Change>,
    /// Files the `-r` pass skipped, sorted by path: `name` is the path,
    /// `error` the reason (overlapping match regions).
    pub skips: Vec<NamedError>,
    /// `-r` template errors that fail the whole run (a placeholder the
    /// query does not define), sorted.
    pub template_errors: Vec<String>,
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
