//! State the parallel walkers share; plain data with no impl blocks, so
//! both types may live in one file.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Mutex;

use crate::lang::Language;
use crate::report::{Counts, Edit, Hit};

/// Languages that had at least one candidate file, so query-compile errors
/// are only reported for these. A `BTreeSet` keeps the report deterministic;
/// `Language` needs `Ord` for it.
pub type Seen = BTreeSet<Language>;

/// Everything the walker threads accumulate behind locks. Paths and hits go
/// in `BTree` containers so the report comes out in a deterministic order.
pub struct Shared {
    pub groups: Mutex<BTreeMap<PathBuf, BTreeSet<Hit>>>,
    pub seen: Mutex<Seen>,
    pub per_language: Mutex<HashMap<Language, Counts>>,
    pub other_skipped: Mutex<usize>,
    pub parse_errors: Mutex<Vec<PathBuf>>,
    pub edits: Mutex<Edits>,
}

/// Files with at least one -r edit: the file's full bytes (the rewrite
/// rebuilds them) and the rendered edits. The BTreeSet sorts edits by
/// range and collapses exact duplicates; a file whose regions overlap with
/// different replacements is skipped by the rewrite rather than guessed
/// at (markdown's two grammars reporting the same node at end of file
/// differ by the trailing newline, so they overlap and skip).
pub type Edits = BTreeMap<PathBuf, (Vec<u8>, BTreeSet<Edit>)>;
