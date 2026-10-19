//! Pure dataset declarations. No study data or host effects.
use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowKind {
    RowNumber,
    Competition,
    Dense,
    RowValue {
        column: usize,
        offset: i64,
    },
    /// The closest earlier present donor; the current row's own present value
    /// answers first only when `include_current` is set.
    Locf {
        column: usize,
        include_current: bool,
    },
    /// The last eligible row by `date`, then by the window's `order_by`, which
    /// holds the declared `tie_break` terms. A row is eligible when `date`,
    /// `reference_date` and `value` are present and `date` is before the
    /// reference, or equal to it when `inclusive`.
    BaselineFlag {
        date: usize,
        reference_date: usize,
        value: usize,
        inclusive: bool,
    },
}

/// `nulls_first` is the resolved placement of missing values, applied after direction
/// orders the present ones; construction order breaks remaining ties.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrderTerm {
    pub column: usize,
    pub descending: bool,
    pub nulls_first: bool,
}

/// Window dependencies bind only completed output columns, never qualified source reads.
/// `order_by` orders each partition; for `baseline_flag` it holds the optional
/// `tie_break` terms, compared only between eligible rows sharing a date.
#[derive(Clone, Debug, PartialEq)]
pub struct Window {
    pub kind: WindowKind,
    pub group_by: Vec<usize>,
    pub order_by: Vec<OrderTerm>,
    pub filter: Option<BoundPredicate>,
}

/// Window operands read either completed output columns or the candidate's original
/// driver membership. Source operands occupy typed virtual slots after output fields.
#[derive(Clone, Debug, PartialEq)]
pub struct ContextWindow {
    pub window: Window,
    pub sources: Vec<(std::string::String, usize)>,
}
impl ContextWindow {
    pub fn schema(
        &self,
        output: &TableSchema,
        source: &TableSchema,
    ) -> Result<TableSchema, PlanError> {
        let mut fields = output.columns().to_vec();
        for (name, column) in &self.sources {
            let kind = source
                .columns()
                .get(*column)
                .ok_or(PlanError::InvalidWindow)?
                .kind;
            fields.push(crate::table::Column {
                name: name.clone(),
                kind,
            });
        }
        TableSchema::new(fields).map_err(|_| PlanError::InvalidWindow)
    }
}
impl Expression {
    pub fn window(&self) -> Option<&Window> {
        match self {
            Self::Window(w) => Some(w),
            Self::ContextWindow(w) => Some(&w.window),
            _ => None,
        }
    }
}
