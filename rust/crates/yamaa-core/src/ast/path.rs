//! Structured spec paths and the one renderer that writes them.
//!
//! A node's path is a chain of segments in its spec's [`Paths`] arena: a
//! mapping field, a keyed mapping entry, a list index, or a list member with
//! an identity. The renderer writes a chain in the notation of P-CLR-2.

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
    /// Write a path in the notation of P-CLR-2: every list member with an
    /// identity by that identity, and a key that is not a plain name quoted
    /// (`rows.week-2`, `dict['NOT REPORTED']`); the root alone is `$`.
    pub fn render(&self, path: PathId) -> String {
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
        for &id in chain.iter().rev() {
            match self.segment(id) {
                Segment::Field(name) => name_segment(&mut text, name, false),
                Segment::Key(key) | Segment::Member { key, .. } => {
                    name_segment(&mut text, key, true)
                }
                Segment::Index(index) => text.push_str(&format!("[{index}]")),
            }
        }
        text
    }
}

/// Append `.name`, or `name` first; with `quote`, a segment outside the
/// plain-name alphabet of P-CLR-1 is written `['...']`, doubling each quote.
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
        assert_eq!(paths.render(paths.root()), "$");
        assert!(paths.is_empty());
    }

    #[test]
    fn every_member_with_an_identity_is_written_by_that_identity() {
        // P-CLR-2: a column by its `name`, a row template and an
        // intermediate by their `id`; a list without identity by position.
        let mut paths = Paths::new();
        let column = member(&mut paths, "columns", 3, "AVAL");
        let derivation = paths.child(column, Segment::Field("derivation"));
        let aggregate = paths.child(derivation, Segment::Key("aggregate".into()));
        let expr = paths.child(aggregate, Segment::Field("expr"));
        assert_eq!(paths.render(expr), "columns.AVAL.derivation.aggregate.expr");
        let row = member(&mut paths, "rows", 1, "week-2");
        let derivations = paths.child(row, Segment::Field("derivations"));
        let entry = paths.child(derivations, Segment::Key("AVAL".into()));
        assert_eq!(paths.render(entry), "rows.week-2.derivations.AVAL");
        let intermediate = member(&mut paths, "intermediates", 0, "DEATH");
        let key = paths.child(intermediate, Segment::Field("key"));
        let position = paths.child(key, Segment::Index(2));
        assert_eq!(paths.render(position), "intermediates.DEATH.key[2]");
    }

    #[test]
    fn value_level_entries_are_written_by_their_identity() {
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
        assert_eq!(
            paths.render(column),
            "columns.AVAL.submission.value_level.SYSBP.where.VSTESTCD"
        );
    }

    #[test]
    fn keys_outside_the_plain_alphabet_are_quoted() {
        let mut paths = Paths::new();
        let column = member(&mut paths, "columns", 0, "RACEN");
        let mapping = paths.child(column, Segment::Field("mapping"));
        let dict = paths.child(mapping, Segment::Field("dict"));
        let entry = paths.child(dict, Segment::Key("NOT REPORTED".into()));
        let quote = paths.child(dict, Segment::Key("O'BRIEN".into()));
        assert_eq!(
            paths.render(entry),
            "columns.RACEN.mapping.dict['NOT REPORTED']"
        );
        assert_eq!(
            paths.render(quote),
            "columns.RACEN.mapping.dict['O''BRIEN']"
        );
        assert_eq!(paths.parent(entry), Some(dict));
        assert_eq!(paths.segment(dict), &Segment::Field("dict"));
        assert_eq!(paths.len(), 7);
        assert_eq!(paths.ids().count(), 7);
    }
}
