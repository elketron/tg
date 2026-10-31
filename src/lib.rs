//! tg: walk a tree with the `ignore` crate (ripgrep's rules), parse every
//! file whose extension maps to a supported grammar, and run the query's
//! compiled grammars over it in parallel.

pub mod lang;
pub mod report;
mod shared;
pub mod template;

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use ignore::{WalkBuilder, WalkState};
use lang::Language;
use report::{Change, Edit, Group, Hit, NamedError, Report};
use shared::Shared;
use streaming_iterator::StreamingIterator;
use template::{MATCH_CAPTURE, Template};

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

/// How a run walks and which languages it runs. `Default` is the tool's
/// defaults: every supported language, ignore files respected, hidden files
/// skipped (ripgrep's walking rules).
#[derive(Debug, Default, Clone)]
pub struct Options {
    /// Restrict the run to these languages: the query is compiled only
    /// against them, and files of every other supported language are
    /// skipped and counted as skipped (like unknown extensions). Empty
    /// means every supported language.
    pub langs: Vec<Language>,
    /// Do not respect ignore files (`.gitignore`, `.ignore`, ...) when
    /// walking.
    pub no_ignore: bool,
    /// Include hidden files and directories when walking.
    pub hidden: bool,
    /// When set, each match's matched region is replaced by the rendered
    /// template; the report then carries old/new content per changed file
    /// instead of match groups.
    pub template: Option<Template>,
}

/// Walk `dir` (respecting `opts`: by default gitignore/ignore-file rules on
/// and hidden files off, matching ripgrep), parse every candidate file with
/// its grammar, and collect the query's matches. Parse failure here means a
/// read error or a grammar that cannot be set; unparseable content itself
/// does not fail — tree-sitter reports an ERROR-tree root instead.
pub fn run(dir: &Path, query_src: &str, opts: &Options) -> Report {
    let shared = Arc::new(Shared {
        groups: Mutex::new(BTreeMap::new()),
        seen: Mutex::new(Default::default()),
        per_language: Mutex::new(HashMap::new()),
        other_skipped: Mutex::new(0),
        parse_errors: Mutex::new(Vec::new()),
        edits: Mutex::new(Default::default()),
    });
    let langs: Arc<Vec<Language>> = Arc::new(if opts.langs.is_empty() {
        lang::LANGUAGES.to_vec()
    } else {
        opts.langs.clone()
    });
    let query_src = match &opts.template {
        Some(_) => Cow::Owned(append_capture(query_src, MATCH_CAPTURE)),
        None => with_text_capture(query_src),
    };
    let (compiled, compile_errors) = compile(&query_src, &langs);
    let mut builder = WalkBuilder::new(dir);
    builder.hidden(!opts.hidden);
    if opts.no_ignore {
        builder
            .parents(false)
            .ignore(false)
            .git_ignore(false)
            .git_global(false)
            .git_exclude(false);
    }
    let (sh, cp, cl, tp) = (
        Arc::clone(&shared),
        Arc::clone(&compiled),
        Arc::clone(&langs),
        opts.template.clone(),
    );
    builder.build_parallel().run(move || {
        let (shared, compiled, langs) = (Arc::clone(&sh), Arc::clone(&cp), Arc::clone(&cl));
        let template = tp.clone();
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
            if !langs.contains(&language) {
                *shared.other_skipped.lock().unwrap() += 1;
                return WalkState::Continue;
            }
            shared.seen.lock().unwrap().insert(language);
            process_file(
                path,
                language,
                &shared,
                &compiled,
                &mut parsers,
                &mut cursor,
                template.as_ref(),
            );
            WalkState::Continue
        })
    });
    // Read the tallies through the locks rather than unwrapping the Arcs:
    // the visitor closures holding clones may outlive the walk's loop.
    let mut report = assemble(&shared, &compiled, &compile_errors);
    if let Some(template) = &opts.template {
        finalize_rewrites(&shared, template, &compiled, &mut report);
    }
    report
}

/// Tree-sitter reports matches for capture-less patterns with an empty
/// capture list and no accessor for the matched node, so such a query
/// could print nothing. When the query has no captures at all, append
/// ` @text` after every top-level pattern so the matched node's text
/// lands in the JSON output's `text` field (and the grep-style output's
/// text slot). A `@` only counts as a real capture outside strings and `;`
/// comments, and so do the parens that decide pattern ends; a stray
/// top-level `)` (invalid query, reported by the compiler) does not end a
/// pattern.
fn with_text_capture(query_src: &str) -> Cow<'_, str> {
    let (names, ends) = scan_query(query_src);
    if !names.is_empty() || ends.is_empty() {
        return Cow::Borrowed(query_src);
    }
    Cow::Owned(insert_captures(query_src, &ends, "text"))
}

/// Append ` @name` after every top-level pattern unconditionally (the -r
/// path: even a query that already has captures needs the whole matched
/// node reachable, so every pattern gets the internal match capture too).
fn append_capture(query_src: &str, name: &str) -> String {
    let ends = scan_query(query_src).1;
    insert_captures(query_src, &ends, name)
}

/// Scan a query text for its real capture names and the end offsets of its
/// top-level patterns (the byte offset just past each closing top-level
/// `)`); a `@` or paren inside a string or `;` comment does not count, and
/// a stray top-level `)` (invalid query, reported by the compiler) does
/// not open a pattern.
fn scan_query(query_src: &str) -> (Vec<String>, Vec<usize>) {
    let mut names: Vec<String> = Vec::new();
    let mut ends: Vec<usize> = Vec::new();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    let mut in_comment = false;
    let mut in_name = false;
    let mut name = String::new();
    for (offset, c) in query_src.char_indices() {
        if in_comment {
            in_comment = c != '\n';
            continue;
        }
        if in_string {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        if in_name {
            if c.is_alphanumeric() || c == '_' {
                name.push(c);
                continue;
            }
            in_name = false;
            names.push(std::mem::take(&mut name));
        }
        match c {
            '@' => in_name = true,
            '"' => in_string = true,
            ';' => in_comment = true,
            '(' => depth += 1,
            ')' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    ends.push(offset + c.len_utf8());
                }
            }
            _ => {}
        }
    }
    if in_name {
        names.push(name);
    }
    (names, ends)
}

/// Insert ` @name` at each of the given offsets (ascending, as produced
/// by `scan_query`).
fn insert_captures(query_src: &str, ends: &[usize], name: &str) -> String {
    let mut out = String::with_capacity(query_src.len() + ends.len() * (name.len() + 2));
    let mut prev = 0;
    for &end in ends {
        out.push_str(&query_src[prev..end]);
        out.push_str(" @");
        out.push_str(name);
        prev = end;
    }
    out.push_str(&query_src[prev..]);
    out
}

fn compile(query_src: &str, langs: &[Language]) -> (Arc<Compiled>, CompileErrors) {
    let mut compiled: Compiled = HashMap::new();
    let mut errors: CompileErrors = HashMap::new();
    for &language in langs {
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
    template: Option<&Template>,
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
            run_matches(cursor, query, &tree, &bytes, path, shared, template);
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
    template: Option<&Template>,
) {
    let mut matches = cursor.matches(query, tree.root_node(), bytes);
    let mut found = Vec::new();
    let mut edits: Vec<Edit> = Vec::new();
    while let Some(m) = matches.next() {
        let mut captures: Vec<(String, String)> = Vec::new();
        let mut line = 0;
        let mut region: Option<(usize, usize)> = None;
        for c in m.captures() {
            let Ok(text) = c.node.utf8_text(bytes) else {
                continue;
            };
            if captures.is_empty() {
                line = c.node.start_position().row + 1;
            }
            let name = &query.capture_names()[c.index as usize];
            if template.is_some() && name == &MATCH_CAPTURE {
                region = Some((c.node.start_byte(), c.node.end_byte()));
            }
            captures.push((name.to_string(), text.to_string()));
        }
        if captures.is_empty() {
            continue;
        }
        match template {
            None => found.push(Hit { line, captures }),
            Some(template) => edits.extend(render_edit(template, &captures, region)),
        }
    }
    if !found.is_empty() {
        let mut guard = shared.groups.lock().unwrap();
        let hits = guard.entry(path.to_path_buf()).or_default();
        hits.extend(found);
    }
    if !edits.is_empty() {
        let mut guard = shared.edits.lock().unwrap();
        let entry = guard
            .entry(path.to_path_buf())
            .or_insert_with(|| (bytes.to_vec(), Default::default()));
        entry.1.extend(edits);
    }
}

/// One match's edit: the rendered template for its matched region. A
/// missing region cannot happen in a rewriting run (every top-level
/// pattern got the match capture), so it yields no edit.
fn render_edit(
    template: &Template,
    captures: &[(String, String)],
    region: Option<(usize, usize)>,
) -> Option<Edit> {
    let (start, end) = region?;
    let lookup = |name: &str| {
        captures
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, text)| text.clone())
    };
    Some(Edit {
        start,
        end,
        replacement: template.render(lookup),
    })
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

/// Check the template's placeholders against the queries actually run
/// (languages without candidate files cannot fail the run), then rebuild
/// each matched file's content. A file whose match regions overlap (two
/// patterns nested, or markdown's two grammars disagreeing) is skipped
/// rather than guessed at. A template error fails the whole run: no file
/// changes and nothing is written. Files whose replacements change nothing
/// are left out of the report.
fn finalize_rewrites(
    shared: &Shared,
    template: &Template,
    compiled: &Compiled,
    report: &mut Report,
) {
    let references = template.references();
    if !references.is_empty() {
        let mut seen: Vec<Language> = report.per_language.keys().copied().collect();
        seen.sort();
        for language in &seen {
            let names: Vec<&str> = language
                .grammars()
                .iter()
                .filter_map(|(grammar, _)| compiled.get(*grammar))
                .flat_map(|q| q.capture_names().iter().copied())
                .map(|n| n as &str)
                .collect();
            // A language whose query could not compile never ran and can
            // never match; its compile error is reported instead, not a
            // template error about captures it cannot have.
            if names.is_empty() {
                continue;
            }
            for name in &references {
                if !names.contains(&name.as_str()) {
                    report.template_errors.push(format!(
                        "template: placeholder %{name} is not defined by the query running \
                         on {}; add the capture to the query or drop the placeholder",
                        language.name()
                    ));
                }
            }
        }
        report.template_errors.sort();
        if !report.template_errors.is_empty() {
            return;
        }
    }
    let edits = shared.edits.lock().unwrap();
    for (path, (bytes, set)) in edits.iter() {
        let edits: Vec<Edit> = set.iter().cloned().collect();
        // Sorted by range; an overlap always shows between neighbours.
        if let Some(pair) = overlapping_pair(&edits) {
            report.skips.push(NamedError {
                name: path.display().to_string(),
                error: format!(
                    "skipped: match regions {}..{} and {}..{} overlap; \
                     the query must match disjoint regions for -r",
                    pair[0].start, pair[0].end, pair[1].start, pair[1].end
                ),
            });
            continue;
        }
        let Ok(old) = std::str::from_utf8(bytes) else {
            report.skips.push(NamedError {
                name: path.display().to_string(),
                error: "skipped: file is not valid UTF-8 and cannot be rewritten".to_string(),
            });
            continue;
        };
        let new = apply_edits(old, &edits);
        if new != old {
            report.changes.push(Change {
                path: path.clone(),
                old: old.to_string(),
                new,
            });
        }
    }
}

fn overlapping_pair(edits: &[Edit]) -> Option<&[Edit]> {
    edits.windows(2).find(|pair| pair[1].start < pair[0].end)
}

fn apply_edits(old: &str, edits: &[Edit]) -> String {
    let mut out = String::with_capacity(old.len());
    let mut pos = 0;
    for edit in edits {
        out.push_str(&old[pos..edit.start]);
        out.push_str(&edit.replacement);
        pos = edit.end;
    }
    out.push_str(&old[pos..]);
    out
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
        changes: Vec::new(),
        skips: Vec::new(),
        template_errors: Vec::new(),
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

#[cfg(test)]
mod tests {
    use super::{append_capture, with_text_capture};

    #[test]
    fn append_capture_adds_it_to_every_top_level_pattern() {
        // The -r path appends even when the query already has captures.
        assert_eq!(append_capture("(a) @x", "match"), "(a) @match @x");
        // A paren inside a string does not end the pattern early.
        assert_eq!(append_capture("(a \"b\")", "match"), "(a \"b\") @match");
        assert_eq!(
            append_capture("foo)", "match"),
            "foo)",
            "stray paren: no pattern"
        );
    }

    #[test]
    fn a_real_capture_skips_the_rewrite() {
        assert_eq!(with_text_capture("(paragraph) @p"), "(paragraph) @p");
    }

    #[test]
    fn capture_less_queries_get_a_text_capture_per_top_level_pattern() {
        assert_eq!(with_text_capture("(paragraph)"), "(paragraph) @text");
        assert_eq!(
            with_text_capture("(a)\n(b)"),
            "(a) @text\n(b) @text",
            "every top-level pattern gets its own text capture"
        );
        assert_eq!(
            with_text_capture("((a) (b))"),
            "((a) (b)) @text",
            "nested parens do not end the pattern"
        );
    }

    #[test]
    fn strings_and_comments_do_not_hide_or_fake_captures_and_parens() {
        // A paren inside a string does not close the pattern early.
        assert_eq!(with_text_capture("(eq \"a)b\")"), "(eq \"a)b\") @text");
        // An @ inside a comment is not a real capture: the rewrite runs.
        assert_eq!(
            with_text_capture("(paragraph) ; see @x"),
            "(paragraph) @text ; see @x"
        );
        // A paren inside a comment does not close a pattern.
        assert_eq!(
            with_text_capture("(foo) ; )\n(b)"),
            "(foo) @text ; )\n(b) @text"
        );
    }

    #[test]
    fn a_stray_top_level_paren_does_not_panic_the_rewriter() {
        // Invalid query (the compiler reports it); the rewriter must leave
        // it alone rather than underflow its depth counter.
        assert_eq!(with_text_capture("foo)"), "foo)");
        assert_eq!(with_text_capture("a) b)"), "a) b)");
    }
}
