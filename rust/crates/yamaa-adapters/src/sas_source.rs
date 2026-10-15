//! Read-only SAS binary input, enabled by the default `readstat` feature.
#[cfg(not(feature = "readstat"))]
use crate::arrow_table::{ArrowTable, TableLimits};
use yamaa_core::diagnostic::SourceFinding;
#[cfg(not(feature = "readstat"))]
use yamaa_core::specification::SourceDeclaration;

#[derive(Debug)]
pub enum Error {
    Invalid(&'static str),
    Typing(yamaa_core::typed_csv::Error),
    Limit {
        resource: &'static str,
        limit: usize,
        actual: Option<usize>,
    },
    Internal,
    /// This build omitted the optional SAS binary input backend.
    Unavailable,
}
impl Error {
    pub fn finding<'a>(&'a self, dataset: &'a str, path: &'a str) -> Option<SourceFinding<'a>> {
        match self {
            Self::Invalid(cause) => Some(SourceFinding::Sas {
                dataset,
                path,
                cause,
            }),
            Self::Typing(error) => error.finding(dataset),
            Self::Limit { .. } | Self::Internal | Self::Unavailable => None,
        }
    }
}

#[derive(Clone, Copy)]
pub struct Limits {
    pub source_bytes: usize,
    pub work_bytes: usize,
    pub retained_bytes: usize,
}

#[cfg(feature = "readstat")]
#[allow(unsafe_code)] // Reviewed ReadStat memory transport and callback ABI.
mod native;
#[cfg(feature = "readstat")]
pub use native::parse;
#[cfg(feature = "readstat")]
pub(crate) use native::parse_with_usage;

/// SAS binary input is unavailable when the `readstat` feature is disabled.
#[cfg(not(feature = "readstat"))]
pub fn parse(
    _bytes: &[u8],
    _source: &SourceDeclaration,
    _limits: Limits,
    _table_limits: TableLimits,
) -> Result<ArrowTable, Error> {
    Err(Error::Unavailable)
}

#[cfg(not(feature = "readstat"))]
pub(crate) fn parse_with_usage(
    bytes: &[u8],
    source: &SourceDeclaration,
    limits: Limits,
    table_limits: TableLimits,
    usage: &mut crate::source_decode_usage::Usage,
) -> Result<ArrowTable, Error> {
    usage.complete();
    parse(bytes, source, limits, table_limits)
}

#[cfg(all(test, not(feature = "readstat")))]
mod tests {
    use super::*;
    use yamaa_core::specification::SourceProfile;

    #[test]
    fn omitted_reader_rejects_both_profiles_with_explicit_boundary_code() {
        for (path, profile) in [
            ("source.xpt", SourceProfile::Xport),
            ("source.sas7bdat", SourceProfile::Sas7bdat),
        ] {
            let source = SourceDeclaration {
                name: "RAW".into(),
                path: path.into(),
                profile,
                types: vec![],
                encoding: Some("windows-1252".into()),
                empty_string_present: false,
                ordinal: None,
            };
            let error = parse(
                b"unused",
                &source,
                Limits {
                    source_bytes: 100,
                    work_bytes: 100,
                    retained_bytes: 100,
                },
                TableLimits {
                    max_rows: 10,
                    max_columns: 10,
                    max_batches: 1,
                    max_cells: 100,
                },
            )
            .unwrap_err();
            assert!(matches!(error, Error::Unavailable));
            assert!(error.finding("RAW", path).is_none());
            let reply = crate::specification_diagnostics::failure(
                &crate::specification_run::Error::SasSource(error),
                Some(&source),
            );
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&reply).unwrap()["outcome"],
                serde_json::json!({"status":"rejected", "stage":"ingest", "code":"sas_reader_unavailable"})
            );
        }
    }
}
