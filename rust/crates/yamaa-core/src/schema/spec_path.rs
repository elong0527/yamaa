//! The spec path notation of a 1.x specification (P-CLR-1, P-CLR-2): the
//! dotted paths a layer's `clear` writes and its findings report.
//!
//! A path is a name followed by `.segment` or `['quoted']` steps. A segment is
//! made of ASCII letters, digits, `_` and `-`; any other mapping key, such as
//! `NOT REPORTED`, is quoted, with `'` written twice. Keyed members are
//! addressed by identity, so `clear` never writes a list index; a finding's
//! path may end in one (`verifications[0]`) and `$` names the root.

use std::borrow::Cow;

/// Whether `text` is one bare segment of a spec path.
pub(crate) fn plain(text: &str) -> bool {
    bare(text) == Some(text.len())
}

/// The length of the bare segment `text` starts with; `None` when it starts
/// with none.
fn bare(text: &str) -> Option<usize> {
    let end = text
        .bytes()
        .position(|byte| !(byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')))
        .unwrap_or(text.len());
    (end > 0).then_some(end)
}

/// Read the step `text` starts with, `.segment` or `['quoted']`: the key it
/// names and the text after it. `None` when `text` starts with no step, such
/// as a list index, an empty segment or an unclosed quote.
pub(crate) fn read_step(text: &str) -> Option<(String, &str)> {
    if let Some(rest) = text.strip_prefix('.') {
        let end = bare(rest)?;
        return Some((rest[..end].to_string(), &rest[end..]));
    }
    let mut rest = text.strip_prefix("['")?;
    let mut key = String::new();
    loop {
        let (part, after) = rest.split_once('\'')?;
        key.push_str(part);
        match after.strip_prefix('\'') {
            Some(after) => {
                key.push('\'');
                rest = after;
            }
            None => return Some((key, after.strip_prefix(']')?)),
        }
    }
}

/// One key as a path step: `.key` when it is a bare segment, else `['key']`.
pub(crate) fn step(key: &str) -> Cow<'_, str> {
    if plain(key) {
        Cow::Owned(format!(".{key}"))
    } else {
        Cow::Owned(format!("['{}']", key.replace('\'', "''")))
    }
}

/// `path` followed by the step that names `key`; a key at the root starts the
/// path bare when it can.
pub(crate) fn join(path: &str, key: &str) -> String {
    if path.is_empty() && plain(key) {
        key.into()
    } else {
        format!("{path}{}", step(key))
    }
}

/// A path a layer writes in `clear`, as the keys it names, root first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SpecPath {
    keys: Vec<String>,
}

impl SpecPath {
    /// Read a `clear` entry: a bare first segment, then `.segment` and
    /// `['quoted']` steps. `$`, list indexes and empty segments name no
    /// clearable value, so they are refused like any other spelling outside
    /// the notation.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        let first = bare(text)?;
        let mut keys = vec![text[..first].to_string()];
        let mut rest = &text[first..];
        while !rest.is_empty() {
            let (key, after) = read_step(rest)?;
            keys.push(key);
            rest = after;
        }
        Some(Self { keys })
    }

    pub(crate) fn keys(&self) -> &[String] {
        &self.keys
    }

    /// The key the path ends at, which a finding about it names.
    pub(crate) fn last(&self) -> &str {
        self.keys.last().expect("a path names at least one key")
    }

    /// Whether this path names `other` or a value that holds it.
    pub(crate) fn covers(&self, other: &Self) -> bool {
        other.keys.starts_with(&self.keys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(text: &str) -> Option<Vec<String>> {
        SpecPath::parse(text).map(|path| path.keys)
    }

    #[test]
    fn a_clear_path_names_keys_by_segment_or_quoted_text() {
        assert_eq!(
            keys("columns.AVAL.verifications"),
            Some(vec![
                "columns".into(),
                "AVAL".into(),
                "verifications".into()
            ])
        );
        assert_eq!(
            keys("rows.collected-occurrence.filter"),
            Some(vec![
                "rows".into(),
                "collected-occurrence".into(),
                "filter".into()
            ])
        );
        assert_eq!(
            keys("columns.RACEN.derivation.mapping.dict['NOT REPORTED']"),
            Some(
                [
                    "columns",
                    "RACEN",
                    "derivation",
                    "mapping",
                    "dict",
                    "NOT REPORTED"
                ]
                .map(String::from)
                .to_vec()
            )
        );
        assert_eq!(
            keys("metadata['it''s'].x"),
            Some(vec!["metadata".into(), "it's".into(), "x".into()])
        );
        assert_eq!(
            keys("metadata['']"),
            Some(vec!["metadata".into(), "".into()])
        );
        for refused in [
            "",
            "$",
            ".scope",
            "metadata.",
            "metadata..scope",
            "columns[0]",
            "columns.AVAL[0]",
            "metadata['open",
            "metadata['x'",
            "metadata['x']y",
            "metadata[\"x\"]",
            "metadata scope",
            "['metadata']",
        ] {
            assert_eq!(keys(refused), None, "{refused:?}");
        }
    }

    #[test]
    fn a_path_covers_itself_and_every_path_below_it() {
        let parse = |text| SpecPath::parse(text).unwrap();
        assert!(parse("columns.X").covers(&parse("columns.X.label")));
        assert!(parse("columns.X").covers(&parse("columns.X")));
        assert!(!parse("columns.X.label").covers(&parse("columns.X")));
        // Keys compare whole: `X` does not cover `XY`.
        assert!(!parse("columns.X").covers(&parse("columns.XY")));
        assert!(parse("metadata['a b']").covers(&parse("metadata['a b'].c")));
    }

    #[test]
    fn a_key_is_written_bare_or_quoted() {
        assert_eq!(join("", "columns"), "columns");
        assert_eq!(join("columns", "AVAL"), "columns.AVAL");
        assert_eq!(join("rows", "week 2"), "rows['week 2']");
        assert_eq!(join("dict", "it's"), "dict['it''s']");
        assert_eq!(join("", "a b"), "['a b']");
        assert!(plain("collected-occurrence"));
        assert!(!plain(""));
        assert!(!plain("a.b"));
    }

    #[test]
    fn a_step_names_one_key_and_leaves_the_text_after_it() {
        for (text, step) in [
            (".rows.event", Some(("rows", ".event"))),
            ("['week 2'].derivations", Some(("week 2", ".derivations"))),
            // A quoted key may hold what would end a step outside quotes.
            ("['a].b'''].c", Some(("a].b'", ".c"))),
            ("['']", Some(("", ""))),
            ("[0].c", None),
            (".", None),
            (".['x']", None),
            ("['open", None),
            ("['x'y", None),
            ("rows.x", None),
        ] {
            assert_eq!(
                read_step(text),
                step.map(|(key, rest)| (key.to_string(), rest)),
                "{text:?}"
            );
        }
    }
}
