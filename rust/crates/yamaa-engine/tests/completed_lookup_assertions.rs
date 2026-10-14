//! Same-attempt selected assertions: actual effects, eager order and opaque ownership.
use std::{cell::RefCell, rc::Rc};
use yamaa_core::{
    bound_expression::{Binding, BoundPredicate, Read},
    dataset::{
        Assignment, BoundFunction, Check, DatasetPlan, Expression, Intermediate,
        IntermediateDerivations, IntermediateKey, IntermediateSource, MatchRead, MatchValue,
        RowMode, RowTemplate, SecondarySource, SourceSchemas, Verification,
    },
    predicate_compiler,
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    value::{ColumnType as T, Value},
};
use yamaa_engine::{
    dataset::{DatasetExecution, ExecutionError, Limits},
    function_invocation::{Argument, FunctionIdentity, FunctionPort, HostError, InvocationPlan},
};

#[derive(Debug)]
struct Failure(Box<u8>, Rc<()>);
type Trace = Rc<RefCell<Vec<(&'static str, usize, usize)>>>;
struct Table {
    name: &'static str,
    schema: TableSchema,
    rows: Vec<Vec<Value>>,
    trace: Trace,
    fail: RefCell<Option<(usize, usize, Failure)>>,
}
impl TableAccess for Table {
    type Error = Failure;
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    fn row_count(&self) -> usize {
        self.rows.len()
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Failure>> {
        self.trace.borrow_mut().push((self.name, row, column));
        if self
            .fail
            .borrow()
            .as_ref()
            .is_some_and(|(r, c, _)| (*r, *c) == (row, column))
        {
            return Err(CellError::Access(self.fail.borrow_mut().take().unwrap().2));
        }
        Ok((&self.rows[row][column]).into())
    }
}
fn schema(names: &[(&str, T)]) -> TableSchema {
    TableSchema::new(
        names
            .iter()
            .map(|(name, kind)| Column {
                name: (*name).into(),
                kind: *kind,
            })
            .collect(),
    )
    .unwrap()
}
fn tables() -> (Table, Table, Trace) {
    let trace = Rc::new(RefCell::new(Vec::new()));
    (
        Table {
            name: "SRC",
            schema: schema(&[("ID", T::Int)]),
            rows: vec![vec![Value::Int(1)]],
            trace: trace.clone(),
            fail: RefCell::default(),
        },
        Table {
            name: "DONOR",
            schema: schema(&[("ID", T::Int), ("X", T::Int), ("Z", T::Int)]),
            rows: vec![vec![Value::Int(1), Value::Int(2), Value::Int(3)]],
            trace: trace.clone(),
            fail: RefCell::default(),
        },
        trace,
    )
}
fn assignment(column: usize, expression: Expression) -> Assignment {
    Assignment {
        column,
        expression,
        path: format!("columns.C{column}.derivation"),
    }
}
fn predicate(text: &str, part: &str) -> BoundPredicate {
    let compiled = predicate_compiler::compile(
        text,
        &format!("verifications[1].assert.{part}"),
        Default::default(),
    )
    .unwrap();
    let bindings = compiled
        .identifiers()
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .map(|name| Binding {
            name: name.into(),
            read: match name {
                "P.X" => Read::Intermediate {
                    index: 0,
                    column: 1,
                },
                "P.Z" => Read::Intermediate {
                    index: 0,
                    column: 2,
                },
                "VALUE" => Read::Column(1),
                _ => panic!("authored name"),
            },
        })
        .collect();
    BoundPredicate::new_contextual(compiled, bindings, vec![]).unwrap()
}
fn plan(
    src: &Table,
    donor: &Table,
    when: Option<&str>,
    require: &str,
    construction_lookup: bool,
    no_match: bool,
) -> DatasetPlan {
    build_plan(
        src,
        donor,
        when,
        require,
        construction_lookup,
        no_match,
        None,
    )
}
fn build_plan(
    src: &Table,
    donor: &Table,
    when: Option<&str>,
    require: &str,
    construction_lookup: bool,
    no_match: bool,
    derived: Option<IntermediateDerivations>,
) -> DatasetPlan {
    DatasetPlan::new_with_intermediates(
        SourceSchemas {
            primary: src.schema.clone(),
            secondary: vec![SecondarySource {
                name: "DONOR".into(),
                schema: donor.schema.clone(),
            }],
        },
        vec![Intermediate {
            identifier: "P".into(),
            path: "intermediates[0]".into(),
            source: IntermediateSource::Input(0),
            keys: vec![IntermediateKey {
                source_column: 0,
                path: "intermediates[0].key".into(),
                value: MatchValue::Variable(MatchRead::Output {
                    column: 0,
                    identifier: "ID".into(),
                }),
            }],
            filter: None,
            selection: None,
            no_match: no_match.then_some(Value::Missing),
            unique: vec![],
            derivations: derived,
            pending_schema: None,
            range: None,
            columns: None,
        }],
        schema(&[("ID", T::Int), ("VALUE", T::Int)]),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![assignment(0, Expression::Source(0))],
            filter: None,
        }],
        vec![assignment(
            1,
            if construction_lookup {
                Expression::Intermediate {
                    index: 0,
                    column: 1,
                }
            } else {
                Expression::Literal(Value::Int(2))
            },
        )],
        vec![0],
        vec![
            Verification {
                path: "verifications[0].unique".into(),
                check: Check::Unique(vec![0]),
            },
            Verification {
                path: "verifications[1].assert".into(),
                check: Check::Assert {
                    when: when.map(|text| predicate(text, "when")),
                    require: predicate(require, "require"),
                },
            },
            Verification {
                path: "verifications[2].row_count".into(),
                check: Check::RowCount {
                    min: Some(0),
                    max: None,
                },
            },
        ],
    )
    .unwrap()
}
fn limits() -> Limits {
    Limits {
        source_rows: 100,
        output_rows: 100,
        output_cells: 1000,
        key_cells: 1000,
        work_cells: 100_000,
        scalar_text_bytes: 100_000,
        output_text_bytes: 100_000,
        identity_cells: 1000,
        identity_text_bytes: 100_000,
    }
}

#[test]
fn selected_choice_is_shared_with_construction_when_require_and_next_attempt_is_fresh() {
    let (src, donor, trace) = tables();
    let plan = plan(
        &src,
        &donor,
        Some("P.X > 0"),
        "P.X = VALUE AND P.Z = 3",
        true,
        false,
    );
    for _ in 0..2 {
        trace.borrow_mut().clear();
        let attempt = plan.execute_observed_sources(&src, &[&donor], limits());
        let execution = attempt.result.unwrap();
        assert_eq!(
            execution.dataset.rows(),
            [vec![Value::Int(1), Value::Int(2)]]
        );
        assert_eq!(
            execution
                .verifications
                .iter()
                .map(|v| (v.path.as_str(), v.evaluated_count, v.failed_count))
                .collect::<Vec<_>>(),
            [
                ("verifications[0].unique", 1, 0),
                ("verifications[1].assert", 1, 0),
                ("verifications[2].row_count", 1, 0)
            ]
        );
        assert!(attempt.handler_counts.is_empty());
        assert_eq!(
            *trace.borrow(),
            [
                ("SRC", 0, 0),
                ("DONOR", 0, 0),
                ("DONOR", 0, 1),
                ("DONOR", 0, 1),
                ("DONOR", 0, 1),
                ("DONOR", 0, 2)
            ]
        );
    }
}
#[test]
fn false_when_still_evaluates_require_and_preserves_original_noncloneable_donor_error() {
    let (src, donor, trace) = tables();
    let payload = Box::new(9);
    let address = &*payload as *const u8;
    let marker = Rc::new(());
    *donor.fail.borrow_mut() = Some((0, 1, Failure(payload, marker.clone())));
    let attempt = plan(&src, &donor, Some("FALSE"), "P.X = 2", false, false)
        .execute_observed_sources(&src, &[&donor], limits());
    let ExecutionError::SecondaryCell {
        path,
        source,
        source_row,
        error: CellError::Access(Failure(payload, retained)),
    } = *attempt.result.unwrap_err()
    else {
        panic!("original opaque source owner")
    };
    assert_eq!(
        (path.as_str(), source, source_row),
        ("intermediates[0]", 0, 0),
        "the assertion context must not invent a destination column"
    );
    assert_eq!(&*payload as *const u8, address);
    assert!(Rc::ptr_eq(&retained, &marker));
    assert_eq!(
        *trace.borrow(),
        [("SRC", 0, 0), ("DONOR", 0, 0), ("DONOR", 0, 1)]
    );
    assert_eq!(attempt.retained_verifications.len(), 1);
    assert_eq!(
        attempt.retained_verifications[0].path,
        "verifications[0].unique"
    );
    assert_eq!(attempt.retained_verifications[0].evaluated_count, 1);
    assert!(attempt.handler_counts.is_empty());
}
#[test]
fn when_failure_precedes_require_and_keeps_original_opaque_payload_and_prefix() {
    let (src, donor, trace) = tables();
    let marker = Rc::new(());
    let payload = Box::new(7);
    let address = &*payload as *const u8;
    *donor.fail.borrow_mut() = Some((0, 2, Failure(payload, marker.clone())));
    let attempt = plan(&src, &donor, Some("P.Z > 0"), "P.X > 0", false, false)
        .execute_observed_sources(&src, &[&donor], limits());
    let ExecutionError::SecondaryCell {
        path,
        error: CellError::Access(Failure(payload, retained)),
        ..
    } = *attempt.result.unwrap_err()
    else {
        panic!("when opaque owner")
    };
    assert_eq!(path, "intermediates[0]");
    assert_eq!(&*payload as *const u8, address);
    assert!(Rc::ptr_eq(&marker, &retained));
    assert_eq!(
        *trace.borrow(),
        [("SRC", 0, 0), ("DONOR", 0, 0), ("DONOR", 0, 2)]
    );
    assert_eq!(attempt.retained_verifications.len(), 1);
}
#[test]
fn absent_lookup_preserves_intermediate_diagnostic_identity_without_column_attribution() {
    let (mut src, donor, trace) = tables();
    src.rows[0][0] = Value::Int(8);
    let attempt = plan(&src, &donor, Some("FALSE"), "P.X > 0", false, false)
        .execute_observed_sources(&src, &[&donor], limits());
    let ExecutionError::UnmatchedKey {
        path,
        dataset,
        intermediate,
        matched_key,
        identity: Some(identity),
    } = *attempt.result.unwrap_err()
    else {
        panic!("native selected-record no-match condition")
    };
    assert_eq!(
        (path.as_str(), dataset.as_str(), intermediate.as_str()),
        ("intermediates[0]", "DONOR", "P")
    );
    assert_eq!(matched_key, [("ID".into(), Value::Int(8))]);
    assert_eq!(identity.position, 0);
    assert_eq!(identity.values, [Value::Int(8)]);
    assert_eq!(*trace.borrow(), [("SRC", 0, 0), ("DONOR", 0, 0)]);
    assert_eq!(attempt.retained_verifications.len(), 1);
}
#[test]
fn zero_output_type_admission_reads_no_donor_cells_or_functions() {
    let (mut src, donor, trace) = tables();
    src.rows.clear();
    let attempt = plan(&src, &donor, None, "P.X = 'invalid'", false, false)
        .execute_observed_sources(&src, &[&donor], limits());
    let ExecutionError::VerificationPredicate { error, records } = *attempt.result.unwrap_err()
    else {
        panic!("schema-only assertion type condition")
    };
    assert_eq!(error.spec_path, "verifications[1].assert.require");
    assert!(matches!(
        error.kind,
        yamaa_core::predicate::ErrorKind::Condition(_)
    ));
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].evaluated_count, 0);
    assert!(trace.borrow().is_empty());
    assert!(attempt.handler_counts.is_empty());
}

struct Calls {
    signature: InvocationPlan,
    count: usize,
}
impl FunctionPort for Calls {
    type Error = Failure;
    fn signature(&self, slot: usize) -> Option<&InvocationPlan> {
        (slot == 0).then_some(&self.signature)
    }
    fn call(&mut self, _: usize, _: &[Argument<'_>]) -> Result<Value, HostError<Failure>> {
        self.count += 1;
        Ok(Value::Int(2))
    }
}
#[test]
fn derived_donor_callback_counts_follow_one_materialization_per_attempt() {
    let (src, mut donor, trace) = tables();
    donor.schema = schema(&[("ID", T::Int)]);
    donor.rows = vec![vec![Value::Int(1)], vec![Value::Int(2)]];
    let signature = InvocationPlan::new(
        FunctionIdentity {
            name: "chosen".into(),
            contract_version: "1".into(),
            implementation_version: "1".into(),
            call: "project.chosen".into(),
        },
        vec![],
        T::Int,
        false,
    )
    .unwrap();
    let output = schema(&[("ID", T::Int), ("X", T::Int)]);
    let nested = DatasetPlan::new(
        output.clone(),
        output,
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![assignment(0, Expression::Source(0))],
            filter: None,
        }],
        vec![assignment(
            1,
            Expression::Function(BoundFunction::new(0, signature.clone(), vec![]).unwrap()),
        )],
        vec![0],
        vec![],
    )
    .unwrap();
    let derived = IntermediateDerivations {
        plan: Rc::new(nested),
        current: vec![],
        stored_columns: 1,
        driver_kinds: vec![],
    };
    let plan = build_plan(
        &src,
        &donor,
        Some("P.X > 0"),
        "P.X = VALUE",
        true,
        false,
        Some(derived),
    );
    let mut calls = Calls {
        signature,
        count: 0,
    };
    for expected in [2, 4] {
        trace.borrow_mut().clear();
        let attempt = plan.execute_observed_functions(&src, &[&donor], &mut calls, limits());
        assert_eq!(
            attempt.result.unwrap().dataset.rows(),
            [vec![Value::Int(1), Value::Int(2)]]
        );
        assert_eq!(
            calls.count, expected,
            "no callback repeats for WHEN/REQUIRE or report projection"
        );
        assert_eq!(
            *trace.borrow(),
            [("SRC", 0, 0), ("DONOR", 0, 0), ("DONOR", 1, 0)]
        );
    }
    let mut empty = src;
    empty.rows.clear();
    trace.borrow_mut().clear();
    let empty_plan = plan;
    assert!(empty_plan
        .execute_observed_functions(&empty, &[&donor], &mut calls, limits())
        .result
        .is_ok());
    assert_eq!(calls.count, 4);
    assert!(trace.borrow().is_empty());
}
