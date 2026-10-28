//! Read-only SAS binary input, enabled by the default `readstat` feature.
#[cfg(not(feature = "readstat"))]
use crate::column_table::ColumnTable;
#[cfg(not(feature = "readstat"))]
use yamaa_core::specification::SourceDeclaration;
use yamaa_core::{csv_source::Field, diagnostic::SourceFinding};

#[derive(Debug)]
pub enum Error {
    /// A file refused with one closed cause that no field locates (SAS-7).
    Invalid(&'static str),
    /// A refusal one field locates (SAS-7): the field's name, or the
    /// position, counted from one, of a field whose name is empty or does
    /// not decode; and for a value that does not decode, its record,
    /// counted from one.
    Field {
        cause: &'static str,
        field: Field,
        record: Option<usize>,
    },
    /// The declaration contradicts the encoding recorded by a SAS dataset.
    EncodingMismatch {
        declared: String,
        stored: String,
    },
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
                field: None,
                record: None,
            }),
            Self::Field {
                cause,
                field,
                record,
            } => Some(SourceFinding::Sas {
                dataset,
                path,
                cause,
                field: Some(field),
                record: *record,
            }),
            Self::EncodingMismatch { declared, stored } => {
                Some(SourceFinding::SasEncodingMismatch {
                    dataset,
                    path,
                    declared,
                    stored,
                })
            }
            Self::Typing(error) => error.finding(dataset),
            Self::Limit { .. } | Self::Internal | Self::Unavailable => None,
        }
    }
}

impl From<yamaa_core::limits::LimitExceeded> for Error {
    fn from(error: yamaa_core::limits::LimitExceeded) -> Self {
        Self::Limit {
            resource: error.resource,
            limit: error.limit,
            actual: error.actual,
        }
    }
}
#[cfg(feature = "readstat")]
#[allow(unsafe_code)] // Reviewed ReadStat memory transport and callback ABI.
mod native;
#[cfg(feature = "readstat")]
pub use native::parse;

/// SAS binary input is unavailable when the `readstat` feature is disabled.
#[cfg(not(feature = "readstat"))]
pub fn parse(_bytes: &[u8], _source: &SourceDeclaration) -> Result<ColumnTable, Error> {
    Err(Error::Unavailable)
}

#[cfg(all(test, not(feature = "readstat")))]
mod tests {
    use super::*;
    use yamaa_core::decoder::Decoder as _;
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
                byte_order_mark: false,
                empty_string_present: false,
                ordinal: None,
            };
            let failure = crate::source_decoding::decode_source(
                &source,
                b"unused",
                yamaa_core::specification::SourceInterpretation::Ordinary,
                None,
            )
            .unwrap_err();
            assert!(
                !crate::decoder::Decoder::<std::convert::Infallible>::default()
                    .continue_after(&failure)
            );
            let crate::source_decoding::Error::SasSource(error) = failure else {
                panic!("SAS capability refusal must keep its source boundary");
            };
            assert!(matches!(error, Error::Unavailable));
            assert!(error.finding("RAW", path).is_none());
            let crate::specification_diagnostics::Rejection::Fault(issue) =
                crate::specification_diagnostics::failure(
                    &crate::source_decoding::Error::SasSource(error),
                    Some(&source),
                )
            else {
                panic!("an omitted SAS reader is an engine fault");
            };
            assert_eq!(
                issue.code,
                yamaa_core::diagnostic::ConditionCode::ApplicationRejected
            );
            assert_eq!(issue.context["step"].as_str(), Some("ingest"));
            assert_eq!(
                issue.context["code"].as_str(),
                Some("sas_reader_unavailable")
            );
        }
    }
}
