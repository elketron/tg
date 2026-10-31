//! State the parallel walkers share; plain data with no impl blocks, so
//! both types may live in one file.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Mutex;

use crate::lang::Language;
use crate::report::{Counts, Hit};

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
}
