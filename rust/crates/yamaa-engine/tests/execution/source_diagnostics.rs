//! Actual capture/decoder ordering with payloads that are neither Clone nor Debug nor Send.
use std::{cell::RefCell, rc::Rc, sync::Arc};
use yamaa_core::{
    diagnostic::{
        admit_source_diagnostics, ConditionCode as C, ContextValue as V, Diagnostic,
        ProjectionError, MAX_PROJECTION_BYTES,
    },
    schema::{
        Document, DocumentLimits, DocumentNode as N, SpecificationDocument, ValidationBudget,
    },
    specification::{PreparedSpecification, SourceDeclaration},
    table::{CellError, TableAccess, TableSchema, ValueRef},
    typed_csv::Error as TypingError,
    value::{ColumnType, Value},
};
use yamaa_engine::{
    dataset,
    specification_run::{
        self as run, CapturedAttempt, PortError, RunError, SourceDecoder, SourcePort,
    },
};

enum Tree<'a> {
    Text(&'a str),
    Map(Vec<(&'a str, Tree<'a>)>),
    List(Vec<Tree<'a>>),
}
impl Tree<'_> {
    fn append(self, nodes: &mut Vec<N>) -> usize {
        let node = match self {
            Self::Text(text) => N::Text(text.into()),
            Self::List(items) => {
                N::Sequence(items.into_iter().map(|item| item.append(nodes)).collect())
            }
            Self::Map(fields) => N::Mapping(
                fields
                    .into_iter()
                    .map(|(key, value)| (Self::Text(key).append(nodes), value.append(nodes)))
                    .collect(),
            ),
        };
        let index = nodes.len();
        nodes.push(node);
        index
    }
}
fn prepared(names: &[&str]) -> PreparedSpecification {
    use Tree::*;
    let tree = Map(vec![
        ("schema_version", Text("1.0")),
        ("domain", Text("TEST")),
        ("base", Text("SRC")),
        (
            "input",
            Map(names
                .iter()
                .map(|&name| (name, Map(vec![("path", Text("held.csv"))])))
                .collect()),
        ),
        ("keys", List(vec![Text("ID")])),
        (
            "columns",
            List(vec![Map(vec![
                ("name", Text("ID")),
                ("type", Text("int")),
                (
                    "derivation",
                    Map(vec![(
                        "value",
                        Map(vec![("source", Map(vec![("variable", Text("SRC.ID"))]))]),
                    )]),
                ),
            ])]),
        ),
        (
            "output",
            Map(vec![
                ("path", Text("result.csv")),
                ("columns", List(vec![Text("ID")])),
            ]),
        ),
    ]);
    let mut nodes = Vec::new();
    let root = tree.append(&mut nodes);
    let document = Document::new(nodes, root, DocumentLimits::default()).unwrap();
    let model =
        SpecificationDocument::admit(document, &mut ValidationBudget::new(Default::default()))
            .unwrap()
            .unwrap();
    PreparedSpecification::prepare(&model).unwrap()
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
type Trace = Rc<RefCell<Vec<String>>>;
struct Opaque {
    marker: Rc<()>,
    cause: Option<TypingError>,
    name: &'static str,
}
struct NoTable;
impl TableAccess for NoTable {
    type Error = Opaque;
    fn schema(&self) -> &TableSchema {
        panic!("failed decoder must not bind a table")
    }
    fn row_count(&self) -> usize {
        panic!("failed decoder must not inspect table rows")
    }
    fn cell(&self, _: usize, _: usize) -> Result<ValueRef<'_>, CellError<Opaque>> {
        panic!("failed decoder must not read study cells")
    }
}
struct Port {
    trace: Trace,
    held: Vec<Arc<[u8]>>,
    reads: usize,
    marker: Rc<()>,
    fail_at: Option<&'static str>,
}
impl SourcePort for Port {
    type Error = Opaque;
    fn inspect(&mut self, source: &SourceDeclaration) -> Result<(), Opaque> {
        self.trace
            .borrow_mut()
            .push(format!("inspect:{}", source.name));
        Ok(())
    }
    fn capture_reads(&self) -> usize {
        self.reads
    }
    fn capture(&mut self, source: &SourceDeclaration, _: usize) -> Result<Arc<[u8]>, Opaque> {
        self.trace
            .borrow_mut()
            .push(format!("capture:{}", source.name));
        if self.fail_at == Some(source.name.as_str()) {
            return Err(Opaque {
                marker: Rc::clone(&self.marker),
                cause: None,
                name: "SECOND",
            });
        }
        let bytes = Arc::from(source.name.as_bytes());
        self.held.push(Arc::clone(&bytes));
        self.reads += 1;
        Ok(bytes)
    }
}
struct Decoder {
    trace: Trace,
    pending: Vec<Option<Opaque>>,
    next: usize,
}
impl SourceDecoder for Decoder {
    type Error = Opaque;
    type Table = NoTable;
    fn continue_after(&self, error: &Opaque) -> bool {
        self.trace
            .borrow_mut()
            .push(format!("classify:{}", error.name));
        error.cause.is_some()
    }
    fn decode(&mut self, source: &SourceDeclaration, bytes: &[u8]) -> Result<NoTable, Opaque> {
        self.trace
            .borrow_mut()
            .push(format!("decode:{}", source.name));
        assert_eq!(bytes, source.name.as_bytes());
        let error = self.pending[self.next]
            .take()
            .expect("each original allocation moves once");
        self.next += 1;
        Err(error)
    }
}
fn fixtures(opaque_third: bool) -> (Port, Decoder, Rc<()>, *const u8) {
    let trace = Trace::default();
    let marker = Rc::new(());
    let third = "\0\"\\β".repeat(2000);
    let pointer = third.as_ptr();
    let pending = vec![
        Some(Opaque {
            marker: Rc::clone(&marker),
            name: "SRC",
            cause: Some(TypingError::UnknownField {
                field: "MISSING".into(),
            }),
        }),
        Some(Opaque {
            marker: Rc::clone(&marker),
            name: "SECOND",
            cause: Some(TypingError::FieldParse {
                field: "B".into(),
                target: ColumnType::Int,
                value: "bad".into(),
            }),
        }),
        Some(Opaque {
            marker: Rc::clone(&marker),
            name: "THIRD",
            cause: if opaque_third {
                None
            } else {
                Some(TypingError::FieldParse {
                    field: "F".into(),
                    target: ColumnType::Int,
                    value: third,
                })
            },
        }),
    ];
    (
        Port {
            trace: Rc::clone(&trace),
            held: vec![],
            reads: 0,
            marker: Rc::clone(&marker),
            fail_at: None,
        },
        Decoder {
            trace,
            pending,
            next: 0,
        },
        marker,
        pointer,
    )
}
fn text(value: &str) -> V {
    V::Scalar(Value::Str(value.into()))
}
fn expected(code: C, path: &str, context: Vec<(&str, V)>) -> Diagnostic {
    Diagnostic {
        code,
        spec_paths: vec![path.into()],
        context: context
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
        source_span: None,
        operand_route: None,
    }
}

#[test]
fn failed_run_keeps_complete_order_all_original_allocations_and_effects_through_projection() {
    let prepared = prepared(&["SRC", "SECOND", "THIRD"]);
    let (mut port, mut decoder, marker, pointer) = fixtures(false);
    let mut attempt = CapturedAttempt::new(prepared.source());
    run::execute_with_port_into(&prepared, &mut port, &mut decoder, limits(), &mut attempt);
    let Err(PortError::Run(RunError::Sources(errors))) = &attempt.result else {
        panic!("expected held decoder failures")
    };
    assert_eq!(
        errors.iter().map(|(index, _)| *index).collect::<Vec<_>>(),
        [0, 1, 2]
    );
    for ((_, error), captured) in errors.iter().zip(&attempt.sources) {
        assert!(Rc::ptr_eq(&error.marker, &marker));
        assert!(captured.table.is_none());
        assert!(captured.read.captured);
        assert_eq!(captured.read.snapshots_created, Some(1));
    }
    for (captured, held) in attempt.sources.iter().zip(&port.held) {
        assert!(Arc::ptr_eq(captured.snapshot.as_ref().unwrap(), held));
    }
    let Some(TypingError::FieldParse { value, .. }) = &errors[2].1.cause else {
        panic!()
    };
    assert_eq!(value.as_ptr(), pointer);
    let trace = port.trace.borrow().clone();
    assert_eq!(
        trace,
        [
            "inspect:SRC",
            "inspect:SECOND",
            "inspect:THIRD",
            "capture:SRC",
            "decode:SRC",
            "classify:SRC",
            "capture:SECOND",
            "decode:SECOND",
            "classify:SECOND",
            "capture:THIRD",
            "decode:THIRD",
            "classify:THIRD"
        ]
    );
    let findings: Vec<_> = errors
        .iter()
        .map(|(index, error)| {
            error
                .cause
                .as_ref()
                .unwrap()
                .finding(&prepared.sources()[*index].name)
                .unwrap()
        })
        .collect();
    let actual: Vec<_> = admit_source_diagnostics(&findings, MAX_PROJECTION_BYTES)
        .unwrap()
        .iter()
        .collect();
    assert_eq!(
        actual,
        vec![
            expected(
                C::SourceTypeUnknownField,
                "input.SRC.types.MISSING",
                vec![("dataset", text("SRC")), ("field", text("MISSING"))]
            ),
            expected(
                C::SourceTypeFieldParse,
                "input.SECOND.types.B",
                vec![
                    ("dataset", text("SECOND")),
                    ("field", text("B")),
                    ("type", text("int")),
                    ("value", text("bad"))
                ]
            ),
            expected(
                C::SourceTypeFieldParse,
                "input.THIRD.types.F",
                vec![
                    ("dataset", text("THIRD")),
                    ("field", text("F")),
                    ("type", text("int")),
                    ("value", text(value))
                ]
            ),
        ]
    );
    assert!(admit_source_diagnostics(&findings[..2], 8192).is_ok());
    assert!(matches!(
        admit_source_diagnostics(&findings, 8192),
        Err(ProjectionError::Limit)
    ));
    assert_eq!(
        admit_source_diagnostics(&findings, MAX_PROJECTION_BYTES)
            .unwrap()
            .iter()
            .collect::<Vec<_>>(),
        actual
    );
    assert_eq!(value.as_ptr(), pointer);
    assert_eq!(*port.trace.borrow(), trace);
    assert_eq!(port.reads, 3);
}

#[test]
fn original_opaque_decoder_payload_aborts_after_completed_prefix_before_later_capture() {
    let prepared = prepared(&["SRC", "SECOND", "THIRD", "NEVER"]);
    let (mut port, mut decoder, marker, _) = fixtures(true);
    let mut attempt = CapturedAttempt::new(prepared.source());
    run::execute_with_port_into(&prepared, &mut port, &mut decoder, limits(), &mut attempt);
    let Err(PortError::Run(RunError::Sources(errors))) = &attempt.result else {
        panic!("expected opaque decoder error")
    };
    assert_eq!(errors.len(), 3);
    assert_eq!(attempt.sources.len(), 3);
    assert!(errors[2].1.cause.is_none());
    assert!(Rc::ptr_eq(&errors[2].1.marker, &marker));
    assert!(errors[..2].iter().all(|(_, error)| error.cause.is_some()));
    assert_eq!(
        port.trace.borrow().as_slice(),
        [
            "inspect:SRC",
            "inspect:SECOND",
            "inspect:THIRD",
            "inspect:NEVER",
            "capture:SRC",
            "decode:SRC",
            "classify:SRC",
            "capture:SECOND",
            "decode:SECOND",
            "classify:SECOND",
            "capture:THIRD",
            "decode:THIRD",
            "classify:THIRD"
        ]
    );
    let before = Rc::strong_count(&marker);
    for _ in 0..2 {
        assert!(errors.iter().any(|(_, error)| error.cause.is_none()));
    }
    assert_eq!(Rc::strong_count(&marker), before);
    assert_eq!(port.reads, 3);
}

#[test]
fn original_opaque_capture_payload_retains_identity_without_decode_or_projection() {
    let prepared = prepared(&["SRC", "SECOND", "THIRD"]);
    let (mut port, mut decoder, marker, _) = fixtures(false);
    port.fail_at = Some("SECOND");
    let mut attempt = CapturedAttempt::new(prepared.source());
    run::execute_with_port_into(&prepared, &mut port, &mut decoder, limits(), &mut attempt);
    let Err(PortError::Capture(error)) = &attempt.result else {
        panic!("expected original capture error")
    };
    assert!(Rc::ptr_eq(&error.marker, &marker));
    assert!(error.cause.is_none());
    assert_eq!(attempt.sources.len(), 2);
    assert!(Arc::ptr_eq(
        attempt.sources[0].snapshot.as_ref().unwrap(),
        &port.held[0]
    ));
    assert!(attempt.sources[1].snapshot.is_none());
    assert!(!attempt.sources[1].read.captured);
    assert_eq!(decoder.next, 1);
    assert_eq!(
        port.trace.borrow().as_slice(),
        [
            "inspect:SRC",
            "inspect:SECOND",
            "inspect:THIRD",
            "capture:SRC",
            "decode:SRC",
            "classify:SRC",
            "capture:SECOND"
        ]
    );
}
