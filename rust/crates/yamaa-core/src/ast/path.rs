//! Structured spec paths and the one renderer that writes them.
//!
//! A node's path is a chain of segments in its spec's [`Paths`] arena: a
//! mapping field, a keyed mapping entry, a list index, or a list member with
//! an identity. The renderer writes a chain in either notation, so a
//! notation change is one switch here instead of a spelling at every site.

/// A node's spec path: an index into its spec's [`Paths`] arena.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PathId(u32);

/// One step from a node to a node it contains.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Segment {
    /// A field of a class, such as `derivation` or `filter`.
    Field(&'static str),
    /// An entry of a mapping keyed by text, such as an input, a row
    /// override, a function argument or a dictionary key.
    Key(String),
    /// An entry of a list without identity, such as `case[1]`.
    Index(u32),
    /// An entry of a list whose entries have an identity: a column by
    /// `name`, a row template, an intermediate or a value-level entry by `id`.
    Member { index: u32, key: String },
}

/// The notation a path is written in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notation {
    /// Today's diagnostic paths: columns and value-level entries by
    /// identity, row templates and intermediates by position
    /// (`columns.AVAL`, `rows[0]`, `intermediates[1]`).
    V0_5,
    /// The 1.0 paths of P-CLR-2: every list member with an identity by
    /// that identity, and a segment that is not a plain name quoted
    /// (`rows.week-2`, `dict['NOT REPORTED']`).
    V1,
}

/// The path arena of one spec. Entry 0 is the root, written `$`.
#[derive(Clone, Debug, PartialEq)]
pub struct Paths {
    nodes: Vec<(Option<PathId>, Segment)>,
}

impl Default for Paths {
    fn default() -> Self {
        Self::new()
    }
}

impl Paths {
    /// An arena holding only the root.
    pub fn new() -> Self {
        Self {
            nodes: vec![(None, Segment::Field("$"))],
        }
    }
    /// The root path.
    pub fn root(&self) -> PathId {
        PathId(0)
    }
    /// The path of the node `segment` names inside the node at `parent`.
    pub fn child(&mut self, parent: PathId, segment: Segment) -> PathId {
        let id = u32::try_from(self.nodes.len()).expect("a spec path arena fits u32 indices");
        self.nodes.push((Some(parent), segment));
        PathId(id)
    }
    /// The containing node's path; the root has none.
    pub fn parent(&self, path: PathId) -> Option<PathId> {
        self.nodes[path.0 as usize].0
    }
    /// The last step of a path; the root's is `Field("$")`.
    pub fn segment(&self, path: PathId) -> &Segment {
        &self.nodes[path.0 as usize].1
    }
    /// The number of paths in the arena, the root included.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }
    /// Every path in the arena, the root first.
    pub fn ids(&self) -> impl Iterator<Item = PathId> {
        (0..self.nodes.len() as u32).map(PathId)
    }
    /// Whether the arena holds only the root.
    pub fn is_empty(&self) -> bool {
        self.nodes.len() == 1
    }
    /// Write a path in `notation`; the root alone is `$`.
    pub fn render(&self, path: PathId, notation: Notation) -> String {
        let mut chain = Vec::new();
        let mut at = Some(path);
        while let Some(id) = at {
            chain.push(id);
            at = self.parent(id);
        }
        chain.pop();
        if chain.is_empty() {
            return "$".into();
        }
        let mut text = String::new();
        let mut parent = None;
        for &id in chain.iter().rev() {
            match self.segment(id) {
                Segment::Field(name) => name_segment(&mut text, name, false),
                Segment::Key(key) => name_segment(&mut text, key, notation == Notation::V1),
                Segment::Index(index) => text.push_str(&format!("[{index}]")),
                Segment::Member { index, key } => {
                    let by_identity = notation == Notation::V1
                        || matches!(
                            parent.map(|parent| self.segment(parent)),
                            Some(Segment::Field("columns" | "value_level"))
                        );
                    if by_identity {
                        name_segment(&mut text, key, notation == Notation::V1);
                    } else {
                        text.push_str(&format!("[{index}]"));
                    }
                }
            }
            parent = Some(id);
        }
        text
    }
}

/// Append `.name`, or `name` first; in 1.0 a segment outside the plain-name
/// alphabet of P-CLR-1 is written `['...']`, doubling each quote.
fn name_segment(text: &mut String, name: &str, quote: bool) {
    let plain = !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'));
    if quote && !plain {
        text.push_str("['");
        text.push_str(&name.replace('\'', "''"));
        text.push_str("']");
        return;
    }
    if !text.is_empty() {
        text.push('.');
    }
    text.push_str(name);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(paths: &mut Paths, field: &'static str, index: u32, key: &str) -> PathId {
        let list = paths.child(paths.root(), Segment::Field(field));
        paths.child(
            list,
            Segment::Member {
                index,
                key: key.into(),
            },
        )
    }

    #[test]
    fn the_root_is_written_as_a_dollar() {
        let paths = Paths::new();
        assert_eq!(paths.render(paths.root(), Notation::V0_5), "$");
        assert_eq!(paths.render(paths.root(), Notation::V1), "$");
        assert!(paths.is_empty());
    }

    #[test]
    fn today_columns_are_named_and_rows_and_intermediates_are_indexed() {
        let mut paths = Paths::new();
        let column = member(&mut paths, "columns", 3, "AVAL");
        let derivation = paths.child(column, Segment::Field("derivation"));
        let compute = paths.child(derivation, Segment::Field("compute"));
        let expr = paths.child(compute, Segment::Field("expr"));
        assert_eq!(
            paths.render(expr, Notation::V0_5),
            "columns.AVAL.derivation.compute.expr"
        );
        let row = member(&mut paths, "rows", 1, "week-2");
        let derivations = paths.child(row, Segment::Field("derivations"));
        let entry = paths.child(derivations, Segment::Key("AVAL".into()));
        assert_eq!(
            paths.render(entry, Notation::V0_5),
            "rows[1].derivations.AVAL"
        );
        assert_eq!(
            paths.render(entry, Notation::V1),
            "rows.week-2.derivations.AVAL"
        );
        let intermediate = member(&mut paths, "intermediates", 0, "DEATH");
        let key = paths.child(intermediate, Segment::Field("key"));
        let position = paths.child(key, Segment::Index(2));
        assert_eq!(
            paths.render(position, Notation::V0_5),
            "intermediates[0].key[2]"
        );
        assert_eq!(
            paths.render(position, Notation::V1),
            "intermediates.DEATH.key[2]"
        );
    }

    #[test]
    fn value_level_entries_are_named_in_both_notations() {
        let mut paths = Paths::new();
        let column = member(&mut paths, "columns", 0, "AVAL");
        let submission = paths.child(column, Segment::Field("submission"));
        let levels = paths.child(submission, Segment::Field("value_level"));
        let entry = paths.child(
            levels,
            Segment::Member {
                index: 2,
                key: "SYSBP".into(),
            },
        );
        let clause = paths.child(entry, Segment::Field("where"));
        let column = paths.child(clause, Segment::Key("VSTESTCD".into()));
        let expected = "columns.AVAL.submission.value_level.SYSBP.where.VSTESTCD";
        assert_eq!(paths.render(column, Notation::V0_5), expected);
        assert_eq!(paths.render(column, Notation::V1), expected);
    }

    #[test]
    fn only_the_1_0_notation_quotes_keys_outside_the_plain_alphabet() {
        let mut paths = Paths::new();
        let column = member(&mut paths, "columns", 0, "RACEN");
        let mapping = paths.child(column, Segment::Field("mapping"));
        let dict = paths.child(mapping, Segment::Field("dict"));
        let entry = paths.child(dict, Segment::Key("NOT REPORTED".into()));
        let quote = paths.child(dict, Segment::Key("O'BRIEN".into()));
        assert_eq!(
            paths.render(entry, Notation::V0_5),
            "columns.RACEN.mapping.dict.NOT REPORTED"
        );
        assert_eq!(
            paths.render(entry, Notation::V1),
            "columns.RACEN.mapping.dict['NOT REPORTED']"
        );
        assert_eq!(
            paths.render(quote, Notation::V1),
            "columns.RACEN.mapping.dict['O''BRIEN']"
        );
        assert_eq!(paths.parent(entry), Some(dict));
        assert_eq!(paths.segment(dict), &Segment::Field("dict"));
        assert_eq!(paths.len(), 7);
        assert_eq!(paths.ids().count(), 7);
    }
}
