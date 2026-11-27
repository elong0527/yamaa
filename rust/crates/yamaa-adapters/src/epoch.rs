//! Civil fields to and from epoch days and seconds: the public form of a
//! date, a datetime and a time, Parquet's and SAS's stored temporals, and
//! the facade's epoch day and second accessors.

use yamaa_core::table::ValueRef;
use yamaa_core::temporal::{Date, DateTime, Precision, Time};

/// A date's epoch day as the 32-bit day count Arrow and Parquet store;
/// `Date::epoch_day` owns the calendar arithmetic, and every day of years
/// 0001 to 9999 fits.
pub fn date_days(date: Date) -> i32 {
    date.epoch_day() as i32
}

/// The date `days` after 1970-01-01, or None outside years 0001 to 9999
/// (`Date::from_epoch_day`).
pub fn date_from_days(days: i64, precision: Precision) -> Option<Date> {
    Date::from_epoch_day(days, precision)
}

/// Convert complete civil time to integral seconds without timezone
/// adjustment (`DateTime::epoch_second`).
pub fn datetime_seconds(value: DateTime) -> i64 {
    value.epoch_second()
}

/// Floor negative epochs into their civil day before extracting time of day
/// (`DateTime::from_epoch_second`).
pub fn datetime_from_seconds(seconds: i64, precision: Precision) -> Option<DateTime> {
    DateTime::from_epoch_second(seconds, precision)
}

/// Days from 1960-01-01, the SAS date origin, to 1970-01-01.
const SAS_EPOCH_DAYS: i64 = 3653;

/// A date, datetime or time as SAS stores it: days since 1960-01-01, seconds
/// since 1960-01-01T00:00:00 or seconds since midnight (SUB-25, XPT-6); None
/// for any other value.
pub fn sas_number(value: ValueRef<'_>) -> Option<i64> {
    match value {
        ValueRef::Date(date) => Some(i64::from(date_days(date)) + SAS_EPOCH_DAYS),
        ValueRef::DateTime(moment) => Some(datetime_seconds(moment) + SAS_EPOCH_DAYS * 86_400),
        ValueRef::Time(time) => Some(i64::from(time.seconds())),
        _ => None,
    }
}

/// The time of day `seconds` after midnight, or None outside 0 to 86399.
pub fn time_from_seconds(seconds: i64) -> Option<Time> {
    Time::from_seconds(u32::try_from(seconds).ok()?).ok()
}
