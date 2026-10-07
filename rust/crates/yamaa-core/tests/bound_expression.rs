use std::convert::Infallible;
use yamaa_core::{
    bound_expression::{Binding, BindingError, BoundNumeric, BoundPredicate, Read, ScopeError},
    evaluation::NumericResolver,
    numeric_compiler::{compile_numeric, CompileLimits},
    predicate::{Limits, Node, Plan, Scalar},
    value::{Selection, Value},
};

fn binding(name: &str, read: Read) -> Binding {
    Binding {
        name: name.into(),
        read,
    }
}

fn numeric(text: &str, bindings: Vec<Binding>) -> Result<BoundNumeric, BindingError> {
    BoundNumeric::new(
        compile_numeric(text, "columns.RESULT.compute", CompileLimits::default()).unwrap(),
        bindings,
    )
}

fn predicate(path: &str, bindings: Vec<Binding>) -> Result<BoundPredicate, BindingError> {
    BoundPredicate::new(
        Plan::new(
            vec![
                Node::Boolean(false),
                Node::IsNull {
                    value: Scalar::Identifier("A".into()),
                    negated: false,
                },
                Node::And(0, 1),
            ],
            2,
            path.into(),
            "FALSE AND A IS NULL".into(),
            Limits::default(),
        )
        .unwrap(),
        bindings,
    )
}

#[test]
fn numeric_names_are_complete_unique_and_sorted_before_scope_admission() {
    let admitted = numeric(
        "B + A + B",
        vec![binding("B", Read::Column(1)), binding("A", Read::Source(0))],
    )
    .unwrap();
    assert_eq!(
        admitted.bindings(),
        &[binding("A", Read::Source(0)), binding("B", Read::Column(1))]
    );
    assert_eq!(admitted.validate(1, &[false, true], false), Ok(()));
    assert!(admitted.reads_source());
    assert_eq!(
        numeric("A + B", vec![binding("A", Read::Source(0))]),
        Err(BindingError::MissingName)
    );
    assert_eq!(
        numeric(
            "A",
            vec![binding("A", Read::Source(0)), binding("A", Read::Column(0))]
        ),
        Err(BindingError::DuplicateName)
    );
    // Existing admission checks each sorted name before looking for absent names.
    assert_eq!(
        numeric(
            "B",
            vec![binding("A", Read::Source(0)), binding("A", Read::Source(0))]
        ),
        Err(BindingError::UnusedName)
    );
    assert_eq!(
        numeric("A + B", vec![binding("Z", Read::Source(0))]),
        Err(BindingError::UnusedName)
    );
}

#[test]
fn predicate_short_circuiting_does_not_skip_static_binding() {
    assert_eq!(
        predicate("rows[0].filter", vec![]),
        Err(BindingError::MissingName)
    );
    assert_eq!(predicate("", vec![]), Err(BindingError::EmptyPath));
    let admitted = predicate("rows[0].filter", vec![binding("A", Read::Column(0))]).unwrap();
    assert_eq!(admitted.validate(0, &[true], true), Ok(()));
    assert_eq!(
        admitted.validate(0, &[false], false),
        Err(BindingError::UnavailableColumn)
    );
    assert_eq!(
        admitted.validate(0, &[], false),
        Err(BindingError::UnavailableColumn)
    );
    assert_eq!(admitted.plan().spec_path(), "rows[0].filter");
}

#[test]
fn scope_validation_preserves_group_and_sorted_name_error_precedence() {
    let admitted = numeric(
        "A + B",
        vec![binding("B", Read::Column(0)), binding("A", Read::Source(2))],
    )
    .unwrap();
    assert_eq!(
        admitted.validate(1, &[], true),
        Err(ScopeError::GroupedSource)
    );
    assert_eq!(
        admitted.validate(1, &[], false),
        Err(ScopeError::InvalidSource)
    );
    assert_eq!(
        admitted.validate(3, &[], false),
        Err(ScopeError::UnavailableColumn)
    );
    assert_eq!(admitted.validate(3, &[true], false), Ok(()));
    let source = predicate("filter", vec![binding("A", Read::Source(2))]).unwrap();
    assert_eq!(
        source.validate(1, &[], true),
        Err(BindingError::GroupedSource)
    );
    assert_eq!(
        source.validate(1, &[], false),
        Err(BindingError::InvalidSource)
    );
    let output = numeric("A", vec![binding("A", Read::Column(0))]).unwrap();
    assert!(!output.reads_source());
    assert_eq!(output.validate(0, &[true], true), Ok(()));
}

#[derive(Default)]
struct Reads(Vec<String>);
impl NumericResolver for Reads {
    type Error = Infallible;
    fn resolve(&mut self, name: &str) -> Result<Selection, Self::Error> {
        self.0.push(name.into());
        Ok(Selection::Present(Value::Int(1)))
    }
}

#[test]
fn immutable_admission_preserves_deferred_failures_and_occurrences() {
    for text in ["A + A + 1 / 0", "A + A + 9223372036854775808"] {
        let admitted = numeric(text, vec![binding("A", Read::Source(0))]).unwrap();
        for _ in 0..2 {
            let mut reads = Reads::default();
            let error = admitted.expression().evaluate(&mut reads).unwrap_err();
            assert_eq!(reads.0, ["A", "A"]);
            assert_eq!(error.evaluation.location.expression, text);
            assert_eq!(admitted.expression().spec_path(), "columns.RESULT.compute");
        }
    }
}
