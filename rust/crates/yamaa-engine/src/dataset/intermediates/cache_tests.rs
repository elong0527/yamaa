//! Exact cumulative cache boundaries through the real named-selection service.
use super::*;
use alloc::rc::Rc;
use core::{
    cell::RefCell,
    mem::{align_of, size_of},
};
use yamaa_core::{table::Column, value::ColumnType};

type Trace = Rc<RefCell<Vec<&'static str>>>;
type Original = core::convert::Infallible;
struct Table {
    schema: TableSchema,
    values: Vec<Value>,
    names: Vec<&'static str>,
    trace: Trace,
}
impl TableAccess for Table {
    type Error = Original;
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    fn row_count(&self) -> usize {
        1
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Original>> {
        assert_eq!(row, 0);
        self.trace.borrow_mut().push(self.names[column]);
        Ok(ValueRef::from(&self.values[column]))
    }
}
fn schema(fields: &[(&str, ColumnType)]) -> TableSchema {
    TableSchema::new(
        fields
            .iter()
            .map(|(name, kind)| Column {
                name: (*name).into(),
                kind: *kind,
            })
            .collect(),
    )
    .unwrap()
}
struct Fixture {
    primary: Table,
    donor: Table,
    plan: DatasetPlan,
    trace: Trace,
}
impl Fixture {
    fn new() -> Self {
        let trace = Trace::default();
        let primary = Table {
            schema: schema(&[("K", ColumnType::Str)]),
            values: vec![Value::Str("x".into())],
            names: vec!["BASE.K"],
            trace: trace.clone(),
        };
        let donor = Table {
            schema: schema(&[("K", ColumnType::Str), ("V", ColumnType::Int)]),
            values: vec![Value::Str("x".into()), Value::Int(7)],
            names: vec!["DONOR.K", "DONOR.V"],
            trace: trace.clone(),
        };
        let plan = DatasetPlan::new_with_intermediates(
            SourceSchemas {
                primary: primary.schema.clone(),
                secondary: vec![SecondarySource {
                    name: "DONOR".into(),
                    schema: donor.schema.clone(),
                }],
            },
            vec![Intermediate {
                identifier: "MATCH".into(),
                path: "intermediates[0]".into(),
                source: IntermediateSource::Input(0),
                keys: vec![IntermediateKey {
                    source_column: 0,
                    path: "intermediates[0].key".into(),
                    value: MatchValue::Variable(MatchRead::Record {
                        column: 0,
                        identifier: "BASE.K".into(),
                    }),
                }],
                filter: None,
                selection: None,
                no_match: None,
                unique: vec![],
                derivations: None,
                pending_schema: None,
                range: None,
                columns: None,
            }],
            schema(&[("ID", ColumnType::Int), ("VALUE", ColumnType::Int)]),
            vec![RowTemplate {
                mode: RowMode::Records,
                filter: None,
                assignments: vec![
                    Assignment {
                        column: 0,
                        path: "columns.ID".into(),
                        expression: Expression::Literal(Value::Int(1)),
                    },
                    Assignment {
                        column: 1,
                        path: "columns.VALUE".into(),
                        expression: Expression::Intermediate {
                            index: 0,
                            column: 1,
                        },
                    },
                ],
            }],
            vec![],
            vec![0],
            vec![],
        )
        .unwrap();
        Self {
            primary,
            donor,
            plan,
            trace,
        }
    }
    fn read(
        &self,
        run: &mut Run,
        budget: &mut Budget,
    ) -> Result<Value, Box<ExecutionError<Original>>> {
        let candidate = Candidate {
            template: Some(0),
            selection_identity: SelectionIdentity::Candidate(0),
            members: vec![0],
            values: vec![Value::Int(1), Value::Missing],
            completed: vec![true, false],
        };
        read(
            0,
            1,
            &self.plan.templates()[0].assignments[1],
            &candidate,
            &self.plan,
            0,
            &self.primary,
            &mut EvaluationState {
                functions: &mut crate::function_invocation::NoFunctions::new(),
                budget,
                handlers: &mut HandlerCounter::default(),
                secondary: &[&self.donor],
                primary: &self.primary,
                self_rows: &[],
                intermediates: run,
                output_rows: &[],
                aggregate_row: None,
                output_offset: 0,
                windows: &mut windows::Cache::default(),
            },
        )
    }
}
fn budget(work: usize) -> Budget {
    Budget::new(Limits {
        source_rows: 100,
        output_rows: 100,
        output_cells: 100,
        key_cells: 100,
        work_cells: work,
        scalar_text_bytes: 100,
        output_text_bytes: 100,
        identity_cells: 100,
        identity_text_bytes: 100,
    })
}
fn refusal(error: ExecutionError<Original>, limit: usize, required: usize) {
    assert!(matches!(error, ExecutionError::Limit {
        resource: Resource::WorkCells, limit: actual, required: Some(needed),
    } if (actual, needed) == (limit, required)));
}

#[test]
fn insertion_admits_one_fixed_entry_at_equality_and_never_reads_a_value_after_refusal() {
    // Two 36-unit scalar search bounds, nine ordinary relation visits, three
    // new typed fields, and one subsequent selected-value visit total 85.
    for limit in [83, 84, 85] {
        let fixture = Fixture::new();
        let mut run = Run::default();
        let result = fixture.read(&mut run, &mut budget(limit));
        if limit == 85 {
            assert_eq!(result.unwrap(), Value::Int(7));
            assert_eq!(*fixture.trace.borrow(), ["BASE.K", "DONOR.K", "DONOR.V"]);
        } else {
            refusal(*result.unwrap_err(), limit, limit + 1);
            assert_eq!(*fixture.trace.borrow(), ["BASE.K", "DONOR.K"]);
        }
        assert_eq!(run.choices.len(), usize::from(limit >= 84));
    }
}

#[test]
fn cache_hit_equality_is_cumulative_and_refusal_enters_no_donor_or_current_read() {
    for limit in [120, 121, 122] {
        let fixture = Fixture::new();
        let mut run = Run::default();
        let mut budget = budget(limit);
        assert_eq!(fixture.read(&mut run, &mut budget).unwrap(), Value::Int(7));
        fixture.trace.borrow_mut().clear();
        let result = fixture.read(&mut run, &mut budget);
        if limit == 122 {
            assert_eq!(result.unwrap(), Value::Int(7));
            assert_eq!(*fixture.trace.borrow(), ["DONOR.V"]);
        } else {
            refusal(*result.unwrap_err(), limit, limit + 1);
            assert!(fixture.trace.borrow().is_empty());
        }
        assert_eq!(run.choices.len(), 1);
    }
}

#[test]
fn one_choice_contains_only_bounded_numeric_identity_and_coordinate_fields() {
    let word = size_of::<usize>();
    type Entry = ((SelectionIdentity, usize), Choice);
    assert_eq!(size_of::<Entry>(), 5 * word);
    assert_eq!(size_of::<(String, Vec<usize>)>(), 6 * word);
    assert_eq!(
        size_of::<BTreeMap<(SelectionIdentity, usize), Choice>>(),
        3 * word
    );
    assert_eq!(size_of::<Run>(), 13 * word);
    assert_eq!(size_of::<key_index::StringIndex>(), 3 * word);
    assert!(align_of::<Entry>() <= word);
    assert!(key_index::search_work(usize::MAX) <= 12 * (1 + usize::BITS as usize));
    let (fixture, mut run) = (Fixture::new(), Run::default());
    assert_eq!(
        fixture.read(&mut run, &mut budget(85)).unwrap(),
        Value::Int(7)
    );
    assert!(matches!(
        run.choices[&(SelectionIdentity::Candidate(0), 0)],
        Choice::Record {
            row: 0,
            multiple: false
        }
    ));
}

#[test]
fn retained_index_strings_and_coordinate_capacity_obey_the_admitted_source_bound() {
    struct Keys {
        schema: TableSchema,
        values: Vec<Value>,
    }
    impl TableAccess for Keys {
        type Error = Original;
        fn schema(&self) -> &TableSchema {
            &self.schema
        }
        fn row_count(&self) -> usize {
            self.values.len()
        }
        fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Original>> {
            assert_eq!(column, 0);
            Ok(ValueRef::from(&self.values[row]))
        }
    }
    let keys = Keys {
        schema: schema(&[("K", ColumnType::Str)]),
        values: (0..256)
            .map(|row| Value::Str(alloc::format!("K{:03}", row % 64)))
            .collect(),
    };
    let fixture = Fixture::new();
    let eligible = (0..256).collect::<Vec<_>>();
    let mut limits = budget(100_000).limits();
    limits.scalar_text_bytes = 10_000;
    let index = key_index::StringIndex::build(
        &keys,
        &fixture.plan.intermediates()[0],
        &eligible,
        0,
        &mut Budget::new(limits),
    )
    .unwrap();
    assert_eq!(index.0.len(), 64);
    assert_eq!(index.0["K015"], [15, 79, 143, 207]);
    let text_bytes = index.0.keys().map(String::capacity).sum::<usize>();
    let coordinate_cells = index.0.values().map(Vec::capacity).sum::<usize>();
    assert_eq!(text_bytes, 256);
    assert_eq!(coordinate_cells, 256);
    assert!(text_bytes <= 256 * 4);
    assert!(coordinate_cells <= 2 * eligible.len());
}
