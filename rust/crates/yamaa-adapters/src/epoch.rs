//! Civil fields to and from epoch days and seconds: the public form of a
//! date and a datetime, Parquet's and SAS's stored temporals, and the
//! facade's epoch day and second accessors.

use yamaa_core::temporal::{Date, DateTime, Precision};

/// Days from 0001-01-01 to 1970-01-01 in the proleptic Gregorian calendar.
const EPOCH_ORDINAL: i64 = 719_162;
const MAX_ORDINAL: i64 = 3_652_058;

/// Count days before January 1 of a positive Gregorian year (including 10000).
fn before_year(year: i64) -> i64 {
    let previous = year - 1;
    365 * previous + previous / 4 - previous / 100 + previous / 400
}

/// Days before the first of each month of a common year.
const BEFORE_MONTH: [i64; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];

/// Convert already validated civil fields to an exact epoch day: the days
/// before its year and month, February 29 counted after February of a leap
/// year, as `month_days` counts them.
pub fn date_days(date: Date) -> i32 {
    let (year, month, day) = date.fields();
    let leap = month > 2 && month_days(year, 2) == 29;
    let ordinal =
        before_year(i64::from(year)) + BEFORE_MONTH[usize::from(month) - 1] + i64::from(leap);
    (ordinal + i64::from(day) - 1 - EPOCH_ORDINAL) as i32
}

/// Return the Gregorian month length for a validated year and month.
fn month_days(year: u16, month: u8) -> i64 {
    match month {
        4 | 6 | 9 | 11 => 30,
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        _ => 31,
    }
}

/// Decode a bounded epoch day without timezone, float arithmetic or host dates.
///
/// The civil fields come from the day's place in its 400-year era of
/// 146,097 days, counted from a March 1, so that the leap day ends each year
/// (the days-to-civil method of Howard Hinnant's "chrono-compatible
/// low-level date algorithms"): a few integer divisions, the same for every
/// day.
pub fn date_from_days(days: i64, precision: Precision) -> Option<Date> {
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
    Date::new(year as u16, month as u8, day as u8, precision).ok()
}

/// Convert complete civil time to integral seconds without timezone adjustment.
pub fn datetime_seconds(value: DateTime) -> i64 {
    let (year, month, day, hour, minute, second) = value.fields();
    let date = Date::new(year, month, day, Precision::Day).expect("validated datetime");
    i64::from(date_days(date)) * 86400
        + i64::from(hour) * 3600
        + i64::from(minute) * 60
        + i64::from(second)
}

/// Floor negative epochs into their civil day before extracting time of day.
pub fn datetime_from_seconds(seconds: i64, precision: Precision) -> Option<DateTime> {
    let date = date_from_days(seconds.div_euclid(86400), Precision::Day)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    // The seam is the constant-time day decoder, which no reader reaches
    // for every day: each day of the years 1 to 9999 decodes to the civil
    // date that `date_days`, counting years and months forward, encodes
    // back to that day, one day after the day before it.
    #[test]
    fn every_day_of_the_years_1_to_9999_decodes_to_the_date_that_encodes_it() {
        let (first, last) = (-EPOCH_ORDINAL, MAX_ORDINAL - EPOCH_ORDINAL);
        let mut previous: Option<Date> = None;
        for day in first..=last {
            let date =
                date_from_days(day, Precision::Day).unwrap_or_else(|| panic!("day {day} decodes"));
            assert_eq!(i64::from(date_days(date)), day, "day {day} encodes back");
            if let Some(previous) = previous {
                assert!(previous < date, "day {day} follows the day before it");
            }
            previous = Some(date);
        }
        assert_eq!(
            date_from_days(first, Precision::Day).map(|date| date.fields()),
            Some((1, 1, 1)),
            "the first day"
        );
        assert_eq!(
            date_from_days(last, Precision::Day).map(|date| date.fields()),
            Some((9999, 12, 31)),
            "the last day"
        );
        for outside in [first - 1, last + 1, i64::MIN, i64::MAX] {
            assert_eq!(
                date_from_days(outside, Precision::Day),
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
    // back through the independent 400-year-era decoder `date_from_days`.
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
            assert_eq!(date_days(date), expected, "{year:04}-{month:02}-{day:02}");
        }
        for days in -719_162..=2_932_896 {
            let date = date_from_days(days, Precision::Day).unwrap();
            assert_eq!(i64::from(date_days(date)), days, "epoch day {days}");
        }
    }
}
