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
    !text.is_empty()
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
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
        let bytes = text.as_bytes();
        let bare = |from: usize| {
            let end = bytes[from..]
                .iter()
                .position(|byte| !(byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')))
                .map_or(bytes.len(), |offset| from + offset);
            (end > from).then_some(end)
        };
        let mut at = bare(0)?;
        let mut keys = vec![text[..at].to_string()];
        while at < bytes.len() {
            match bytes[at] {
                b'.' => {
                    let end = bare(at + 1)?;
                    keys.push(text[at + 1..end].to_string());
                    at = end;
                }
                b'[' if bytes.get(at + 1) == Some(&b'\'') => {
                    let mut key = String::new();
                    let mut index = at + 2;
                    loop {
                        let offset = text[index..].find('\'')?;
                        key.push_str(&text[index..index + offset]);
                        index += offset + 1;
                        if bytes.get(index) == Some(&b'\'') {
                            key.push('\'');
                            index += 1;
                        } else {
                            break;
                        }
                    }
                    if bytes.get(index) != Some(&b']') {
                        return None;
                    }
                    keys.push(key);
                    at = index + 1;
                }
                _ => return None,
            }
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
}
