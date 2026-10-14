//! Engine source admission keeps held snapshots, typed coordinates and opaque effects.
use std::{cell::RefCell, rc::Rc, sync::Arc};
use yamaa_core::{
    odm_source::FIELDS,
    schema::{
        Document, DocumentLimits, DocumentNode as N, SpecificationDocument, ValidationBudget,
    },
    specification::{PreparedSpecification, SourceDeclaration, SourceInterpretation},
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    value::ColumnType,
};
use yamaa_engine::{
    dataset::ExecutionError,
    specification_run::{
        self as run, CapturedAttempt, PortError, RunError, SourceDecoder, SourceIssue, SourcePort,
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
            Self::Map(fields) => N::Mapping(
                fields
                    .into_iter()
                    .map(|(key, value)| (Self::Text(key).append(nodes), value.append(nodes)))
                    .collect(),
            ),
            Self::List(children) => N::Sequence(
                children
                    .into_iter()
                    .map(|child| child.append(nodes))
                    .collect(),
            ),
        };
        let index = nodes.len();
        nodes.push(node);
        index
    }
}
fn prepared() -> PreparedSpecification {
    use Tree::*;
    let column = |name, operation, item| {
        Map(vec![
            ("name", Text(name)),
            ("type", Text("str")),
            (
                "derivation",
                Map(vec![("value", Map(vec![(operation, Text(item))]))]),
            ),
        ])
    };
    let tree = Map(vec![
        ("schema_version", Text("1.0")),
        ("domain", Text("TEST")),
        ("base", Text("SRC")),
        (
            "input",
            Map(vec![
                ("SRC", Map(vec![("path", Text("source.csv"))])),
                ("LATER", Map(vec![("path", Text("later.csv"))])),
            ]),
        ),
        ("keys", List(vec![Text("ID")])),
        (
            "columns",
            List(vec![
                column("ID", "source", "SRC.SubjectKey"),
                column("VAL", "odm", "SRC.IT.TEST.OID"),
            ]),
        ),
        (
            "output",
            Map(vec![
                ("path", Text("result.csv")),
                ("columns", List(vec![Text("ID"), Text("VAL")])),
            ]),
        ),
    ]);
    let mut nodes = Vec::new();
    let root = tree.append(&mut nodes);
    let document = Document::new(nodes, root, DocumentLimits::default()).unwrap();
    let admitted =
        SpecificationDocument::admit(document, &mut ValidationBudget::new(Default::default()))
            .unwrap()
            .unwrap();
    PreparedSpecification::prepare(&admitted).unwrap()
}
#[derive(Debug)]
struct Original(Rc<()>);
type Trace = Rc<RefCell<Vec<String>>>;
struct Table {
    schema: TableSchema,
    trace: Trace,
    payload: Rc<()>,
    fail: Option<(usize, usize)>,
}
impl TableAccess for Table {
    type Error = Original;
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    fn row_count(&self) -> usize {
        2
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Original>> {
        self.trace.borrow_mut().push(format!("cell:{row}:{column}"));
        if self.fail == Some((row, column)) {
            return Err(CellError::Access(Original(Rc::clone(&self.payload))));
        }
        if column == 2 {
            Ok(ValueRef::Missing)
        } else {
            Ok(ValueRef::Str("present"))
        }
    }
}
struct Port {
    trace: Trace,
    reads: usize,
}
impl SourcePort for Port {
    type Error = Original;
    fn capture_reads(&self) -> usize {
        self.reads
    }
    fn capture(&mut self, source: &SourceDeclaration, limit: usize) -> Result<Arc<[u8]>, Original> {
        assert!(limit >= 4);
        self.trace
            .borrow_mut()
            .push(format!("capture:{}", source.name));
        self.reads += 1;
        Ok(Arc::from(b"held".as_slice()))
    }
}
struct Decoder {
    trace: Trace,
    payload: Rc<()>,
    fail: Option<(usize, usize)>,
}
impl SourceDecoder for Decoder {
    type Error = Original;
    type Table = Table;
    fn decode(&mut self, _: &SourceDeclaration, _: &[u8]) -> Result<Table, Original> {
        panic!("authored source interpretation was lost")
    }
    fn decode_with_interpretation(
        &mut self,
        source: &SourceDeclaration,
        bytes: &[u8],
        interpretation: SourceInterpretation,
    ) -> Result<Table, Original> {
        assert_eq!(bytes, b"held");
        self.trace
            .borrow_mut()
            .push(format!("decode:{}", source.name));
        if source.name == "LATER" {
            assert_eq!(interpretation, SourceInterpretation::Ordinary);
            return Err(Original(Rc::clone(&self.payload)));
        }
        assert_eq!(interpretation, SourceInterpretation::Odm);
        Ok(Table {
            schema: TableSchema::new(
                FIELDS
                    .map(|name| Column {
                        name: name.into(),
                        kind: ColumnType::Str,
                    })
                    .to_vec(),
            )
            .unwrap(),
            trace: Rc::clone(&self.trace),
            payload: Rc::clone(&self.payload),
            fail: self.fail,
        })
    }
}
fn fixtures(fail: Option<(usize, usize)>) -> (Port, Decoder) {
    let trace = Trace::default();
    (
        Port {
            trace: Rc::clone(&trace),
            reads: 0,
        },
        Decoder {
            trace,
            payload: Rc::new(()),
            fail,
        },
    )
}
fn limits() -> run::Limits {
    run::Limits {
        source_bytes: 1024,
        source_cells: 100,
        execution: yamaa_engine::dataset::Limits {
            source_rows: 100,
            output_rows: 100,
            output_cells: 1000,
            key_cells: 1000,
            work_cells: 10000,
            scalar_text_bytes: 10000,
            output_text_bytes: 10000,
            identity_cells: 10000,
            identity_text_bytes: 10000,
        },
    }
}

#[test]
fn required_value_findings_keep_csv_coordinates_and_later_original_decoder_failure() {
    let prepared = prepared();
    let (mut port, mut decoder) = fixtures(None);
    let mut attempt = CapturedAttempt::new(prepared.source());
    run::execute_with_port_into(&prepared, &mut port, &mut decoder, limits(), &mut attempt);
    let Err(PortError::Run(RunError::AdmittedSources(errors))) = attempt.result else {
        panic!("admitted source failure")
    };
    assert_eq!(errors.len(), 2);
    assert_eq!(errors[0].0, 0);
    let SourceIssue::OdmValues(values) = &errors[0].1 else {
        panic!("required values")
    };
    assert_eq!(
        values,
        &[yamaa_core::odm_source::MissingValues {
            field: "SubjectKey",
            record: 2,
            records: 2
        }]
    );
    let SourceIssue::Decode(Original(payload)) = &errors[1].1 else {
        panic!("original decoder failure")
    };
    assert_eq!(errors[1].0, 1);
    assert!(Rc::ptr_eq(payload, &decoder.payload));
    assert!(attempt.sources[0].snapshot.is_some() && attempt.sources[0].table.is_some());
    assert!(attempt.sources[1].snapshot.is_some() && attempt.sources[1].table.is_none());
    let mut expected = vec!["capture:SRC".to_string(), "decode:SRC".to_string()];
    for column in yamaa_core::odm_source::REQUIRED {
        for row in 0..2 {
            expected.push(format!("cell:{row}:{column}"));
        }
    }
    expected.extend(["capture:LATER".into(), "decode:LATER".into()]);
    assert_eq!(*decoder.trace.borrow(), expected);
    assert_eq!(port.reads, 2);
}

#[test]
fn original_identifier_access_failure_suppresses_partial_findings_and_later_captures() {
    let prepared = prepared();
    let (mut port, mut decoder) = fixtures(Some((1, 5)));
    let mut attempt = CapturedAttempt::new(prepared.source());
    run::execute_with_port_into(&prepared, &mut port, &mut decoder, limits(), &mut attempt);
    let execution = attempt.result.unwrap();
    assert!(execution.handler_counts.is_empty());
    assert!(execution.retained_verifications.is_empty());
    match *execution.result.unwrap_err() {
        ExecutionError::Cell {
            path,
            source_row: 1,
            error: CellError::Access(Original(payload)),
        } => {
            assert_eq!(path, "input.SRC.FormOID");
            assert!(Rc::ptr_eq(&payload, &decoder.payload));
        }
        error => panic!("original payload lost: {error:?}"),
    }
    assert_eq!(port.reads, 1);
    assert_eq!(attempt.sources.len(), 1);
    assert!(attempt.sources[0].snapshot.is_some() && attempt.sources[0].table.is_some());
    let mut expected = vec!["capture:SRC".to_string(), "decode:SRC".to_string()];
    for column in [0, 1, 2, 3, 5] {
        for row in 0..2 {
            expected.push(format!("cell:{row}:{column}"));
        }
    }
    assert_eq!(*decoder.trace.borrow(), expected);
}

#[test]
fn complete_source_cell_admission_precedes_required_identifier_reads() {
    let prepared = prepared();
    let (mut port, mut decoder) = fixtures(Some((0, 0)));
    let mut attempt = CapturedAttempt::new(prepared.source());
    let mut limits = limits();
    limits.source_cells = 21;
    run::execute_with_port_into(&prepared, &mut port, &mut decoder, limits, &mut attempt);
    assert!(matches!(
        attempt.result,
        Err(PortError::Run(RunError::SourceCells))
    ));
    assert_eq!(*decoder.trace.borrow(), ["capture:SRC", "decode:SRC"]);
    assert!(attempt.sources[0].table.is_some());
    assert_eq!(port.reads, 1);
}
