//! Service contracts.
#[allow(dead_code)]
mod support;
mod service {
    mod specification_run {
        //! Fake native ports pin application order without a host runtime or codec.
        use std::{cell::RefCell, rc::Rc, sync::Arc};
        use yamaa_core::{
            dataset::ExecutionError,
            specification_run::{
                self as run, CapturedAttempt, PortError, RunError, SourceDecoder,
                SpecificationSourcePort,
            },
        };
        use yamaa_core::{
            schema::{Document, DocumentNode as N, SpecificationDocument},
            specification::{PreparedSpecification, SourceDeclaration},
            table::{CellError, Column, TableAccess, TableSchema, ValueRef},
            value::{ColumnType, Value},
        };

        use crate::support::document::Tree;

        fn document(expression: &str, input_path: &str) -> SpecificationDocument {
            document_output(expression, input_path, "result.csv", &["ID", "VALUE"])
        }
        fn document_output(
            expression: &str,
            input_path: &str,
            output_path: &str,
            projection: &[&str],
        ) -> SpecificationDocument {
            let column = |name, op, field, value| {
                Tree::map(vec![
                    ("name", Tree::text(name)),
                    ("type", Tree::text("int")),
                    (
                        "derivation",
                        Tree::map(vec![(
                            "value",
                            Tree::map(vec![(op, Tree::map(vec![(field, Tree::text(value))]))]),
                        )]),
                    ),
                ])
            };
            let tree = Tree::map(vec![
                ("schema_version", Tree::text("1.0")),
                ("domain", Tree::text("TEST")),
                (
                    "input",
                    Tree::map(vec![(
                        "SRC",
                        Tree::map(vec![("path", Tree::text(input_path))]),
                    )]),
                ),
                ("keys", Tree::List(vec![Tree::text("ID")])),
                (
                    "columns",
                    Tree::List(vec![
                        column("ID", "source", "variable", "SRC.ID"),
                        column("VALUE", "compute", "expr", expression),
                    ]),
                ),
                (
                    "output",
                    Tree::map(vec![
                        ("path", Tree::text(output_path)),
                        (
                            "columns",
                            Tree::List(projection.iter().map(Tree::text).collect()),
                        ),
                    ]),
                ),
            ]);
            let mut nodes = Vec::new();
            let root = tree.append(&mut nodes);
            let document = Document::new(nodes, root).unwrap();
            SpecificationDocument::admit(document).unwrap().unwrap()
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
        impl SpecificationSourcePort for Port {
            fn resource_failure(
                &self,
                _error: &Self::Error,
            ) -> Option<yamaa_core::resource::ResourceFailure> {
                None
            }
            fn inspect(&mut self, _source: &SourceDeclaration) -> Result<(), Self::Error> {
                Ok(())
            }

            type Error = Payload;
            fn capture_reads(&self) -> usize {
                self.reads
            }
            fn capture(
                &mut self,
                source: &SourceDeclaration,
                origin: &mut Option<yamaa_core::specification_run::SourceOrigin>,
            ) -> Result<Arc<[u8]>, Payload> {
                *origin = Some(yamaa_core::specification_run::SourceOrigin::Resource);
                self.trace.borrow_mut().push("capture");
                assert_eq!(source.path, "input.csv");
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
            fn continue_after(&self, _error: &Self::Error) -> bool {
                false
            }

            type Error = Payload;
            type Table = Table;
            fn decode(
                &mut self,
                declaration: &SourceDeclaration,
                bytes: &[u8],
                _interpretation: yamaa_core::specification::SourceInterpretation,
                _contract: Option<&yamaa_core::producer_contract::Contract>,
            ) -> Result<Table, Payload> {
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

        #[test]
        fn held_bytes_and_captured_sources_refuse_the_same_non_odm_table_geometry() {
            struct Geometry {
                schema: TableSchema,
                rows: usize,
            }
            impl TableAccess for Geometry {
                type Error = Payload;
                fn schema(&self) -> &TableSchema {
                    &self.schema
                }
                fn row_count(&self) -> usize {
                    self.rows
                }
                fn cell(
                    &self,
                    _row: usize,
                    _column: usize,
                ) -> Result<ValueRef<'_>, CellError<Payload>> {
                    panic!("table admission must precede all cell reads")
                }
            }
            struct GeometryDecoder {
                rows: usize,
                columns: usize,
            }
            impl SourceDecoder for GeometryDecoder {
                type Error = Payload;
                type Table = Geometry;
                fn continue_after(&self, _error: &Payload) -> bool {
                    false
                }
                fn decode(
                    &mut self,
                    _source: &SourceDeclaration,
                    _bytes: &[u8],
                    interpretation: yamaa_core::specification::SourceInterpretation,
                    _contract: Option<&yamaa_core::producer_contract::Contract>,
                ) -> Result<Geometry, Payload> {
                    assert_ne!(
                        interpretation,
                        yamaa_core::specification::SourceInterpretation::Odm
                    );
                    Ok(Geometry {
                        schema: TableSchema::new(
                            (0..self.columns)
                                .map(|index| Column {
                                    name: format!("C{index}"),
                                    kind: ColumnType::Int,
                                })
                                .collect(),
                        )
                        .unwrap(),
                        rows: self.rows,
                    })
                }
            }
            use yamaa_core::limits::Limits;
            let prepared =
                PreparedSpecification::prepare(&document("ID + 1", "input.csv")).unwrap();
            for (rows, columns, resource) in [
                (Limits::TABLE_ROWS + 1, 1, "table_rows"),
                (3_575_419, 179, "table_cells"),
                (0, Limits::COLUMNS + 1, "columns"),
            ] {
                let expected = Limits::table(rows, columns).unwrap_err();
                assert_eq!(expected.resource, resource);
                let mut decoder = GeometryDecoder { rows, columns };
                let decoded = run::execute_bytes(&prepared, b"held snapshot", &mut decoder);
                assert!(decoded.table.is_some());
                assert!(matches!(
                    decoded.result,
                    Err(RunError::Boundary { source: 0, error }) if error == expected
                ));

                let (mut port, _) = fixtures();
                let mut attempt = CapturedAttempt::new(prepared.source());
                run::execute_into(
                    &prepared,
                    &mut port,
                    &mut decoder,
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    &mut attempt,
                    yamaa_core::application_observer::Scope::new(
                        &yamaa_core::application_observer::Noop,
                    ),
                );
                assert!(attempt.sources[0].read.captured);
                assert!(attempt.sources[0].table.is_some());
                assert_eq!(
                    attempt.sources[0].snapshot.as_deref(),
                    Some(b"held snapshot".as_slice())
                );
                assert!(matches!(
                    attempt.result,
                    Err(PortError::Run(RunError::Boundary { source: 0, error }))
                        if error == expected
                ));
            }
        }

        #[test]
        fn held_non_odm_bytes_refuse_oversized_tables_before_any_cell_access() {
            struct Oversized(TableSchema);
            impl TableAccess for Oversized {
                type Error = Payload;
                fn schema(&self) -> &TableSchema {
                    &self.0
                }
                fn row_count(&self) -> usize {
                    yamaa_core::limits::Limits::TABLE_ROWS + 1
                }
                fn cell(&self, _: usize, _: usize) -> Result<ValueRef<'_>, CellError<Payload>> {
                    panic!("table geometry must be admitted before reading cells");
                }
            }
            struct OversizedDecoder;
            impl SourceDecoder for OversizedDecoder {
                type Error = Payload;
                type Table = Oversized;
                fn continue_after(&self, _: &Payload) -> bool {
                    false
                }
                fn decode(
                    &mut self,
                    _: &SourceDeclaration,
                    _: &[u8],
                    _: yamaa_core::specification::SourceInterpretation,
                    _: Option<&yamaa_core::producer_contract::Contract>,
                ) -> Result<Oversized, Payload> {
                    Ok(Oversized(source()))
                }
            }
            let prepared =
                PreparedSpecification::prepare(&document("ID + 1", "input.csv")).unwrap();
            let attempt = run::execute_bytes(&prepared, b"held snapshot", &mut OversizedDecoder);
            assert!(
                attempt.table.is_some(),
                "retain the decoded observation on refusal"
            );
            assert!(matches!(
                attempt.result,
                Err(RunError::Boundary {
                    source: 0,
                    error: yamaa_core::limits::LimitExceeded {
                        resource: "table_rows",
                        limit: 10_000_000,
                        actual: Some(10_000_001),
                    }
                })
            ));
        }

        #[test]
        fn reuse_repeats_effects_and_reports_actual_cached_captures() {
            let prepared =
                PreparedSpecification::prepare(&document("ID + 1", "input.csv")).unwrap();
            let (mut port, mut decoder) = fixtures();
            let mut attempt = CapturedAttempt::new(prepared.source());
            for created in [1, 0] {
                decoder.trace.borrow_mut().clear();
                run::execute_into(
                    &prepared,
                    &mut port,
                    &mut decoder,
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    &mut attempt,
                    yamaa_core::application_observer::Scope::new(
                        &yamaa_core::application_observer::Noop,
                    ),
                );
                assert_eq!(attempt.sources[0].read.snapshots_created, Some(created));
                assert_eq!(
                    attempt.sources[0].read.origin,
                    Some(run::SourceOrigin::Resource)
                );
                assert!(Arc::ptr_eq(
                    attempt.sources[0].snapshot.as_ref().unwrap(),
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
            let prepared =
                PreparedSpecification::prepare(&document("ID + 1", "input.csv")).unwrap();
            let (mut port, mut decoder) = fixtures();
            let mut attempt = CapturedAttempt::new(prepared.source());
            port.fail = true;
            run::execute_into(
                &prepared,
                &mut port,
                &mut decoder,
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                &mut attempt,
                yamaa_core::application_observer::Scope::new(
                    &yamaa_core::application_observer::Noop,
                ),
            );
            assert_eq!(
                attempt.sources[0].read.origin,
                Some(run::SourceOrigin::Resource)
            );
            let result = std::mem::replace(&mut attempt.result, Err(PortError::Incomplete));
            let Err(PortError::Capture(Payload(payload))) = result else {
                panic!("capture error")
            };
            assert!(Rc::ptr_eq(&payload, &port.payload));
            assert!(
                !attempt.sources[0].read.captured
                    && attempt.sources[0].snapshot.is_none()
                    && attempt.sources[0].table.is_none()
            );
            assert_eq!(*decoder.trace.borrow(), ["capture"]);
            port.fail = false;
            decoder.fail = true;
            decoder.trace.borrow_mut().clear();
            run::execute_into(
                &prepared,
                &mut port,
                &mut decoder,
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                &mut attempt,
                yamaa_core::application_observer::Scope::new(
                    &yamaa_core::application_observer::Noop,
                ),
            );
            let Err(PortError::Run(RunError::Sources(ref failures))) = attempt.result else {
                panic!("decode error");
            };
            assert_eq!(failures.len(), 1);
            assert_eq!(failures[0].0, 0);
            assert!(Rc::ptr_eq(&failures[0].1 .0, &decoder.payload));
            assert!(attempt.sources[0].snapshot.is_some() && attempt.sources[0].table.is_none());
            assert_eq!(*decoder.trace.borrow(), ["capture", "decode"]);
            decoder.fail = false;
            decoder.trace.borrow_mut().clear();
            let invalid = PreparedSpecification::prepare(&document("ID +", "input.csv")).unwrap();
            run::execute_into(
                &invalid,
                &mut port,
                &mut decoder,
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                &mut attempt,
                yamaa_core::application_observer::Scope::new(
                    &yamaa_core::application_observer::Noop,
                ),
            );
            assert!(matches!(
                attempt.result,
                Err(PortError::Run(RunError::Bind(_)))
            ));
            assert!(attempt.sources[0].table.is_some());
            assert_eq!(
                *decoder.trace.borrow(),
                ["capture", "decode", "schema", "schema"]
            );
        }
        #[test]
        fn regressing_capture_reads_preserve_the_snapshot_and_precede_decoding() {
            let prepared =
                PreparedSpecification::prepare(&document("ID + 1", "input.csv")).unwrap();
            let (mut port, mut decoder) = fixtures();
            let mut attempt = CapturedAttempt::new(prepared.source());
            port.regress = true;
            port.reads = 1;
            run::execute_into(
                &prepared,
                &mut port,
                &mut decoder,
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                &mut attempt,
                yamaa_core::application_observer::Scope::new(
                    &yamaa_core::application_observer::Noop,
                ),
            );
            assert!(matches!(attempt.result, Err(PortError::CaptureAccounting)));
            assert_eq!(attempt.sources[0].read.snapshots_created, None);
            assert!(attempt.sources[0].snapshot.is_some() && attempt.sources[0].table.is_none());
            assert_eq!(*decoder.trace.borrow(), ["capture"]);
        }

        #[test]
        fn runtime_errors_and_panic_boundaries_keep_the_observed_source() {
            let prepared =
                PreparedSpecification::prepare(&document("ID + 1", "input.csv")).unwrap();
            let (mut port, mut decoder) = fixtures();
            let mut attempt = CapturedAttempt::new(prepared.source());
            decoder.fail_cell = true;
            run::execute_into(
                &prepared,
                &mut port,
                &mut decoder,
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                &mut attempt,
                yamaa_core::application_observer::Scope::new(
                    &yamaa_core::application_observer::Noop,
                ),
            );
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
            assert!(attempt.sources[0].snapshot.is_some() && attempt.sources[0].table.is_some());
            decoder.fail_cell = false;
            decoder.panic_cell = true;
            let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run::execute_into(
                    &prepared,
                    &mut port,
                    &mut decoder,
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    &mut attempt,
                    yamaa_core::application_observer::Scope::new(
                        &yamaa_core::application_observer::Noop,
                    ),
                );
            }));
            assert!(panic.is_err());
            assert!(
                attempt.sources[0].read.captured
                    && attempt.sources[0].snapshot.is_some()
                    && attempt.sources[0].table.is_some()
            );
            assert!(matches!(attempt.result, Err(PortError::Incomplete)));
        }

        mod publication {
            use super::*;
            use yamaa_core::specification::OutputFinding;
            use yamaa_core::{
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
                fn rejected(
                    &mut self,
                    findings: &[OutputFinding],
                ) -> Result<Self::Report, Payload> {
                    self.event("rejected")?;
                    self.findings = findings
                        .iter()
                        .map(|f| match f {
                            OutputFinding::UnknownProfile { .. } => "profile",
                            OutputFinding::DuplicateColumn { .. } => "duplicate",
                            OutputFinding::UndeclaredColumn { .. } => "undeclared",
                            OutputFinding::InternalKey { .. } => "internal",
                            OutputFinding::InvalidDecimals { .. } => "precision",
                            OutputFinding::DecimalsNotApplicable { .. } => "precision_profile",
                            OutputFinding::DuplicateOrderTerm { .. } => "order_duplicate",
                            OutputFinding::UndeclaredOrderColumn { .. } => "order_undeclared",
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
                ) -> Result<Vec<u8>, Payload> {
                    self.event("encode")?;
                    assert_eq!(dataset.rows().len(), 1);
                    assert_eq!(projection, [1, 0]);
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
                PreparedSpecification::prepare(&document_output(
                    "ID + 1",
                    "input.csv",
                    path,
                    projection,
                ))
                .unwrap()
            }
            fn execution(prepared: &PreparedSpecification) -> Execution {
                let (_, mut decoder) = fixtures();
                run::execute_bytes(prepared, b"held snapshot", &mut decoder)
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
                    output::complete(&prepared, None, &mut report, &mut codec, &mut publisher)
                        .unwrap(),
                    "failure"
                );
                assert_eq!(*report.trace.borrow(), ["failure"]);
                for _ in 0..2 {
                    report.trace.borrow_mut().clear();
                    assert_eq!(
                        output::complete(
                            &prepared,
                            Some(&execution),
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
            fn owned_result_only_publishes_retained_bytes_when_saved() {
                let plan = prepared("result.csv", &["VALUE", "ID"]);
                let execution = execution(&plan);
                let (mut report, mut codec, mut publisher) = hosts();
                let result =
                    output::prepare(&plan, Some(&execution), &mut report, &mut codec).unwrap();
                assert_eq!(*report.trace.borrow(), ["begin", "encode", "report"]);
                assert_eq!(result.artifact().unwrap().projection(), [1, 0]);
                assert_eq!(result.artifact().unwrap().path(), "result.csv");
                assert_eq!(result.artifact().unwrap().bytes(), b"VALUE,ID\n8,7\n");
                // Everything used to execute and encode can disappear before saving.
                drop(execution);
                drop(plan);
                drop(codec);
                drop(report);
                publisher.trace.borrow_mut().clear();
                publisher.fail = Some("publish");
                let Err(output::SaveError::Publish(Payload(payload))) = result.save(&mut publisher)
                else {
                    panic!("opaque publication failure");
                };
                assert!(Rc::ptr_eq(&payload, &publisher.payload));
                assert_eq!(*publisher.trace.borrow(), ["publish"]);
                publisher.fail = None;
                for _ in 0..2 {
                    assert_eq!(*result.save(&mut publisher).unwrap(), "success");
                }
                assert_eq!(*publisher.trace.borrow(), ["publish", "publish", "publish"]);
            }
            #[test]
            fn failed_or_rejected_build_cannot_reach_save_authority() {
                let plan = prepared("result.csv", &["VALUE", "ID"]);
                let (mut report, mut codec, mut publisher) = hosts();
                let failure = output::prepare(&plan, None, &mut report, &mut codec).unwrap();
                assert!(matches!(
                    failure.save(&mut publisher),
                    Err(output::SaveError::FailedBuild)
                ));
                assert!(failure.artifact().is_none());
                assert_eq!(*report.trace.borrow(), ["failure"]);
                report.trace.borrow_mut().clear();
                let invalid = prepared("result.csv", &["ID", "ID"]);
                let execution = execution(&invalid);
                let rejected =
                    output::prepare(&invalid, Some(&execution), &mut report, &mut codec).unwrap();
                assert!(matches!(
                    rejected.save(&mut publisher),
                    Err(output::SaveError::FailedBuild)
                ));
                assert!(rejected.artifact().is_none());
                assert_eq!(*report.trace.borrow(), ["begin", "rejected"]);
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
                        &mut report,
                        &mut codec,
                        &mut publisher,
                    )
                    .unwrap_err();
                    let payload = match error {
                        CompleteError::Report(p)
                        | CompleteError::Encode(p)
                        | CompleteError::Publish(p) => p,
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
        }

        mod source_collection {
            use super::*;
            fn prepared() -> PreparedSpecification {
                let original = document("ID + 1", "input.csv");
                let d = original.document();
                let root = d.root();
                let input = d.field(root, "input").unwrap();
                let mut nodes = d.nodes().to_vec();
                for (name, path) in [("SECOND", "second.csv"), ("THIRD", "third.csv")] {
                    let name = Tree::text(name).append(&mut nodes);
                    let value = Tree::map(vec![("path", Tree::text(path))]).append(&mut nodes);
                    let N::Mapping(fields) = &mut nodes[input] else {
                        unreachable!()
                    };
                    fields.push((name, value));
                }
                let base = Tree::text("base").append(&mut nodes);
                let driver = Tree::text("SRC").append(&mut nodes);
                let N::Mapping(fields) = &mut nodes[root] else {
                    unreachable!()
                };
                fields.push((base, driver));
                fn reorder(id: usize, source: &[N], ordered: &mut Vec<N>) -> usize {
                    let node = match &source[id] {
                        N::Mapping(fields) => N::Mapping(
                            fields
                                .iter()
                                .map(|&(a, b)| {
                                    (reorder(a, source, ordered), reorder(b, source, ordered))
                                })
                                .collect(),
                        ),
                        N::Sequence(items) => N::Sequence(
                            items
                                .iter()
                                .map(|&id| reorder(id, source, ordered))
                                .collect(),
                        ),
                        node => node.clone(),
                    };
                    let id = ordered.len();
                    ordered.push(node);
                    id
                }
                let mut ordered = Vec::new();
                let root = reorder(root, &nodes, &mut ordered);
                let d = Document::new(ordered, root).unwrap();
                let model = SpecificationDocument::admit(d).unwrap().unwrap();
                PreparedSpecification::prepare(&model).unwrap()
            }
            struct Capture {
                trace: Trace,
                requests: Vec<String>,
                seen: Vec<String>,
                reads: usize,
                fail: Option<String>,
                payload: Rc<()>,
            }
            impl SpecificationSourcePort for Capture {
                fn resource_failure(
                    &self,
                    _error: &Self::Error,
                ) -> Option<yamaa_core::resource::ResourceFailure> {
                    None
                }
                fn inspect(&mut self, _source: &SourceDeclaration) -> Result<(), Self::Error> {
                    Ok(())
                }

                type Error = Payload;
                fn capture_reads(&self) -> usize {
                    self.reads
                }
                fn capture(
                    &mut self,
                    source: &SourceDeclaration,
                    origin: &mut Option<yamaa_core::specification_run::SourceOrigin>,
                ) -> Result<Arc<[u8]>, Payload> {
                    *origin = Some(yamaa_core::specification_run::SourceOrigin::Resource);
                    self.trace.borrow_mut().push("capture");
                    self.requests.push(source.name.clone());
                    if self.fail.as_deref() == Some(source.name.as_str()) {
                        return Err(Payload(Rc::clone(&self.payload)));
                    }
                    if !self.seen.contains(&source.name) {
                        self.seen.push(source.name.clone());
                        self.reads += 1;
                    }
                    Ok(Arc::from(b"held snapshot".as_slice()))
                }
            }
            #[derive(Debug)]
            struct DecodeFailure {
                recoverable: bool,
                payload: Rc<()>,
            }
            struct Decode {
                trace: Trace,
                fail: Vec<String>,
                recoverable: bool,
                panic: bool,
                payload: Rc<()>,
            }
            impl SourceDecoder for Decode {
                type Error = DecodeFailure;
                type Table = Table;
                fn continue_after(&self, error: &DecodeFailure) -> bool {
                    error.recoverable
                }
                fn decode(
                    &mut self,
                    source: &SourceDeclaration,
                    _bytes: &[u8],
                    _interpretation: yamaa_core::specification::SourceInterpretation,
                    _contract: Option<&yamaa_core::producer_contract::Contract>,
                ) -> Result<Table, DecodeFailure> {
                    self.trace.borrow_mut().push("decode");
                    if self.fail.contains(&source.name) {
                        assert!(!self.panic, "injected decoder interruption");
                        return Err(DecodeFailure {
                            recoverable: self.recoverable,
                            payload: Rc::clone(&self.payload),
                        });
                    }
                    Ok(Table {
                        schema: source_schema(),
                        trace: Rc::clone(&self.trace),
                        fail: false,
                        panic: false,
                        payload: Rc::clone(&self.payload),
                    })
                }
            }
            fn source_schema() -> TableSchema {
                super::source()
            }
            fn ports() -> (Capture, Decode) {
                let trace = Trace::default();
                let payload = Rc::new(());
                (
                    Capture {
                        trace: Rc::clone(&trace),
                        requests: Vec::new(),
                        seen: Vec::new(),
                        reads: 0,
                        fail: None,
                        payload: Rc::clone(&payload),
                    },
                    Decode {
                        trace,
                        fail: Vec::new(),
                        recoverable: true,
                        panic: false,
                        payload,
                    },
                )
            }
            #[derive(Debug)]
            struct InspectError {
                cause: Option<yamaa_core::resource::ResourceFailure>,
                payload: Rc<()>,
            }
            struct Inspection {
                capture: Capture,
                inspected: Vec<String>,
                failures: Vec<(String, Option<yamaa_core::resource::ResourceFailure>)>,
                counter_change: bool,
                panic_at: Option<String>,
            }
            impl SpecificationSourcePort for Inspection {
                type Error = InspectError;
                fn resource_failure(
                    &self,
                    error: &InspectError,
                ) -> Option<yamaa_core::resource::ResourceFailure> {
                    error.cause
                }
                fn inspect(&mut self, source: &SourceDeclaration) -> Result<(), InspectError> {
                    self.capture.trace.borrow_mut().push("inspect");
                    self.inspected.push(source.name.clone());
                    assert_ne!(
                        self.panic_at.as_deref(),
                        Some(source.name.as_str()),
                        "injected inspection interruption"
                    );
                    if self.counter_change {
                        self.capture.reads += 1;
                    }
                    if let Some((_, cause)) =
                        self.failures.iter().find(|(name, _)| name == &source.name)
                    {
                        return Err(InspectError {
                            cause: *cause,
                            payload: Rc::clone(&self.capture.payload),
                        });
                    }
                    Ok(())
                }
                fn capture_reads(&self) -> usize {
                    self.capture.capture_reads()
                }
                fn capture(
                    &mut self,
                    source: &SourceDeclaration,
                    origin: &mut Option<yamaa_core::specification_run::SourceOrigin>,
                ) -> Result<Arc<[u8]>, InspectError> {
                    *origin = Some(yamaa_core::specification_run::SourceOrigin::Resource);
                    self.capture
                        .capture(source, origin)
                        .map_err(|error| InspectError {
                            cause: None,
                            payload: error.0,
                        })
                }
            }
            fn inspection() -> (Inspection, Decode) {
                let (capture, decoder) = ports();
                (
                    Inspection {
                        capture,
                        inspected: vec![],
                        failures: vec![],
                        counter_change: false,
                        panic_at: None,
                    },
                    decoder,
                )
            }
            #[test]
            fn every_source_is_inspected_before_capture_on_each_reused_build() {
                let prepared = prepared();
                let (mut port, mut decoder) = inspection();
                let mut attempt = CapturedAttempt::new(prepared.source());
                for created in [1, 0] {
                    port.capture.trace.borrow_mut().clear();
                    port.inspected.clear();
                    run::execute_into(
                        &prepared,
                        &mut port,
                        &mut decoder,
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        &mut attempt,
                        yamaa_core::application_observer::Scope::new(
                            &yamaa_core::application_observer::Noop,
                        ),
                    );
                    assert_eq!(port.inspected, ["SRC", "SECOND", "THIRD"]);
                    assert_eq!(
                        &port.capture.trace.borrow()[..12],
                        [
                            "inspect", "inspect", "inspect", "capture", "decode", "schema",
                            "capture", "decode", "schema", "capture", "decode", "schema"
                        ]
                    );
                    assert!(attempt
                        .sources
                        .iter()
                        .all(|source| source.read.snapshots_created == Some(created)));
                    assert!(attempt.result.as_ref().unwrap().result.is_ok());
                }
                assert_eq!(port.capture.reads, 3);
            }
            #[test]
            fn classified_inspection_findings_collect_without_any_study_effect() {
                use yamaa_core::resource::ResourceFailure::{Missing, NotRegularFile};
                let prepared = prepared();
                let (mut port, mut decoder) = inspection();
                port.failures = vec![
                    ("SRC".into(), Some(Missing)),
                    ("THIRD".into(), Some(NotRegularFile)),
                ];
                let mut attempt = CapturedAttempt::new(prepared.source());
                run::execute_into(
                    &prepared,
                    &mut port,
                    &mut decoder,
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    &mut attempt,
                    yamaa_core::application_observer::Scope::new(
                        &yamaa_core::application_observer::Noop,
                    ),
                );
                assert!(attempt.sources.is_empty());
                let Err(PortError::Inspect(errors)) = &attempt.result else {
                    panic!("inspection findings");
                };
                assert_eq!(
                    errors
                        .iter()
                        .map(|e| (&*e.source.name, e.failure))
                        .collect::<Vec<_>>(),
                    [("SRC", Some(Missing)), ("THIRD", Some(NotRegularFile))]
                );
                assert!(errors
                    .iter()
                    .all(|e| Rc::ptr_eq(&e.error.payload, &port.capture.payload)));
                assert_eq!(port.inspected, ["SRC", "SECOND", "THIRD"]);
                assert_eq!(*decoder.trace.borrow(), ["inspect", "inspect", "inspect"]);
                assert!(port.capture.requests.is_empty());
                assert_eq!(port.capture.reads, 0);
            }
            #[test]
            fn changed_content_inspection_keeps_original_payload_and_stops_later_authority() {
                use yamaa_core::resource::ResourceFailure::{ContentChanged, Missing};
                let prepared = prepared();
                let (mut port, mut decoder) = inspection();
                port.failures = vec![
                    ("SRC".into(), Some(ContentChanged)),
                    ("THIRD".into(), Some(Missing)),
                ];
                let mut attempt = CapturedAttempt::new(prepared.source());
                run::execute_into(
                    &prepared,
                    &mut port,
                    &mut decoder,
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    &mut attempt,
                    yamaa_core::application_observer::Scope::new(
                        &yamaa_core::application_observer::Noop,
                    ),
                );
                let Err(PortError::Inspect(errors)) = &attempt.result else {
                    panic!("content change missing");
                };
                assert_eq!(errors.len(), 1);
                assert_eq!(errors[0].failure, Some(ContentChanged));
                assert!(Rc::ptr_eq(&errors[0].error.payload, &port.capture.payload));
                assert_eq!(port.inspected, ["SRC"]);
                assert_eq!(*decoder.trace.borrow(), ["inspect"]);
                assert!(attempt.sources.is_empty() && port.capture.requests.is_empty());
                assert_eq!(port.capture.reads, 0);
            }
            #[test]
            fn opaque_inspection_failure_stops_later_authority_and_keeps_original_payload() {
                use yamaa_core::resource::ResourceFailure::Missing;
                let prepared = prepared();
                let (mut port, mut decoder) = inspection();
                port.failures = vec![("SRC".into(), Some(Missing)), ("SECOND".into(), None)];
                let mut attempt = CapturedAttempt::new(prepared.source());
                run::execute_into(
                    &prepared,
                    &mut port,
                    &mut decoder,
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    &mut attempt,
                    yamaa_core::application_observer::Scope::new(
                        &yamaa_core::application_observer::Noop,
                    ),
                );
                let Err(PortError::Inspect(errors)) = &attempt.result else {
                    panic!("opaque inspection failure");
                };
                assert_eq!(errors.len(), 2);
                assert_eq!(errors[1].failure, None);
                assert!(Rc::ptr_eq(&errors[1].error.payload, &port.capture.payload));
                assert_eq!(port.inspected, ["SRC", "SECOND"]);
                assert!(attempt.sources.is_empty() && port.capture.requests.is_empty());
                assert_eq!(port.capture.reads, 0);
            }
            #[test]
            fn inspecting_cannot_increment_capture_accounting() {
                let prepared = prepared();
                let (mut port, mut decoder) = inspection();
                port.counter_change = true;
                let mut attempt = CapturedAttempt::new(prepared.source());
                run::execute_into(
                    &prepared,
                    &mut port,
                    &mut decoder,
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    &mut attempt,
                    yamaa_core::application_observer::Scope::new(
                        &yamaa_core::application_observer::Noop,
                    ),
                );
                assert!(matches!(attempt.result, Err(PortError::CaptureAccounting)));
                assert_eq!(port.inspected, ["SRC"]);
                assert!(attempt.sources.is_empty() && port.capture.requests.is_empty());
                assert_eq!(*decoder.trace.borrow(), ["inspect"]);
            }
            #[test]
            fn inspection_panic_retains_an_incomplete_attempt_without_inventing_reads() {
                let prepared = prepared();
                let (mut port, mut decoder) = inspection();
                port.panic_at = Some("SECOND".into());
                let mut attempt = CapturedAttempt::new(prepared.source());
                assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run::execute_into(
                        &prepared,
                        &mut port,
                        &mut decoder,
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        &mut attempt,
                        yamaa_core::application_observer::Scope::new(
                            &yamaa_core::application_observer::Noop,
                        ),
                    );
                }))
                .is_err());
                assert!(matches!(attempt.result, Err(PortError::Incomplete)));
                assert_eq!(port.inspected, ["SRC", "SECOND"]);
                assert!(attempt.sources.is_empty() && port.capture.requests.is_empty());
                assert_eq!(port.capture.reads, 0);
            }
            #[test]
            fn ordered_collection_caches_snapshots_but_repeats_execution() {
                let prepared = prepared();
                let (mut port, mut decoder) = ports();
                let mut attempt = CapturedAttempt::new(prepared.source());
                for created in [1, 0] {
                    port.trace.borrow_mut().clear();
                    run::execute_into(
                        &prepared,
                        &mut port,
                        &mut decoder,
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        &mut attempt,
                        yamaa_core::application_observer::Scope::new(
                            &yamaa_core::application_observer::Noop,
                        ),
                    );
                    assert!(attempt.result.as_ref().unwrap().result.is_ok());
                    assert_eq!(attempt.sources.len(), 3);
                    assert!(attempt
                        .sources
                        .iter()
                        .all(|s| s.read.snapshots_created == Some(created)
                            && s.snapshot.is_some()
                            && s.table.is_some()));
                    assert_eq!(
                        &port.trace.borrow()[..9],
                        [
                            "capture", "decode", "schema", "capture", "decode", "schema",
                            "capture", "decode", "schema"
                        ]
                    );
                }
                assert_eq!(
                    port.requests,
                    ["SRC", "SECOND", "THIRD", "SRC", "SECOND", "THIRD"]
                );
                assert_eq!(port.reads, 3);
            }
            #[test]
            fn semantic_decode_findings_collect_but_opaque_failures_abort() {
                let prepared = prepared();
                for recoverable in [true, false] {
                    let (mut port, mut decoder) = ports();
                    decoder.fail = vec!["SRC".into(), "THIRD".into()];
                    decoder.recoverable = recoverable;
                    let mut attempt = CapturedAttempt::new(prepared.source());
                    run::execute_into(
                        &prepared,
                        &mut port,
                        &mut decoder,
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        &mut attempt,
                        yamaa_core::application_observer::Scope::new(
                            &yamaa_core::application_observer::Noop,
                        ),
                    );
                    let Err(PortError::Run(RunError::Sources(errors))) = attempt.result else {
                        panic!("ingestion errors")
                    };
                    assert_eq!(
                        errors.iter().map(|(index, _)| *index).collect::<Vec<_>>(),
                        if recoverable { vec![0, 2] } else { vec![0] }
                    );
                    assert!(errors
                        .iter()
                        .all(|(_, error)| Rc::ptr_eq(&error.payload, &decoder.payload)));
                    assert_eq!(port.requests.len(), if recoverable { 3 } else { 1 });
                    assert!(!port.trace.borrow().contains(&"cell"));
                }
            }

            #[test]
            fn later_capture_failure_and_decoder_panic_retain_prior_observations() {
                let prepared = prepared();
                let (mut port, mut decoder) = ports();
                port.fail = Some("SECOND".into());
                let mut attempt = CapturedAttempt::new(prepared.source());
                run::execute_into(
                    &prepared,
                    &mut port,
                    &mut decoder,
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    &mut attempt,
                    yamaa_core::application_observer::Scope::new(
                        &yamaa_core::application_observer::Noop,
                    ),
                );
                let Err(PortError::Capture(Payload(ref payload))) = attempt.result else {
                    panic!("capture failure")
                };
                assert!(Rc::ptr_eq(payload, &port.payload));
                assert_eq!(attempt.sources.len(), 2);
                assert!(attempt.sources[0].table.is_some());
                assert!(!attempt.sources[1].read.captured);
                port.fail = None;
                decoder.fail = vec!["SECOND".into()];
                decoder.panic = true;
                assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run::execute_into(
                        &prepared,
                        &mut port,
                        &mut decoder,
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        &mut attempt,
                        yamaa_core::application_observer::Scope::new(
                            &yamaa_core::application_observer::Noop,
                        ),
                    )
                }))
                .is_err());
                assert!(matches!(attempt.result, Err(PortError::Incomplete)));
                assert_eq!(attempt.sources.len(), 2);
                assert!(attempt.sources[0].table.is_some());
                assert!(
                    attempt.sources[1].snapshot.is_some() && attempt.sources[1].table.is_none()
                );
            }
        }

        mod domain_usecase {
            use super::*;
            use yamaa_core::domain;
            use yamaa_core::specification::PrepareError;

            #[test]
            fn known_verification_findings_need_no_data_and_do_not_move_build_failures() {
                let original = document("ID / 0", "input.csv");
                let raw = original.document();
                assert_eq!(raw.root() + 1, raw.nodes().len());
                let mut nodes = raw.nodes()[..raw.root()].to_vec();
                let N::Mapping(fields) = &raw.nodes()[raw.root()] else {
                    panic!("mapping")
                };
                let mut fields = fields.clone();
                let name = nodes.len();
                nodes.push(N::Text("verifications".into()));
                let verification =
                    Tree::List(vec![Tree::map(vec![("row_count", Tree::map(vec![]))])])
                        .append(&mut nodes);
                fields.push((name, verification));
                let root = nodes.len();
                nodes.push(N::Mapping(fields));
                let model = SpecificationDocument::admit(Document::new(nodes, root).unwrap())
                    .unwrap()
                    .unwrap();
                let checked = domain::check(&model).unwrap();
                let (mut port, mut decoder) = fixtures();
                for _ in 0..2 {
                    let findings = checked.verification_declaration_diagnostics();
                    assert_eq!(findings.len(), 1);
                    assert_eq!(
                        (
                            findings[0].definition().condition,
                            findings[0].definition().requirement
                        ),
                        ("invalid_declaration", Some("REQ-0399"))
                    );
                    assert_eq!(findings[0].spec_paths, ["verifications[0].row_count"]);
                    assert!(port.trace.borrow().is_empty());
                    assert_eq!((port.requests, port.reads), (0, 0));
                }
                let attempt = domain::build(&model, &mut port, &mut decoder).unwrap();
                let execution = attempt.result.unwrap();
                let ExecutionError::Numeric { error, .. } = *execution.result.unwrap_err() else {
                    panic!("arithmetic precedes the deferred declaration")
                };
                assert_eq!(
                    error.diagnostic().unwrap().definition().condition,
                    "division_by_zero"
                );
            }

            #[test]
            fn unknown_source_profile_precedes_every_study_effect() {
                let (mut port, mut decoder) = fixtures();
                let document = document("ID + 1", "input.unknown");
                let Err(PrepareError::Invalid(check_findings)) = domain::check(&document) else {
                    panic!("check must reject unknown source profile");
                };
                let Err(PrepareError::Invalid(build_findings)) =
                    domain::build(&document, &mut port, &mut decoder)
                else {
                    panic!("build must reject unknown source profile");
                };
                assert_eq!(check_findings, build_findings);
                assert_eq!(check_findings.len(), 1);
                let diagnostic = check_findings[0].diagnostic();
                assert_eq!(
                    diagnostic.code,
                    yamaa_core::diagnostic::ConditionCode::SourceUnknownProfile
                );
                assert_eq!(diagnostic.spec_paths, ["input.SRC.path"]);
                assert_eq!(
                    diagnostic.context,
                    [("dataset", "SRC"), ("path", "input.unknown")]
                        .into_iter()
                        .map(|(key, value)| (
                            key.into(),
                            yamaa_core::json::Value::String(value.into())
                        ))
                        .collect()
                );
                assert!(port.trace.borrow().is_empty());
                assert_eq!((port.requests, port.reads), (0, 0));
            }

            #[test]
            fn check_does_not_evaluate_and_reused_builds_have_fresh_attempts() {
                let checked = domain::check(&document("ID + 1", "input.csv")).unwrap();
                let (mut port, mut decoder) = fixtures();
                let mut attempt = CapturedAttempt::new(checked.compiled().source());
                for created in [1, 0] {
                    port.trace.borrow_mut().clear();
                    checked.build_into(
                        &mut port,
                        &mut decoder,
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        &mut attempt,
                        yamaa_core::application_observer::Scope::new(
                            &yamaa_core::application_observer::Noop,
                        ),
                    );
                    assert_eq!(attempt.sources.len(), 1);
                    assert_eq!(attempt.sources[0].read.snapshots_created, Some(created));
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
                        [vec![Value::Int(7), Value::Int(8)]]
                    );
                    assert_eq!(&port.trace.borrow()[..3], ["capture", "decode", "schema"]);
                }
                assert_eq!((port.requests, port.reads), (2, 1));
                // An admitted arithmetic expression is not evaluated by check.
                let zero = document("ID / 0", "input.csv");
                domain::check(&zero).unwrap();
                let attempt = domain::build(&zero, &mut port, &mut decoder).unwrap();
                assert!(attempt.result.unwrap().result.is_err());
            }

            #[test]
            fn one_shot_build_preserves_opaque_errors_without_retry() {
                let (mut port, mut decoder) = fixtures();
                port.fail = true;
                let attempt =
                    domain::build(&document("ID + 1", "input.csv"), &mut port, &mut decoder)
                        .unwrap();
                let Err(PortError::Capture(Payload(payload))) = attempt.result else {
                    panic!("original capture failure");
                };
                assert!(Rc::ptr_eq(&payload, &port.payload));
                assert_eq!(*port.trace.borrow(), ["capture"]);
                assert_eq!(port.requests, 1);
                assert!(!attempt.sources[0].read.captured);
            }

            #[test]
            fn checked_build_keeps_partial_evidence_across_the_host_panic_fence() {
                let checked = domain::check(&document("ID + 1", "input.csv")).unwrap();
                let (mut port, mut decoder) = fixtures();
                decoder.panic_cell = true;
                let mut attempt = CapturedAttempt::new(checked.compiled().source());
                let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    checked.build_into(
                        &mut port,
                        &mut decoder,
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        &mut attempt,
                        yamaa_core::application_observer::Scope::new(
                            &yamaa_core::application_observer::Noop,
                        ),
                    );
                }));
                assert!(panic.is_err());
                assert!(matches!(attempt.result, Err(PortError::Incomplete)));
                assert!(attempt.sources[0].snapshot.is_some());
                assert!(attempt.sources[0].table.is_some());
                assert_eq!(port.requests, 1);
            }
        }
    }
    mod submission_inventory {
        use std::rc::Rc;
        use yamaa_core::schema::{Document, DocumentNode as N};
        use yamaa_core::submission_inventory::{self as service, Ports};
        fn domain(name: &str) -> Document {
            Document::new(
                vec![
                    N::Text("domain".into()),
                    N::Text(name.into()),
                    N::Mapping(vec![(0, 1)]),
                ],
                2,
            )
            .unwrap()
        }
        fn environment() -> Document {
            Document::new(
                vec![
                    N::Text("sdtm".into()),
                    N::Text("specs".into()),
                    N::Text("first.yaml".into()),
                    N::Text("second.yaml".into()),
                    N::Sequence(vec![2, 3]),
                    N::Mapping(vec![(1, 4)]),
                    N::Mapping(vec![(0, 5)]),
                ],
                6,
            )
            .unwrap()
        }
        /// The project file, lowered as preparation lowers it.
        fn project() -> yamaa_core::ast::Project {
            yamaa_core::front_end::v0_5::project::project(&environment(), None).unwrap()
        }
        struct Captured {
            document: Document,
            identity: String,
        }
        #[derive(Debug)]
        struct Original(Rc<()>);
        struct Metadata {
            calls: Vec<(String, String)>,
            fail: bool,
            original: Rc<()>,
        }
        impl Ports for Metadata {
            type Specification = Captured;
            type Error = Original;
            fn capture(&mut self, declaring: &str, written: &str) -> Result<Captured, Original> {
                self.calls.push((declaring.into(), written.into()));
                if self.fail && self.calls.len() == 2 {
                    return Err(Original(Rc::clone(&self.original)));
                }
                Ok(Captured {
                    document: domain(if self.calls.len() == 1 { "DM" } else { "ADSL" }),
                    identity: format!("{declaring}/{written}"),
                })
            }
            fn document(captured: &Captured) -> &Document {
                &captured.document
            }
            fn identity(captured: &Captured) -> &str {
                &captured.identity
            }
        }
        #[test]
        fn native_capture_retains_original_metadata_owners_and_never_accepts_a_data_port() {
            let mut port = Metadata {
                calls: vec![],
                fail: false,
                original: Rc::new(()),
            };
            let held =
                service::capture(&project(), "original/environment.yaml", &mut port).unwrap();
            assert_eq!(held.inventory.names(), ["ADSL", "DM"]);
            assert_eq!(held.specifications.len(), 2);
            assert_eq!(
                port.calls,
                [
                    ("original/environment.yaml".into(), "first.yaml".into()),
                    ("original/environment.yaml".into(), "second.yaml".into())
                ]
            );
        }
        #[test]
        fn an_uncloneable_boundary_failure_keeps_its_exact_original_payload_and_written_request() {
            let original = Rc::new(());
            let mut port = Metadata {
                calls: vec![],
                fail: true,
                original: Rc::clone(&original),
            };
            let Err(service::Error::Port { written, error }) =
                service::capture(&project(), "environment.yaml", &mut port)
            else {
                panic!("original capture failure")
            };
            assert_eq!(written, "second.yaml");
            assert!(Rc::ptr_eq(&original, &error.0));
            assert_eq!(port.calls.len(), 2);
        }
    }
}
