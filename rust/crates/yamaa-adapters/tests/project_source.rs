#![cfg(feature = "yaml")]
//! Project contracts: the decoder port's implementation.
mod project {
    mod decoder {
        use crate::support::project_call_compiler as support;
        use std::{cell::RefCell, rc::Rc, sync::Arc};
        use yamaa_adapters::decoder::Decoder;
        use yamaa_core::{
            dataset::ExecutionError,
            decoder::Decoder as _,
            function_invocation::{Argument, FailureKind, HostError},
            host_functions::{HostFunctions, Lock},
            project_activation::Failure,
            project_domain,
            specification_run::{CapturedAttempt, PortError},
        };
        use yamaa_core::{
            function_signature::{LogicalSignature, ProjectFunctionIdentity},
            project_environment::{Draft, Environment, LockKind, LockReference},
            project_function::Language,
            specification::{SourceDeclaration, SourceInterpretation, SourceProfile},
            table::{CellError, TableAccess, ValueRef},
            value::{ColumnType, Value},
        };

        #[derive(Debug, PartialEq)]
        struct Payload(Rc<()>);

        #[test]
        fn wide_producer_contract_decodes_1024_and_refuses_1025_actual_fields() {
            let names = (0..1_024)
                .map(|column| format!("F{column:04}"))
                .collect::<Vec<_>>();
            let columns = names
                .iter()
                .map(|name| {
                    format!("  - {{name: {name}, type: str, label: Field, derivation: \"'x'\"}}\n")
                })
                .collect::<String>();
            let document = crate::support::specification_run::prepare_as(
                "wide-producer.yaml",
                format!("yamaa: '1.0'\ntype: spec\ndomain: TEST\nkeys: [F0000]\ninput: {{RAW: raw.csv}}\ncolumns:\n{columns}output: {{path: produced.csv, columns: [{}]}}\n", names.join(", ")).as_bytes(),
            );
            let contract =
                yamaa_core::producer_contract::prepare_document(document.document()).unwrap();
            let source = SourceDeclaration {
                name: "P".into(),
                path: "produced.csv".into(),
                types: names
                    .iter()
                    .map(|name| (name.clone(), ColumnType::Str))
                    .collect(),
                profile: SourceProfile::Csv,
                encoding: None,
                byte_order_mark: false,
                empty_string_present: false,
                ordinal: None,
            };
            let bytes = format!("{}\n{}\n", names.join(","), vec!["x"; 1_024].join(","));
            let mut decoder = Decoder::<Payload>::default();
            let table = decoder
                .table(
                    &source,
                    bytes.as_bytes(),
                    SourceInterpretation::Ordinary,
                    Some(&contract),
                )
                .unwrap();
            assert_eq!(table.schema().columns().len(), 1_024);
            assert_eq!(table.cell(0, 1_023).unwrap(), ValueRef::Str("x"));
            let excess = format!("{},EXCESS\n", names.join(","));
            assert!(matches!(
                decoder.table(
                    &source,
                    excess.as_bytes(),
                    SourceInterpretation::Ordinary,
                    Some(&contract)
                ),
                Err(yamaa_adapters::specification_run::Error::Source(
                    yamaa_adapters::csv_source::TextTableError::Csv(
                        yamaa_core::csv_source::Error::Limit {
                            resource: "columns",
                            limit: 1024,
                            actual: Some(1025),
                        }
                    )
                ))
            ));
            assert_eq!(
                decoder
                    .table(
                        &source,
                        bytes.as_bytes(),
                        SourceInterpretation::Ordinary,
                        Some(&contract)
                    )
                    .unwrap()
                    .cell(0, 0)
                    .unwrap(),
                ValueRef::Str("x")
            );
        }

        #[test]
        fn producer_csv_admits_original_header_before_typing_and_owns_portable_mismatch() {
            let document = crate::support::specification_run::prepare_as(
                "held-producer.yaml",
                b"yamaa: '1.0'\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {RAW: raw.csv}\ncolumns:\n  - {name: ID, type: int, label: Identifier, derivation: RAW.ID}\n  - {name: VALUE, type: int, label: Value, derivation: RAW.VALUE}\noutput: {path: produced.csv, columns: [ID, VALUE]}\n",
            );
            let contract =
                yamaa_core::producer_contract::prepare_document(document.document()).unwrap();
            let source = SourceDeclaration {
                name: "P".into(),
                path: "produced.csv".into(),
                types: vec![
                    ("ID".into(), ColumnType::Int),
                    ("VALUE".into(), ColumnType::Int),
                ],
                profile: SourceProfile::Csv,
                encoding: None,
                byte_order_mark: false,
                empty_string_present: false,
                ordinal: None,
            };
            let mut decoder = Decoder::<Payload>::default();
            for bytes in [
                b"VALUE,ID\nnot-an-int,also-bad\n".as_slice(),
                b"ID\nnot-an-int\n",
                b"ID,VALUE,EXTRA\nnot-an-int,also-bad,x\n",
            ] {
                let yamaa_adapters::specification_run::Error::ProducerMetadata(diagnostic) =
                    decoder
                        .table(
                            &source,
                            bytes,
                            SourceInterpretation::Ordinary,
                            Some(&contract),
                        )
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
                .table(
                    &source,
                    b"ID,VALUE\n1,13\n",
                    SourceInterpretation::Ordinary,
                    Some(&contract),
                )
                .unwrap();
            assert_eq!(table.cell(0, 1).unwrap(), ValueRef::Int(13));
            let mut ordinal_source = source.clone();
            ordinal_source.ordinal = Some("REC".into());
            let table = decoder
                .table(
                    &ordinal_source,
                    b"ID,VALUE\n1,13\n2,17\n",
                    SourceInterpretation::Ordinary,
                    Some(&contract),
                )
                .unwrap();
            assert_eq!(
                table
                    .schema()
                    .columns()
                    .iter()
                    .map(|c| c.name.as_str())
                    .collect::<Vec<_>>(),
                ["ID", "VALUE", "REC"]
            );
            assert_eq!(
                (table.cell(0, 2).unwrap(), table.cell(1, 2).unwrap()),
                (ValueRef::Int(1), ValueRef::Int(2))
            );
            ordinal_source.ordinal = Some("ID".into());
            for bytes in [b"ID,VALUE\n1,13\n".as_slice(), b"ID,VALUE\n"] {
                assert!(matches!(
                    decoder.table(
                        &ordinal_source,
                        bytes,
                        SourceInterpretation::Ordinary,
                        Some(&contract)
                    ),
                    Err(yamaa_adapters::specification_run::Error::Ordinal(
                        yamaa_core::source_ordinal::Error::Collision
                    ))
                ));
            }

            let header = (0..1_025)
                .map(|i| format!("F{i}"))
                .collect::<Vec<_>>()
                .join(",")
                + "\n";
            assert!(matches!(
                decoder.table(
                    &source,
                    header.as_bytes(),
                    SourceInterpretation::Ordinary,
                    Some(&contract)
                ),
                Err(yamaa_adapters::specification_run::Error::Source(
                    yamaa_adapters::csv_source::TextTableError::Csv(
                        yamaa_core::csv_source::Error::Limit {
                            resource: "columns",
                            limit: 1024,
                            actual: Some(1025),
                        }
                    )
                ))
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
                encoding: None,
                byte_order_mark: false,
                empty_string_present: true,
                ordinal: None,
            };
            let table = decoder
        .table(
            &source,
            b"I,S,F,D,T\n9223372036854775807,\"x\0y\",-0.0,2024-02-29,2024-02-29T12:34:56\n",
            SourceInterpretation::Ordinary,
            None,
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
            let owned = table.into_table().expect("an input read whole");
            let ValueRef::Str(text) = owned.cell(0, 1).unwrap() else {
                unreachable!()
            };
            assert_eq!(text.as_ptr(), retained);
            let error = decoder
                .table(
                    &source,
                    b"I,S,F,D,T\nbad,x,1,2024-02-29,2024-02-29T12:34:56\n",
                    SourceInterpretation::Ordinary,
                    None,
                )
                .err()
                .unwrap();
            assert!(decoder.continue_after(&error));
            let wide_header = (0..1025)
                .map(|index| format!("C{index}"))
                .collect::<Vec<_>>()
                .join(",")
                + "\n";
            let error = decoder
                .table(
                    &source,
                    wide_header.as_bytes(),
                    SourceInterpretation::Ordinary,
                    None,
                )
                .err()
                .unwrap();
            assert!(matches!(
                &error,
                yamaa_adapters::specification_run::Error::Source(
                    yamaa_adapters::csv_source::TextTableError::Csv(
                        yamaa_core::csv_source::Error::Limit {
                            resource: "columns",
                            limit: 1024,
                            actual: Some(1025),
                        }
                    )
                )
            ));
            assert!(!decoder.continue_after(&error));

            #[cfg(feature = "parquet")]
            {
                let bytes = &crate::support::parquet_files::file("text")[..];
                for present in [false, true] {
                    let source = SourceDeclaration {
                        name: "SRC".into(),
                        path: "text.parquet".into(),
                        types: vec![("I".into(), ColumnType::Int), ("S".into(), ColumnType::Str)],
                        profile: SourceProfile::Parquet,
                        encoding: None,
                        byte_order_mark: false,
                        empty_string_present: present,
                        ordinal: None,
                    };
                    let table = decoder
                        .table(&source, bytes, SourceInterpretation::Ordinary, None)
                        .unwrap();
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
        }

        type Trace = Rc<RefCell<Vec<&'static str>>>;
        struct Activation {
            trace: Trace,
            payload: Rc<()>,
            live_failed: bool,
            lock_failed: bool,
        }
        impl HostFunctions for Activation {
            type Handle = ();
            type Error = Payload;
            fn verify_lock(
                &mut self,
                language: Language,
                lock: Lock<'_>,
                functions: &[ProjectFunctionIdentity],
            ) -> Result<(), Payload> {
                assert_eq!(language, Language::Python);
                assert_eq!(lock.reference.kind, LockKind::Uv);
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
            fn invoke(
                &mut self,
                _: &(),
                args: &[Argument<'_>],
            ) -> Result<Value, HostError<Payload>> {
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
        use yamaa_core::resolver;
        impl resolver::Resolver for Source {
            type Error = Payload;
            fn inspect(
                &mut self,
                name: resolver::Name<'_>,
            ) -> Result<resolver::Located, Self::Error> {
                let resolver::Name::Input(source) = name else {
                    panic!("unexpected request {name:?}")
                };
                assert_eq!(source.path, "input.csv");
                self.trace.borrow_mut().push("inspect");
                Ok(resolver::Located {
                    identity: source.path.clone(),
                    display_path: source.path.clone(),
                })
            }
            fn capture(
                &mut self,
                name: resolver::Name<'_>,
            ) -> Result<resolver::Captured, Self::Error> {
                let resolver::Name::Input(_) = name else {
                    panic!("unexpected request {name:?}")
                };
                let before = self.reads;
                self.trace.borrow_mut().push("capture");
                self.reads = 1;
                Ok(resolver::Captured::resource(
                    Arc::clone(&self.bytes),
                    self.reads != before,
                ))
            }
            fn failure(&self, _: resolver::Name<'_>, _: &Self::Error) -> Option<resolver::Failure> {
                None
            }
            fn rebase(
                &mut self,
                _: &resolver::Located,
                _: &resolver::Located,
                _: &str,
            ) -> Result<String, Self::Error> {
                panic!("an input is never rebased")
            }
        }
        #[test]
        fn real_decoder_builds_reactivate_cached_sources_and_retain_original_non_send_errors() {
            let environment = Environment::admit(
                Some(Language::Python),
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
                },
            )
            .unwrap()
            .into_execution();
            let raw = b"yamaa: '1.0'\ntype: spec\ndomain: TEST\ninput: {SRC: {path: input.csv, types: {ID: int}}}\nkeys: [ID]\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: VALUE, type: int, derivation: {function: {name: id, args: {x: SRC.ID}}}}\noutput: {path: output.csv, columns: [ID, VALUE]}\n";
            let document = crate::support::specification_run::prepare(raw);
            let checked = project_domain::check_owned_tree_linked(
                document.tree().clone(),
                document.document(),
                environment,
                Default::default(),
            )
            .unwrap();
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
            for created in [Some(1), Some(0)] {
                checked
                    .build_into(
                        (&mut activation, b""),
                        &mut source,
                        &mut decoder,
                        &mut attempt,
                        (
                            None,
                            yamaa_core::application_observer::Scope::new(
                                &yamaa_core::application_observer::Noop,
                            ),
                        ),
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
                    (&mut activation, b""),
                    &mut source,
                    &mut decoder,
                    &mut attempt,
                    (
                        None,
                        yamaa_core::application_observer::Scope::new(
                            &yamaa_core::application_observer::Noop,
                        ),
                    ),
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
                    (&mut activation, b""),
                    &mut source,
                    &mut decoder,
                    &mut attempt,
                    (
                        None,
                        yamaa_core::application_observer::Scope::new(
                            &yamaa_core::application_observer::Noop,
                        ),
                    ),
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
    }
}
