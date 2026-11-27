//! Complete Gregorian dates and zone-free civil datetimes (VALUE-13 to VALUE-17 and VALUE-21).

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

    /// The exact epoch day, the signed count of days from 1970-01-01: the
    /// days before its year and month, February 29 counted after February
    /// of a leap year, as `month_length` counts them.
    pub fn epoch_day(self) -> i64 {
        let leap = self.month > 2 && month_length(self.year, 2) == Some(29);
        before_year(i64::from(self.year))
            + BEFORE_MONTH[usize::from(self.month) - 1]
            + i64::from(leap)
            + i64::from(self.day)
            - 1
            - EPOCH_ORDINAL
    }

    /// The date `days` after 1970-01-01 with the supplied collected precision,
    /// or None outside 0001-01-01 to 9999-12-31, decoded without timezone,
    /// float arithmetic or host dates.
    ///
    /// The civil fields come from the day's place in its 400-year era of
    /// 146,097 days, counted from a March 1, so that the leap day ends each
    /// year (the days-to-civil method of Howard Hinnant's "chrono-compatible
    /// low-level date algorithms"): a few integer divisions, the same for
    /// every day.
    pub fn from_epoch_day(days: i64, precision: Precision) -> Option<Self> {
        let ordinal = days.checked_add(EPOCH_ORDINAL)?;
        if !(0..=MAX_ORDINAL).contains(&ordinal) {
            return None;
        }
        // Days from 0000-03-01, positive within the bound.
        let shifted = ordinal + 306;
        let era = shifted / 146_097;
        let of_era = shifted % 146_097;
        let year_of_era = (of_era - of_era / 1_460 + of_era / 36_524 - of_era / 146_096) / 365;
        let of_year = of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        // Months from March, each run of five a 153-day cycle.
        let from_march = (5 * of_year + 2) / 153;
        let day = of_year - (153 * from_march + 2) / 5 + 1;
        let month = if from_march < 10 {
            from_march + 3
        } else {
            from_march - 9
        };
        let year = era * 400 + year_of_era + i64::from(month <= 2);
        Self::new(year as u16, month as u8, day as u8, precision).ok()
    }
}

/// Days from 0001-01-01 to 1970-01-01 in the proleptic Gregorian calendar.
const EPOCH_ORDINAL: i64 = 719_162;
/// Days from 0001-01-01 to 9999-12-31, the last day of the calendar.
const MAX_ORDINAL: i64 = 3_652_058;

/// Count days before January 1 of a positive Gregorian year.
fn before_year(year: i64) -> i64 {
    let previous = year - 1;
    365 * previous + previous / 4 - previous / 100 + previous / 400
}

/// Days before the first of each month of a common year.
const BEFORE_MONTH: [i64; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];

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

    /// The integral seconds from 1970-01-01T00:00:00, without timezone
    /// adjustment.
    pub fn epoch_second(self) -> i64 {
        self.date.epoch_day() * 86_400
            + i64::from(self.hour) * 3600
            + i64::from(self.minute) * 60
            + i64::from(self.second)
    }

    /// The moment `seconds` after 1970-01-01T00:00:00 with the supplied
    /// collected precision, or None outside the calendar's years 0001 to
    /// 9999; a negative count floors into its civil day before the time of
    /// day is taken.
    pub fn from_epoch_second(seconds: i64, precision: Precision) -> Option<Self> {
        let date = Date::from_epoch_day(seconds.div_euclid(86_400), Precision::Day)?;
        let time = seconds.rem_euclid(86_400);
        Self::new(
            date,
            (time / 3600) as u8,
            ((time % 3600) / 60) as u8,
            (time % 60) as u8,
            precision,
        )
        .ok()
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

    /// Parse the closed temporal lexical form without zones; an omitted `ss` is second zero at
    /// collected precision `minute`.
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
        // An omitted `ss` names second zero, which no one collected: the
        // value's collected precision is the finest field its text shows
        // (VALUE-15, VALUE-16, TIME-7).
        let (second, precision) = if bytes.len() == 19 {
            (digits(&bytes[17..19])? as u8, Precision::Second)
        } else {
            (0, Precision::Minute)
        };
        Self::new(
            date,
            digits(&bytes[11..13])? as u8,
            digits(&bytes[14..16])? as u8,
            second,
            precision,
        )
    }
}

/// A time of day resolved to a whole second, with no date, zone or
/// collected precision (VALUE-13): every value is complete.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Time {
    hour: u8,
    minute: u8,
    second: u8,
}

impl Time {
    /// The seconds in one day; a time of day counts fewer.
    pub const DAY_SECONDS: u32 = 86_400;

    /// Validate clock fields: hour 0 to 23, minute and second 0 to 59.
    pub fn new(hour: u8, minute: u8, second: u8) -> Result<Self, TemporalError> {
        if hour > 23 || minute > 59 || second > 59 {
            return Err(TemporalError::InvalidTime);
        }
        Ok(Self {
            hour,
            minute,
            second,
        })
    }

    /// The time `seconds` after midnight, or an error from one day on.
    pub fn from_seconds(seconds: u32) -> Result<Self, TemporalError> {
        if seconds >= Self::DAY_SECONDS {
            return Err(TemporalError::InvalidTime);
        }
        Self::new(
            (seconds / 3600) as u8,
            (seconds / 60 % 60) as u8,
            (seconds % 60) as u8,
        )
    }

    /// Return clock fields in most-significant-first order.
    pub fn fields(self) -> (u8, u8, u8) {
        (self.hour, self.minute, self.second)
    }

    /// The whole seconds since midnight, 0 to 86399.
    pub fn seconds(self) -> u32 {
        u32::from(self.hour) * 3600 + u32::from(self.minute) * 60 + u32::from(self.second)
    }
}

impl fmt::Display for Time {
    /// Write canonical fixed-width text, seconds always shown.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02}:{:02}:{:02}", self.hour, self.minute, self.second)
    }
}

/// Parse the `hh:mm` or `hh:mm:ss` clock form; an omitted second is zero.
fn parse_time(bytes: &[u8]) -> Result<Time, TemporalError> {
    if !matches!(bytes.len(), 5 | 8) || bytes[2] != b':' || (bytes.len() == 8 && bytes[5] != b':') {
        return Err(TemporalError::InvalidForm);
    }
    let second = if bytes.len() == 8 {
        digits(&bytes[6..8])? as u8
    } else {
        0
    };
    Time::new(
        digits(&bytes[..2])? as u8,
        digits(&bytes[3..5])? as u8,
        second,
    )
}

impl FromStr for Time {
    type Err = TemporalError;

    /// Parse the closed clock form without a zone, offset or fraction.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        parse_time(text.as_bytes())
    }
}

impl DateTime {
    /// The time of day of this moment.
    pub fn time(self) -> Time {
        Time {
            hour: self.hour,
            minute: self.minute,
            second: self.second,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The seam is the constant-time day decoder, which no reader reaches
    // for every day: each day of the years 1 to 9999 decodes to the civil
    // date that `epoch_day`, counting years and months forward, encodes
    // back to that day, one day after the day before it.
    #[test]
    fn every_day_of_the_years_1_to_9999_decodes_to_the_date_that_encodes_it() {
        let (first, last) = (-EPOCH_ORDINAL, MAX_ORDINAL - EPOCH_ORDINAL);
        let mut previous: Option<Date> = None;
        for day in first..=last {
            let date = Date::from_epoch_day(day, Precision::Day)
                .unwrap_or_else(|| panic!("day {day} decodes"));
            assert_eq!(date.epoch_day(), day, "day {day} encodes back");
            if let Some(previous) = previous {
                assert!(previous < date, "day {day} follows the day before it");
            }
            previous = Some(date);
        }
        assert_eq!(
            Date::from_epoch_day(first, Precision::Day).map(Date::fields),
            Some((1, 1, 1)),
            "the first day"
        );
        assert_eq!(
            Date::from_epoch_day(last, Precision::Day).map(Date::fields),
            Some((9999, 12, 31)),
            "the last day"
        );
        for outside in [first - 1, last + 1, i64::MIN, i64::MAX] {
            assert_eq!(
                Date::from_epoch_day(outside, Precision::Day),
                None,
                "day {outside}"
            );
        }
    }

    // The seam is the epoch day of a civil date (#4201), read from a table of
    // days before each month. The expected days are counted by hand in the
    // proleptic Gregorian calendar: the epoch, the leap days of 2000 and
    // 2024, the common years 1900, 2023 and 2100 past February, and the
    // calendar's first and last days. Every day of 0001 to 9999 then reads
    // back through the independent 400-year-era decoder `from_epoch_day`.
    #[test]
    fn a_date_counts_its_epoch_day_and_every_day_reads_back() {
        let cases = [
            ((1970, 1, 1), 0),
            ((2000, 2, 29), 11_016),
            ((2000, 3, 1), 11_017),
            ((1900, 3, 1), -25_508),
            ((2023, 3, 1), 19_417),
            ((2024, 2, 29), 19_782),
            ((2100, 3, 1), 47_541),
            ((1, 1, 1), -719_162),
            ((9999, 12, 31), 2_932_896),
        ];
        for ((year, month, day), expected) in cases {
            let date = Date::new(year, month, day, Precision::Day).unwrap();
            assert_eq!(date.epoch_day(), expected, "{year:04}-{month:02}-{day:02}");
        }
        for days in -719_162..=2_932_896 {
            let date = Date::from_epoch_day(days, Precision::Day).unwrap();
            assert_eq!(date.epoch_day(), days, "epoch day {days}");
        }
    }
}
