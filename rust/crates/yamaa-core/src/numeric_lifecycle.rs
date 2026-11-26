//! Handler sites and their run-local counts, which the run report's handler
//! rows show (COLUMN-14). The engine converts and recovers a completed value
//! in `dataset::columnar::scalar` (COLUMN-8, COLUMN-13).

/// The handler a counted site answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandlerKind {
    Absent,
    Missing,
    Unmapped,
    Invalid,
    Unconvertible,
    MultipleMatches,
    NoMatch,
    /// `to_date` read year or year-month text and gave missing.
    Partial,
    /// Both imputation bounds lay inside the collected interval and crossed.
    CrossedBounds,
}

impl HandlerKind {
    /// Portable handler vocabulary spelling for later host report serialization.
    pub fn name(self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::Missing => "missing",
            Self::Unmapped => "unmapped",
            Self::Invalid => "invalid",
            Self::Unconvertible => "unconvertible",
            Self::MultipleMatches => {
                crate::diagnostic::ConditionCode::DatasetMultipleMatches
                    .definition()
                    .condition
            }
            Self::NoMatch => "no_match",
            Self::Partial => "partial",
            Self::CrossedBounds => "crossed_bounds",
        }
    }
}

/// One declared handler path's run-local count, including zero firings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandlerCount {
    pub spec_path: String,
    pub handler: HandlerKind,
    pub count: u64,
}

/// Explicit resource failure rather than wrapped or saturated audit data.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("handler count overflow at {spec_path} for {handler:?}")]
pub struct HandlerCountOverflow {
    pub spec_path: String,
    pub handler: HandlerKind,
}
