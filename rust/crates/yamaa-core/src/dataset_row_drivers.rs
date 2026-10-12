//! Select one stored source and one ordinary row mode without granting data access.
use super::*;
impl RowMode {
    /// Zero names the primary source; positive coordinates name existing secondary ports.
    pub fn source_index(&self) -> usize {
        match self {
            Self::Driver { source, .. } => *source,
            _ => 0,
        }
    }
    /// Executable driver wrappers contain exactly one record or grouped mode.
    pub fn local_mode(&self) -> &Self {
        match self {
            Self::Driver { mode, .. } => mode,
            _ => self,
        }
    }
    pub(super) fn context<'a>(
        &'a self,
        primary: &'a TableSchema,
        secondary: &'a [SecondarySource],
    ) -> Result<(&'a TableSchema, &'a Self), PlanError> {
        match self {
            Self::Driver { source, mode } => {
                if *source == 0 || !matches!(mode.as_ref(), Self::Records | Self::Groups(_)) {
                    return Err(PlanError::InvalidSource);
                }
                let schema = &secondary
                    .get(source - 1)
                    .ok_or(PlanError::InvalidSource)?
                    .schema;
                Ok((schema, mode))
            }
            _ => Ok((primary, self)),
        }
    }
}
