//! SELF preparation uses authored phases and output metadata, never study ports.

/// What a named intermediate reads: a declared input, or the completed rows
/// of the specification itself.
#[derive(Clone, Copy, Debug)]
pub(super) enum Relation {
    Input(usize),
    SelfRows,
}
