//! Declaration-ordered conversion recovery shared by every dataset assignment.
use super::*;
use crate::numeric_lifecycle::{recover_conversion, ConversionRecoveryError, LiteralHandler};

/// Bind one assignment's literal replacement in the caller's resolved declaration order.
#[derive(Clone, Debug, PartialEq)]
pub struct ConversionHandler {
    pub assignment_path: String,
    pub handler: LiteralHandler,
}

impl DatasetPlan {
    /// Bind ordered handlers before accessing source data. Repeated assignment paths
    /// may represent a shared default in several templates, but must target one column.
    pub fn with_conversion_handlers(mut self, declarations: Vec<ConversionHandler>) -> Result<Self, PlanError> {
        let mut sites = BTreeMap::new();
        let mut paths = alloc::collections::BTreeSet::new();
        for (index, declaration) in declarations.iter().enumerate() {
            if declaration.assignment_path.is_empty() || declaration.handler.spec_path.is_empty()
                || sites.insert(declaration.assignment_path.clone(), index).is_some()
                || !paths.insert(&declaration.handler.spec_path) {
                return Err(PlanError::InvalidConversionHandler);
            }
            let mut matches = self.templates.iter().flat_map(|template| &template.assignments)
                .chain(&self.columns).filter(|assignment| assignment.path == declaration.assignment_path);
            let first = matches.next().ok_or(PlanError::InvalidConversionHandler)?;
            if matches.any(|assignment| assignment.column != first.column) {
                return Err(PlanError::InvalidConversionHandler);
            }
        }
        self.conversion_sites = sites;
        self.conversion_handlers = declarations;
        Ok(self)
    }
}

/// Recover initial conversion once; only a reached replacement consumes text budget.
pub(super) fn recover<E>(
    original: ConversionError,
    assignment: &Assignment,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    budget: &mut Budget,
    handlers: &mut HandlerCounter,
) -> Result<Value, Box<ExecutionError<E>>> {
    let handler = plan.conversion_sites.get(&assignment.path)
        .map(|index| &plan.conversion_handlers[*index].handler);
    if let Some(LiteralHandler { value: Value::Str(text), .. }) = handler {
        budget.scalar_text(text.len())?;
    }
    let column_path = alloc::format!("columns.{}", plan.output.columns()[assignment.column].name);
    match recover_conversion(plan.output.columns()[assignment.column].kind, &column_path, original, handler, handlers) {
        Ok(value) => Ok(value),
        Err(ConversionRecoveryError::Accounting { error, .. }) => Err(Box::new(ExecutionError::HandlerAccounting(error))),
        Err(ConversionRecoveryError::Conversion { spec_path, error })
        | Err(ConversionRecoveryError::HandlerConversion { spec_path, replacement: error, .. }) => {
            let identity = failure_identity(candidate, &plan.keys, row, budget)?;
            Err(Box::new(ExecutionError::Conversion { path: spec_path, output_row: row, error, identity }))
        }
    }
}
