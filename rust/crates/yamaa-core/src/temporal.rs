//! Complete Gregorian dates and zone-free civil datetimes (REQ-0539–0573).

use core::{cmp::Ordering, fmt, str::FromStr};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DatePrecision {
    Year,
    Month,
    Day,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateTimePrecision {
    Day,
    Second,
}

/// Construction errors are mapped to the consuming operation's condition.
/// Parsing, conversion and imputation do not share a diagnostic phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TemporalError {
    InvalidForm,
    InvalidDate,
    InvalidTime,
}

#[derive(Clone, Copy, Debug)]
pub struct Date {
    year: u16,
    month: u8,
    day: u8,
    precision: DatePrecision,
}

impl Date {
    pub fn new(
        year: u16,
        month: u8,
        day: u8,
        precision: DatePrecision,
    ) -> Result<Self, TemporalError> {
        let leap =
            year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
        let days = match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if leap => 29,
            2 => 28,
            _ => return Err(TemporalError::InvalidDate),
        };
        if !(1..=9999).contains(&year) || day == 0 || day > days {
            return Err(TemporalError::InvalidDate);
        }
        Ok(Self {
            year,
            month,
            day,
            precision,
        })
    }

    pub fn fields(self) -> (u16, u8, u8) {
        (self.year, self.month, self.day)
    }

    pub fn collected_precision(self) -> DatePrecision {
        self.precision
    }
}

impl PartialEq for Date {
    fn eq(&self, other: &Self) -> bool {
        self.fields() == other.fields()
    }
}

impl Eq for Date {}

impl PartialOrd for Date {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Date {
    fn cmp(&self, other: &Self) -> Ordering {
        self.fields().cmp(&other.fields())
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

fn digits(bytes: &[u8]) -> Result<u16, TemporalError> {
    bytes.iter().try_fold(0, |number, byte| {
        if byte.is_ascii_digit() {
            Ok(number * 10 + u16::from(byte - b'0'))
        } else {
            Err(TemporalError::InvalidForm)
        }
    })
}

fn parse_date(bytes: &[u8]) -> Result<Date, TemporalError> {
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return Err(TemporalError::InvalidForm);
    }
    Date::new(
        digits(&bytes[..4])?,
        digits(&bytes[5..7])? as u8,
        digits(&bytes[8..])? as u8,
        DatePrecision::Day,
    )
}

impl FromStr for Date {
    type Err = TemporalError;

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
    precision: DateTimePrecision,
}

impl DateTime {
    /// Construct complete fields with explicitly supplied datetime precision.
    /// This is not the prohibited column conversion from date to datetime.
    pub fn new(
        date: Date,
        hour: u8,
        minute: u8,
        second: u8,
        precision: DateTimePrecision,
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

    pub fn collected_precision(self) -> DateTimePrecision {
        self.precision
    }
}

impl PartialEq for DateTime {
    fn eq(&self, other: &Self) -> bool {
        self.fields() == other.fields()
    }
}

impl Eq for DateTime {}

impl PartialOrd for DateTime {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DateTime {
    fn cmp(&self, other: &Self) -> Ordering {
        self.fields().cmp(&other.fields())
    }
}

impl fmt::Display for DateTime {
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
            DateTimePrecision::Second,
        )
    }
}
