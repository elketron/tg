//! Language inference: file extension -> tree-sitter grammar.

use std::path::Path;

/// The languages `tg` knows about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, clap::ValueEnum)]
pub enum Language {
    Markdown,
    Rust,
    #[value(name = "typescript")]
    TypeScript,
    #[value(name = "csharp")]
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

    /// Every grammar a query must be compiled against for this language:
    /// one per language, except markdown, whose split parser needs both the
    /// block and the inline grammar to see the whole tree. The `&str` is the
    /// grammar's name, as used in error messages.
    pub fn grammars(self) -> Vec<(&'static str, tree_sitter::Language)> {
        match self {
            Language::Markdown => vec![
                ("markdown", tree_sitter_md::LANGUAGE.into()),
                ("markdown_inline", tree_sitter_md::INLINE_LANGUAGE.into()),
            ],
            other => vec![(other.name(), other.grammar())],
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

    #[test]
    fn markdown_lists_both_grammars_and_every_other_language_one() {
        let names: Vec<_> = Language::Markdown
            .grammars()
            .iter()
            .map(|(name, _)| *name)
            .collect();
        assert_eq!(names, ["markdown", "markdown_inline"]);
        for lang in [Language::Rust, Language::TypeScript, Language::CSharp] {
            let names: Vec<_> = lang.grammars().iter().map(|(name, _)| *name).collect();
            assert_eq!(names, [lang.name()], "{}", lang.name());
        }
    }
}

#[cfg(test)]
mod value_enum_tests {
    use super::Language;
    use clap::ValueEnum;

    /// The names `--langs` accepts on the command line must be exactly the
    /// names `Language::name` prints (clap kebab-cases multi-word variants
    /// by default; `typescript` and `csharp` need explicit pins).
    #[test]
    fn cli_value_names_match_language_names() {
        for lang in Language::value_variants() {
            let name = clap::ValueEnum::to_possible_value(lang)
                .unwrap()
                .get_name()
                .to_string();
            assert_eq!(name, lang.name());
        }
    }
}
