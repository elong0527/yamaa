//! The closed CSV input profile's models: the text table one captured byte
//! snapshot admits, and the profile's refusals with their findings. The
//! parser is the adapters' (#3030 section 2.10: no format code in core).

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Field {
    Index(usize),
    Name(String),
}
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("csv source profile: cause={cause:?}, record={record:?}, field={field:?}")]
    Profile {
        cause: ProfileCause,
        record: usize,
        field: Field,
    },
    #[error("csv source limit: resource={resource:?}, limit={limit:?}, actual={actual:?}")]
    Limit {
        resource: &'static str,
        limit: usize,
        actual: Option<usize>,
    },
}
/// Closed semantic causes detected by the profile scanner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileCause {
    InvalidText,
    /// A leading byte-order mark the input does not declare, or a declared
    /// one that is absent (CSV-10); `declared` is the input's declaration.
    ByteOrderMark {
        declared: bool,
    },
    HeaderAbsent,
    FieldNameEmpty,
    FieldNameDuplicate,
    RecordWidth,
    QuoteUnterminated,
    CarriageReturn,
    TextAfterQuote,
    QuoteInBareField,
}
impl ProfileCause {
    pub fn code(self) -> crate::diagnostic::ConditionCode {
        use crate::diagnostic::ConditionCode as C;
        match self {
            Self::InvalidText => C::CsvInvalidText,
            Self::ByteOrderMark { .. } => C::CsvByteOrderMark,
            Self::HeaderAbsent => C::CsvHeaderAbsent,
            Self::FieldNameEmpty => C::CsvFieldNameEmpty,
            Self::FieldNameDuplicate => C::CsvFieldNameDuplicate,
            Self::RecordWidth => C::CsvRecordWidth,
            Self::QuoteUnterminated => C::CsvQuoteUnterminated,
            Self::CarriageReturn => C::CsvCarriageReturn,
            Self::TextAfterQuote => C::CsvTextAfterQuote,
            Self::QuoteInBareField => C::CsvQuoteInBareField,
        }
    }
}
impl Error {
    pub fn rule(&self) -> Option<&'static str> {
        match self {
            Self::Profile { cause, .. } => cause.code().definition().rule,
            Self::Limit { .. } => None,
        }
    }
    /// Borrow classified profile facts without allocating diagnostic context.
    pub fn finding<'a>(
        &'a self,
        dataset: &'a str,
        path: &'a str,
    ) -> Option<crate::diagnostic::SourceFinding<'a>> {
        let Self::Profile {
            cause,
            record,
            field,
        } = self
        else {
            return None;
        };
        Some(crate::diagnostic::SourceFinding::Csv {
            cause: *cause,
            record: *record,
            field,
            dataset,
            path,
        })
    }
}

/// Every value is collected text or missing; no numeric, NA-token or locale inference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CsvSource {
    pub names: Vec<String>,
    pub records: Vec<Vec<Option<String>>>,
}
