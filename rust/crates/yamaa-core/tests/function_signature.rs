use yamaa_core::{
    function_signature::{FunctionIdentity, InvocationPlan, Parameter, PlanError, Presence},
    value::{ColumnType, Value, ValueType},
};

fn identity() -> FunctionIdentity {
    FunctionIdentity {
        name: "example".into(),
        contract_version: "1".into(),
        implementation_version: "2".into(),
        call: "artifact.example".into(),
    }
}
/// No accepted plan can carry duplicate names, invalid defaults or empty identity.
#[test]
fn plans_reject_invalid_normalized_signatures() {
    let base = vec![Parameter {
        name: "x".into(),
        host_name: "host_x".into(),
        kind: ValueType::Int,
        accepts_missing: false,
        presence: Presence::Required,
    }];
    let build = |p| InvocationPlan::new(identity(), p, ColumnType::Int, false);
    for name in ["", "1x", "bad-name", "snow\u{96ea}"] {
        let mut p = base.clone();
        p[0].name = name.into();
        assert_eq!(build(p), Err(PlanError::InvalidName { parameter: 0 }));
    }
    assert_eq!(
        build(vec![base[0].clone(), base[0].clone()]),
        Err(PlanError::DuplicateName { parameter: 1 })
    );
    let mut p = vec![base[0].clone(), base[0].clone()];
    p[1].name = "y".into();
    assert_eq!(build(p), Err(PlanError::DuplicateHostName { parameter: 1 }));
    let mut p = base.clone();
    p[0].host_name.clear();
    assert_eq!(build(p), Err(PlanError::EmptyHostName { parameter: 0 }));
    for default in [Value::Missing, Value::float(1.0), Value::Bool(true)] {
        let mut p = base.clone();
        p[0].presence = Presence::Optional(default);
        assert_eq!(build(p), Err(PlanError::InvalidDefault { parameter: 0 }));
    }
    for slot in 0..4 {
        let mut id = identity();
        [
            &mut id.name,
            &mut id.contract_version,
            &mut id.implementation_version,
            &mut id.call,
        ][slot]
            .clear();
        assert_eq!(
            InvocationPlan::new(id, base.clone(), ColumnType::Int, false),
            Err(PlanError::EmptyIdentity)
        );
    }
}

#[test]
fn signature_preserves_authored_order_and_missing_defaults() {
    let parameters = vec![
        Parameter {
            name: "z".into(),
            host_name: "host_z".into(),
            kind: ValueType::Int,
            accepts_missing: true,
            presence: Presence::Optional(Value::Missing),
        },
        Parameter {
            name: "a".into(),
            host_name: "host_a".into(),
            kind: ValueType::Int,
            accepts_missing: false,
            presence: Presence::Required,
        },
    ];
    let admitted =
        InvocationPlan::new(identity(), parameters.clone(), ColumnType::Date, true).unwrap();
    assert_eq!(admitted.parameters(), parameters);
    assert_eq!(admitted.identity(), &identity());
    assert_eq!(admitted.returns(), ColumnType::Date);
    assert!(admitted.may_return_missing());
}
