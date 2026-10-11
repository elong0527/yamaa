//! Complete record attempts have stable choices and separate diagnostic positions.
use std::{cell::RefCell, rc::Rc};
use yamaa_core::{
    bound_expression::{Binding, BoundPredicate, Read},
    dataset::{
        Assignment, DatasetPlan, Expression, Intermediate, IntermediateKey, MatchRead, MatchValue,
        RowMode, RowTemplate, SecondarySource, SourceSchemas,
    },
    predicate::{Comparison, Node, Scalar},
    schema::{
        Document, DocumentLimits, DocumentNode as N, SpecificationDocument, ValidationBudget,
    },
    specification::{PreparedSpecification, SourceDeclaration},
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    value::{ColumnType as T, Value},
};
use yamaa_engine::dataset::{DatasetExecution, ExecutionError, Limits, Resource};
#[derive(Debug)]
struct Failure(Box<u8>, Rc<()>);
type Trace = Rc<RefCell<Vec<(&'static str, usize, usize)>>>;
struct Table {
    name: &'static str,
    schema: TableSchema,
    rows: Vec<Vec<Value>>,
    fail: RefCell<Option<(usize, usize, Failure)>>,
    trace: Trace,
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
            .is_some_and(|(r, c, _)| *r == row && *c == column)
        {
            return Err(CellError::Access(self.fail.borrow_mut().take().unwrap().2));
        }
        Ok(ValueRef::from(&self.rows[row][column]))
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
    let text = |s: &str| Value::Str(s.into());
    (
        Table {
            name: "SRC",
            schema: schema(&[("ID", T::Str), ("K", T::Str), ("KEEP", T::Str)]),
            rows: vec![
                vec![text("a"), text("x"), text("0")],
                vec![text("b"), text("y"), text("1")],
                vec![text("c"), text("z"), text("0")],
            ],
            fail: RefCell::default(),
            trace: trace.clone(),
        },
        Table {
            name: "DONOR",
            schema: schema(&[("K", T::Str), ("V", T::Str)]),
            rows: vec![vec![text("x"), text("X")], vec![text("y"), text("Y")]],
            fail: RefCell::default(),
            trace: trace.clone(),
        },
        trace,
    )
}
fn filter(value: &str) -> BoundPredicate {
    BoundPredicate::new(
        yamaa_core::predicate::Plan::new(
            vec![Node::Compare {
                operator: Comparison::Equal,
                left: Scalar::Identifier("SRC.KEEP".into()),
                right: Scalar::Literal(Value::Str(value.into())),
            }],
            0,
            "rows.filter".into(),
            format!("SRC.KEEP = '{value}'"),
            Default::default(),
        )
        .unwrap(),
        vec![Binding {
            name: "SRC.KEEP".into(),
            read: Read::Source(2),
        }],
    )
    .unwrap()
}
fn plan(src: &Table, donor: &Table, same_record: bool, target: T, absence: bool) -> DatasetPlan {
    let assign = |column, expression, path: String| Assignment {
        column,
        expression,
        path,
    };
    let templates = ["L", "H"]
        .iter()
        .enumerate()
        .map(|(index, section)| {
            let path = |name| format!("rows[{index}].derivations.{name}.source");
            RowTemplate {
                mode: RowMode::Records,
                assignments: vec![
                    assign(0, Expression::Source(0), path("ID")),
                    assign(
                        1,
                        Expression::Literal(Value::Str((*section).into())),
                        path("SECTION"),
                    ),
                    assign(
                        2,
                        if same_record {
                            Expression::Literal(Value::Str(
                                if index == 0 { "x" } else { "y" }.into(),
                            ))
                        } else {
                            Expression::Source(1)
                        },
                        path("KEY"),
                    ),
                    assign(
                        3,
                        Expression::Intermediate {
                            index: 0,
                            column: 1,
                        },
                        path("ROWV"),
                    ),
                ],
                filter: (!same_record).then(|| filter(if index == 0 { "0" } else { "1" })),
            }
        })
        .collect();
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
            source: 0,
            keys: vec![IntermediateKey {
                source_column: 0,
                path: "intermediates[0].key".into(),
                value: MatchValue::Variable(MatchRead::Output {
                    column: 2,
                    identifier: "KEY".into(),
                }),
            }],
            filter: None,
            selection: None,
            no_match: absence.then_some(Value::Missing),
        }],
        schema(&[
            ("ID", T::Str),
            ("SECTION", T::Str),
            ("KEY", T::Str),
            ("ROWV", target),
            ("COLV", T::Str),
        ]),
        templates,
        vec![assign(
            4,
            Expression::Intermediate {
                index: 0,
                column: 1,
            },
            "columns.COLV.derivation.source".into(),
        )],
        vec![0, 1],
        vec![],
    )
    .unwrap()
}
fn limits() -> Limits {
    Limits {
        source_rows: 100,
        output_rows: 100,
        output_cells: 100,
        key_cells: 100,
        work_cells: 10_000,
        scalar_text_bytes: 10_000,
        output_text_bytes: 10_000,
        identity_cells: 100,
        identity_text_bytes: 10_000,
    }
}
#[test]
fn discarded_attempts_never_reuse_choices_and_retained_columns_reuse_their_token() {
    let (src, donor, trace) = tables();
    let attempt =
        plan(&src, &donor, false, T::Str, true).execute_observed_sources(&src, &[&donor], limits());
    let t = |s: &str| Value::Str(s.into());
    assert_eq!(
        attempt.result.unwrap().dataset.rows(),
        [
            vec![t("a"), t("L"), t("x"), t("X"), t("X")],
            vec![t("c"), t("L"), t("z"), Value::Missing, Value::Missing],
            vec![t("b"), t("H"), t("y"), t("Y"), t("Y")]
        ]
    );
    assert_eq!(
        attempt
            .handler_counts
            .iter()
            .map(|h| (h.spec_path.as_str(), h.handler.name(), h.count))
            .collect::<Vec<_>>(),
        [
            ("rows[0].derivations.ROWV.source.no_match", "no_match", 1),
            ("rows[1].derivations.ROWV.source.no_match", "no_match", 1),
            ("columns.COLV.derivation.source.no_match", "no_match", 1)
        ]
    );
    assert_eq!(
        trace
            .borrow()
            .iter()
            .filter(|&&(name, _, col)| name == "DONOR" && col == 0)
            .count(),
        12,
        "six choices, no extra column selection"
    );
    assert_eq!(
        trace
            .borrow()
            .iter()
            .filter(|&&(name, _, col)| name == "DONOR" && col == 1)
            .count(),
        6,
        "cached choice still reads each value occurrence"
    );
}
#[test]
fn the_same_physical_record_under_two_templates_has_distinct_completed_match_values() {
    let (src, donor, trace) = tables();
    let attempt =
        plan(&src, &donor, true, T::Str, true).execute_observed_sources(&src, &[&donor], limits());
    let rows = attempt.result.unwrap().dataset.rows().to_vec();
    assert_eq!(rows.len(), 6);
    for row in &rows[..3] {
        assert_eq!(row[3..], [Value::Str("X".into()), Value::Str("X".into())]);
    }
    for row in &rows[3..] {
        assert_eq!(row[3..], [Value::Str("Y".into()), Value::Str("Y".into())]);
    }
    assert!(attempt.handler_counts.is_empty());
    assert_eq!(
        trace
            .borrow()
            .iter()
            .filter(|&&(name, _, col)| name == "DONOR" && col == 0)
            .count(),
        12
    );
}
#[test]
fn opaque_selected_cell_identity_survives_and_prevents_filter_and_later_reads() {
    let (src, donor, trace) = tables();
    let payload = Box::new(9);
    let address = &*payload as *const u8;
    let marker = Rc::new(());
    *donor.fail.borrow_mut() = Some((0, 1, Failure(payload, marker.clone())));
    let attempt =
        plan(&src, &donor, false, T::Str, true).execute_observed_sources(&src, &[&donor], limits());
    let ExecutionError::SecondaryCell {
        path,
        source,
        source_row,
        error: CellError::Access(Failure(payload, retained)),
    } = *attempt.result.unwrap_err()
    else {
        panic!("original opaque donor failure")
    };
    assert_eq!(
        (path.as_str(), source, source_row),
        ("intermediates[0]", 0, 0)
    );
    assert_eq!(&*payload as *const u8, address);
    assert!(Rc::ptr_eq(&retained, &marker));
    assert_eq!(
        *trace.borrow(),
        [
            ("SRC", 0, 0),
            ("SRC", 0, 1),
            ("DONOR", 0, 0),
            ("DONOR", 1, 0),
            ("DONOR", 0, 1)
        ]
    );
    assert!(attempt.handler_counts.is_empty());
}
#[test]
fn failure_position_is_retained_row_position_not_private_attempt_token() {
    let (src, donor, _) = tables();
    let attempt = plan(&src, &donor, false, T::Str, false).execute_observed_sources(
        &src,
        &[&donor],
        limits(),
    );
    let ExecutionError::UnmatchedKey {
        matched_key,
        identity: Some(identity),
        ..
    } = *attempt.result.unwrap_err()
    else {
        panic!("unmatched boundary")
    };
    assert_eq!(
        identity.position, 1,
        "third attempt follows only one accepted candidate"
    );
    assert_eq!(
        identity.values,
        [Value::Str("c".into()), Value::Str("L".into())]
    );
    assert_eq!(matched_key, [("K".into(), Value::Str("z".into()))]);
}
#[test]
fn cumulative_work_equality_refusal_and_fresh_attempt_are_effect_safe() {
    let (src, donor, trace) = tables();
    let plan = plan(&src, &donor, false, T::Str, true);
    let minimum = (0..1000)
        .find(|&work| {
            let mut l = limits();
            l.work_cells = work;
            plan.execute_observed_sources(&src, &[&donor], l)
                .result
                .is_ok()
        })
        .unwrap();
    let mut l = limits();
    l.work_cells = minimum;
    let full = plan.execute_observed_sources(&src, &[&donor], l);
    assert!(full.result.is_ok());
    l.work_cells = minimum - 1;
    let refused = plan.execute_observed_sources(&src, &[&donor], l);
    assert!(
        matches!(*refused.result.unwrap_err(),ExecutionError::Limit{resource:Resource::WorkCells,limit,required:Some(required)} if limit==minimum-1 && required==minimum)
    );
    trace.borrow_mut().clear();
    l.work_cells = 0;
    let zero = plan.execute_observed_sources(&src, &[&donor], l);
    assert!(zero.result.is_err());
    assert!(trace.borrow().is_empty());
    assert!(zero.handler_counts.is_empty());
    assert!(plan
        .execute_observed_sources(&src, &[&donor], limits())
        .result
        .is_ok());
}
#[test]
fn retained_output_text_exact_byte_boundary_and_zero_rows_have_no_selection_effects() {
    let (mut src, donor, trace) = tables();
    let plan = plan(&src, &donor, false, T::Str, true);
    let mut l = limits();
    l.output_text_bytes = 14;
    assert!(plan
        .execute_observed_sources(&src, &[&donor], l)
        .result
        .is_ok());
    l.output_text_bytes = 13;
    assert!(matches!(
        *plan
            .execute_observed_sources(&src, &[&donor], l)
            .result
            .unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::OutputTextBytes,
            limit: 13,
            required: Some(14)
        }
    ));
    src.rows.clear();
    trace.borrow_mut().clear();
    let empty = plan.execute_observed_sources(&src, &[&donor], limits());
    assert!(empty.result.unwrap().dataset.rows().is_empty());
    assert!(trace.borrow().is_empty());
    assert!(empty.handler_counts.is_empty());
}

enum Tree<'a> {
    Text(&'a str),
    Scalar(N),
    Map(Vec<(&'a str, Tree<'a>)>),
    List(Vec<Tree<'a>>),
}
impl Tree<'_> {
    fn append(self, nodes: &mut Vec<N>) -> usize {
        let node = match self {
            Self::Text(v) => N::Text(v.into()),
            Self::Scalar(v) => v,
            Self::List(items) => N::Sequence(items.into_iter().map(|v| v.append(nodes)).collect()),
            Self::Map(items) => N::Mapping(
                items
                    .into_iter()
                    .map(|(k, v)| (Self::Text(k).append(nodes), v.append(nodes)))
                    .collect(),
            ),
        };
        let id = nodes.len();
        nodes.push(node);
        id
    }
}
fn port_document(
    reordered: bool,
    drivers: [Option<&str>; 2],
    grouped: bool,
    field: &str,
) -> SpecificationDocument {
    use Tree::*;
    let source = |name| {
        Map(vec![(
            "value",
            Map(vec![("source", Map(vec![("variable", Text(name))]))]),
        )])
    };
    let mut inputs = vec![
        ("SRC", Map(vec![("path", Text("src.csv"))])),
        ("DONOR", Map(vec![("path", Text("donor.csv"))])),
    ];
    if reordered {
        inputs.reverse();
    }
    let mut rows = Vec::new();
    for (id, driver) in ["first", "second"].iter().zip(drivers) {
        let mut fields = vec![
            ("id", Text(id)),
            ("derivations", Map(vec![("ROWV", source(field))])),
        ];
        if let Some(driver) = driver {
            fields.push(("dataset", Text(driver)));
        }
        if grouped {
            fields.push(("group_by", List(vec![Text("SRC.ID")])));
        }
        rows.push(Map(fields));
    }
    let tree = Map(vec![
        ("schema_version", Text("1.0")),
        ("domain", Text("TEST")),
        ("input", Map(inputs)),
        ("keys", List(vec![Text("ID")])),
        (
            "intermediates",
            List(vec![Map(vec![
                ("id", Text("P")),
                ("dataset", Text("DONOR")),
                ("key", Map(vec![("K", Text("KEY"))])),
                ("no_match", Scalar(N::Null)),
            ])]),
        ),
        (
            "columns",
            List(vec![
                Map(vec![
                    ("name", Text("ID")),
                    ("type", Text("str")),
                    ("derivation", source("SRC.ID")),
                ]),
                Map(vec![
                    ("name", Text("KEY")),
                    ("type", Text("str")),
                    ("derivation", source("SRC.K")),
                ]),
                Map(vec![("name", Text("ROWV")), ("type", Text("str"))]),
                Map(vec![
                    ("name", Text("COLV")),
                    ("type", Text("str")),
                    ("derivation", source("P.V")),
                ]),
            ]),
        ),
        ("rows", List(rows)),
        (
            "output",
            Map(vec![
                ("path", Text("out.csv")),
                (
                    "columns",
                    List(vec![Text("ID"), Text("KEY"), Text("ROWV"), Text("COLV")]),
                ),
            ]),
        ),
    ]);
    let mut nodes = Vec::new();
    let root = tree.append(&mut nodes);
    SpecificationDocument::admit(
        Document::new(nodes, root, DocumentLimits::default()).unwrap(),
        &mut ValidationBudget::new(Default::default()),
    )
    .unwrap()
    .unwrap()
}

#[test]
fn all_inputs_inspect_then_capture_decode_in_authored_order_and_move_opaque_failure_once() {
    use std::sync::Arc;
    use yamaa_engine::specification_run::{
        self as run, PortError, RunError, SourceDecoder, SourcePort,
    };
    type IoTrace = Rc<RefCell<Vec<String>>>;
    struct Port {
        trace: IoTrace,
        reads: usize,
        fail: Option<Failure>,
    }
    impl SourcePort for Port {
        type Error = Failure;
        fn inspect(&mut self, source: &SourceDeclaration) -> Result<(), Failure> {
            self.trace
                .borrow_mut()
                .push(format!("inspect:{}", source.name));
            Ok(())
        }
        fn capture_reads(&self) -> usize {
            self.reads
        }
        fn capture(
            &mut self,
            source: &SourceDeclaration,
            limit: usize,
        ) -> Result<Arc<[u8]>, Failure> {
            self.trace
                .borrow_mut()
                .push(format!("capture:{}", source.name));
            assert!(limit >= 1);
            if source.name == "SRC" {
                if let Some(error) = self.fail.take() {
                    return Err(error);
                }
            }
            self.reads += 1;
            Ok(Arc::from([1u8]))
        }
    }
    struct Decoder {
        trace: IoTrace,
        fail: Option<Failure>,
    }
    impl SourceDecoder for Decoder {
        type Error = Failure;
        type Table = Table;
        fn decode(&mut self, source: &SourceDeclaration, bytes: &[u8]) -> Result<Table, Failure> {
            self.trace
                .borrow_mut()
                .push(format!("decode:{}", source.name));
            assert_eq!(bytes, [1]);
            if source.name == "SRC" {
                if let Some(error) = self.fail.take() {
                    return Err(error);
                }
            }
            let (src, donor, _) = tables();
            Ok(if source.name == "SRC" { src } else { donor })
        }
    }
    let prepared = PreparedSpecification::prepare(&port_document(
        true,
        [Some("SRC"), Some("SRC")],
        false,
        "P.V",
    ))
    .unwrap();
    for capture_failure in [false, true] {
        let io = Rc::new(RefCell::new(Vec::new()));
        let payload = Box::new(7);
        let address = &*payload as *const u8;
        let marker = Rc::new(());
        let mut failure = Some(Failure(payload, marker.clone()));
        let mut port = Port {
            trace: io.clone(),
            reads: 0,
            fail: if capture_failure {
                failure.take()
            } else {
                None
            },
        };
        let mut decoder = Decoder {
            trace: io.clone(),
            fail: failure,
        };
        let mut attempt = run::CapturedAttempt::new(prepared.source());
        run::execute_with_port_into(
            &prepared,
            &mut port,
            &mut decoder,
            run::Limits {
                source_bytes: 100,
                source_cells: 100,
                execution: limits(),
            },
            &mut attempt,
        );
        let error = if capture_failure {
            let PortError::Capture(error) = attempt.result.unwrap_err() else {
                panic!("original capture payload")
            };
            error
        } else {
            let PortError::Run(RunError::Sources(mut errors)) = attempt.result.unwrap_err() else {
                panic!("original decoder payload")
            };
            assert_eq!(errors.len(), 1);
            let (source, error) = errors.pop().unwrap();
            assert_eq!(source, 1);
            error
        };
        assert_eq!(&*error.0 as *const u8, address);
        assert!(Rc::ptr_eq(&error.1, &marker));
        let mut expected = vec![
            "inspect:DONOR",
            "inspect:SRC",
            "capture:DONOR",
            "decode:DONOR",
            "capture:SRC",
        ];
        if !capture_failure {
            expected.push("decode:SRC");
        }
        assert_eq!(*io.borrow(), expected, "no later request or retry");
        assert!(attempt.sources[0].snapshot.is_some());
        assert!(attempt.sources[0].table.is_some());
        assert_eq!(attempt.sources[0].read.source.name, "DONOR");
        assert_eq!(port.reads, if capture_failure { 1 } else { 2 });
        assert!(attempt.sources[1].table.is_none());
    }
}
