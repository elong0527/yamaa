//! The join of a pair relation (LOOKUP-27): the records of a named
//! intermediate that writes `inner_join` are the matching pairs of its two
//! sides. Binding gives the relation one secondary port, which every reader
//! reads as it reads an input's, and keeps here what fills it: each side's
//! records, the key pairs that match them, the joined record a pair's filter
//! and derivations read, and the filter. The relation's derivations and
//! `unique` checks are its intermediate's own.
//!
//! A counted relation (LOOKUP-29), a named intermediate that writes
//! `repeat`, is the same join with a generated right side: each left record
//! pairs with each whole number from 1 to the count it gives, and the
//! joined record is the left side's fields, then the number.
use super::*;

/// The records of one side of a pair relation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PairRecords {
    /// Every record of a source port: zero for the driver, or one past a
    /// secondary input's position.
    Source(usize),
    /// The records a row template driving on this intermediate reads: its
    /// filtered records, its derivations complete (ROW-2).
    Intermediate(usize),
    /// The right side of a counted relation, which no relation stores: for
    /// each left record, one record for each whole number from 1 to the
    /// count the left record gives, holding that number (LOOKUP-29).
    Counted(PairCount),
}

/// The count of a counted relation (LOOKUP-29): the value each left record
/// gives, an `int`, and the path a count that is no count is reported at,
/// `intermediates.<id>.repeat.count`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PairCount {
    pub value: PairValue,
    pub path: String,
}

/// One side of a pair relation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PairSide {
    /// The side as the declaration names it.
    pub name: String,
    pub records: PairRecords,
    /// The side's visible fields, in its own layout, in the order they fill
    /// the joined record. A generated side's one field is its number.
    pub columns: Vec<usize>,
}

/// A value each left record gives: the left value of one key pair, or a
/// counted relation's count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairValue {
    /// A field of the joined record from the left side.
    Field(usize),
    /// A value each left record's key values give: a column of the plan
    /// in [`PairJoin::values`] (LOOKUP-6).
    Value(usize),
}

/// One key pair: the left value and the joined record's field from the
/// right side that it must equal (LOOKUP-9), with the path its refusals are
/// reported at and the right field's name qualified by the right side.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PairKey {
    pub left: PairValue,
    pub right: usize,
    pub path: String,
    pub source: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PairJoin {
    pub left: PairSide,
    pub right: PairSide,
    /// Each key pair, in written order.
    pub keys: Vec<PairKey>,
    /// The joined record: the left side's visible fields, then the right
    /// side's.
    pub schema: TableSchema,
    /// The values a key's expressions, or a count's, give each left record,
    /// evaluated before the pairing (LOOKUP-6, LOOKUP-29): a donor plan
    /// whose stored fields are the left side's visible fields.
    pub values: Option<IntermediateDerivations>,
    /// The pair filter over the joined record, asked of every matching pair
    /// before any derivation is evaluated.
    pub filter: Option<RelationPredicate>,
    /// The secondary port through which every reader reads the relation.
    pub port: usize,
    /// The relation's fields as its readers read them: the exposed
    /// derivations, in declaration order.
    pub fields: TableSchema,
}

impl PairJoin {
    /// How many fields of the joined record come from the left side.
    pub fn left_width(&self) -> usize {
        self.left.columns.len()
    }
}

/// The port of a record relation (LOOKUP-3): a named intermediate another
/// intermediate's `dataset` names. The run fills it once, before any row is
/// built, with the records a row template driving on the intermediate would
/// read, its filter applied and its derivations complete (ROW-2); every
/// reader reads it as it reads an input's.
#[derive(Clone, Debug, PartialEq)]
pub struct RecordPort {
    /// The secondary port the run fills, after the inputs', in the order
    /// the intermediates are derived, as a pair relation's is.
    pub port: usize,
    /// The intermediate's layout columns the port exposes, in order: its
    /// visible stored fields, then its visible derived fields.
    pub columns: Vec<usize>,
    /// The exposed fields, each of one column type.
    pub fields: TableSchema,
}
