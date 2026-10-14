//! Select one stored source and one ordinary row mode without granting data access.
use super::*;
impl RowMode {
    /// Zero names the primary source; positive coordinates name existing secondary ports.
    pub fn source_index(&self) -> usize {
        match self {
            Self::Driver { source, .. } | Self::IntermediateDriver { source, .. } => *source,
            _ => 0,
        }
    }
    /// Executable driver wrappers contain exactly one record or grouped mode.
    pub fn local_mode(&self) -> &Self {
        match self {
            Self::Driver { mode, .. } | Self::IntermediateDriver { mode, .. } => mode,
            _ => self,
        }
    }
    pub(super) fn context<'a>(
        &'a self,
        primary: &'a TableSchema,
        secondary: &'a [SecondarySource],
        intermediates: &'a [Intermediate],
    ) -> Result<(&'a TableSchema, &'a Self), PlanError> {
        match self {
            Self::IntermediateDriver {
                index,
                source,
                mode,
            } => {
                let item = intermediates
                    .get(*index)
                    .ok_or(PlanError::InvalidIntermediate)?;
                if item.source.is_self()
                    || !matches!(mode.as_ref(), Self::Records | Self::Groups(_))
                    || item.current_fields().next().is_some()
                {
                    return Err(PlanError::InvalidIntermediate);
                }
                let actual = match item.source {
                    IntermediateSource::Input(index) => index + 1,
                    IntermediateSource::Primary { .. } => 0,
                    _ => return Err(PlanError::InvalidIntermediate),
                };
                if *source != actual {
                    return Err(PlanError::InvalidIntermediate);
                }
                Ok((
                    item.source_schema(secondary)
                        .ok_or(PlanError::InvalidIntermediate)?,
                    mode,
                ))
            }
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
