//! The `-r` replacement template: literal text with `%name` placeholders.
//! Types with impl blocks live in their own file (one per type), so the
//! plain-data part (`TemplatePart`) lives in `report.rs`.

use crate::report::TemplatePart;

/// The capture name the `-r` pipeline gives every top-level pattern of the
/// query; `%match` expands to it, and it is what lets a query that already
/// has captures still report its whole matched node.
pub(crate) const MATCH_CAPTURE: &str = "match";

/// A parsed `-r` template: literal segments and `%name` placeholders.
/// `%match` is reserved for the whole matched node (a query capture named
/// `match` cannot be referenced by a template: `%match` always means the
/// node). The only parse error is a `%` with no name after it; any other
/// name is an error at run time when the query lacks that capture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    parts: Vec<TemplatePart>,
}

impl Template {
    /// Parse a template; a `%` followed by no name (end of string,
    /// whitespace, punctuation or another `%`) is an error: a placeholder
    /// is always `%` plus a capture name.
    pub fn parse(src: &str) -> Result<Self, String> {
        let mut parts: Vec<TemplatePart> = Vec::new();
        let mut literal = String::new();
        let mut chars = src.chars().peekable();
        while let Some(c) = chars.next() {
            if c != '%' {
                literal.push(c);
                continue;
            }
            let name = take_name(&mut chars);
            if name.is_empty() {
                return Err(format!(
                    "template: '%' in {src:?} must be followed by a capture name \
                     (letters, digits, underscore); the query must define every \
                     capture the template uses"
                ));
            }
            if !literal.is_empty() {
                parts.push(TemplatePart::Literal(std::mem::take(&mut literal)));
            }
            parts.push(TemplatePart::Capture(name));
        }
        if !literal.is_empty() {
            parts.push(TemplatePart::Literal(literal));
        }
        Ok(Self { parts })
    }

    /// The capture names the template references, besides `%match`; the
    /// query must define every one of them or the run errors out.
    pub fn references(&self) -> Vec<String> {
        self.parts
            .iter()
            .filter_map(|p| match p {
                TemplatePart::Capture(name) if name != MATCH_CAPTURE => Some(name.clone()),
                _ => None,
            })
            .collect()
    }

    /// Render the template against one match's captures; `capture(name)`
    /// answers the text of capture `@name` (including the internal match
    /// capture). The pipeline checks `references` against the query before
    /// rendering, so every name here exists.
    pub fn render(&self, capture: impl Fn(&str) -> Option<String>) -> String {
        let mut out = String::new();
        for part in &self.parts {
            match part {
                TemplatePart::Literal(text) => out.push_str(text),
                TemplatePart::Capture(name) => {
                    if let Some(text) = capture(name) {
                        out.push_str(&text);
                    }
                }
            }
        }
        out
    }
}

fn take_name(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    let mut name = String::new();
    while let Some(&c) = chars.peek() {
        if !(c.is_alphanumeric() || c == '_') {
            break;
        }
        name.push(c);
        chars.next();
    }
    name
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_literals_and_placeholders() {
        let t = Template::parse("fn %na_me -> done").unwrap();
        assert_eq!(t.references(), vec!["na_me".to_string()]);
        let cap = |n: &str| Some(format!("<{n}>"));
        assert_eq!(t.render(cap), "fn <na_me> -> done");
    }

    #[test]
    fn match_placeholder_is_not_a_query_reference() {
        let t = Template::parse("%match; %x").unwrap();
        assert_eq!(t.references(), vec!["x".to_string()]);
    }

    #[test]
    fn a_bare_template_is_pure_literal() {
        let t = Template::parse("nothing dynamic").unwrap();
        assert!(t.references().is_empty());
        assert_eq!(t.render(|_| None), "nothing dynamic");
    }

    #[test]
    fn a_percent_without_a_name_is_a_parse_error() {
        assert!(Template::parse("trailing%").is_err());
        assert!(Template::parse("% name").is_err());
        assert!(Template::parse("%%x").is_err());
    }
}
