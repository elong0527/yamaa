//! The portable regular-expression subset, shared by schema constraints,
//! predicates, extraction, replacement and completed-string column checks.
//!
//! [`parser`] admits the subset and lowers it to `regex` crate syntax. The
//! crate matches in time linear in the subject, with leftmost-first semantics:
//! the earliest starting match wins, and from that start the first successful
//! path when alternatives are tried left to right and greedy (lazy) quantifiers
//! prefer more (fewer) iterations. A capture inside a repetition reports the
//! last iteration that entered it, and once the minimum count is met an
//! iteration that matches the empty string ends the repetition. Matching has no
//! step budget and cannot fail; only compilation refuses, for syntax nesting or
//! compiled size.

mod parser;

use crate::limits::Limits;

/// ECMA-262 WhiteSpace and LineTerminator, without U+0085 (TEXT-8), as
/// inclusive scalar ranges: the one declaration of `\s`, which lowering
/// spells as a class and `str_substr` breaks a text at (TEXT-16).
const SPACE: [(u32, u32); 11] = [
    (0x9, 0xD),
    (0x20, 0x20),
    (0xA0, 0xA0),
    (0x1680, 0x1680),
    (0x2000, 0x200A),
    (0x2028, 0x2028),
    (0x2029, 0x2029),
    (0x202F, 0x202F),
    (0x205F, 0x205F),
    (0x3000, 0x3000),
    (0xFEFF, 0xFEFF),
];

/// Whether `scalar` is whitespace, a member of `\s` (TEXT-8).
pub fn is_space(scalar: char) -> bool {
    let scalar = u32::from(scalar);
    SPACE
        .iter()
        .any(|&(low, high)| (low..=high).contains(&scalar))
}

/// The fixed boundary a compilation refusal names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    /// Syntax nesting beyond `Limits::SEMANTIC_DEPTH`.
    Depth,
    /// A compiled program larger than `Limits::REGEX_SIZE` bytes.
    Size,
}

/// Invalid syntax owns TEXT-6; a resource refusal never masquerades as it.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CompileError {
    #[error("regex invalid: byte={byte:?}, reason={reason:?}")]
    Invalid { byte: usize, reason: &'static str },
    #[error("regex limit: resource={resource:?}, limit={limit:?}")]
    Limit { resource: Resource, limit: usize },
}

/// Group zero is the whole match; None is an unentered group, Some("") an empty capture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match<'s> {
    pub groups: Vec<Option<&'s str>>,
}

/// A compiled pattern; two patterns are equal when their sources are.
#[derive(Clone, Debug)]
pub struct Pattern {
    source: String,
    groups: usize,
    search: regex::Regex,
    full: regex::Regex,
}
impl PartialEq for Pattern {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source
    }
}
impl Eq for Pattern {}

/// The parser admits at most the semantic depth of crate syntax nesting; the
/// full-match wrapper adds a concatenation and a group. The crate compiles
/// recursively, so this also bounds its stack use.
const NEST_LIMIT: u32 = Limits::SEMANTIC_DEPTH as u32 + 2;

impl Pattern {
    /// Validate the subset, then compile the search and full-match programs.
    pub fn compile(source: &str) -> Result<Self, CompileError> {
        let lowered = parser::lower(source)?;
        debug_assert!(lowered.nesting <= Limits::SEMANTIC_DEPTH);
        let search = build(&lowered.pattern)?;
        let full = build(&format!(r"\A(?:{})\z", lowered.pattern))?;
        debug_assert_eq!(search.captures_len(), lowered.groups + 1);
        Ok(Self {
            source: source.into(),
            groups: lowered.groups,
            search,
            full,
        })
    }
    /// Count numbered captures; the whole match is group zero in returned results.
    pub fn group_count(&self) -> usize {
        self.groups
    }
    /// Whether the pattern matches anywhere in the subject.
    pub fn is_match(&self, subject: &str) -> bool {
        self.search.is_match(subject)
    }
    /// Whether one match spans the whole subject, as the schema `pattern`
    /// keyword requires. Alternatives that stop early do not hide later ones.
    pub fn is_full_match(&self, subject: &str) -> bool {
        self.full.is_match(subject)
    }
    /// The leftmost-first match and all of its groups.
    pub fn search<'s>(&self, subject: &'s str) -> Option<Match<'s>> {
        let found = self.search.captures(subject)?;
        Some(Match {
            groups: found
                .iter()
                .map(|group| group.map(|group| group.as_str()))
                .collect(),
        })
    }
    /// Every match of a left-to-right scan that resumes where the previous
    /// match ended, or one scalar later after an empty match (ECMA-262 global
    /// replacement with the Unicode flag). An empty match where a nonempty one
    /// ended still counts. Assertions see the whole subject. Spans are subject
    /// byte offsets.
    pub fn search_all(&self, subject: &str) -> Vec<(usize, usize)> {
        let mut spans = Vec::new();
        let mut next = 0;
        while let Some(found) = self.search.find_at(subject, next) {
            spans.push((found.start(), found.end()));
            next = if found.is_empty() {
                match subject[found.end()..].chars().next() {
                    Some(c) => found.end() + c.len_utf8(),
                    None => break,
                }
            } else {
                found.end()
            };
        }
        spans
    }
}

fn build(pattern: &str) -> Result<regex::Regex, CompileError> {
    regex::RegexBuilder::new(pattern)
        .size_limit(Limits::REGEX_SIZE)
        .nest_limit(NEST_LIMIT)
        .build()
        .map_err(|error| match error {
            regex::Error::CompiledTooBig(limit) => CompileError::Limit {
                resource: Resource::Size,
                limit,
            },
            // The lowering writes only escaped crate syntax within the nest limit.
            error => unreachable!("lowered pattern {pattern:?} was rejected: {error}"),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowering_spells_every_shorthand_and_escapes_every_literal() {
        for (source, lowered) in [
            (r"\d\D", "[0-9][^0-9]"),
            (r"\w\W", "[0-9A-Za-z_][^0-9A-Za-z_]"),
            (
                r"[\s]",
                r"[\x{9}-\x{D}\x{20}\x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF}]",
            ),
            (".", r"[^\x{A}\x{D}\x{2028}\x{2029}]"),
            (r"\b\B", r"(?-u:\b)(?-u:\B)"),
            (r"[^a\D]", r"[^\x{61}[^0-9]]"),
            ("[]", r"[^\x{0}-\x{10FFFF}]"),
            ("[^]", r"[\x{0}-\x{10FFFF}]"),
            (r"\uD800", r"[^\x{0}-\x{10FFFF}]"),
            (r"[\uD800-\uE001]", r"[\x{E000}-\x{E001}]"),
            ("a.b#c d", r"a[^\x{A}\x{D}\x{2028}\x{2029}]b\x{23}c\x{20}d"),
            ("(?<x>a)(?:b)*?c{2,}d{3}", "(a)(?:b){0,}?c{2,}d{3}"),
            ("^a|b$", "^a|b$"),
        ] {
            assert_eq!(parser::lower(source).unwrap().pattern, lowered, "{source}");
        }
    }

    /// The measured nesting never undercounts the crate's own: each lowered
    /// pattern compiles with exactly that nest limit.
    #[test]
    fn measured_nesting_bounds_the_crate_count() {
        let mut sources = vec![
            "a".to_owned(),
            "ab".into(),
            "a|b".into(),
            "(a)*".into(),
            r"[^a\D]+".into(),
            r"[\w\s-]{2,3}?".into(),
            r"\bx\B|.$".into(),
            r"\uD800[\uD800-\uDFFF]".into(),
            "(?:(?:a|b)*c|d)+".into(),
        ];
        let mut nested = "a".to_owned();
        for _ in 0..32 {
            nested = format!("(?:a|b{nested}*c)");
        }
        sources.push(nested);
        sources.push("(".repeat(128) + "a" + &")".repeat(128));
        for source in sources {
            let lowered = parser::lower(&source).unwrap();
            assert!(
                regex::RegexBuilder::new(&lowered.pattern)
                    .nest_limit(lowered.nesting as u32)
                    .build()
                    .is_ok(),
                "{source}: {}",
                lowered.nesting
            );
        }
    }
}
