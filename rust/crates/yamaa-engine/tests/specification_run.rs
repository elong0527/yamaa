//! Fake native ports pin application order without a host runtime or codec.
use std::{cell::RefCell, rc::Rc, sync::Arc};
use yamaa_core::{
    schema::{
        Document, DocumentLimits, DocumentNode as N, SpecificationDocument, ValidationBudget,
    },
    specification::{PreparedSpecification, SourceDeclaration},
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    value::{ColumnType, Value},
};
use yamaa_engine::{
    dataset::{self, ExecutionError},
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
            Self::Text(value) => N::Text(value.into()),
            Self::List(values) => {
                N::Sequence(values.into_iter().map(|v| v.append(nodes)).collect())
            }
            Self::Map(fields) => N::Mapping(
                fields
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

fn document(expression: &str, input_path: &str) -> SpecificationDocument {
    document_output(expression, input_path, "result.csv", &["ID", "VALUE"])
}
fn document_output(
    expression: &str,
    input_path: &str,
    output_path: &str,
    projection: &[&str],
) -> SpecificationDocument {
    use Tree::*;
    let column = |name, op, field, value| {
        Map(vec![
            ("name", Text(name)),
            ("type", Text("int")),
            (
                "derivation",
                Map(vec![(
                    "value",
                    Map(vec![(op, Map(vec![(field, Text(value))]))]),
                )]),
            ),
        ])
    };
    let tree = Map(vec![
        ("schema_version", Text("1.0")),
        ("domain", Text("TEST")),
        (
            "input",
            Map(vec![("SRC", Map(vec![("path", Text(input_path))]))]),
        ),
        ("keys", List(vec![Text("ID")])),
        (
            "columns",
            List(vec![
                column("ID", "source", "variable", "SRC.ID"),
                column("VALUE", "compute", "expr", expression),
            ]),
        ),
        (
            "output",
            Map(vec![
                ("path", Text(output_path)),
                (
                    "columns",
                    List(projection.iter().map(|name| Text(name)).collect()),
                ),
            ]),
        ),
    ]);
    let mut nodes = Vec::new();
    let root = tree.append(&mut nodes);
    let document = Document::new(nodes, root, DocumentLimits::default()).unwrap();
    SpecificationDocument::admit(document, &mut ValidationBudget::new(Default::default()))
        .unwrap()
        .unwrap()
}

fn source() -> TableSchema {
    TableSchema::new(vec![Column {
        name: "ID".into(),
        kind: ColumnType::Int,
    }])
    .unwrap()
}

#[derive(Debug)]
struct Payload(Rc<()>);
type Trace = Rc<RefCell<Vec<&'static str>>>;
struct Port {
    trace: Trace,
    snapshot: Arc<[u8]>,
    requests: usize,
    reads: usize,
    fail: bool,
    regress: bool,
    payload: Rc<()>,
}
impl SourcePort for Port {
    type Error = Payload;
    fn capture_reads(&self) -> usize {
        self.reads
    }
    fn capture(&mut self, source: &SourceDeclaration, limit: usize) -> Result<Arc<[u8]>, Payload> {
        self.trace.borrow_mut().push("capture");
        assert_eq!(source.path, "input.csv");
        assert_eq!(limit, 1024);
        self.requests += 1;
        if self.regress {
            self.reads -= 1;
        } else if self.requests == 1 {
            self.reads += 1;
        }
        if self.fail {
            return Err(Payload(Rc::clone(&self.payload)));
        }
        Ok(Arc::clone(&self.snapshot))
    }
}
struct Table {
    schema: TableSchema,
    trace: Trace,
    fail: bool,
    panic: bool,
    payload: Rc<()>,
}
impl TableAccess for Table {
    type Error = Payload;
    fn schema(&self) -> &TableSchema {
        self.trace.borrow_mut().push("schema");
        &self.schema
    }
    fn row_count(&self) -> usize {
        1
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Payload>> {
        assert_eq!((row, column), (0, 0));
        self.trace.borrow_mut().push("cell");
        assert!(!self.panic, "injected panic in native table boundary");
        if self.fail {
            return Err(CellError::Access(Payload(Rc::clone(&self.payload))));
        }
        Ok(ValueRef::Int(7))
    }
}
struct Decoder {
    trace: Trace,
    fail: bool,
    fail_cell: bool,
    panic_cell: bool,
    payload: Rc<()>,
}
impl SourceDecoder for Decoder {
    type Error = Payload;
    type Table = Table;
    fn decode(&mut self, declaration: &SourceDeclaration, bytes: &[u8]) -> Result<Table, Payload> {
        self.trace.borrow_mut().push("decode");
        assert_eq!(declaration.name, "SRC");
        assert_eq!(bytes, b"held snapshot");
        if self.fail {
            return Err(Payload(Rc::clone(&self.payload)));
        }
        Ok(Table {
            schema: source(),
            trace: Rc::clone(&self.trace),
            fail: self.fail_cell,
            panic: self.panic_cell,
            payload: Rc::clone(&self.payload),
        })
    }
}
fn fixtures() -> (Port, Decoder) {
    let trace = Trace::default();
    let payload = Rc::new(());
    (
        Port {
            trace: Rc::clone(&trace),
            snapshot: Arc::from(b"held snapshot".as_slice()),
            requests: 0,
            reads: 0,
            fail: false,
            regress: false,
            payload: Rc::clone(&payload),
        },
        Decoder {
            trace,
            fail: false,
            fail_cell: false,
            panic_cell: false,
            payload,
        },
    )
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
#[test]
fn reuse_repeats_effects_and_reports_actual_cached_captures() {
    let prepared = PreparedSpecification::prepare(&document("ID + 1", "input.csv")).unwrap();
    let (mut port, mut decoder) = fixtures();
    let mut attempt = CapturedAttempt::new(prepared.source());
    for created in [1, 0] {
        decoder.trace.borrow_mut().clear();
        run::execute_with_port_into(&prepared, &mut port, &mut decoder, limits(), &mut attempt);
        assert_eq!(attempt.read.snapshots_created, Some(created));
        assert!(Arc::ptr_eq(
            attempt.snapshot.as_ref().unwrap(),
            &port.snapshot
        ));
        let execution = attempt.result.as_ref().unwrap().result.as_ref().unwrap();
        assert_eq!(
            execution.dataset.rows(),
            [vec![Value::Int(7), Value::Int(8)]]
        );
        let trace = decoder.trace.borrow();
        assert_eq!(&trace[..3], ["capture", "decode", "schema"]);
        assert!(trace.iter().position(|s| *s == "cell").unwrap() > 2);
    }
    assert_eq!(port.requests, 2);
}
#[test]
fn capture_decode_and_binding_failures_stop_later_effects_without_losing_payloads() {
    let prepared = PreparedSpecification::prepare(&document("ID + 1", "input.csv")).unwrap();
    let (mut port, mut decoder) = fixtures();
    let mut attempt = CapturedAttempt::new(prepared.source());
    port.fail = true;
    run::execute_with_port_into(&prepared, &mut port, &mut decoder, limits(), &mut attempt);
    let result = std::mem::replace(&mut attempt.result, Err(PortError::Incomplete));
    let Err(PortError::Capture(Payload(payload))) = result else {
        panic!("capture error")
    };
    assert!(Rc::ptr_eq(&payload, &port.payload));
    assert!(!attempt.read.captured && attempt.snapshot.is_none() && attempt.table.is_none());
    assert_eq!(*decoder.trace.borrow(), ["capture"]);
    port.fail = false;
    decoder.fail = true;
    decoder.trace.borrow_mut().clear();
    run::execute_with_port_into(&prepared, &mut port, &mut decoder, limits(), &mut attempt);
    let Err(PortError::Run(RunError::Decode(Payload(ref payload)))) = attempt.result else {
        panic!("decode error")
    };
    assert!(Rc::ptr_eq(payload, &decoder.payload));
    assert!(attempt.snapshot.is_some() && attempt.table.is_none());
    assert_eq!(*decoder.trace.borrow(), ["capture", "decode"]);
    decoder.fail = false;
    decoder.trace.borrow_mut().clear();
    let invalid = PreparedSpecification::prepare(&document("ID +", "input.csv")).unwrap();
    run::execute_with_port_into(&invalid, &mut port, &mut decoder, limits(), &mut attempt);
    assert!(matches!(
        attempt.result,
        Err(PortError::Run(RunError::Bind(_)))
    ));
    assert!(attempt.table.is_some());
    assert_eq!(*decoder.trace.borrow(), ["capture", "decode", "schema"]);
}
#[test]
fn accounting_and_resource_failures_preserve_snapshots_and_precede_later_effects() {
    let prepared = PreparedSpecification::prepare(&document("ID + 1", "input.csv")).unwrap();
    let (mut port, mut decoder) = fixtures();
    let mut attempt = CapturedAttempt::new(prepared.source());
    port.regress = true;
    port.reads = 1;
    run::execute_with_port_into(&prepared, &mut port, &mut decoder, limits(), &mut attempt);
    assert!(matches!(attempt.result, Err(PortError::CaptureAccounting)));
    assert_eq!(attempt.read.snapshots_created, None);
    assert!(attempt.snapshot.is_some() && attempt.table.is_none());
    assert_eq!(*decoder.trace.borrow(), ["capture"]);
    decoder.trace.borrow_mut().clear();
    let bytes = run::execute_bytes(
        &prepared,
        b"held snapshot",
        &mut decoder,
        run::Limits {
            source_bytes: 0,
            ..limits()
        },
    );
    assert!(matches!(
        bytes.result,
        Err(RunError::SourceBytes { limit: 0 })
    ));
    assert!(decoder.trace.borrow().is_empty());
    let cells = run::execute_bytes(
        &prepared,
        b"held snapshot",
        &mut decoder,
        run::Limits {
            source_cells: 0,
            ..limits()
        },
    );
    assert!(matches!(cells.result, Err(RunError::SourceCells)));
    assert!(cells.table.is_some() && !decoder.trace.borrow().contains(&"cell"));
}
#[test]
fn runtime_errors_and_panic_boundaries_keep_the_observed_source() {
    let prepared = PreparedSpecification::prepare(&document("ID + 1", "input.csv")).unwrap();
    let (mut port, mut decoder) = fixtures();
    let mut attempt = CapturedAttempt::new(prepared.source());
    decoder.fail_cell = true;
    run::execute_with_port_into(&prepared, &mut port, &mut decoder, limits(), &mut attempt);
    let result = std::mem::replace(&mut attempt.result, Err(PortError::Incomplete));
    let Err(error) = result.unwrap().result else {
        panic!("cell error")
    };
    let ExecutionError::Cell {
        error: CellError::Access(Payload(payload)),
        ..
    } = *error
    else {
        panic!("opaque access error")
    };
    assert!(Rc::ptr_eq(&payload, &decoder.payload));
    assert!(attempt.snapshot.is_some() && attempt.table.is_some());
    decoder.fail_cell = false;
    decoder.panic_cell = true;
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run::execute_with_port_into(&prepared, &mut port, &mut decoder, limits(), &mut attempt);
    }));
    assert!(panic.is_err());
    assert!(attempt.read.captured && attempt.snapshot.is_some() && attempt.table.is_some());
    assert!(matches!(attempt.result, Err(PortError::Incomplete)));
}

mod publication {
    use super::*;
    use yamaa_core::specification::OutputFinding;
    use yamaa_engine::{
        dataset::{Dataset, Execution},
        specification_output::{
            self as output, ArtifactEncoder, ArtifactPort, CompleteError, OutputReport,
        },
    };

    struct Host {
        trace: Trace,
        fail: Option<&'static str>,
        payload: Rc<()>,
        bytes: Vec<u8>,
        findings: Vec<&'static str>,
    }
    impl Host {
        fn event(&self, name: &'static str) -> Result<(), Payload> {
            self.trace.borrow_mut().push(name);
            if self.fail == Some(name) {
                Err(Payload(Rc::clone(&self.payload)))
            } else {
                Ok(())
            }
        }
    }
    impl OutputReport for Host {
        type Error = Payload;
        type Report = &'static str;
        fn failure(&mut self) -> Result<Self::Report, Payload> {
            self.event("failure")?;
            Ok("failure")
        }
        fn begin(&mut self, _: &Execution) -> Result<(), Payload> {
            self.event("begin")
        }
        fn rejected(&mut self, findings: &[OutputFinding]) -> Result<Self::Report, Payload> {
            self.event("rejected")?;
            self.findings = findings
                .iter()
                .map(|f| match f {
                    OutputFinding::UnknownProfile { .. } => "profile",
                    OutputFinding::DuplicateColumn { .. } => "duplicate",
                    OutputFinding::UndeclaredColumn { .. } => "undeclared",
                    OutputFinding::InternalKey { .. } => "internal",
                })
                .collect();
            Ok("rejected")
        }
        fn success(
            &mut self,
            execution: &Execution,
            projection: &[usize],
            bytes: &[u8],
        ) -> Result<Self::Report, Payload> {
            self.event("report")?;
            assert_eq!(
                execution.dataset.rows(),
                [vec![Value::Int(7), Value::Int(8)]]
            );
            assert_eq!(projection, [1, 0]);
            assert_eq!(bytes, b"VALUE,ID\n8,7\n");
            Ok("success")
        }
    }
    impl ArtifactEncoder for Host {
        type Error = Payload;
        fn encode(
            &mut self,
            dataset: &Dataset,
            projection: &[usize],
            limit: usize,
        ) -> Result<Vec<u8>, Payload> {
            self.event("encode")?;
            assert_eq!(dataset.rows().len(), 1);
            assert_eq!(projection, [1, 0]);
            assert_eq!(limit, 16);
            Ok(self.bytes.clone())
        }
    }
    impl ArtifactPort for Host {
        type Error = Payload;
        fn publish(&mut self, path: &str, bytes: &[u8]) -> Result<(), Payload> {
            self.event("publish")?;
            assert_eq!(path, "result.csv");
            assert_eq!(bytes, self.bytes);
            Ok(())
        }
    }
    fn hosts() -> (Host, Host, Host) {
        let trace = Trace::default();
        let payload = Rc::new(());
        let new = || Host {
            trace: Rc::clone(&trace),
            fail: None,
            payload: Rc::clone(&payload),
            bytes: b"VALUE,ID\n8,7\n".to_vec(),
            findings: Vec::new(),
        };
        (new(), new(), new())
    }
    fn prepared(path: &str, projection: &[&str]) -> PreparedSpecification {
        PreparedSpecification::prepare(&document_output("ID + 1", "input.csv", path, projection))
            .unwrap()
    }
    fn execution(prepared: &PreparedSpecification) -> Execution {
        let (_, mut decoder) = fixtures();
        run::execute_bytes(prepared, b"held snapshot", &mut decoder, limits())
            .result
            .unwrap()
            .result
            .unwrap()
    }
    #[test]
    fn publication_requires_native_success_and_repeats_only_explicit_requests() {
        let prepared = prepared("result.csv", &["VALUE", "ID"]);
        let execution = execution(&prepared);
        let (mut report, mut codec, mut publisher) = hosts();
        assert_eq!(
            output::complete(&prepared, None, 16, &mut report, &mut codec, &mut publisher).unwrap(),
            "failure"
        );
        assert_eq!(*report.trace.borrow(), ["failure"]);
        for _ in 0..2 {
            report.trace.borrow_mut().clear();
            assert_eq!(
                output::complete(
                    &prepared,
                    Some(&execution),
                    16,
                    &mut report,
                    &mut codec,
                    &mut publisher
                )
                .unwrap(),
                "success"
            );
            assert_eq!(
                *report.trace.borrow(),
                ["begin", "encode", "report", "publish"]
            );
        }
    }
    #[test]
    fn output_findings_and_report_failures_block_encoding_or_publication() {
        let invalid = prepared("result.unknown", &["ID", "ID", "ABSENT"]);
        let execution = execution(&invalid);
        let (mut report, mut codec, mut publisher) = hosts();
        assert_eq!(
            output::complete(
                &invalid,
                Some(&execution),
                16,
                &mut report,
                &mut codec,
                &mut publisher
            )
            .unwrap(),
            "rejected"
        );
        assert_eq!(*report.trace.borrow(), ["begin", "rejected"]);
        assert_eq!(report.findings, ["profile", "duplicate", "undeclared"]);
        let valid = prepared("result.csv", &["VALUE", "ID"]);
        for stage in ["begin", "encode", "report", "publish"] {
            let (mut report, mut codec, mut publisher) = hosts();
            report.fail = Some(stage);
            codec.fail = Some(stage);
            publisher.fail = Some(stage);
            let error = output::complete(
                &valid,
                Some(&execution),
                16,
                &mut report,
                &mut codec,
                &mut publisher,
            )
            .unwrap_err();
            let payload = match error {
                CompleteError::Report(p) | CompleteError::Encode(p) | CompleteError::Publish(p) => {
                    p
                }
                _ => panic!("wrong failure"),
            };
            assert!(Rc::ptr_eq(&payload.0, &report.payload));
            let trace = report.trace.borrow();
            let end = ["begin", "encode", "report", "publish"]
                .iter()
                .position(|s| *s == stage)
                .unwrap();
            assert_eq!(*trace, ["begin", "encode", "report", "publish"][..=end]);
        }
    }
    #[test]
    fn encoder_overrun_never_reaches_report_or_publication() {
        let prepared = prepared("result.csv", &["VALUE", "ID"]);
        let execution = execution(&prepared);
        let (mut report, mut codec, mut publisher) = hosts();
        codec.bytes = vec![0; 17];
        assert!(matches!(
            output::complete(
                &prepared,
                Some(&execution),
                16,
                &mut report,
                &mut codec,
                &mut publisher
            ),
            Err(CompleteError::OutputLimit)
        ));
        assert_eq!(*report.trace.borrow(), ["begin", "encode"]);
    }
}
