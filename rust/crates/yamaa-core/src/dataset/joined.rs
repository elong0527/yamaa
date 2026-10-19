//! One implicit source operand retains its donor identity and completed key pairs.
use super::*;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JoinedRead {
    pub source: AggregateSource,
    pub column: usize,
    pub keys: Vec<MatchKey>,
    /// Row-template joins read these raw driver fields; column joins read output keys.
    pub record_keys: Option<Vec<usize>>,
    pub name: String,
    pub schema: std::sync::Arc<TableSchema>,
}
