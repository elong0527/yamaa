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
}
