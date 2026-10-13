//! Pure dataset declarations and admission. No study data or host effects.
use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowKind {
    RowNumber,
    Competition,
    Dense,
    RowValue { column: usize, offset: i64 },
    PreviousNonMissing { column: usize },
    Locf { column: usize },
    BaselineFlag { date: usize, reference_date: usize },
}

/// Null placement is independent of direction; construction order breaks remaining ties.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrderTerm {
    pub column: usize,
    pub descending: bool,
    pub nulls_first: bool,
}

/// Window dependencies bind only completed output columns, never qualified source reads.
#[derive(Clone, Debug, PartialEq)]
pub struct Window {
    pub kind: WindowKind,
    pub group_by: Vec<usize>,
    pub order_by: Vec<OrderTerm>,
    pub filter: Option<BoundPredicate>,
}
impl Window {
    /// Reject incomplete dependencies and enforce operation-specific ordering before source access.
    pub(super) fn validate(
        &self,
        available: &[bool],
        output: &TableSchema,
    ) -> Result<(), PlanError> {
        let baseline = matches!(self.kind, WindowKind::BaselineFlag { .. });
        if self.order_by.is_empty() != baseline
            || self.group_by.iter().enumerate().any(|(i, column)| {
                !available.get(*column).copied().unwrap_or(false)
                    || self.group_by[..i].contains(column)
            })
            || self
                .order_by
                .iter()
                .any(|term| !available.get(term.column).copied().unwrap_or(false))
        {
            return Err(PlanError::InvalidWindow);
        }
        if let WindowKind::RowValue { offset: 0, .. } = self.kind {
            return Err(PlanError::InvalidWindow);
        }
        let source = match self.kind {
            WindowKind::RowValue { column, .. }
            | WindowKind::PreviousNonMissing { column }
            | WindowKind::Locf { column } => Some(column),
            _ => None,
        };
        if source.is_some_and(|column| !available.get(column).copied().unwrap_or(false)) {
            return Err(PlanError::InvalidWindow);
        }
        if let WindowKind::BaselineFlag {
            date,
            reference_date,
        } = self.kind
        {
            if !available.get(date).copied().unwrap_or(false)
                || !available.get(reference_date).copied().unwrap_or(false)
            {
                return Err(PlanError::InvalidWindow);
            }
            let kind = output.columns()[date].kind;
            if !matches!(
                kind,
                crate::value::ColumnType::Date | crate::value::ColumnType::DateTime
            ) || output.columns()[reference_date].kind != kind
            {
                return Err(PlanError::InvalidWindow);
            }
        }
        if let Some(filter) = &self.filter {
            filter
                .validate(0, available, true)
                .map_err(PlanError::Filter)?;
        }
        Ok(())
    }
}

/// Window operands read either completed output columns or the candidate's original
/// driver membership. Source operands occupy typed virtual slots after output fields.
#[derive(Clone, Debug, PartialEq)]
pub struct ContextWindow {
    pub window: Window,
    pub sources: Vec<(alloc::string::String, usize)>,
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
    pub(super) fn validate(
        &self,
        available: &[bool],
        output: &TableSchema,
        source: &TableSchema,
        mode: &RowMode,
    ) -> Result<(), PlanError> {
        if self.sources.is_empty() {
            return Err(PlanError::InvalidWindow);
        }
        if let RowMode::Groups(groups) = mode {
            if self.sources.iter().any(|(_, c)| !groups.contains(c)) {
                return Err(PlanError::InvalidWindow);
            }
        }
        let schema = self.schema(output, source)?;
        let mut slots = available.to_vec();
        slots.extend(core::iter::repeat_n(true, self.sources.len()));
        self.window.validate(&slots, &schema)
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
