//! Civil/epoch conversion for the private precision-bearing Arrow representation.

use yamaa_core::temporal::{Date, DatePrecision, DateTime, DateTimePrecision};

/// Days from 0001-01-01 to 1970-01-01 in the proleptic Gregorian calendar.
const EPOCH_ORDINAL: i64 = 719_162;
const MAX_ORDINAL: i64 = 3_652_058;

/// Count days before January 1 of a positive Gregorian year (including 10000).
fn before_year(year: i64) -> i64 {
    let previous = year - 1;
    365 * previous + previous / 4 - previous / 100 + previous / 400
}

/// Convert already validated civil fields to an exact Arrow Date32 day.
pub(crate) fn date_days(date: Date) -> i32 {
    let (year, month, day) = date.fields();
    let mut ordinal = before_year(i64::from(year));
    for prior in 1..month {
        ordinal += month_days(year, prior);
    }
    (ordinal + i64::from(day) - 1 - EPOCH_ORDINAL) as i32
}

/// Return the Gregorian month length for a validated year and month.
fn month_days(year: u16, month: u8) -> i64 {
    match month {
        4 | 6 | 9 | 11 => 30,
        2 if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) => {
            29
        }
        2 => 28,
        _ => 31,
    }
}

/// Decode a bounded epoch day without timezone, float arithmetic or host dates.
pub(crate) fn date_from_days(days: i64, precision: DatePrecision) -> Option<Date> {
    let ordinal = days.checked_add(EPOCH_ORDINAL)?;
    if !(0..=MAX_ORDINAL).contains(&ordinal) {
        return None;
    }
    let (mut low, mut high) = (1, 10000);
    while low + 1 < high {
        let middle = (low + high) / 2;
        if before_year(middle) <= ordinal {
            low = middle;
        } else {
            high = middle;
        }
    }
    let year = low as u16;
    let mut remaining = ordinal - before_year(low);
    let mut month = 1;
    while remaining >= month_days(year, month) {
        remaining -= month_days(year, month);
        month += 1;
    }
    Date::new(year, month, remaining as u8 + 1, precision).ok()
}

/// Convert complete civil time to integral seconds without timezone adjustment.
pub(crate) fn datetime_seconds(value: DateTime) -> i64 {
    let (year, month, day, hour, minute, second) = value.fields();
    let date = Date::new(year, month, day, DatePrecision::Day).expect("validated datetime");
    i64::from(date_days(date)) * 86400
        + i64::from(hour) * 3600
        + i64::from(minute) * 60
        + i64::from(second)
}

/// Floor negative epochs into their civil day before extracting time of day.
pub(crate) fn datetime_from_seconds(
    seconds: i64,
    precision: DateTimePrecision,
) -> Option<DateTime> {
    let date = date_from_days(seconds.div_euclid(86400), DatePrecision::Day)?;
    let time = seconds.rem_euclid(86400);
    DateTime::new(
        date,
        (time / 3600) as u8,
        ((time % 3600) / 60) as u8,
        (time % 60) as u8,
        precision,
    )
    .ok()
}
