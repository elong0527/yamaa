//! Service contracts: how a partitioned run reads its driver input (#3792).
//! A specification that reads no other input, and whose plan needs no whole
//! table, reads it by partition and admits each partition on its own; any
//! other run reads every input whole and admits it whole, exactly as an
//! unpartitioned run does.
//!
//! The seam is the run's choice of read and its admission of inputs beyond
//! the row limit, which no fixture can hold. The inputs are geometry alone:
//! a table reports its schema and record count and panics on any cell read,
//! so admission is observed before data. A run that passes admission is
//! cancelled at its key grain's first key, so the attempt ends as cancelled
//! rather than with a limit.
mod service {
    mod partitioned_reading {
        use std::cell::{Cell, RefCell};
        use std::num::NonZeroUsize;
        use std::rc::Rc;
        use std::sync::Arc;
        use yamaa_core::{
            application_observer::{Noop, Scope},
            dataset::{ExecutionError, Partitioning},
            decoder,
            limits::{LimitExceeded, Limits},
            resolver::{Captured, Failure, Located, Name, Resolver},
            schema::Document,
            specification::{SourceDeclaration, SourceInterpretation},
            specification_run::{self as run, CapturedAttempt, PortError, RunError},
            table::{CellError, Column, TableAccess, TableSchema, ValueRef},
            value::ColumnType,
        };

        use crate::support::document::Tree;
        use crate::support::specification::{header, prepare};

        /// A specification of the inputs `inputs`, all CSV files, keyed by
        /// `ID` from the first, whose `VALUE` derives `value`.
        fn document(inputs: &[&str], value: Tree) -> Document {
            let column = |name, derivation| {
                Tree::map(vec![
                    ("name", Tree::text(name)),
                    ("type", Tree::text("int")),
                    ("derivation", derivation),
                ])
            };
            let mut fields = header();
            if inputs.len() > 1 {
                fields.push(("base", Tree::text("SRC")));
            }
            fields.extend([
                ("domain", Tree::text("TEST")),
                (
                    "input",
                    Tree::map(
                        inputs
                            .iter()
                            .map(|name| {
                                let path = format!("{}.csv", name.to_ascii_lowercase());
                                (*name, Tree::map(vec![("path", Tree::text(&path))]))
                            })
                            .collect(),
                    ),
                ),
                ("keys", Tree::List(vec![Tree::text("ID")])),
                (
                    "columns",
                    Tree::List(vec![
                        column("ID", Tree::map(vec![("value", Tree::text("SRC.ID"))])),
                        column("VALUE", value),
                    ]),
                ),
                (
                    "output",
                    Tree::map(vec![
                        ("path", Tree::text("result.csv")),
                        (
                            "columns",
                            Tree::List(vec![Tree::text("ID"), Tree::text("VALUE")]),
                        ),
                    ]),
                ),
            ]);
            Tree::map(fields).document()
        }

        /// A derivation every partition can run alone.
        fn row_wise() -> Tree {
            Tree::map(vec![("value", Tree::text("ID + 1"))])
        }

        /// A row number over every record, or within each declared group.
        fn windowed(group_by: &[&str]) -> Tree {
            let mut window = vec![("order_by", Tree::List(vec![Tree::text("ID")]))];
            if !group_by.is_empty() {
                window.push((
                    "group_by",
                    Tree::List(group_by.iter().map(Tree::text).collect()),
                ));
            }
            Tree::map(vec![(
                "row_number",
                Tree::map(vec![("window", Tree::map(window))]),
            )])
        }

        fn schema() -> TableSchema {
            TableSchema::new(vec![Column {
                name: "ID".into(),
                kind: ColumnType::Int,
            }])
            .unwrap()
        }

        #[derive(Debug)]
        struct Payload;

        /// Serves every input as the same held bytes.
        struct Port;
        impl Resolver for Port {
            type Error = Payload;
            fn inspect(&mut self, name: Name<'_>) -> Result<Located, Payload> {
                let Name::Input(source) = name else {
                    panic!("unexpected request {name:?}")
                };
                Ok(Located {
                    identity: source.path.clone(),
                    display_path: source.path.clone(),
                })
            }
            fn capture(&mut self, _: Name<'_>) -> Result<Captured, Payload> {
                Ok(Captured::resource(Arc::from(b"held".as_slice()), true))
            }
            fn failure(&self, _: Name<'_>, _: &Payload) -> Option<Failure> {
                None
            }
            fn rebase(&mut self, _: &Located, _: &Located, _: &str) -> Result<String, Payload> {
                panic!("an input is never rebased")
            }
        }

        /// One table of `rows` records of [`schema`].
        struct Geometry(TableSchema, usize);
        impl TableAccess for Geometry {
            type Error = Payload;
            fn schema(&self) -> &TableSchema {
                &self.0
            }
            fn row_count(&self) -> usize {
                self.1
            }
            fn cell(&self, _: usize, _: usize) -> Result<ValueRef<'_>, CellError<Payload>> {
                panic!("admission precedes every cell read")
            }
        }

        /// An input read whole, or by partition.
        enum Input {
            Whole(Geometry),
            Partitions(Vec<Geometry>),
        }
        impl TableAccess for Input {
            type Error = Payload;
            fn schema(&self) -> &TableSchema {
                match self {
                    Self::Whole(table) => table.schema(),
                    Self::Partitions(parts) => parts[0].schema(),
                }
            }
            fn row_count(&self) -> usize {
                match self {
                    Self::Whole(table) => table.row_count(),
                    Self::Partitions(parts) => parts.iter().map(Geometry::row_count).sum(),
                }
            }
            fn cell(&self, _: usize, _: usize) -> Result<ValueRef<'_>, CellError<Payload>> {
                panic!("admission precedes every cell read")
            }
        }

        /// Decodes every input as `records` records of [`schema`], whole or
        /// in partitions, and records each request.
        struct Inputs {
            records: usize,
            requests: Rc<RefCell<Vec<&'static str>>>,
        }
        impl decoder::Decoder for Inputs {
            crate::support::ports::no_documents!();
            type Error = Payload;
            type Table = Input;
            fn continue_after(&self, _: &Payload) -> bool {
                false
            }
            fn table(
                &mut self,
                _: &SourceDeclaration,
                _: &[u8],
                _: SourceInterpretation,
                _: Option<&yamaa_core::producer_contract::Contract>,
            ) -> Result<Input, Payload> {
                self.requests.borrow_mut().push("whole");
                Ok(Input::Whole(Geometry(schema(), self.records)))
            }
            fn partition_schema(
                &mut self,
                _: &SourceDeclaration,
                _: &[u8],
                _: SourceInterpretation,
            ) -> Option<TableSchema> {
                self.requests.borrow_mut().push("header");
                Some(schema())
            }
            fn partitioned(
                &mut self,
                _: &SourceDeclaration,
                _: &[u8],
                _: SourceInterpretation,
                _: Option<&yamaa_core::producer_contract::Contract>,
                rows: NonZeroUsize,
            ) -> Result<Input, Payload> {
                self.requests.borrow_mut().push("partitioned");
                let parts = (0..self.records)
                    .step_by(rows.get())
                    .map(|start| Geometry(schema(), rows.get().min(self.records - start)))
                    .collect();
                Ok(Input::Partitions(parts))
            }
            fn partitions(input: &Input) -> Vec<&dyn TableAccess<Error = Payload>> {
                match input {
                    Input::Whole(_) => vec![input],
                    Input::Partitions(parts) => parts
                        .iter()
                        .map(|part| part as &dyn TableAccess<Error = Payload>)
                        .collect(),
                }
            }
        }

        /// How an attempt ended: cancelled after admission, or refused at
        /// an input's limit.
        #[derive(Debug, PartialEq)]
        enum End {
            Cancelled,
            Refused { source: usize, error: LimitExceeded },
        }

        /// Run `document` over inputs of `records` records each, divided as
        /// `partitioning` asks: how the attempt ended and what it asked the
        /// decoder for.
        fn attempt(
            document: &Document,
            records: usize,
            partitioning: Option<Partitioning>,
        ) -> (End, Vec<&'static str>) {
            let prepared = prepare(document).unwrap();
            let requests = Rc::default();
            let mut inputs = Inputs {
                records,
                requests: Rc::clone(&requests),
            };
            // The run admits its inputs after the first checkpoint, verifies
            // its intermediates at the second, and its key grain admits the
            // driver's records again before its first key, the third, which
            // cancels.
            let polls = Cell::new(0);
            let probe = || {
                polls.set(polls.get() + 1);
                polls.get() > 2
            };
            let mut attempt = CapturedAttempt::new(prepared.source());
            run::execute_into(
                &prepared,
                &mut Port,
                &mut inputs,
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                &mut attempt,
                Scope::new(&Noop)
                    .cancellable(&probe)
                    .partitioned(partitioning),
            );
            let end = match attempt.result {
                Ok(execution) => match execution.result.map(|_| ()).map_err(|error| *error) {
                    Err(ExecutionError::Cancelled) => End::Cancelled,
                    other => panic!("an attempt that passed admission ended {other:?}"),
                },
                Err(PortError::Run(RunError::Boundary { source, error })) => {
                    End::Refused { source, error }
                }
                Err(other) => panic!("an attempt ended {other:?}"),
            };
            let requests = requests.borrow().clone();
            (end, requests)
        }

        fn partitions(
            rows: usize,
            leading_keys: Option<usize>,
            whole_table_fallback: bool,
        ) -> Option<Partitioning> {
            Some(Partitioning {
                rows: NonZeroUsize::new(rows).unwrap(),
                leading_keys: leading_keys.map(|keys| NonZeroUsize::new(keys).unwrap()),
                whole_table_fallback,
            })
        }

        /// The whole table of `records` records, refused.
        fn refused(records: usize) -> End {
            End::Refused {
                source: 0,
                error: Limits::table(records, 1).unwrap_err(),
            }
        }

        /// Each case runs a specification over inputs of 11,000,000 records,
        /// 1,000,000 beyond the row limit, or 12,000,000 for one whose
        /// first partition is itself beyond it.
        #[test]
        fn a_run_reads_by_partition_only_a_single_input_its_plan_divides() {
            const RECORDS: usize = Limits::TABLE_ROWS + 1_000_000;
            for (inputs, value, records, partitioning, expected, requests, case) in [
                (
                    &["SRC"][..],
                    row_wise(),
                    RECORDS,
                    partitions(6_000_000, None, false),
                    End::Cancelled,
                    vec!["header", "partitioned"],
                    "partitions of 6,000,000 and 5,000,000 records, each admitted",
                ),
                (
                    &["SRC"],
                    row_wise(),
                    RECORDS,
                    None,
                    refused(RECORDS),
                    vec!["whole"],
                    "an unpartitioned run admits the whole input",
                ),
                (
                    &["SRC"],
                    row_wise(),
                    RECORDS + 1_000_000,
                    partitions(RECORDS, None, false),
                    End::Refused {
                        source: 0,
                        error: Limits::table(RECORDS, 1).unwrap_err(),
                    },
                    vec!["header", "partitioned"],
                    "a partition beyond the limit names its own records",
                ),
                (
                    &["SRC"],
                    windowed(&[]),
                    RECORDS,
                    partitions(6_000_000, None, true),
                    refused(RECORDS),
                    vec!["header", "whole"],
                    "the whole-table fallback reads and admits the whole input",
                ),
                (
                    &["SRC"],
                    windowed(&[]),
                    RECORDS,
                    partitions(6_000_000, None, false),
                    refused(RECORDS),
                    vec!["header", "whole"],
                    "a plan that needs the whole table admits the whole input first",
                ),
                (
                    &["SRC"],
                    windowed(&["ID"]),
                    RECORDS,
                    partitions(6_000_000, Some(1), false),
                    refused(RECORDS),
                    vec!["header", "whole"],
                    "a key-bounded window reads and admits the whole input before cutting keys",
                ),
                (
                    &["SRC", "LOOKUP"],
                    row_wise(),
                    RECORDS,
                    partitions(6_000_000, None, false),
                    refused(RECORDS),
                    vec!["whole"],
                    "a specification of two inputs reads each whole",
                ),
            ] {
                assert_eq!(
                    attempt(&document(inputs, value), records, partitioning),
                    (expected, requests),
                    "{case}"
                );
            }
        }
    }
}
