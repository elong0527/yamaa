//! Metadata never coerces an unknown derivation into a stored-column kind.
use yamaa_core::{
    dataset::{
        CurrentField, CurrentRead, RelationField, RelationFieldOrigin, RelationRange,
        RelationRangeBound, RelationRangeError, RelationShape, RelationShapeError,
    },
    value::{ColumnType, ValueType},
};
fn fields() -> Vec<RelationField> {
    vec![
        RelationField::stored(0, "ID".into(), ColumnType::Int),
        RelationField::stored(1, "LOW".into(), ColumnType::Float),
        RelationField {
            identifier: "DERIVED".into(),
            kind: None,
            origin: RelationFieldOrigin::Derived(0),
        },
        RelationField {
            identifier: "FLAG".into(),
            kind: Some(ValueType::Bool),
            origin: RelationFieldOrigin::Derived(1),
        },
    ]
}
#[test]
fn projection_restricts_reads_while_key_filter_and_order_metadata_stay_available() {
    let shape = RelationShape::new(fields(), Some(vec![2])).unwrap();
    assert_eq!(shape.visible_index("DERIVED"), Some(2));
    assert_eq!(shape.visible_index("LOW"), None);
    assert_eq!(shape.index("LOW"), Some(1));
    assert_eq!(shape.fields()[2].kind, None);
    assert_eq!(shape.fields()[3].kind, Some(ValueType::Bool));
}
#[test]
fn declaration_shadowing_wrong_origins_and_invalid_projection_cannot_publish_a_shape() {
    let mut f = fields();
    f[2].identifier = "LOW".into();
    assert_eq!(
        RelationShape::new(f, None),
        Err(RelationShapeError::DuplicateIdentifier)
    );
    let mut f = fields();
    f[2].origin = RelationFieldOrigin::Derived(1);
    assert_eq!(
        RelationShape::new(f, None),
        Err(RelationShapeError::InvalidOrigin)
    );
    for projection in [vec![4], vec![1, 1], vec![usize::MAX]] {
        assert_eq!(
            RelationShape::new(fields(), Some(projection)),
            Err(RelationShapeError::InvalidProjection)
        );
    }
}
#[test]
fn only_derived_fields_may_have_unknown_or_boolean_logical_kind() {
    for kind in [None, Some(ValueType::Bool)] {
        let mut fields = fields();
        fields[0].kind = kind;
        assert_eq!(
            RelationShape::new(fields, None),
            Err(RelationShapeError::InvalidKind)
        );
    }
}
#[test]
fn empty_projection_hides_every_field_without_discarding_donor_metadata() {
    let shape = RelationShape::new(fields(), Some(vec![])).unwrap();
    assert_eq!(shape.visible_index("ID"), None);
    assert_eq!(shape.index("ID"), Some(0));
    assert_eq!(shape.fields().len(), 4);
}
#[test]
fn fixed_relation_width_precedes_coordinate_admission_and_keeps_exact_shape() {
    let fields = |count| {
        (0..count)
            .map(|index| RelationField::stored(index, format!("F{index}"), ColumnType::Int))
            .collect()
    };
    let shape = RelationShape::new(fields(1024), Some(vec![1023])).unwrap();
    assert_eq!(shape.fields().len(), 1024);
    assert_eq!(shape.visible_index("F1023"), Some(1023));
    assert_eq!(
        RelationShape::new(fields(1025), Some(vec![usize::MAX])),
        Err(RelationShapeError::Limit(
            yamaa_core::limits::LimitExceeded {
                resource: "columns",
                limit: 1024,
                actual: Some(1025)
            }
        ))
    );
    assert_eq!(
        RelationShape::new(fields(1024), Some(vec![usize::MAX])),
        Err(RelationShapeError::InvalidProjection)
    );
}

#[test]
fn unknown_derived_bound_kind_survives_while_forged_coordinates_or_kinds_are_rejected() {
    let shape = RelationShape::new(fields(), None).unwrap();
    let make = |column, kind| {
        RelationRange::new(
            "range".into(),
            CurrentField {
                read: CurrentRead::Output(0),
                identifier: "V".into(),
            },
            None,
            Some(RelationRangeBound {
                column,
                identifier: "DERIVED".into(),
                kind,
            }),
            None,
        )
        .unwrap()
    };
    assert!(make(2, None).validate_shape(&shape).is_ok());
    assert_eq!(
        make(2, Some(ValueType::Str)).validate_shape(&shape),
        Err(RelationRangeError::InvalidMetadata)
    );
    assert_eq!(
        make(usize::MAX, None).validate_shape(&shape),
        Err(RelationRangeError::InvalidMetadata)
    );
}
