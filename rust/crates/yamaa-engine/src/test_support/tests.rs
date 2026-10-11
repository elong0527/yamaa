//! The same application-boundary evidence runs as default units and opt-in integration.
use super::*;
use alloc::{boxed::Box, collections::BTreeMap, rc::Rc, sync::Arc, vec, vec::Vec};
use function_invocation::{
    FailureKind, FunctionIdentity, FunctionPort, HostError, InvocationPlan, Parameter, Presence,
    ProjectFunctionIdentity, ProjectInvocationPlan,
};
use output::{OutputReport, SaveError};
use run::{CapturedAttempt, PortError, RunError};
use yamaa_core::{
    schema::{
        Document, DocumentLimits, DocumentNode as N, SpecificationDocument, ValidationBudget,
    },
    specification::{OutputFinding, PreparedSpecification, SourceDeclaration},
    table::{CellError, Column, TableAccess, TableSchema},
    value::{ColumnType, Value, ValueType},
};

/// Deliberately neither Clone nor Send/Sync. Its allocated token pins identity.
#[derive(Debug)]
struct Opaque {
    token: Box<u8>,
    _caller_thread: Rc<()>,
}
fn failure() -> (Opaque, *const u8) {
    let token = Box::new(23);
    let identity = token.as_ref() as *const u8;
    (
        Opaque {
            token,
            _caller_thread: Rc::new(()),
        },
        identity,
    )
}
fn assert_identity(error: Opaque, identity: *const u8) {
    assert_eq!(error.token.as_ref() as *const u8, identity);
    assert_eq!(*error.token, 23);
}

enum Tree<'a> {
    Text(&'a str),
    Map(Vec<(&'a str, Tree<'a>)>),
    List(Vec<Tree<'a>>),
}
impl Tree<'_> {
    fn append(self, nodes: &mut Vec<N>) -> usize {
        let value = match self {
            Self::Text(value) => N::Text(value.into()),
            Self::List(values) => {
                N::Sequence(values.into_iter().map(|v| v.append(nodes)).collect())
            }
            Self::Map(values) => N::Mapping(
                values
                    .into_iter()
                    .map(|(k, v)| (Self::Text(k).append(nodes), v.append(nodes)))
                    .collect(),
            ),
        };
        let id = nodes.len();
        nodes.push(value);
        id
    }
}
fn prepared(expression: &str) -> PreparedSpecification {
    use Tree::*;
    let tree = Map(vec![
        ("schema_version", Text("1.0")),
        ("domain", Text("TEST")),
        (
            "input",
            Map(vec![("SRC", Map(vec![("path", Text("input.csv"))]))]),
        ),
        ("keys", List(vec![Text("ID")])),
        (
            "output",
            Map(vec![
                ("path", Text("result.csv")),
                ("columns", List(vec![Text("VALUE"), Text("ID")])),
            ]),
        ),
        (
            "columns",
            List(vec![
                Map(vec![
                    ("name", Text("ID")),
                    ("type", Text("int")),
                    (
                        "derivation",
                        Map(vec![(
                            "value",
                            Map(vec![("source", Map(vec![("variable", Text("SRC.ID"))]))]),
                        )]),
                    ),
                ]),
                Map(vec![
                    ("name", Text("VALUE")),
                    ("type", Text("int")),
                    (
                        "derivation",
                        Map(vec![(
                            "value",
                            Map(vec![("compute", Map(vec![("expr", Text(expression))]))]),
                        )]),
                    ),
                ]),
            ]),
        ),
    ]);
    let mut nodes = Vec::new();
    let root = tree.append(&mut nodes);
    let document = Document::new(nodes, root, DocumentLimits::default()).unwrap();
    let document =
        SpecificationDocument::admit(document, &mut ValidationBudget::new(Default::default()))
            .unwrap()
            .unwrap();
    PreparedSpecification::prepare(&document).unwrap()
}
fn limits() -> run::Limits {
    run::Limits {
        source_bytes: 1024,
        source_cells: 10,
        execution: dataset::Limits {
            source_rows: 10,
            output_rows: 10,
            output_cells: 100,
            key_cells: 100,
            work_cells: 1000,
            scalar_text_bytes: 1000,
            output_text_bytes: 1000,
            identity_cells: 100,
            identity_text_bytes: 1000,
        },
    }
}
fn table(log: EventLog) -> FixtureTable<Opaque> {
    FixtureTable::new(
        TableSchema::new(vec![Column {
            name: "ID".into(),
            kind: ColumnType::Int,
        }])
        .unwrap(),
        vec![vec![Value::Int(7)]],
        log,
    )
}
fn source(
    log: EventLog,
    declaration: &SourceDeclaration,
    outcome: Result<Arc<[u8]>, Opaque>,
) -> ScriptedSource<Opaque> {
    let mut source = ScriptedSource::new(log);
    source.inspections.push_back(InspectionStep {
        source: declaration.clone(),
        outcome: Ok(()),
    });
    source.captures.push_back(CaptureStep {
        source: declaration.clone(),
        byte_limit: 1024,
        new_snapshots: usize::from(outcome.is_ok()),
        outcome,
    });
    source
}
fn decoder(
    log: EventLog,
    declaration: &SourceDeclaration,
    outcome: Result<FixtureTable<Opaque>, Opaque>,
) -> ScriptedDecoder<Opaque, FixtureTable<Opaque>> {
    let mut decoder = ScriptedDecoder::new(log);
    decoder.steps.push_back(DecodeStep {
        source: declaration.clone(),
        bytes: b"captured\n".to_vec(),
        outcome,
    });
    decoder
}

#[test]
fn capture_and_inspection_failures_preserve_original_payload_and_stop_effects() {
    let prepared = prepared("ID + 1");
    for inspect in [false, true] {
        let log = EventLog::default();
        let (error, identity) = failure();
        let mut port = if inspect {
            let mut port = source(
                log.clone(),
                prepared.source(),
                Ok(Arc::from(b"captured\n".as_slice())),
            );
            port.inspections[0].outcome = Err(error);
            port
        } else {
            source(log.clone(), prepared.source(), Err(error))
        };
        let mut decoder = decoder(log.clone(), prepared.source(), Ok(table(log.clone())));
        let mut attempt = CapturedAttempt::new(prepared.source());
        run::execute_with_port_into(&prepared, &mut port, &mut decoder, limits(), &mut attempt);
        match attempt.result {
            Err(PortError::Capture(error)) if !inspect => assert_identity(error, identity),
            Err(PortError::Inspect(mut failures)) if inspect => {
                assert_eq!(failures.len(), 1);
                assert_identity(failures.remove(0).error, identity);
            }
            _ => panic!("original source failure"),
        }
        let mut expected = vec![Event::Inspect(prepared.source().clone())];
        if !inspect {
            expected.push(Event::Capture {
                source: prepared.source().clone(),
                byte_limit: 1024,
            });
        }
        assert_eq!(&*log.events(), expected);
        assert_eq!(decoder.steps.len(), 1, "decode was never called");
        assert_eq!(port.captures.len(), usize::from(inspect));
    }
}

#[test]
fn decode_failure_and_invalid_binding_keep_snapshot_without_reading_cells() {
    for invalid_binding in [false, true] {
        let prepared = prepared(if invalid_binding { "ID +" } else { "ID + 1" });
        let log = EventLog::default();
        let snapshot: Arc<[u8]> = Arc::from(b"captured\n".as_slice());
        let mut port = source(log.clone(), prepared.source(), Ok(Arc::clone(&snapshot)));
        let (error, identity) = failure();
        let outcome = if invalid_binding {
            Ok(table(log.clone()))
        } else {
            Err(error)
        };
        let mut decoder = decoder(log.clone(), prepared.source(), outcome);
        let mut attempt = CapturedAttempt::new(prepared.source());
        run::execute_with_port_into(&prepared, &mut port, &mut decoder, limits(), &mut attempt);
        match attempt.result {
            Err(PortError::Run(RunError::Sources(mut failures))) if !invalid_binding => {
                assert_eq!(failures.len(), 1);
                let (index, error) = failures.remove(0);
                assert_eq!(index, 0);
                assert_identity(error, identity);
            }
            Err(PortError::Run(RunError::Bind(_))) if invalid_binding => {}
            _ => panic!("expected decode or binding failure"),
        }
        assert!(Arc::ptr_eq(
            attempt.sources[0].snapshot.as_ref().unwrap(),
            &snapshot
        ));
        assert_eq!(attempt.sources[0].read.snapshots_created, Some(1));
        assert_eq!(attempt.sources[0].table.is_some(), invalid_binding);
        let mut expected = vec![
            Event::Inspect(prepared.source().clone()),
            Event::Capture {
                source: prepared.source().clone(),
                byte_limit: 1024,
            },
            Event::Decode {
                source: prepared.source().clone(),
                bytes: b"captured\n".to_vec(),
            },
        ];
        if invalid_binding {
            expected.push(Event::Schema);
        }
        assert_eq!(&*log.events(), expected);
        assert!(port.captures.is_empty() && decoder.steps.is_empty());
    }
}

struct Report(EventLog);
impl OutputReport for Report {
    type Error = Opaque;
    type Report = &'static str;
    fn failure(&mut self) -> Result<Self::Report, Opaque> {
        self.0.record(Event::Observation("failure"));
        Ok("failure")
    }
    fn begin(&mut self, _: &dataset::Execution) -> Result<(), Opaque> {
        self.0.record(Event::Observation("begin"));
        Ok(())
    }
    fn rejected(&mut self, _: &[OutputFinding]) -> Result<Self::Report, Opaque> {
        self.0.record(Event::Observation("rejected"));
        Ok("rejected")
    }
    fn success(
        &mut self,
        _: &dataset::Execution,
        _: &[usize],
        _: &[u8],
    ) -> Result<Self::Report, Opaque> {
        self.0.record(Event::Observation("report"));
        Ok("success")
    }
}
fn execution(log: EventLog) -> (PreparedSpecification, dataset::Execution) {
    let prepared = prepared("ID + 1");
    let mut port = source(
        log.clone(),
        prepared.source(),
        Ok(Arc::from(b"captured\n".as_slice())),
    );
    let mut decoder = decoder(log.clone(), prepared.source(), Ok(table(log)));
    let mut attempt = CapturedAttempt::new(prepared.source());
    run::execute_with_port_into(&prepared, &mut port, &mut decoder, limits(), &mut attempt);
    let execution = attempt.result.unwrap().result.unwrap();
    assert_eq!(
        execution.dataset.rows(),
        [vec![Value::Int(7), Value::Int(8)]]
    );
    (prepared, execution)
}

#[test]
fn held_output_publishes_exact_bytes_only_on_explicit_save_without_retry() {
    let log = EventLog::default();
    let (prepared, execution) = execution(log.clone());
    let captured = log.events().len();
    assert!(matches!(
        &log.events()[..3],
        [
            Event::Inspect(_),
            Event::Capture { .. },
            Event::Decode { .. }
        ]
    ));
    let mut codec = ScriptedEncoder::<Opaque>::new(log.clone());
    codec.steps.push_back(EncodeStep {
        projection: vec![1, 0],
        byte_limit: 16,
        outcome: Ok(b"VALUE,ID\n8,7\n".to_vec()),
    });
    let mut report = Report(log.clone());
    let result = output::prepare(&prepared, Some(&execution), 16, &mut report, &mut codec).unwrap();
    assert_eq!(
        &log.events()[captured..],
        [
            Event::Observation("begin"),
            Event::Encode {
                projection: vec![1, 0],
                byte_limit: 16
            },
            Event::Observation("report")
        ]
    );
    drop(prepared);
    drop(execution);
    drop(codec);
    drop(report);
    let mut publisher = ScriptedPublisher::new(log.clone());
    let (error, identity) = failure();
    for outcome in [Err(error), Ok(())] {
        publisher.steps.push_back(PublishStep {
            path: "result.csv".into(),
            bytes: b"VALUE,ID\n8,7\n".to_vec(),
            outcome,
        });
    }
    let Err(SaveError::Publish(error)) = result.save(&mut publisher) else {
        panic!("publication failure");
    };
    assert_identity(error, identity);
    assert_eq!(publisher.steps.len(), 1, "no implicit retry after failure");
    assert_eq!(*result.save(&mut publisher).unwrap(), "success");
    assert!(publisher.steps.is_empty());
    assert_eq!(
        &log.events()[captured + 3..],
        [
            Event::Publish {
                path: "result.csv".into(),
                bytes: b"VALUE,ID\n8,7\n".to_vec()
            },
            Event::Publish {
                path: "result.csv".into(),
                bytes: b"VALUE,ID\n8,7\n".to_vec()
            }
        ]
    );
}

#[test]
fn encoding_failure_and_failed_build_never_reach_publication() {
    let log = EventLog::default();
    let (prepared, execution) = execution(log.clone());
    log.clear();
    let mut report = Report(log.clone());
    let mut codec = ScriptedEncoder::new(log.clone());
    let (error, identity) = failure();
    codec.steps.push_back(EncodeStep {
        projection: vec![1, 0],
        byte_limit: 16,
        outcome: Err(error),
    });
    let mut publisher = ScriptedPublisher::<Opaque>::new(log.clone());
    let Err(output::CompleteError::Encode(error)) = output::complete(
        &prepared,
        Some(&execution),
        16,
        &mut report,
        &mut codec,
        &mut publisher,
    ) else {
        panic!("encoding failure");
    };
    assert_identity(error, identity);
    assert_eq!(
        &*log.events(),
        [
            Event::Observation("begin"),
            Event::Encode {
                projection: vec![1, 0],
                byte_limit: 16
            }
        ]
    );
    log.clear();
    let failed = output::prepare(&prepared, None, 16, &mut report, &mut codec).unwrap();
    assert!(matches!(
        failed.save(&mut publisher),
        Err(SaveError::FailedBuild)
    ));
    assert_eq!(&*log.events(), [Event::Observation("failure")]);
}

#[test]
fn callbacks_borrow_metadata_and_move_original_failures_with_exact_mapped_arguments() {
    let log = EventLog::default();
    let mut functions = ScriptedFunctions::new(log.clone());
    functions.legacy.push(
        InvocationPlan::new(
            FunctionIdentity {
                name: "render".into(),
                contract_version: "1.0".into(),
                implementation_version: "1.0".into(),
                call: "fixture.render".into(),
            },
            vec![
                Parameter {
                    name: "x".into(),
                    host_name: "item".into(),
                    kind: ValueType::Int,
                    accepts_missing: false,
                    presence: Presence::Required,
                },
                Parameter {
                    name: "suffix".into(),
                    host_name: "ending".into(),
                    kind: ValueType::Str,
                    accepts_missing: false,
                    presence: Presence::Optional(Value::Str("default".into())),
                },
            ],
            ColumnType::Str,
            false,
        )
        .unwrap(),
    );
    assert!(core::ptr::eq(
        functions.signature(0).unwrap(),
        &functions.legacy[0]
    ));
    assert!(functions.project_signature(0).is_none() && log.events().is_empty());
    let plan = functions.legacy[0].clone();
    let expected = vec![
        ("item".into(), Value::Int(7)),
        ("ending".into(), Value::Str("default".into())),
    ];
    let (raised, raised_id) = failure();
    let (invalid, invalid_id) = failure();
    for outcome in [
        Err(HostError::Raised(raised)),
        Err(HostError::InvalidResult(invalid)),
        Ok(Value::Str("owned".into())),
    ] {
        functions.steps.push_back(CallStep {
            slot: 0,
            arguments: expected.clone(),
            outcome,
        });
    }
    let unknown = BTreeMap::from([("unknown".into(), Value::Int(7))]);
    assert!(matches!(
        function_invocation::invoke(&plan, &unknown, &mut functions)
            .unwrap_err()
            .kind,
        FailureKind::UnknownArguments(_)
    ));
    assert!(log.events().is_empty() && functions.steps.len() == 3);
    let supplied = BTreeMap::from([("x".into(), Value::Int(7))]);
    let FailureKind::CallFailed(error) =
        function_invocation::invoke(&plan, &supplied, &mut functions)
            .unwrap_err()
            .kind
    else {
        panic!("raised callback");
    };
    assert_identity(error, raised_id);
    assert_eq!(functions.steps.len(), 2);
    let FailureKind::InvalidHostResult(error) =
        function_invocation::invoke(&plan, &supplied, &mut functions)
            .unwrap_err()
            .kind
    else {
        panic!("invalid callback representation");
    };
    assert_identity(error, invalid_id);
    assert_eq!(functions.steps.len(), 1);
    assert_eq!(
        function_invocation::invoke(&plan, &supplied, &mut functions).unwrap(),
        Value::Str("owned".into())
    );
    assert!(functions.steps.is_empty());
    assert_eq!(
        &*log.events(),
        vec![
            Event::Call {
                slot: 0,
                arguments: expected
            };
            3
        ]
    );
}

#[test]
fn versionless_callbacks_use_the_same_script_and_preserve_signed_zero_observations() {
    let log = EventLog::default();
    let mut functions = ScriptedFunctions::<Opaque>::new(log.clone());
    functions.project.push(
        ProjectInvocationPlan::new(
            ProjectFunctionIdentity {
                name: "identity".into(),
                call: "fixture.identity".into(),
            },
            vec![Parameter {
                name: "x".into(),
                host_name: "x".into(),
                kind: ValueType::Float,
                accepts_missing: false,
                presence: Presence::Required,
            }],
            ColumnType::Float,
            false,
        )
        .unwrap(),
    );
    assert!(core::ptr::eq(
        functions.project_signature(0).unwrap(),
        &functions.project[0]
    ));
    let plan = functions.project[0].clone();
    assert!(log.events().is_empty());
    let value = Value::float(-0.0);
    functions.steps.push_back(CallStep {
        slot: 0,
        arguments: vec![("x".into(), value.clone())],
        outcome: Ok(value.clone()),
    });
    let returned = function_invocation::invoke_project(
        &plan,
        &BTreeMap::from([("x".into(), value.clone())]),
        &mut functions,
    )
    .unwrap();
    let Value::Float(returned) = returned else {
        panic!("float returned");
    };
    assert_eq!(returned.get().to_bits(), (-0.0f64).to_bits());
    assert_eq!(
        &*log.events(),
        [Event::Call {
            slot: 0,
            arguments: vec![("x".into(), value)]
        }]
    );
    assert_ne!(
        &*log.events(),
        [Event::Call {
            slot: 0,
            arguments: vec![("x".into(), Value::float(0.0))]
        }]
    );
}

#[test]
fn fixture_tables_keep_empty_schema_and_report_both_invalid_coordinates() {
    let schema = TableSchema::new(vec![Column {
        name: "ID".into(),
        kind: ColumnType::Int,
    }])
    .unwrap();
    let log = EventLog::default();
    let empty = FixtureTable::<Opaque>::new(schema.clone(), vec![], log.clone());
    assert_eq!(empty.schema(), &schema);
    assert_eq!(empty.row_count(), 0);
    assert!(matches!(
        empty.cell(0, 0),
        Err(CellError::OutOfBounds { row: 0, column: 0 })
    ));
    let full = table(log.clone());
    assert!(matches!(
        full.cell(0, 1),
        Err(CellError::OutOfBounds { row: 0, column: 1 })
    ));
    assert!(matches!(
        full.cell(1, 0),
        Err(CellError::OutOfBounds { row: 1, column: 0 })
    ));
}
