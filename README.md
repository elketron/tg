# tg

ripgrep-style parallel search where the pattern is a [tree-sitter](https://tree-sitter.github.io/tree-sitter/)
query. Zero config, no rule files: give it a plain S-expression query, it infers the language per
file, parses each candidate file and prints the matches. Every example below runs verbatim from the
repository root against the test fixtures.

## Install

```sh
cargo install --path .
# or
cargo build --release && ./target/release/tg --help
```

## Usage

```
tg [OPTIONS] [QUERY] [DIR]
```

The query is a tree-sitter query (S-expression), given as an argument or read from a file with
`--file`; exactly one of the two is required, and with `--file` every positional is a directory
(same interface as `rg -f query.scm dir`). `DIR` is the directory to search (default: current
directory). Supported languages, inferred per file from the extension (case-insensitive): markdown
(`.md`), rust (`.rs`), typescript (`.ts`, `.tsx`), csharp (`.cs`). Files of any other extension are
skipped.

| flag | meaning |
|------|---------|
| `-f, --file <PATH>` | read the query from this file instead of the argument; a lone positional is then the directory (`tg --file query.scm src/`) |
| `--langs <LIST>` | comma-separated languages from `markdown,rust,typescript,csharp`; restricts the run to those, every other supported file is skipped |
| `--no-ignore` | do not respect ignore files (`.gitignore`, `.ignore`, ...) when walking |
| `--hidden` | include hidden files and directories |
| `--json` | print one JSON object per match, one per line (NDJSON); see [JSON output](#json-output) |

File walking follows ripgrep's rules via the `ignore` crate: ignore files respected, hidden files
skipped, both opt-out-able with the flags above.

## Examples

Markdown — every paragraph (from the block grammar; `--file` works the same, `tg --file q.scm .`):

```console
$ tg '(paragraph) @p' tests/fixtures/tree

tests/fixtures/tree/README.md
  3: Some *markdown*.
  5: Second paragraph.

tests/fixtures/tree/sub/note.md
  1: second note
```

Rust — every function, printing its name (the first capture):

```console
$ tg '(function_item name: (identifier) @name)' tests/fixtures/tree

tests/fixtures/tree/main.rs
  1: main
```

TypeScript:

```console
$ tg '(lexical_declaration (variable_declarator name: (identifier) @name) @decl)' tests/fixtures/tree

tests/fixtures/tree/app.ts
  1: const x = 1;
```

C#:

```console
$ tg '(class_declaration name: (identifier) @class) @decl' tests/fixtures/tree

tests/fixtures/tree/Program.cs
  1: class Program { static void Main() {} }
```

Restrict the run to one language (the query then runs only there; a query that is invalid for the
other languages reports nothing for them):

```console
$ tg '(class_declaration name: (identifier) @class) @decl' --langs csharp tests/fixtures/tree

tests/fixtures/tree/Program.cs
  1: class Program { static void Main() {} }
```

The same query written to a file, read back with `--file`:

```console
$ echo '(paragraph) @p' > q.scm
$ tg --file q.scm tests/fixtures/walk

tests/fixtures/walk/visible.md
  3: Visible paragraph.
```

## Output

Default (grep-style): matches go to stdout, grouped by file (paths printed as given, files sorted),
a blank line between files, each hit indented with its line number and the **first line of its first
capture**. Two matches with the same line and identical captures collapse into one — markdown has a
split block/inline grammar and both grammars run over a file, so one hit reported by both may either
collapse (identical captures) or print twice (captures differing only in trailing whitespace); hits
are sorted by line, then capture text.

Everything else — query-compile errors per language, parse failures, and the per-language parse
counts — goes to stderr.

### JSON output

`--json` emits NDJSON: one compact JSON object per match, one per line, on stdout. Fields are the
named captures plus `file` and `line`. **JSON field order is alphabetical, not capture order**
(`serde_json` sorts map keys). A query with no captures reports the matched node's full text under a
`text` field. `file` and `line` are contract fields: a capture named `file` or `line` is overridden
by them.

```console
$ tg '(function_item name: (identifier) @name) @fn' --json tests/fixtures/tree
{"file":"tests/fixtures/tree/main.rs","fn":"fn main() {}","line":1,"name":"main"}
```

## Exit codes

- `0` — success. A query that fails to compile for *some* languages is normal (queries are
  validated per grammar): those languages report their error on stderr and the others run.
- `1` — no query given (or the `--file` could not be read), or the query compiled against none of
  the languages that have files here.

## Design notes

- Grammars come from the crates.io `tree-sitter-*` family; markdown uses `tree-sitter-md` and runs
  both its block and inline grammars per file, unioning the matches.
- A query with no captures is internally rewritten with a `@text` capture per top-level pattern
  (tree-sitter only reports captures), which is what puts `text` in the JSON output.
- Parse failure means a read error or a grammar that cannot be set; unparseable content still
  "parses" (tree-sitter reports an ERROR-rooted tree).
