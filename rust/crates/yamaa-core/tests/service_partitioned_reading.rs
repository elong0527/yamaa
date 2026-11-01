//! Service contracts: how a partitioned run reads its driver input (#3792).
//! A specification whose plan needs no whole table reads it by partition and
//! admits each partition on its own, after reading and admitting every other
//! input whole (#4191); any other run reads every input whole and admits it
//! whole, exactly as an unpartitioned run does.
//!
//! The seam is the run's choice of read and its admission of inputs beyond
//! the row limit, which no fixture can hold. The inputs are geometry alone:
//! a table reports its schema and record count and panics on any cell read,
//! so admission is observed before data. Only the partitions a scan visits
//! answer a cell, each record's key, which a key-bounded cut reads (#3830).
//! A run that passes admission is cancelled at its key grain's first key, so
//! the attempt ends as cancelled rather than with a limit.
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
        fn document_tree(inputs: &[&str], value: Tree) -> Tree {
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
            Tree::map(fields)
        }

        fn document(inputs: &[&str], value: Tree) -> Document {
            document_tree(inputs, value).document()
        }

        /// Two intermediate sides of one physical input still form a
        /// global pair relation (LOOKUP-27).
        fn paired() -> Document {
            let mut tree = document_tree(&["SRC"], row_wise());
            tree.remove(&["columns", "0", "derivation"]);
            tree.remove(&["columns", "1", "derivation"]);
            let side = |name| {
                Tree::map(vec![
                    ("id", Tree::text(name)),
                    ("dataset", Tree::text("SRC")),
                    ("filter", Tree::text("SRC.ID > 0")),
                ])
            };
            tree.set(
                &["intermediates"],
                Tree::list(vec![
                    side("L"),
                    side("R"),
                    Tree::map(vec![
                        ("id", Tree::text("P")),
                        ("dataset", Tree::text("L")),
                        ("inner_join", Tree::text("R")),
                        ("key", Tree::list(vec![Tree::text("ID")])),
                        (
                            "derivations",
                            Tree::map(vec![
                                ("ID", Tree::text("L.ID")),
                                ("VALUE", Tree::text("R.ID")),
                            ]),
                        ),
                    ]),
                ]),
            );
            tree.set(
                &["rows"],
                Tree::list(vec![Tree::map(vec![
                    ("id", Tree::text("pairs")),
                    ("dataset", Tree::text("P")),
                    (
                        "derivations",
                        Tree::map(vec![
                            ("ID", Tree::text("P.ID")),
                            ("VALUE", Tree::text("P.VALUE")),
                        ]),
                    ),
                ])]),
            );
            tree.document()
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

        /// An input read whole; a partition its scan visits, whose records
        /// all hold the key `key`; or the input the scan read, which holds
        /// no record.
        enum Input {
            Whole(Geometry),
            Visited(Geometry, i64),
            Scanned(Geometry),
        }
        impl TableAccess for Input {
            type Error = Payload;
            fn schema(&self) -> &TableSchema {
                match self {
                    Self::Whole(table) | Self::Visited(table, _) | Self::Scanned(table) => {
                        table.schema()
                    }
                }
            }
            fn row_count(&self) -> usize {
                match self {
                    Self::Whole(table) | Self::Visited(table, _) | Self::Scanned(table) => {
                        table.row_count()
                    }
                }
            }
            /// Only the scan reads cells: a key-bounded cut reads each
            /// record's key.
            fn cell(&self, _: usize, _: usize) -> Result<ValueRef<'_>, CellError<Payload>> {
                match self {
                    Self::Visited(_, key) => Ok(ValueRef::Int(*key)),
                    _ => panic!("admission precedes every cell read"),
                }
            }
        }

        /// Decodes every input as `records` records of [`schema`], the input
        /// named `LOOKUP` as `lookup` records, whole or in partitions, and
        /// records each request. Decoding the input named `failing` fails,
        /// ending the attempt.
        struct Inputs {
            records: usize,
            lookup: usize,
            failing: Option<&'static str>,
            requests: Rc<RefCell<Vec<&'static str>>>,
        }
        impl Inputs {
            /// The records of `source`, or its decoding failure.
            fn records(&self, source: &SourceDeclaration) -> Result<usize, Payload> {
                match source.name.as_str() {
                    name if Some(name) == self.failing => Err(Payload),
                    "LOOKUP" => Ok(self.lookup),
                    _ => Ok(self.records),
                }
            }
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
                source: &SourceDeclaration,
                _: &[u8],
                _: SourceInterpretation,
                _: Option<&yamaa_core::producer_contract::Contract>,
            ) -> Result<Input, Payload> {
                self.requests.borrow_mut().push("whole");
                Ok(Input::Whole(Geometry(schema(), self.records(source)?)))
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
            /// Visits partitions of `rows` records, the records of the
            /// `n`th all holding the key `n`.
            fn partitioned(
                &mut self,
                source: &SourceDeclaration,
                _: &[u8],
                _: SourceInterpretation,
                _: Option<&yamaa_core::producer_contract::Contract>,
                (rows, _): (NonZeroUsize, NonZeroUsize),
                visit: &mut dyn FnMut(&Input, usize),
            ) -> Option<Result<Input, Payload>> {
                self.requests.borrow_mut().push("partitioned");
                if let Err(error) = self.records(source) {
                    return Some(Err(error));
                }
                for (key, start) in (0..self.records.max(1)).step_by(rows.get()).enumerate() {
                    let records = rows.get().min(self.records - start);
                    visit(
                        &Input::Visited(Geometry(schema(), records), key as i64),
                        start,
                    );
                }
                Some(Ok(Input::Scanned(Geometry(schema(), self.records))))
            }
            fn partition(
                &mut self,
                _: &Input,
                _: &SourceDeclaration,
                _: &[u8],
                _: SourceInterpretation,
                _: Option<&yamaa_core::producer_contract::Contract>,
                range: std::ops::Range<usize>,
            ) -> Result<Input, Payload> {
                self.requests.borrow_mut().push("partition");
                Ok(Input::Whole(Geometry(schema(), range.len())))
            }
        }

        /// How an attempt ended: cancelled after admission, or refused at
        /// an input's limit.
        #[derive(Debug, PartialEq)]
        enum End {
            Cancelled,
            Refused {
                source: usize,
                error: LimitExceeded,
            },
            Global {
                path: String,
            },
            /// Refused by the decoding failures of the inputs at `sources`,
            /// having requested the first `requested` inputs.
            Undecoded {
                sources: Vec<usize>,
                requested: usize,
            },
        }

        /// Run `document` over inputs of `records` records each, divided as
        /// `partitioning` asks: how the attempt ended and what it asked the
        /// decoder for.
        fn attempt(
            document: &Document,
            records: usize,
            partitioning: Option<Partitioning>,
        ) -> (End, Vec<&'static str>) {
            attempt_of(document, (records, records, None), partitioning)
        }

        /// [`attempt`] over inputs of `records` records each, but `LOOKUP`'s
        /// `lookup`, and whose input `failing` fails to decode, if any.
        fn attempt_of(
            document: &Document,
            (records, lookup, failing): (usize, usize, Option<&'static str>),
            partitioning: Option<Partitioning>,
        ) -> (End, Vec<&'static str>) {
            let prepared = prepare(document).unwrap();
            let requests = Rc::default();
            let mut inputs = Inputs {
                records,
                lookup,
                failing,
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
                    Err(ExecutionError::PartitionGlobal { path }) => End::Global { path },
                    other => panic!("an attempt that passed admission ended {other:?}"),
                },
                Err(PortError::Run(RunError::Boundary { source, error })) => {
                    End::Refused { source, error }
                }
                Err(PortError::Run(RunError::Sources(failures))) => End::Undecoded {
                    sources: failures.into_iter().map(|(source, _)| source).collect(),
                    requested: attempt.sources.len(),
                },
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
                whole_output: false,
                threads: NonZeroUsize::MIN,
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
        fn a_run_reads_by_partition_only_a_driver_input_its_plan_divides() {
            const RECORDS: usize = Limits::TABLE_ROWS + 1_000_000;
            for (inputs, value, records, partitioning, expected, requests, case) in [
                (
                    &["SRC"][..],
                    row_wise(),
                    RECORDS,
                    partitions(6_000_000, None, false),
                    End::Cancelled,
                    vec!["header", "partitioned", "partition"],
                    "partitions of 6,000,000 and 5,000,000 records, each admitted, the first decoded",
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
                    End::Cancelled,
                    vec!["header", "partitioned", "partition"],
                    "a key-bounded window reads by partition, cut between the runs of its key",
                ),
            ] {
                assert_eq!(
                    attempt(&document(inputs, value), records, partitioning),
                    (expected, requests),
                    "{case}"
                );
            }
        }

        /// A specification of two inputs whose driver input `SRC` holds
        /// 11,000,000 records reads its other input `LOOKUP` whole first,
        /// then decides from both schemas how to read its driver input
        /// (#4191). The driver input is declared first unless a case says
        /// otherwise, so the run asks for its header before `LOOKUP` and
        /// again once `LOOKUP` has decoded; declared last, it is read in its
        /// place. A decoding failure ends the attempt as reading the inputs
        /// in declaration order does: the driver input's, before `LOOKUP` is
        /// requested.
        #[test]
        fn a_driver_input_beside_another_input_reads_by_partition_after_it() {
            const RECORDS: usize = Limits::TABLE_ROWS + 1_000_000;
            let six = partitions(6_000_000, None, false);
            let src = &["SRC", "LOOKUP"][..];
            for (inputs, value, geometry, partitioning, expected, requests, case) in [
                (
                    src,
                    row_wise(),
                    (RECORDS, 1, None),
                    six,
                    End::Cancelled,
                    vec!["header", "whole", "header", "partitioned", "partition"],
                    "the other input whole, then the driver by partition, the first decoded",
                ),
                (
                    &["LOOKUP", "SRC"],
                    row_wise(),
                    (RECORDS, 1, None),
                    six,
                    End::Cancelled,
                    vec!["whole", "header", "partitioned", "partition"],
                    "a driver input declared last, read by partition in its place",
                ),
                (
                    src,
                    row_wise(),
                    (RECORDS, RECORDS, None),
                    six,
                    End::Refused {
                        source: 1,
                        error: Limits::table(RECORDS, 1).unwrap_err(),
                    },
                    vec!["header", "whole", "header", "partitioned"],
                    "the other input is admitted whole and refused, the driver by partition",
                ),
                (
                    src,
                    windowed(&[]),
                    (RECORDS, 1, None),
                    partitions(6_000_000, None, true),
                    refused(RECORDS),
                    vec!["header", "whole", "header", "whole"],
                    "a plan that needs the whole table reads its driver whole, after the other",
                ),
                (
                    src,
                    row_wise(),
                    (1, 1, Some("SRC")),
                    partitions(1, None, false),
                    End::Undecoded {
                        sources: vec![0],
                        requested: 1,
                    },
                    vec!["header", "whole", "header", "partitioned"],
                    "the driver input's decoding failure, before the other input is requested",
                ),
                (
                    src,
                    row_wise(),
                    (1, 1, Some("LOOKUP")),
                    partitions(1, None, false),
                    End::Undecoded {
                        sources: vec![1],
                        requested: 2,
                    },
                    vec!["header", "whole", "whole"],
                    "the other input's decoding failure, the driver input then read whole",
                ),
            ] {
                assert_eq!(
                    attempt_of(&document(inputs, value), geometry, partitioning),
                    (expected, requests),
                    "{case}"
                );
            }
        }

        #[test]
        fn a_pair_relation_refuses_strict_partitions_and_admits_its_input_whole() {
            // The service's read/admission seam uses geometry, not cells:
            // a fixture cannot hold an input beyond the row limit.
            let document = paired();
            for leading in [None, Some(1)] {
                assert_eq!(
                    attempt(&document, 1, partitions(1, leading, false)),
                    (
                        End::Global {
                            path: "intermediates.P".into()
                        },
                        vec!["header", "whole"]
                    ),
                    "a global pair under {leading:?} partition keys"
                );
            }
            let records = Limits::TABLE_ROWS + 1;
            for fallback in [false, true] {
                assert_eq!(
                    attempt(&document, records, partitions(1, None, fallback)),
                    (refused(records), vec!["header", "whole"]),
                    "the whole input limit with fallback {fallback}"
                );
            }
        }
    }
}
