//! tg: walk a tree with the `ignore` crate (ripgrep's rules), parse every
//! file whose extension maps to a supported grammar, and run the query's
//! compiled grammars over it in parallel.

pub mod lang;
pub mod report;
mod shared;

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use ignore::{WalkBuilder, WalkState};
use lang::Language;
use report::{Group, Hit, NamedError, Report};
use shared::Shared;
use streaming_iterator::StreamingIterator;

/// The query compiled once per grammar up front; `tree_sitter::Query` is
/// `Send + Sync`, so every walker thread can share it. Maps grammar name ->
/// query.
type Compiled = HashMap<&'static str, tree_sitter::Query>;

/// Languages the query failed to compile against, with the error message
/// verbatim. Recorded per grammar for languages with several (markdown); a
/// language that compiled on at least one grammar runs the query and is not
/// reported.
type CompileErrors = HashMap<Language, String>;

/// Per-thread parser caches, made once per walker thread. `None` when the
/// grammar cannot be set on a parser.
type Parsers = HashMap<&'static str, Option<tree_sitter::Parser>>;

/// Walk `dir` (gitignore/ignore-file rules and hidden-file rules on, matching
/// ripgrep's defaults), parse every candidate file with its grammar, and
/// collect the query's matches. Parse failure here means a read error or a
/// grammar that cannot be set; unparseable content itself does not fail —
/// tree-sitter reports an ERROR-tree root instead.
pub fn run(dir: &Path, query_src: &str) -> Report {
    let shared = Arc::new(Shared {
        groups: Mutex::new(BTreeMap::new()),
        seen: Mutex::new(Default::default()),
        per_language: Mutex::new(HashMap::new()),
        other_skipped: Mutex::new(0),
        parse_errors: Mutex::new(Vec::new()),
    });
    let (compiled, compile_errors) = compile(&with_text_capture(query_src));
    let (sh, cp) = (Arc::clone(&shared), Arc::clone(&compiled));
    WalkBuilder::new(dir).build_parallel().run(move || {
        let (shared, compiled) = (Arc::clone(&sh), Arc::clone(&cp));
        let mut parsers: Parsers = HashMap::new();
        let mut cursor = tree_sitter::QueryCursor::new();
        Box::new(move |result| {
            let Ok(entry) = result else {
                return WalkState::Continue;
            };
            if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                return WalkState::Continue;
            }
            let path = entry.path();
            let Some(language) = lang::language_for(path) else {
                *shared.other_skipped.lock().unwrap() += 1;
                return WalkState::Continue;
            };
            shared.seen.lock().unwrap().insert(language);
            process_file(
                path,
                language,
                &shared,
                &compiled,
                &mut parsers,
                &mut cursor,
            );
            WalkState::Continue
        })
    });
    // Read the tallies through the locks rather than unwrapping the Arcs:
    // the visitor closures holding clones may outlive the walk's loop.
    assemble(&shared, &compiled, &compile_errors)
}

/// Tree-sitter reports matches for capture-less patterns with an empty
/// capture list and no accessor for the matched node, so such a query
/// could print nothing. When the query has no captures at all, append
/// ` @text` after every top-level pattern so the matched node's text
/// lands in the JSON output's `text` field (and the grep-style output's
/// text slot). Strings and `;` comments are respected so their parens
/// don't count as nesting.
fn with_text_capture(query_src: &str) -> Cow<'_, str> {
    if query_src.contains('@') {
        return Cow::Borrowed(query_src);
    }
    let mut out = String::with_capacity(query_src.len() + 6);
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    let mut in_comment = false;
    for c in query_src.chars() {
        out.push(c);
        if in_comment {
            in_comment = c != '\n';
        } else if in_string {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
        } else {
            match c {
                '"' => in_string = true,
                ';' => in_comment = true,
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        out.push_str(" @text");
                    }
                }
                _ => {}
            }
        }
    }
    Cow::Owned(out)
}

fn compile(query_src: &str) -> (Arc<Compiled>, CompileErrors) {
    let mut compiled: Compiled = HashMap::new();
    let mut errors: CompileErrors = HashMap::new();
    for language in lang::LANGUAGES {
        for (name, grammar) in language.grammars() {
            match tree_sitter::Query::new(&grammar, query_src) {
                Ok(query) => {
                    compiled.insert(name, query);
                }
                Err(e) => {
                    errors.entry(language).or_insert_with(|| e.to_string());
                }
            }
        }
    }
    (Arc::new(compiled), errors)
}

/// Parse one candidate file with every grammar of its language and run the
/// query compiled for each of those grammars over the tree. A language whose
/// query failed to compile is parsed (so parse counts and errors stay
/// per-language true) but produces no matches.
fn process_file(
    path: &Path,
    language: Language,
    shared: &Shared,
    compiled: &Compiled,
    parsers: &mut Parsers,
    cursor: &mut tree_sitter::QueryCursor,
) {
    let Ok(bytes) = fs::read(path) else {
        report_failure(shared, language, path);
        return;
    };
    let mut ok = true;
    // A file counts failed when any grammar of its language could not be
    // set or could not parse it (unparseable content itself does not fail:
    // tree-sitter reports an ERROR-tree root instead).
    for (name, grammar) in language.grammars() {
        let Some(parser) = parser(parsers, name, &grammar) else {
            ok = false;
            continue;
        };
        let Some(tree) = parser.parse(&bytes, None) else {
            ok = false;
            continue;
        };
        if let Some(query) = compiled.get(name) {
            run_matches(cursor, query, &tree, &bytes, path, shared);
        }
    }
    if ok {
        tally(shared, language, true);
    } else {
        report_failure(shared, language, path);
    }
}

fn parser<'a>(
    parsers: &'a mut Parsers,
    name: &'static str,
    grammar: &tree_sitter::Language,
) -> Option<&'a mut tree_sitter::Parser> {
    parsers
        .entry(name)
        .or_insert_with(|| {
            let mut p = tree_sitter::Parser::new();
            p.set_language(grammar).ok().map(|()| p)
        })
        .as_mut()
}

fn run_matches(
    cursor: &mut tree_sitter::QueryCursor,
    query: &tree_sitter::Query,
    tree: &tree_sitter::Tree,
    bytes: &[u8],
    path: &Path,
    shared: &Shared,
) {
    let mut matches = cursor.matches(query, tree.root_node(), bytes);
    let mut found = Vec::new();
    while let Some(m) = matches.next() {
        let mut captures: Vec<(String, String)> = Vec::new();
        let mut line = 0;
        for c in m.captures() {
            let Ok(text) = c.node.utf8_text(bytes) else {
                continue;
            };
            if captures.is_empty() {
                line = c.node.start_position().row + 1;
            }
            let name = query.capture_names()[c.index as usize].to_string();
            captures.push((name, text.to_string()));
        }
        if captures.is_empty() {
            continue;
        }
        found.push(Hit { line, captures });
    }
    if !found.is_empty() {
        let mut guard = shared.groups.lock().unwrap();
        let hits = guard.entry(path.to_path_buf()).or_default();
        hits.extend(found);
    }
}

fn tally(shared: &Shared, language: Language, ok: bool) {
    let mut guard = shared.per_language.lock().unwrap();
    let c = guard.entry(language).or_default();
    if ok {
        c.ok += 1;
    } else {
        c.failed += 1;
    }
}

fn report_failure(shared: &Shared, language: Language, path: &Path) {
    tally(shared, language, false);
    shared.parse_errors.lock().unwrap().push(path.to_path_buf());
}

/// Turn the shared accumulators into the report. Query-compile errors are
/// only reported for languages that actually had candidate files, and only
/// when every grammar of that language failed. The run is fatal when at
/// least one language had files and none could run the query.
fn assemble(shared: &Shared, compiled: &Compiled, errors: &CompileErrors) -> Report {
    let seen = shared.seen.lock().unwrap().clone();
    let mut query_errors: Vec<NamedError> = Vec::new();
    for language in &seen {
        let Some(error) = errors.get(language) else {
            continue;
        };
        // A language that compiled on at least one of its grammars (markdown)
        // ran the query; only languages where every grammar failed are
        // reported. `seen` is ordered by enum variant; errors are sorted by
        // name below.
        if language
            .grammars()
            .iter()
            .any(|(name, _)| compiled.contains_key(name))
        {
            continue;
        }
        query_errors.push(NamedError {
            name: language.name().to_string(),
            error: error.clone(),
        });
    }
    query_errors.sort_by(|a, b| a.name.cmp(&b.name));
    let fatal = !seen.is_empty() && query_errors.len() == seen.len();
    Report {
        groups: shared
            .groups
            .lock()
            .unwrap()
            .iter()
            .map(|(path, hits)| Group {
                path: path.clone(),
                hits: hits.iter().cloned().collect(),
            })
            .collect(),
        query_errors,
        // Sorted: parallel workers append the failures out of order.
        parse_errors: {
            let mut errors = shared.parse_errors.lock().unwrap().clone();
            errors.sort();
            errors
        },
        per_language: shared.per_language.lock().unwrap().clone(),
        other_skipped: *shared.other_skipped.lock().unwrap(),
        fatal,
    }
}
