//! Pure dataset declarations. No study data or host effects.
use super::*;
use crate::conversion::LiteralHandler;
/// Bind one assignment's literal replacement in the caller's resolved declaration order.
#[derive(Clone, Debug, PartialEq)]
pub struct ConversionHandler {
    pub assignment_path: String,
    pub handler: LiteralHandler,
}

impl DatasetPlan {
    /// Literal replacements in the compiler's resolved declaration order. A
    /// shared default in several templates repeats its assignment path.
    pub(crate) fn with_conversion_handlers(mut self, declarations: Vec<ConversionHandler>) -> Self {
        self.conversion_sites = declarations
            .iter()
            .enumerate()
            .map(|(index, declaration)| (declaration.assignment_path.clone(), index))
            .collect();
        self.conversion_handlers = declarations;
        self
    }
}
