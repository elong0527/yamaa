//! Execution contracts: row intermediate context.
mod execution {
    mod row_intermediate_context {
        //! Complete record attempts have stable choices and separate diagnostic positions.
        use crate::support::table_schema as schema;
        use std::{cell::RefCell, rc::Rc};

        use yamaa_core::{
            schema::{Document, DocumentNode as N},
            specification::SourceDeclaration,
            table::{CellError, TableAccess, TableSchema, ValueRef},
            value::{ColumnType as T, Value},
        };
        #[derive(Debug)]
        struct Failure(Box<u8>, Rc<()>);
        type Trace = Rc<RefCell<Vec<(&'static str, usize, usize)>>>;
        struct Table {
            name: &'static str,
            schema: TableSchema,
            rows: Vec<Vec<Value>>,
            fail: RefCell<Option<(usize, usize, Failure)>>,
            trace: Trace,
        }
        impl TableAccess for Table {
            type Error = Failure;
            fn schema(&self) -> &TableSchema {
                &self.schema
            }
            fn row_count(&self) -> usize {
                self.rows.len()
            }
            fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Failure>> {
                self.trace.borrow_mut().push((self.name, row, column));
                if self
                    .fail
                    .borrow()
                    .as_ref()
                    .is_some_and(|(r, c, _)| *r == row && *c == column)
                {
                    return Err(CellError::Access(self.fail.borrow_mut().take().unwrap().2));
                }
                Ok(ValueRef::from(&self.rows[row][column]))
            }
        }
        fn tables() -> (Table, Table, Trace) {
            let trace = Rc::new(RefCell::new(Vec::new()));
            let text = |s: &str| Value::Str(s.into());
            (
                Table {
                    name: "SRC",
                    schema: schema(&[("ID", T::Str), ("K", T::Str), ("KEEP", T::Str)]),
                    rows: vec![
                        vec![text("a"), text("x"), text("0")],
                        vec![text("b"), text("y"), text("1")],
                        vec![text("c"), text("z"), text("0")],
                    ],
                    fail: RefCell::default(),
                    trace: trace.clone(),
                },
                Table {
                    name: "DONOR",
                    schema: schema(&[("K", T::Str), ("V", T::Str)]),
                    rows: vec![vec![text("x"), text("X")], vec![text("y"), text("Y")]],
                    fail: RefCell::default(),
                    trace: trace.clone(),
                },
                trace,
            )
        }

        use crate::support::document::Tree;
        use crate::support::specification::prepare;

        fn port_document(
            reordered: bool,
            drivers: [Option<&str>; 2],
            grouped: bool,
            field: &str,
        ) -> Document {
            let source = |name| Tree::map(vec![("value", Tree::text(name))]);
            let mut inputs = vec![
                ("SRC", Tree::map(vec![("path", Tree::text("src.csv"))])),
                ("DONOR", Tree::map(vec![("path", Tree::text("donor.csv"))])),
            ];
            if reordered {
                inputs.reverse();
            }
            let mut rows = Vec::new();
            for (id, driver) in ["first", "second"].iter().zip(drivers) {
                let mut fields = vec![
                    ("id", Tree::text(id)),
                    ("derivations", Tree::map(vec![("ROWV", source(field))])),
                ];
                if let Some(driver) = driver {
                    fields.push(("dataset", Tree::text(driver)));
                }
                if grouped {
                    fields.push(("group_by", Tree::List(vec![Tree::text("SRC.ID")])));
                }
                rows.push(Tree::map(fields));
            }
            let tree = Tree::map(vec![
                ("yamaa", Tree::text("1.0")),
                ("type", Tree::text("spec")),
                ("domain", Tree::text("TEST")),
                ("input", Tree::map(inputs)),
                ("keys", Tree::List(vec![Tree::text("ID")])),
                (
                    "intermediates",
                    Tree::List(vec![Tree::map(vec![
                        ("id", Tree::text("P")),
                        ("dataset", Tree::text("DONOR")),
                        ("key", Tree::map(vec![("K", Tree::text("KEY"))])),
                        ("absent", Tree::Scalar(N::Null)),
                    ])]),
                ),
                (
                    "columns",
                    Tree::List(vec![
                        Tree::map(vec![
                            ("name", Tree::text("ID")),
                            ("type", Tree::text("str")),
                            ("derivation", source("SRC.ID")),
                        ]),
                        Tree::map(vec![
                            ("name", Tree::text("KEY")),
                            ("type", Tree::text("str")),
                            ("derivation", source("SRC.K")),
                        ]),
                        Tree::map(vec![
                            ("name", Tree::text("ROWV")),
                            ("type", Tree::text("str")),
                        ]),
                        Tree::map(vec![
                            ("name", Tree::text("COLV")),
                            ("type", Tree::text("str")),
                            ("derivation", source("P.V")),
                        ]),
                    ]),
                ),
                ("rows", Tree::List(rows)),
                (
                    "output",
                    Tree::map(vec![
                        ("path", Tree::text("out.csv")),
                        (
                            "columns",
                            Tree::List(vec![
                                Tree::text("ID"),
                                Tree::text("KEY"),
                                Tree::text("ROWV"),
                                Tree::text("COLV"),
                            ]),
                        ),
                    ]),
                ),
            ]);
            tree.document()
        }

        /// A port interaction: the order of the resolver's and the decoder's
        /// requests, and an opaque failure that moves once, which no benchmark
        /// observes.
        #[test]
        fn all_inputs_inspect_then_capture_decode_in_authored_order_and_move_opaque_failure_once() {
            use std::sync::Arc;
            use yamaa_core::decoder;
            use yamaa_core::resolver::{self, Captured, Located, Name, Resolver};
            use yamaa_core::specification_run::{self as run, PortError, RunError};
            type IoTrace = Rc<RefCell<Vec<String>>>;
            struct Port {
                trace: IoTrace,
                reads: usize,
                fail: Option<Failure>,
            }
            impl Resolver for Port {
                type Error = Failure;
                fn inspect(&mut self, name: Name<'_>) -> Result<Located, Failure> {
                    let Name::Input(source) = name else {
                        panic!("unexpected request {name:?}")
                    };
                    self.trace
                        .borrow_mut()
                        .push(format!("inspect:{}", source.name));
                    Ok(Located {
                        identity: source.path.clone(),
                        display_path: source.path.clone(),
                    })
                }
                fn capture(&mut self, name: Name<'_>) -> Result<Captured, Failure> {
                    let Name::Input(source) = name else {
                        panic!("unexpected request {name:?}")
                    };
                    self.trace
                        .borrow_mut()
                        .push(format!("capture:{}", source.name));
                    if source.name == "SRC" {
                        if let Some(error) = self.fail.take() {
                            return Err(error);
                        }
                    }
                    self.reads += 1;
                    Ok(Captured::resource(Arc::from([1u8]), true))
                }
                fn failure(&self, _: Name<'_>, _: &Failure) -> Option<resolver::Failure> {
                    None
                }
                fn rebase(&mut self, _: &Located, _: &Located, _: &str) -> Result<String, Failure> {
                    panic!("an input is never rebased")
                }
            }
            struct Decoder {
                trace: IoTrace,
                fail: Option<Failure>,
            }
            impl decoder::Decoder for Decoder {
                crate::support::ports::no_documents!();
                fn continue_after(&self, _error: &Self::Error) -> bool {
                    false
                }
                type Error = Failure;
                type Table = Table;
                fn table(
                    &mut self,
                    source: &SourceDeclaration,
                    bytes: &[u8],
                    _interpretation: yamaa_core::specification::SourceInterpretation,
                    _contract: Option<&yamaa_core::producer_contract::Contract>,
                ) -> Result<Table, Failure> {
                    self.trace
                        .borrow_mut()
                        .push(format!("decode:{}", source.name));
                    assert_eq!(bytes, [1]);
                    if source.name == "SRC" {
                        if let Some(error) = self.fail.take() {
                            return Err(error);
                        }
                    }
                    let (src, donor, _) = tables();
                    Ok(if source.name == "SRC" { src } else { donor })
                }
            }
            let prepared = prepare(&port_document(
                true,
                [Some("SRC"), Some("SRC")],
                false,
                "P.V",
            ))
            .unwrap();
            for capture_failure in [false, true] {
                let io = Rc::new(RefCell::new(Vec::new()));
                let payload = Box::new(7);
                let address = &*payload as *const u8;
                let marker = Rc::new(());
                let mut failure = Some(Failure(payload, marker.clone()));
                let mut port = Port {
                    trace: io.clone(),
                    reads: 0,
                    fail: if capture_failure {
                        failure.take()
                    } else {
                        None
                    },
                };
                let mut decoder = Decoder {
                    trace: io.clone(),
                    fail: failure,
                };
                let mut attempt = run::CapturedAttempt::new(prepared.source());
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
                let error = if capture_failure {
                    let PortError::Capture(error) = attempt.result.unwrap_err() else {
                        panic!("original capture payload")
                    };
                    error
                } else {
                    let PortError::Run(RunError::Sources(mut errors)) = attempt.result.unwrap_err()
                    else {
                        panic!("original decoder payload")
                    };
                    assert_eq!(errors.len(), 1);
                    let (source, error) = errors.pop().unwrap();
                    assert_eq!(source, 1);
                    error
                };
                assert_eq!(&*error.0 as *const u8, address);
                assert!(Rc::ptr_eq(&error.1, &marker));
                let mut expected = vec![
                    "inspect:DONOR",
                    "inspect:SRC",
                    "capture:DONOR",
                    "decode:DONOR",
                    "capture:SRC",
                ];
                if !capture_failure {
                    expected.push("decode:SRC");
                }
                assert_eq!(*io.borrow(), expected, "no later request or retry");
                assert!(attempt.sources[0].snapshot.is_some());
                assert!(attempt.sources[0].table.is_some());
                assert_eq!(attempt.sources[0].read.source.name, "DONOR");
                assert_eq!(port.reads, if capture_failure { 1 } else { 2 });
                assert!(attempt.sources[1].table.is_none());
            }
        }
    }
}
