#[path = "../../yamaa-core/tests/support/project_call_compiler.rs"]
#[allow(dead_code)]
mod support;
use std::{cell::RefCell, rc::Rc, sync::Arc};
use yamaa_adapters::project_source_decoder::Decoder;
use yamaa_core::{
    function_signature::{LogicalSignature, ProjectFunctionIdentity},
    project_environment::{Draft, Environment, LockKind, LockReference},
    project_function::Language,
    specification::{SourceDeclaration, SourceProfile},
    table::{CellError, TableAccess, ValueRef},
    value::{ColumnType, Value},
};
use yamaa_engine::{
    dataset::{self, ExecutionError},
    function_invocation::{Argument, FailureKind, HostError},
    project_activation::{ActivationPort, Failure},
    project_domain,
    specification_run::{CapturedAttempt, Limits, PortError, SourceDecoder, SourcePort},
};

#[derive(Debug, PartialEq)]
struct Payload(Rc<()>);

#[test]
fn bounded_producer_csv_admits_original_header_before_typing_and_owns_portable_mismatch() {
    use yamaa_engine::producer_build::{DecodeError, DecodeLimits, DecodePort};
    let document = yamaa_adapters::shipped_schema::capture().unwrap().prepare_standalone(
        yamaa_adapters::specification_source::Source {
            identity: "held-producer.yaml".into(),
            bytes: b"schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {RAW: raw.csv}\ncolumns:\n  - {name: ID, type: int, label: Identifier, derivation: RAW.ID}\n  - {name: VALUE, type: int, label: Value, derivation: RAW.VALUE}\noutput: {path: produced.csv, columns: [ID, VALUE]}\n".to_vec(),
        }
    ).unwrap();
    let contract =
        yamaa_core::producer_contract::prepare(document.model(), Default::default()).unwrap();
    let source = SourceDeclaration {
        name: "P".into(),
        path: "produced.csv".into(),
        types: vec![
            ("ID".into(), ColumnType::Int),
            ("VALUE".into(), ColumnType::Int),
        ],
        profile: SourceProfile::Csv,
        empty_string_present: false,
    };
    let limits = DecodeLimits {
        cells: 100,
        storage_bytes: 4096,
        work_bytes: 4096,
    };
    let mut decoder = Decoder::<Payload>::default();
    for bytes in [
        b"VALUE,ID\nnot-an-int,also-bad\n".as_slice(),
        b"ID\nnot-an-int\n",
        b"ID,VALUE,EXTRA\nnot-an-int,also-bad,x\n",
    ] {
        let DecodeError::Metadata(diagnostic) = decoder
            .decode_bounded(&source, bytes, Some(&contract), limits)
            .err()
            .unwrap()
        else {
            panic!("header mismatch must precede declared typing")
        };
        assert_eq!(
            diagnostic.code,
            yamaa_core::diagnostic::ConditionCode::ProducerContractMismatch
        );
        assert_eq!(diagnostic.spec_paths, ["input.P.schema", "input.P.path"]);
    }
    let table = decoder
        .decode_bounded(&source, b"ID,VALUE\n1,13\n", Some(&contract), limits)
        .unwrap();
    assert_eq!(table.cell(0, 1).unwrap(), ValueRef::Int(13));
    for bounded in [
        DecodeLimits { cells: 1, ..limits },
        DecodeLimits {
            storage_bytes: 1,
            ..limits
        },
        DecodeLimits {
            work_bytes: 1,
            ..limits
        },
    ] {
        assert!(matches!(
            decoder.decode_bounded(&source, b"ID,VALUE\n1,13\n", Some(&contract), bounded),
            Err(DecodeError::Codec(_))
        ));
    }
    let header = (0..65)
        .map(|i| format!("F{i}"))
        .collect::<Vec<_>>()
        .join(",")
        + "\n";
    assert!(matches!(
        decoder.decode_bounded(&source, header.as_bytes(), Some(&contract), limits),
        Err(DecodeError::Limit("producer_source_fields"))
    ));
}

#[test]
fn decoded_profiles_keep_exact_cells_bounds_and_ingestion_failure_categories() {
    let mut decoder = Decoder::<Payload>::default();
    let source = SourceDeclaration {
        name: "SRC".into(),
        path: "input.csv".into(),
        types: vec![
            ("I".into(), ColumnType::Int),
            ("S".into(), ColumnType::Str),
            ("F".into(), ColumnType::Float),
            ("D".into(), ColumnType::Date),
            ("T".into(), ColumnType::DateTime),
        ],
        profile: SourceProfile::Csv,
        empty_string_present: true,
    };
    let table = decoder
        .decode(
            &source,
            b"I,S,F,D,T\n9223372036854775807,\"x\0y\",-0.0,2024-02-29,2024-02-29T12:34:56\n",
        )
        .unwrap();
    assert_eq!(table.row_count(), 1);
    assert_eq!(table.cell(0, 0).unwrap(), ValueRef::Int(i64::MAX));
    assert_eq!(table.cell(0, 1).unwrap(), ValueRef::Str("x\0y"));
    let ValueRef::Float(value) = table.cell(0, 2).unwrap() else {
        panic!("exact float")
    };
    assert_eq!(value.get().to_bits(), (-0.0_f64).to_bits());
    let ValueRef::Date(value) = table.cell(0, 3).unwrap() else {
        panic!("exact date")
    };
    assert_eq!(value.to_string(), "2024-02-29");
    let ValueRef::DateTime(value) = table.cell(0, 4).unwrap() else {
        panic!("exact datetime")
    };
    assert_eq!(value.to_string(), "2024-02-29T12:34:56");
    assert_eq!(
        table.cell(1, 0),
        Err(CellError::OutOfBounds { row: 1, column: 0 })
    );
    assert_eq!(
        table.cell(0, 5),
        Err(CellError::OutOfBounds { row: 0, column: 5 })
    );
    let ValueRef::Str(text) = table.cell(0, 1).unwrap() else {
        unreachable!()
    };
    let retained = text.as_ptr();
    let owned = table.into_table();
    let ValueRef::Str(text) = owned.cell(0, 1).unwrap() else {
        unreachable!()
    };
    assert_eq!(text.as_ptr(), retained);
    let error = decoder
        .decode(
            &source,
            b"I,S,F,D,T\nbad,x,1,2024-02-29,2024-02-29T12:34:56\n",
        )
        .err()
        .unwrap();
    assert!(decoder.continue_after(&error));
    let error = decoder
        .decode(&source, &vec![b'x'; 16_777_217])
        .err()
        .unwrap();
    assert!(!decoder.continue_after(&error));

    let bytes = include_bytes!("fixtures/pq/text.parquet");
    for present in [false, true] {
        let source = SourceDeclaration {
            name: "SRC".into(),
            path: "text.parquet".into(),
            types: vec![("I".into(), ColumnType::Int), ("S".into(), ColumnType::Str)],
            profile: SourceProfile::Parquet,
            empty_string_present: present,
        };
        let table = decoder.decode(&source, bytes).unwrap();
        assert_eq!(
            table.cell(0, 1).unwrap(),
            if present {
                ValueRef::Str("")
            } else {
                ValueRef::Missing
            }
        );
        assert_eq!(table.cell(1, 1).unwrap(), ValueRef::Str("X"));
    }
}

type Trace = Rc<RefCell<Vec<&'static str>>>;
struct Activation {
    trace: Trace,
    payload: Rc<()>,
    live_failed: bool,
    lock_failed: bool,
}
impl ActivationPort for Activation {
    type Handle = ();
    type Error = Payload;
    fn verify_lock(
        &mut self,
        language: Language,
        lock: &LockReference,
        functions: &[ProjectFunctionIdentity],
    ) -> Result<(), Payload> {
        assert_eq!(language, Language::Python);
        assert_eq!(lock.kind, LockKind::Uv);
        assert_eq!(
            functions
                .iter()
                .map(|f| f.name.as_str())
                .collect::<Vec<_>>(),
            ["id"]
        );
        self.trace.borrow_mut().push("lock");
        if self.lock_failed {
            Err(Payload(Rc::clone(&self.payload)))
        } else {
            Ok(())
        }
    }
    fn bind(
        &mut self,
        identity: &ProjectFunctionIdentity,
        signature: &LogicalSignature,
    ) -> Result<(), Payload> {
        assert_eq!(identity.name, "id");
        assert_eq!(signature.parameters()[0].host_name, "x");
        self.trace.borrow_mut().push("bind");
        Ok(())
    }
    fn invoke(&mut self, _: &(), args: &[Argument<'_>]) -> Result<Value, HostError<Payload>> {
        let [Argument {
            name: "x",
            value: ValueRef::Int(value),
        }] = args
        else {
            panic!("exact argument")
        };
        self.trace
            .borrow_mut()
            .push(if *value == 1 { "live" } else { "case" });
        if self.live_failed && *value == 1 {
            return Err(HostError::Raised(Payload(Rc::clone(&self.payload))));
        }
        Ok(Value::Int(*value))
    }
}
struct Source {
    trace: Trace,
    bytes: Arc<[u8]>,
    reads: usize,
}
impl SourcePort for Source {
    type Error = Payload;
    fn inspect(&mut self, source: &SourceDeclaration) -> Result<(), Payload> {
        assert_eq!(source.path, "input.csv");
        self.trace.borrow_mut().push("inspect");
        Ok(())
    }
    fn capture_reads(&self) -> usize {
        self.reads
    }
    fn capture(&mut self, _: &SourceDeclaration, maximum: usize) -> Result<Arc<[u8]>, Payload> {
        assert!(self.bytes.len() <= maximum);
        self.trace.borrow_mut().push("capture");
        self.reads = 1;
        Ok(Arc::clone(&self.bytes))
    }
}
#[test]
fn real_decoder_builds_reactivate_cached_sources_and_retain_original_non_send_errors() {
    let environment = Environment::admit(
        Language::Python,
        Draft {
            language: Some(Language::Python),
            lock: Some(LockReference {
                written: "uv.lock".into(),
                kind: LockKind::Uv,
            }),
            functions: Some(vec![
                support::function("unused").definition().clone(),
                support::function("id").definition().clone(),
            ]),
            codelists: vec![],
            has_study: false,
            submissions: vec![],
        },
    )
    .unwrap()
    .into_execution();
    let raw = b"schema_version: '1.0'\ndomain: TEST\ninput: {SRC: {path: input.csv, types: {ID: int}}}\nkeys: [ID]\ncolumns:\n  - {name: ID, type: int, derivation: {value: {source: {variable: SRC.ID}}}}\n  - {name: VALUE, type: int, derivation: {value: {function: {name: id, args: {x: SRC.ID}}}}}\noutput: {path: output.csv, columns: [ID, VALUE]}\n";
    let decoded = yamaa_adapters::yaml_decode::decode_yaml(raw, Default::default()).unwrap();
    let model = yamaa_core::schema::SpecificationDocument::admit(
        decoded.document,
        &mut yamaa_core::schema::ValidationBudget::new(Default::default()),
    )
    .unwrap()
    .unwrap();
    let checked = project_domain::check_owned(&model, environment).unwrap();
    let trace = Rc::new(RefCell::new(vec![]));
    let payload = Rc::new(());
    let mut activation = Activation {
        trace: Rc::clone(&trace),
        payload: Rc::clone(&payload),
        live_failed: false,
        lock_failed: false,
    };
    let mut source = Source {
        trace: Rc::clone(&trace),
        bytes: Arc::from(b"ID\n1\n".as_slice()),
        reads: 0,
    };
    let mut decoder = Decoder::<Payload>::default();
    let mut attempt = CapturedAttempt::new(checked.compiled().source());
    let limits = Limits {
        source_bytes: 1024,
        source_cells: 64,
        execution: dataset::Limits {
            source_rows: 4,
            output_rows: 4,
            output_cells: 16,
            key_cells: 8,
            work_cells: 64,
            scalar_text_bytes: 1024,
            output_text_bytes: 1024,
            identity_cells: 16,
            identity_text_bytes: 1024,
        },
    };
    for created in [Some(1), Some(0)] {
        checked
            .build_into(
                &mut activation,
                &mut source,
                &mut decoder,
                limits,
                &mut attempt,
            )
            .unwrap();
        assert_eq!(attempt.sources[0].read.snapshots_created, created);
        assert_eq!(
            attempt
                .result
                .as_ref()
                .unwrap()
                .result
                .as_ref()
                .unwrap()
                .dataset
                .rows(),
            &[vec![Value::Int(1), Value::Int(1)]]
        );
        assert_eq!(
            attempt.sources[0]
                .table
                .as_ref()
                .unwrap()
                .cell(0, 0)
                .unwrap(),
            ValueRef::Int(1)
        );
        assert!(Arc::ptr_eq(
            attempt.sources[0].snapshot.as_ref().unwrap(),
            &source.bytes
        ));
    }
    let once = ["lock", "bind", "case", "case", "inspect", "capture", "live"];
    assert_eq!(
        &*trace.borrow(),
        &once.into_iter().chain(once).collect::<Vec<_>>()
    );
    activation.live_failed = true;
    checked
        .build_into(
            &mut activation,
            &mut source,
            &mut decoder,
            limits,
            &mut attempt,
        )
        .unwrap();
    let ExecutionError::ProjectFunction { path, error, .. } = attempt
        .result
        .as_ref()
        .unwrap()
        .result
        .as_ref()
        .unwrap_err()
        .as_ref()
    else {
        panic!("project failure")
    };
    assert_eq!(path, "columns.VALUE.derivation.function");
    let FailureKind::CallFailed(Payload(original)) = &error.kind else {
        panic!("original host payload")
    };
    assert!(Rc::ptr_eq(original, &payload));
    assert_eq!(source.reads, 1);
    activation.lock_failed = true;
    trace.borrow_mut().clear();
    let Failure::Lock(Payload(original)) = checked
        .build_into(
            &mut activation,
            &mut source,
            &mut decoder,
            limits,
            &mut attempt,
        )
        .unwrap_err()
    else {
        panic!("original lock failure")
    };
    assert!(Rc::ptr_eq(&original, &payload));
    assert!(attempt.sources.is_empty());
    assert!(matches!(attempt.result, Err(PortError::Incomplete)));
    assert_eq!(&*trace.borrow(), &["lock"]);
}
