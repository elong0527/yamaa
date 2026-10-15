use core::cmp::Ordering;
use yamaa_core::dataset::{Check, CountBoundError, RowCountBound, MAX_COUNT_BOUND_BYTES};

fn bound(value: &str) -> RowCountBound {
    RowCountBound::new(value).unwrap()
}

#[test]
fn exact_count_bounds_preserve_decimal_order_and_machine_count_width() {
    let values = [
        "-9999999999999999999999999999999999999",
        "-9223372036854775809",
        "-9223372036854775808",
        "-10",
        "-2",
        "-1",
        "0",
        "1",
        "2",
        "10",
        "9223372036854775807",
        "9223372036854775808",
        "18446744073709551615",
        "18446744073709551616",
        "9999999999999999999999999999999999999",
    ];
    for (i, a) in values.iter().enumerate() {
        for (j, b) in values.iter().enumerate() {
            assert_eq!(bound(a).cmp(&bound(b)), i.cmp(&j));
        }
    }
    assert_eq!(bound("0").compare_count(0), Ordering::Equal);
    assert_eq!(bound("-1").compare_count(0), Ordering::Less);
    assert_eq!(bound("1").compare_count(0), Ordering::Greater);
    assert_eq!(
        bound(&usize::MAX.to_string()).compare_count(usize::MAX),
        Ordering::Equal
    );
    assert_eq!(
        bound("9999999999999999999999999999999999999").compare_count(usize::MAX),
        Ordering::Greater
    );
    let check = Check::WideRowCount {
        min: Some(bound("-9999999999999999999999999999999999999")),
        max: Some(bound("9999999999999999999999999999999999999")),
    };
    assert_eq!(check.row_count_accepts(0), Some(true));
    assert_eq!(check.row_count_accepts(usize::MAX), Some(true));
    for count in [0, 1, 2, usize::MAX] {
        assert_eq!(
            Check::RowCount {
                min: Some(-1),
                max: Some(i64::MAX)
            }
            .row_count_accepts(count),
            Check::WideRowCount {
                min: Some(bound("-1")),
                max: Some(bound(&i64::MAX.to_string()))
            }
            .row_count_accepts(count)
        );
    }
}

#[test]
fn bound_admission_precedes_copy_and_rejects_noncanonical_spellings() {
    for invalid in ["", "-", "+1", "-0", "00", "01", "-01", " 1", "1.0", "١"] {
        assert_eq!(RowCountBound::new(invalid), Err(CountBoundError::Invalid));
    }
    let boundary = "9".repeat(MAX_COUNT_BOUND_BYTES);
    assert_eq!(bound(&boundary).decimal(), boundary);
    assert_eq!(
        RowCountBound::new(&(boundary + "9")),
        Err(CountBoundError::Limit)
    );
}
