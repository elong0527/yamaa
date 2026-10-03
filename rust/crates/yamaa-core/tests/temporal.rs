use yamaa_core::temporal::{Date, DatePrecision as DP, DateTime, DateTimePrecision as TP};

#[test]
fn calendar_boundaries_and_gregorian_century_rules() {
    for text in [
        "0001-01-01",
        "9999-12-31",
        "1600-02-29",
        "2000-02-29",
        "2024-02-29",
    ] {
        let date: Date = text.parse().unwrap();
        assert_eq!(date.to_string(), text);
        assert_eq!(date.collected_precision(), DP::Day);
    }
    for text in [
        "0000-01-01",
        "10000-01-01",
        "1900-02-29",
        "2100-02-29",
        "2025-02-29",
        "2025-04-31",
        "2025-00-01",
        "2025-13-01",
        "2025-01-00",
        "2025-01-32",
    ] {
        assert!(text.parse::<Date>().is_err(), "{text}");
    }
}

#[test]
fn lexical_forms_reject_partial_dates_zones_offsets_and_unicode_digits() {
    for text in [
        "2025-01",
        "20250112",
        "2025-1-2",
        " 2025-01-12",
        "2025-01-12\n",
        "２０２５-01-12",
        "2025-01-12T14:00:00",
        "éé-01-12",
    ] {
        assert!(text.parse::<Date>().is_err(), "{text}");
    }
    for text in [
        "2025-01-12",
        "14:00:00",
        "2025-01-12 14:00:00",
        "2025-01-12t14:00:00",
        "2025-01-12T14:00:00Z",
        "2025-01-12T14:00:00+02:00",
        "2025-01-12T14:00:00.5",
        "2025-01-12T24:00",
        "2025-01-12T23:59:60",
        "2025-01-12T23:60",
        "2025-01-12T4:00",
        "2025-01-12T14:00\n",
        "2025-01-12Té:00",
        "éééééééé",
    ] {
        assert!(text.parse::<DateTime>().is_err(), "{text}");
    }
}

#[test]
fn minute_form_is_complete_to_second_and_dst_has_no_effect() {
    let minute: DateTime = "2025-01-12T14:00".parse().unwrap();
    assert_eq!(minute.to_string(), "2025-01-12T14:00:00");
    assert_eq!(minute.collected_precision(), TP::Second);
    for text in [
        "2025-03-09T02:30:00",
        "2025-11-02T01:30:00",
        "0001-01-01T00:00:00",
        "9999-12-31T23:59:59",
    ] {
        assert_eq!(text.parse::<DateTime>().unwrap().to_string(), text);
    }
}

#[test]
fn precision_survives_selection_but_not_equality_or_canonical_text() {
    let parsed: Date = "2025-01-12".parse().unwrap();
    for precision in [DP::Year, DP::Month, DP::Day] {
        let date = Date::new(2025, 1, 12, precision).unwrap();
        assert_eq!(date, parsed);
        assert_eq!(date.cmp(&parsed), std::cmp::Ordering::Equal);
        assert_eq!(date.collected_precision(), precision);
        assert_eq!(
            date.to_string()
                .parse::<Date>()
                .unwrap()
                .collected_precision(),
            DP::Day
        );
    }
    let a = DateTime::new(parsed, 14, 0, 0, TP::Day).unwrap();
    let b = DateTime::new(parsed, 14, 0, 0, TP::Second).unwrap();
    assert_eq!(a, b);
    assert_eq!(a.cmp(&b), std::cmp::Ordering::Equal);
    assert_eq!(a.collected_precision(), TP::Day);
    assert_eq!(
        a.to_string()
            .parse::<DateTime>()
            .unwrap()
            .collected_precision(),
        TP::Second
    );
    assert!(b < "2025-01-12T14:00:01".parse().unwrap());
}

#[test]
fn a_full_gregorian_cycle_has_146097_distinct_ordered_days() {
    let mut count = 0;
    let mut previous = None;
    for year in 1600..2000 {
        for month in 1..=12 {
            for day in 1..=31 {
                if let Ok(date) = Date::new(year, month, day, DP::Day) {
                    assert_eq!(date.to_string().parse::<Date>(), Ok(date));
                    if let Some(previous) = previous {
                        assert!(previous < date);
                    }
                    previous = Some(date);
                    count += 1;
                }
            }
        }
    }
    assert_eq!(count, 146097);
}
