//! Complete Gregorian dates and zone-free civil datetimes (REQ-0539-0573).

use std::{cmp::Ordering, fmt, str::FromStr};

/// One collected-precision ladder for both temporal types: the finest field
/// the collected source supplied. `None` marks a value no collected field
/// supplied, such as an imputation fallback. A date is at most `Day`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Precision {
    None,
    Year,
    Month,
    Day,
    Hour,
    Minute,
    Second,
}

impl Precision {
    /// The ladder's word, as `precision` returns it.
    pub fn word(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Year => "year",
            Self::Month => "month",
            Self::Day => "day",
            Self::Hour => "hour",
            Self::Minute => "minute",
            Self::Second => "second",
        }
    }
}

/// Construction errors are mapped to the consuming operation's condition.
/// Parsing, conversion and imputation do not share a diagnostic phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TemporalError {
    #[error("temporal invalid form")]
    InvalidForm,
    #[error("temporal invalid date")]
    InvalidDate,
    #[error("temporal invalid time")]
    InvalidTime,
}

#[derive(Clone, Copy, Debug)]
pub struct Date {
    year: u16,
    month: u8,
    day: u8,
    precision: Precision,
}

/// Days in a month of the proleptic Gregorian calendar, or None outside 1 to 12.
pub fn month_length(year: u16, month: u8) -> Option<u8> {
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    Some(match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return None,
    })
}

impl Date {
    /// Validate complete calendar fields and attach the supplied collected
    /// precision, which is at most `Day` for a date.
    pub fn new(year: u16, month: u8, day: u8, precision: Precision) -> Result<Self, TemporalError> {
        let days = month_length(year, month).ok_or(TemporalError::InvalidDate)?;
        if !(1..=9999).contains(&year) || day == 0 || day > days || precision > Precision::Day {
            return Err(TemporalError::InvalidDate);
        }
        Ok(Self {
            year,
            month,
            day,
            precision,
        })
    }

    /// Return civil fields in most-significant-first chronological order.
    pub fn fields(self) -> (u16, u8, u8) {
        (self.year, self.month, self.day)
    }

    /// Return the finest field supplied by collection, excluding imputed fields.
    pub fn collected_precision(self) -> Precision {
        self.precision
    }

    /// The same day with another collected precision, capped at `Day`.
    pub fn with_precision(self, precision: Precision) -> Self {
        Self {
            precision: precision.min(Precision::Day),
            ..self
        }
    }
}

impl PartialEq for Date {
    /// Compare civil fields only; collected precision does not affect equality.
    fn eq(&self, other: &Self) -> bool {
        self.fields() == other.fields()
    }
}

impl Eq for Date {}

impl PartialOrd for Date {
    /// Return the total chronological order, ignoring collected precision.
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Date {
    /// Order complete civil fields lexicographically, ignoring collected precision.
    fn cmp(&self, other: &Self) -> Ordering {
        self.fields().cmp(&other.fields())
    }
}

impl fmt::Display for Date {
    /// Write canonical fixed-width text, omitting collected precision metadata.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

/// Parse a two- or four-byte ASCII decimal field from a validated layout.
fn digits(bytes: &[u8]) -> Result<u16, TemporalError> {
    bytes.iter().try_fold(0, |number, byte| {
        if byte.is_ascii_digit() {
            Ok(number * 10 + u16::from(byte - b'0'))
        } else {
            Err(TemporalError::InvalidForm)
        }
    })
}

/// Parse the exact ten-byte date form and reject invalid Gregorian fields.
fn parse_date(bytes: &[u8]) -> Result<Date, TemporalError> {
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return Err(TemporalError::InvalidForm);
    }
    Date::new(
        digits(&bytes[..4])?,
        digits(&bytes[5..7])? as u8,
        digits(&bytes[8..])? as u8,
        Precision::Day,
    )
}

impl FromStr for Date {
    type Err = TemporalError;

    /// Parse the closed temporal lexical form without zones or implicit field imputation.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        parse_date(text.as_bytes())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DateTime {
    date: Date,
    hour: u8,
    minute: u8,
    second: u8,
    precision: Precision,
}

impl DateTime {
    /// Construct complete fields with explicitly supplied datetime precision;
    /// the date's own collected precision is not consulted.
    /// This is not the prohibited column conversion from date to datetime.
    pub fn new(
        date: Date,
        hour: u8,
        minute: u8,
        second: u8,
        precision: Precision,
    ) -> Result<Self, TemporalError> {
        if hour > 23 || minute > 59 || second > 59 {
            return Err(TemporalError::InvalidTime);
        }
        Ok(Self {
            date,
            hour,
            minute,
            second,
            precision,
        })
    }

    /// The same moment with another collected precision.
    pub fn with_precision(self, precision: Precision) -> Self {
        Self { precision, ..self }
    }

    /// The calendar date of this moment, with collected precision capped at `Day`.
    pub fn date(self) -> Date {
        self.date.with_precision(self.precision)
    }

    /// Return civil fields in most-significant-first chronological order.
    pub fn fields(self) -> (u16, u8, u8, u8, u8, u8) {
        (
            self.date.year,
            self.date.month,
            self.date.day,
            self.hour,
            self.minute,
            self.second,
        )
    }

    /// Return the finest field supplied by collection, excluding imputed fields.
    pub fn collected_precision(self) -> Precision {
        self.precision
    }
}

impl PartialEq for DateTime {
    /// Compare civil fields only; collected precision does not affect equality.
    fn eq(&self, other: &Self) -> bool {
        self.fields() == other.fields()
    }
}

impl Eq for DateTime {}

impl PartialOrd for DateTime {
    /// Return the total chronological order, ignoring collected precision.
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DateTime {
    /// Order complete civil fields lexicographically, ignoring collected precision.
    fn cmp(&self, other: &Self) -> Ordering {
        self.fields().cmp(&other.fields())
    }
}

impl fmt::Display for DateTime {
    /// Write canonical fixed-width text, omitting collected precision metadata.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}T{:02}:{:02}:{:02}",
            self.date, self.hour, self.minute, self.second
        )
    }
}

impl FromStr for DateTime {
    type Err = TemporalError;

    /// Parse the closed temporal lexical form without zones or implicit field imputation.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let bytes = text.as_bytes();
        if !matches!(bytes.len(), 16 | 19)
            || bytes[10] != b'T'
            || bytes[13] != b':'
            || (bytes.len() == 19 && bytes[16] != b':')
        {
            return Err(TemporalError::InvalidForm);
        }
        let date = parse_date(&bytes[..10])?;
        let second = if bytes.len() == 19 {
            digits(&bytes[17..19])? as u8
        } else {
            0
        };
        Self::new(
            date,
            digits(&bytes[11..13])? as u8,
            digits(&bytes[14..16])? as u8,
            second,
            Precision::Second,
        )
    }
}
