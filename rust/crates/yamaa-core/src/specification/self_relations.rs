//! SELF preparation uses authored phases and output metadata, never study ports.

/// What a named intermediate reads: a declared input or pair relation, by
/// its position after the inputs (LOOKUP-27), the completed rows of the
/// specification itself, or, for a pair relation, its two sides.
#[derive(Clone, Copy, Debug)]
pub(super) enum Relation {
    Input(usize),
    SelfRows,
    Pair,
}
