//! Executor behaviour no specification can reach.
use crate::table::ValueRef;
use std::convert::Infallible;

/// A buffer the executor cannot reserve fails the run as an allocation
/// failure instead of aborting the process: a frame, a filled vector, the
/// rows of a record grain, and the hash partition.
#[test]
fn a_refused_reservation_is_an_allocation_failure() {
    use crate::dataset::execution::ExecutionError;
    let mut frame = super::frame::Frame::new(3);
    let refused = super::reserved::<(), Infallible>(frame.try_reserve(usize::MAX, 0));
    assert!(
        matches!(*refused.unwrap_err(), ExecutionError::Allocation { spec_paths } if spec_paths.is_empty())
    );
    let refused = super::filled::<u64, Infallible>(usize::MAX, 0);
    assert!(
        matches!(*refused.unwrap_err(), ExecutionError::Allocation { spec_paths } if spec_paths.is_empty())
    );
    // A table without columns claims any number of records (#4782).
    let refused = super::Members::records::<Infallible>(None, usize::MAX).err();
    assert!(
        matches!(*refused.unwrap(), ExecutionError::Allocation { spec_paths } if spec_paths.is_empty())
    );
    assert!(super::group::partition(usize::MAX, 1, |_, _| ValueRef::Missing).is_err());
    // A reservation that fits changes nothing a reader sees.
    super::reserved::<(), Infallible>(frame.try_reserve(4, 4)).unwrap();
    assert_eq!(frame.rows(), 0);
    // A record grain's rows each read one record: every record in order, or
    // the eligible ones in theirs.
    for (eligible, expected) in [(None, vec![0, 1, 2]), (Some(&[2, 0][..]), vec![2, 0])] {
        let rows = super::Members::records::<Infallible>(eligible, 3)
            .ok()
            .unwrap();
        let read: Vec<_> = (0..rows.len()).map(|row| rows.of(row).to_vec()).collect();
        let each: Vec<_> = expected.iter().map(|&record| vec![record]).collect();
        assert_eq!((read, rows.total()), (each, expected.len()), "{eligible:?}");
    }
}

/// The accepted dataset lends each column of one type whole, and its view
/// reads every row as the cell does (#4201); a column with no type of its
/// own, every row missing or values of two types, lends none and is read a
/// cell at a time.
#[test]
fn the_dataset_lends_each_typed_column_as_its_cells_read() {
    use crate::table::{Column, TableAccess, TableSchema};
    use crate::temporal::{Date, DateTime, Precision};
    use crate::value::{ColumnType, FiniteFloat, Value};
    let day = Date::new(2024, 2, 29, Precision::Day).unwrap();
    let moment = DateTime::new(day, 1, 2, 3, Precision::Second).unwrap();
    let float = Value::Float(FiniteFloat::new(-0.0).unwrap());
    let kinds = [
        ColumnType::Str,
        ColumnType::Int,
        ColumnType::Float,
        ColumnType::Date,
        ColumnType::DateTime,
        ColumnType::Str,
        ColumnType::Int,
    ];
    let schema = TableSchema::new(
        kinds
            .iter()
            .enumerate()
            .map(|(index, &kind)| Column {
                name: format!("C{index}"),
                kind,
            })
            .collect(),
    )
    .unwrap();
    let rows = vec![
        vec![
            Value::Str("ab".into()),
            Value::Int(1),
            float.clone(),
            Value::Missing,
            Value::DateTime(moment),
            Value::Missing,
            Value::Int(1),
        ],
        vec![
            Value::Missing,
            Value::Missing,
            Value::Missing,
            Value::Date(day),
            Value::Missing,
            Value::Missing,
            Value::Str("x".into()),
        ],
        vec![
            Value::Str(String::new()),
            Value::Int(-3),
            float,
            Value::Date(day),
            Value::DateTime(moment),
            Value::Missing,
            Value::Int(2),
        ],
    ];
    let dataset = crate::dataset::Dataset::from_rows(schema, rows);
    let lent: Vec<bool> = (0..kinds.len())
        .map(|column| dataset.column(column).is_some())
        .collect();
    assert_eq!(
        lent,
        [true, true, true, true, true, false, false],
        "the typed columns lend views; an all-missing and a mixed column do not"
    );
    for column in 0..kinds.len() {
        let Some(view) = dataset.column(column) else {
            continue;
        };
        let read: Vec<_> = (0..3).map(|row| view.get(row)).collect();
        let cells: Vec<_> = (0..3)
            .map(|row| dataset.cell(row, column).unwrap())
            .collect();
        assert_eq!((view.len(), read), (3, cells), "column C{column}");
    }
}

/// A pair relation's records are released once its port holds them when no
/// reader reads the relation again, as a lookup over it and a row template
/// driving on it read its port (LOOKUP-27), and kept when its `unique`
/// checks read them (#5107); the output and the handler counts are the same
/// either way, run whole or partitioned.
#[test]
fn a_pair_relation_no_later_reader_reads_is_released_once_its_port_fills() {
    // Seam: the run's relation state after it fills the pair ports, which no
    // output, diagnostic or handler count shows.
    use crate::dataset::{Dataset, Partitioning};
    use crate::function_invocation::NoFunctions;
    use crate::table::{Column, TableAccess, TableSchema};
    use crate::test_document::Tree;
    use crate::test_specification::{column, derivation, document, prepare};
    use crate::value::{ColumnType, Value};
    use std::num::NonZeroUsize;
    let schema = |columns: &[(&str, ColumnType)]| {
        TableSchema::new(
            columns
                .iter()
                .map(|&(name, kind)| Column {
                    name: name.into(),
                    kind,
                })
                .collect(),
        )
        .unwrap()
    };
    let text = |value: &str| Value::Str(value.into());
    let src = Dataset::from_rows(
        schema(&[("ID", ColumnType::Int), ("K", ColumnType::Str)]),
        vec![
            vec![Value::Int(1), text("a")],
            vec![Value::Int(2), text("b")],
        ],
    );
    let rec = Dataset::from_rows(
        schema(&[("K", ColumnType::Str), ("N", ColumnType::Int)]),
        vec![
            vec![text("a"), Value::Int(2)],
            vec![text("b"), Value::Int(3)],
        ],
    );
    // Each REC record once for each number from 1 to its count (LOOKUP-29).
    let pairs = |unique: bool| {
        let mut fields = vec![
            ("id", Tree::text("PAIRS")),
            ("dataset", Tree::text("REC")),
            (
                "repeat",
                Tree::map(vec![
                    ("count", Tree::text("REC.N")),
                    ("index", Tree::text("NUMBER")),
                ]),
            ),
            (
                "derivations",
                Tree::map(vec![
                    ("K", derivation(Tree::text("REC.K"))),
                    ("I", derivation(Tree::text("NUMBER"))),
                ]),
            ),
        ];
        if unique {
            let columns = Tree::list(vec![Tree::text("K"), Tree::text("I")]);
            let check = Tree::map(vec![("unique", Tree::map(vec![("columns", columns)]))]);
            fields.push(("verifications", Tree::list(vec![check])));
        }
        Tree::map(fields)
    };
    // The last pair of each row's key, read through the pair relation's port.
    let last = Tree::map(vec![
        ("id", Tree::text("LAST")),
        ("dataset", Tree::text("PAIRS")),
        ("key", Tree::list(vec![Tree::text("K")])),
        ("order_by", Tree::list(vec![Tree::text("PAIRS.I")])),
        ("keep", Tree::text("last")),
    ]);
    let input = Tree::map(vec![
        ("SRC", Tree::map(vec![("path", Tree::text("SRC.csv"))])),
        ("REC", Tree::map(vec![("path", Tree::text("REC.csv"))])),
    ]);
    let output = |columns: &[&str]| {
        Tree::map(vec![
            ("path", Tree::text("out.csv")),
            (
                "columns",
                Tree::list(columns.iter().map(Tree::text).collect()),
            ),
        ])
    };
    let looked_up = |unique: bool| {
        document(
            vec![
                column("K", "str", derivation(Tree::text("SRC.K"))),
                column("LASTI", "int", derivation(Tree::text("LAST.I"))),
            ],
            vec![
                ("input", input.clone()),
                ("base", Tree::text("SRC")),
                (
                    "intermediates",
                    Tree::list(vec![pairs(unique), last.clone()]),
                ),
                ("output", output(&["ID", "K", "LASTI"])),
            ],
        )
    };
    // One row per pair, driven on the pair relation itself (ROW-2).
    let driven = document(
        Vec::new(),
        vec![
            ("input", input.clone()),
            ("keys", Tree::list(vec![Tree::text("K"), Tree::text("I")])),
            ("intermediates", Tree::list(vec![pairs(false)])),
            (
                "columns",
                Tree::list(vec![
                    column("K", "str", derivation(Tree::text("PAIRS.K"))),
                    column("I", "int", derivation(Tree::text("PAIRS.I"))),
                ]),
            ),
            (
                "rows",
                Tree::list(vec![Tree::map(vec![
                    ("id", Tree::text("pairs")),
                    ("dataset", Tree::text("PAIRS")),
                ])]),
            ),
            ("output", output(&["K", "I"])),
        ],
    );
    let id_k_i = |id: i64, k: &str, i: i64| vec![Value::Int(id), text(k), Value::Int(i)];
    let k_i = |k: &str, i: i64| vec![text(k), Value::Int(i)];
    let cases = [
        (
            "read through its port alone",
            looked_up(false),
            false,
            vec![id_k_i(1, "a", 2), id_k_i(2, "b", 3)],
        ),
        (
            "verified by its unique check",
            looked_up(true),
            true,
            vec![id_k_i(1, "a", 2), id_k_i(2, "b", 3)],
        ),
        (
            "driving a row template, which reads its port",
            driven,
            false,
            vec![
                k_i("a", 1),
                k_i("a", 2),
                k_i("b", 1),
                k_i("b", 2),
                k_i("b", 3),
            ],
        ),
    ];
    for (label, tree, kept, rows) in cases {
        let prepared =
            prepare(&tree.document()).unwrap_or_else(|error| panic!("{label}: {error:?}"));
        // Binding reads the inputs' schemas in declaration order; the
        // driver input is the run's primary, the other its one secondary.
        let (primary, other) = match prepared.source().name.as_str() {
            "SRC" => (&src, &rec),
            _ => (&rec, &src),
        };
        let plan = prepared
            .bind_sources(&[src.schema(), rec.schema()])
            .unwrap_or_else(|error| panic!("{label}: {error:?}"));
        let relation = plan.named(super::RegionId::DATASET, 0).unwrap();
        let secondary: [&dyn TableAccess<Error = Infallible>; 1] = [other];
        let dates = super::TextDates::default();
        let mut functions = NoFunctions::new();
        let mut run = super::Run::new(
            &plan,
            (primary, &secondary),
            &mut functions,
            &|| false,
            &dates,
        );
        run.admit().unwrap();
        let frame = super::frame::Frame::new(0);
        let inputs = run.inputs();
        let ports = run.pair_ports(&frame, &inputs).unwrap();
        let held = run.relations.materialized(relation, None).is_some();
        assert_eq!(
            (plan.relation(relation).reread, held, ports[0].0.rows()),
            (kept, kept, 5),
            "{label}: the plan's decision, the records the run holds and the port's records"
        );
        let whole = plan.execute(primary, &secondary, &mut NoFunctions::new(), None, &|| {
            false
        });
        let partitioning = Partitioning {
            rows: NonZeroUsize::MIN,
            leading_keys: None,
            whole_table_fallback: true,
            threads: NonZeroUsize::MIN,
        };
        let divided = plan.execute_divided(
            Some(partitioning),
            primary,
            &secondary,
            &mut NoFunctions::new(),
            None,
            &|| false,
        );
        for (mode, attempt) in [("whole", &whole), ("partitioned", &divided)] {
            let result = attempt
                .result
                .as_ref()
                .map(|execution| execution.dataset.rows());
            assert_eq!(
                result.as_ref().ok(),
                Some(&rows),
                "{label}, {mode}: {:?}",
                result.as_ref().err()
            );
        }
        assert_eq!(whole.handler_counts, divided.handler_counts, "{label}");
    }
}
