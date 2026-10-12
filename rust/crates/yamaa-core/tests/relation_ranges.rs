//! Inclusive range narrowing preserves original endpoint effects and exact values.
use yamaa_core::{
    dataset::{
        CurrentField, CurrentRead, RelationRange, RelationRangeBound, RelationRangeError,
        RelationRangeFailure, RelationRangeTypes,
    },
    table::ValueRef,
    value::{FiniteFloat, ValueType},
};
fn bound(column: usize, kind: Option<ValueType>) -> RelationRangeBound {
    RelationRangeBound {
        column,
        identifier: if column == 0 { "LOW" } else { "HIGH" }.into(),
        kind,
    }
}
fn range(lower: bool, upper: bool) -> RelationRange {
    RelationRange::new(
        "intermediates[0].between".into(),
        CurrentField {
            read: CurrentRead::Output(2),
            identifier: "VALUE".into(),
        },
        None,
        lower.then(|| bound(0, None)),
        upper.then(|| bound(1, None)),
        16_777_216,
    )
    .unwrap()
}
#[test]
fn declared_bounds_are_optional_but_cannot_both_be_omitted() {
    for (lo, hi) in [(true, false), (false, true), (true, true)] {
        let mut reads = Vec::new();
        assert!(range(lo, hi)
            .allows(ValueRef::Int(7), |column| {
                reads.push(column);
                Ok::<_, ()>(ValueRef::Int(7))
            })
            .unwrap());
        assert_eq!(
            reads,
            [(lo, 0), (hi, 1)]
                .into_iter()
                .filter_map(|(stated, column)| stated.then_some(column))
                .collect::<Vec<_>>()
        );
    }
    assert_eq!(
        RelationRange::new(
            "range".into(),
            CurrentField {
                read: CurrentRead::Output(0),
                identifier: "V".into()
            },
            None,
            None,
            None,
            4096
        ),
        Err(RelationRangeError::NoBounds)
    );
}
#[test]
fn missing_current_or_stated_lower_stops_before_later_opaque_reads() {
    let mut reads = Vec::new();
    assert!(!range(true, true)
        .allows(ValueRef::Missing, |column| {
            reads.push(column);
            Err::<ValueRef<'_>, _>("unexpected read")
        })
        .unwrap());
    assert!(reads.is_empty());
    assert!(!range(true, true)
        .allows(ValueRef::Int(7), |column| {
            reads.push(column);
            if column == 0 {
                Ok(ValueRef::Missing)
            } else {
                Err("upper must stay unread")
            }
        })
        .unwrap());
    assert_eq!(reads, vec![0]);
    reads.clear();
    assert!(!range(true, true)
        .allows(ValueRef::Int(7), |column| {
            reads.push(column);
            Ok::<_, ()>(if column == 0 {
                ValueRef::Int(1)
            } else {
                ValueRef::Missing
            })
        })
        .unwrap());
    assert_eq!(reads, vec![0, 1]);
}
#[test]
fn both_present_endpoints_are_read_before_lower_exclusion_and_upper_comparison_is_lazy() {
    let mut reads = Vec::new();
    assert!(!range(true, true)
        .allows(ValueRef::Int(3), |column| {
            reads.push(column);
            Ok::<_, ()>(if column == 0 {
                ValueRef::Int(4)
            } else {
                ValueRef::Str("unused incompatible comparison")
            })
        })
        .unwrap());
    assert_eq!(reads, vec![0, 1]);
}
#[test]
fn endpoint_access_failure_preserves_original_box_and_complete_read_prefix() {
    #[derive(Debug, PartialEq, Eq)]
    struct Opaque(&'static str);
    let mut original = Some(Box::new(Opaque("held source failure")));
    let address = original.as_ref().unwrap().as_ref() as *const Opaque;
    let mut reads = Vec::new();
    let error = range(true, true)
        .allows(ValueRef::Int(3), |column| {
            reads.push(column);
            if column == 0 {
                Ok(ValueRef::Int(4))
            } else {
                Err(original.take().unwrap())
            }
        })
        .unwrap_err();
    assert_eq!(reads, vec![0, 1]);
    let RelationRangeFailure::Access(error) = error else {
        panic!("original access owner expected")
    };
    assert_eq!(error.as_ref() as *const Opaque, address);
}
#[test]
fn runtime_type_failure_reports_all_present_endpoint_types_after_both_reads() {
    let mut reads = Vec::new();
    let error = range(true, true)
        .allows(ValueRef::Int(7), |column| {
            reads.push(column);
            Ok::<_, ()>(if column == 0 {
                ValueRef::Str("wrong")
            } else {
                ValueRef::Float(FiniteFloat::new(8.0).unwrap())
            })
        })
        .unwrap_err();
    assert_eq!(reads, vec![0, 1]);
    assert_eq!(
        error,
        RelationRangeFailure::Incomparable(RelationRangeTypes {
            value: ValueType::Int,
            lower: Some(ValueType::Str),
            upper: Some(ValueType::Float)
        })
    );
}
#[test]
fn large_signed_integer_bounds_compare_without_rounding_to_binary64() {
    let value = 9_007_199_254_740_993;
    assert!(!range(false, true)
        .allows(ValueRef::Int(value), |_| Ok::<_, ()>(ValueRef::Float(
            FiniteFloat::new(9_007_199_254_740_992.0).unwrap()
        )))
        .unwrap());
    assert!(range(true, true)
        .allows(ValueRef::Int(i64::MAX), |column| Ok::<_, ()>(
            if column == 0 {
                ValueRef::Int(i64::MIN)
            } else {
                ValueRef::Int(i64::MAX)
            }
        ))
        .unwrap());
    assert!(range(true, false)
        .allows(ValueRef::Int(i64::MIN), |_| Ok::<_, ()>(ValueRef::Float(
            FiniteFloat::new(-9_223_372_036_854_775_808.0).unwrap()
        )))
        .unwrap());
}
#[test]
fn known_type_mismatch_is_refused_before_any_endpoint_context_exists() {
    assert_eq!(
        RelationRange::new(
            "range".into(),
            CurrentField {
                read: CurrentRead::Output(0),
                identifier: "DATE".into()
            },
            Some(ValueType::Date),
            Some(bound(0, Some(ValueType::Int))),
            Some(bound(1, Some(ValueType::Int))),
            4096
        ),
        Err(RelationRangeError::Incomparable(RelationRangeTypes {
            value: ValueType::Date,
            lower: Some(ValueType::Int),
            upper: Some(ValueType::Int)
        }))
    );
}
