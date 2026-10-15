//! Pure dataset declarations and admission. No study data or host effects.
use super::*;
use crate::conversion::LiteralHandler;
/// Bind one assignment's literal replacement in the caller's resolved declaration order.
#[derive(Clone, Debug, PartialEq)]
pub struct ConversionHandler {
    pub assignment_path: String,
    pub handler: LiteralHandler,
}

impl DatasetPlan {
    /// Bind ordered handlers before accessing source data. Repeated assignment paths
    /// may represent a shared default in several templates, but must target one column.
    pub fn with_conversion_handlers(
        mut self,
        declarations: Vec<ConversionHandler>,
    ) -> Result<Self, PlanError> {
        let mut sites = BTreeMap::new();
        let mut paths = alloc::collections::BTreeSet::new();
        for (index, declaration) in declarations.iter().enumerate() {
            if declaration.assignment_path.is_empty()
                || declaration.handler.spec_path.is_empty()
                || sites
                    .insert(declaration.assignment_path.clone(), index)
                    .is_some()
                || !paths.insert(&declaration.handler.spec_path)
            {
                return Err(PlanError::InvalidConversionHandler);
            }
            let mut matches = self
                .templates
                .iter()
                .flat_map(|template| &template.assignments)
                .chain(&self.columns)
                .filter(|assignment| assignment.path == declaration.assignment_path);
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
