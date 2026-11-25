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
    /// A positional record that ends before its last declared field's last
    /// position (CSV-18).
    RecordShort,
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
            Self::RecordShort => C::CsvRecordShort,
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

/// A csv input's positional reading (CSV-18): the leading lines it drops,
/// then each record's fields at their declared character positions, with
/// no delimiter, no quoting and no header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FixedWidth {
    pub skip: usize,
    pub fields: Vec<PositionalField>,
}

/// One declared field of a positional source: its name and its first and
/// last character positions, counted from 1 and both included.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PositionalField {
    pub name: String,
    pub first: usize,
    pub last: usize,
}

impl PositionalField {
    /// What a `columns` text writes: `<first>-<last>`, each position counted
    /// from 1, a position too large to count, or text off that form, which
    /// the schema's pattern refuses. Whether the positions are ordered is the
    /// declaration's check, not the reading's.
    pub fn columns(text: &str) -> Columns {
        let number = |text: &str| {
            let digits = text.bytes().all(|byte| byte.is_ascii_digit());
            (!text.is_empty() && digits && !text.starts_with('0'))
                .then(|| text.parse::<usize>().ok())
        };
        match text
            .split_once('-')
            .map(|(first, last)| (number(first), number(last)))
        {
            Some((Some(Some(first)), Some(Some(last)))) => Columns::Positions(first, last),
            Some((Some(_), Some(_))) => Columns::TooLarge,
            _ => Columns::Malformed,
        }
    }
}

/// What a `columns` text of a positional field writes (CSV-18).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Columns {
    /// The first and last positions, counted from 1.
    Positions(usize, usize),
    /// Positions of the form, one too large to count.
    TooLarge,
    /// Text off the form `<first>-<last>`.
    Malformed,
}

impl FixedWidth {
    /// The fields' names, in declared order.
    pub fn names(&self) -> Vec<String> {
        self.fields.iter().map(|field| field.name.clone()).collect()
    }

    /// The position of the last declared field's last character: the
    /// fewest characters a record holds.
    pub fn width(&self) -> usize {
        self.fields.last().map_or(0, |field| field.last)
    }

    /// The one-based field at character `position` of a record: the first
    /// field that has not ended before it, or the last field after them all.
    pub fn field_at(&self, position: usize) -> usize {
        let ended = self
            .fields
            .iter()
            .take_while(|field| field.last < position)
            .count();
        (ended + 1).min(self.fields.len().max(1))
    }
}
