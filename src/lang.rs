//! Language inference: file extension -> tree-sitter grammar.

use std::path::Path;

/// The languages `tg` knows about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Markdown,
    Rust,
    TypeScript,
    CSharp,
}

pub const LANGUAGES: [Language; 4] = [
    Language::Markdown,
    Language::Rust,
    Language::TypeScript,
    Language::CSharp,
];

impl Language {
    pub fn name(self) -> &'static str {
        match self {
            Language::Markdown => "markdown",
            Language::Rust => "rust",
            Language::TypeScript => "typescript",
            Language::CSharp => "csharp",
        }
    }

    pub fn grammar(self) -> tree_sitter::Language {
        match self {
            // `tree-sitter-md` bundles both grammars of the split markdown
            // parser: `LANGUAGE` (block) and `INLINE_LANGUAGE` (inline).
            Language::Markdown => tree_sitter_md::LANGUAGE.into(),
            Language::Rust => tree_sitter_rust::LANGUAGE.into(),
            Language::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Language::CSharp => tree_sitter_c_sharp::LANGUAGE.into(),
        }
    }
}

/// Map a file path to its language by extension, case-insensitively (like
/// ripgrep's ignore matching). Unknown extensions have no language and are
/// skipped.
pub fn language_for(path: &Path) -> Option<Language> {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("md") => Some(Language::Markdown),
        Some("rs") => Some(Language::Rust),
        Some("ts") | Some("tsx") => Some(Language::TypeScript),
        Some("cs") => Some(Language::CSharp),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_language_from_extension() {
        for (name, want) in [
            ("README.md", Some(Language::Markdown)),
            ("src/main.rs", Some(Language::Rust)),
            ("a/b/c.ts", Some(Language::TypeScript)),
            ("a/b/c.tsx", Some(Language::TypeScript)),
            ("x/y/Program.cs", Some(Language::CSharp)),
        ] {
            assert_eq!(language_for(Path::new(name)), want, "{name}");
        }
    }

    #[test]
    fn extensions_match_case_insensitively() {
        for (name, want) in [
            ("README.MD", Some(Language::Markdown)),
            ("src/MAIN.Rs", Some(Language::Rust)),
            ("a/b/c.TSX", Some(Language::TypeScript)),
            ("x/y/Program.CS", Some(Language::CSharp)),
        ] {
            assert_eq!(language_for(Path::new(name)), want, "{name}");
        }
    }

    #[test]
    fn unknown_or_missing_extensions_have_no_language() {
        assert_eq!(language_for(Path::new("notes.txt")), None);
        assert_eq!(language_for(Path::new("data.json")), None);
        assert_eq!(language_for(Path::new("LICENSE")), None);
        assert_eq!(language_for(Path::new("weird.md.txt")), None);
    }

    #[test]
    fn every_language_grammar_loads() {
        for lang in LANGUAGES {
            let mut parser = tree_sitter::Parser::new();
            assert!(
                parser.set_language(&lang.grammar()).is_ok(),
                "{} grammar failed to load",
                lang.name()
            );
        }
    }

    #[test]
    fn markdown_inline_grammar_loads() {
        let mut parser = tree_sitter::Parser::new();
        assert!(
            parser
                .set_language(&tree_sitter_md::INLINE_LANGUAGE.into())
                .is_ok()
        );
        assert!(parser.parse("- [ ] a task\n", None).is_some());
    }
}
