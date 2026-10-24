//! Execution contracts: dataset diagnostic.
mod execution {
    mod dataset_diagnostic {
        mod borrowed {

            use yamaa_core::dataset::{ExecutionError, RowIdentity};

            use yamaa_core::{
                reduction::NumericReducer,
                table::CellError,
                temporal::{Date, Precision},
                value::{Value, ValueType},
            };

            /// A unit seam no benchmark reaches: the diagnostic projection of
            /// an `ExecutionError` borrows a host payload with neither `Debug`
            /// nor `Clone`, keeps every identity, key and partition, and
            /// leaves the host's error where it was.
            #[test]
            fn borrowed_failure_projection_retains_every_identity_key_partition_and_host_payload() {
                // Projection compiles for a payload with neither Debug nor Clone.
                struct Payload(Box<u8>);
                let identity = || {
                    Some(RowIdentity {
                        position: 7,
                        values: vec![Value::Int(i64::MAX), Value::Missing],
                    })
                };
                let observed = || {
                    vec![
                        ("flag".into(), Value::Bool(false)),
                        ("missing".into(), Value::Missing),
                    ]
                };
                let date = Value::Date(Date::new(2024, 2, 29, Precision::Month).unwrap());
                let faults: Vec<ExecutionError<Payload>> = vec![
                    ExecutionError::MultipleValues {
                        path: "columns.A.source".into(),
                        identifier: "SRC.A".into(),
                        value_count: 3,
                        identity: identity(),
                    },
                    ExecutionError::UnmatchedKey {
                        path: "columns.A.source".into(),
                        dataset: "DM".into(),
                        intermediate: "I".into(),
                        matched_key: observed(),
                        identity: identity(),
                    },
                    ExecutionError::UnmatchedKey {
                        path: "columns.A.derivation.source".into(),
                        dataset: "DM".into(),
                        intermediate: "intermediate(DM)".into(),
                        matched_key: observed(),
                        identity: identity(),
                    },
                    ExecutionError::MultipleMatches {
                        path: "columns.A.source".into(),
                        dataset: "DM".into(),
                        intermediate: "I".into(),
                        match_count: 2,
                        matched_key: observed(),
                        identity: identity(),
                    },
                    ExecutionError::BaselineAmbiguity {
                        path: "columns.A.baseline".into(),
                        column: "A".into(),
                        date,
                        match_count: 2,
                        partition: observed(),
                    },
                    ExecutionError::ReductionType {
                        path: "columns.A.aggregate".into(),
                        expression: "SUM(SRC.A)".into(),
                        reducer: NumericReducer::Sum,
                        source: "SRC.A".into(),
                        actual: ValueType::Str,
                    },
                ];
                for (error, rule) in faults.iter().zip([
                    "COLUMN-2",
                    "LOOKUP-24",
                    "LOOKUP-24",
                    "LOOKUP-10",
                    "WINDOW-9",
                    "FORMULA-23",
                ]) {
                    let finding = error.diagnostic().unwrap().unwrap();
                    assert_eq!(finding.definition().rule, Some(rule));
                    assert_eq!(error.diagnostic(), Ok(Some(finding)));
                    match error {
                        ExecutionError::MultipleValues {
                            identity: Some(value),
                            ..
                        }
                        | ExecutionError::UnmatchedKey {
                            identity: Some(value),
                            ..
                        }
                        | ExecutionError::MultipleMatches {
                            identity: Some(value),
                            ..
                        } => {
                            assert_eq!(value.position, 7);
                            assert_eq!(value.values, [Value::Int(i64::MAX), Value::Missing]);
                        }
                        ExecutionError::BaselineAmbiguity { partition, .. } => {
                            assert_eq!(*partition, observed())
                        }
                        ExecutionError::ReductionType { source, .. } => assert_eq!(source, "SRC.A"),
                        _ => panic!("retained cause changed"),
                    }
                    if let ExecutionError::MultipleMatches { matched_key, .. }
                    | ExecutionError::UnmatchedKey { matched_key, .. } = error
                    {
                        assert_eq!(*matched_key, observed());
                    }
                }
                let payload = Payload(Box::new(9));
                let address = &*payload.0 as *const u8;
                let error = ExecutionError::Cell {
                    path: "source".into(),
                    source_row: 9,
                    error: CellError::Access(payload),
                };
                assert!(error.diagnostic().unwrap().is_none());
                let ExecutionError::Cell {
                    error: CellError::Access(Payload(value)),
                    ..
                } = error
                else {
                    panic!("original host error changed")
                };
                assert_eq!(&*value as *const u8, address);
            }
        }
    }
}
