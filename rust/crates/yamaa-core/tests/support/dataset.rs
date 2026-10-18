//! Independent ordering, admission, failure and committed benchmark observations.
// Reexports share the authored fixture types across automatically discovered feature tests.
#![allow(unused_imports)]
pub(crate) use crate::support::table_schema as schema;
pub(crate) use std::cell::RefCell;
pub(crate) use yamaa_core::{
    reduction::NumericReducer,
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    temporal::{Date, Precision},
    value::{ColumnType, Value},
};

pub(crate) use yamaa_core::{
    dataset::{
        Assignment, Check, DatasetPlan, ExecutionError, Expression, PlanError, RowMode,
        RowTemplate, Verification,
    },
    table_grouping::{partition, GroupingError},
};

pub(crate) struct Table {
    pub(crate) schema: TableSchema,
    pub(crate) rows: Vec<Vec<Value>>,
    pub(crate) reads: RefCell<Vec<(usize, usize)>>,
    pub(crate) fail: Option<(usize, usize)>,
}
impl TableAccess for Table {
    type Error = &'static str;
    /// Expose the immutable test snapshot schema.
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    /// Expose source cardinality without a cell read.
    fn row_count(&self) -> usize {
        self.rows.len()
    }
    /// Observe reads and inject opaque access failures without altering values.
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Self::Error>> {
        self.reads.borrow_mut().push((row, column));
        if self.fail == Some((row, column)) {
            return Err(CellError::Access("source failure"));
        }
        self.rows
            .get(row)
            .and_then(|values| values.get(column))
            .map(ValueRef::from)
            .ok_or(CellError::OutOfBounds { row, column })
    }
}
/// Construct an ordered schema independently of plan compilation.
/// Build an observable normalized source port.
pub(crate) fn table(columns: &[(&str, ColumnType)], rows: Vec<Vec<Value>>) -> Table {
    Table {
        schema: schema(columns),
        rows,
        reads: RefCell::default(),
        fail: None,
    }
}
/// Give a bound assignment deterministic test provenance.
pub(crate) fn assign(column: usize, expression: Expression) -> Assignment {
    Assignment {
        column,
        expression,
        path: format!("columns.C{column}.derivation"),
    }
}
/// Attach deterministic declaration provenance to one check.
pub(crate) fn verification(check: Check) -> Verification {
    Verification {
        path: "verifications[0]".into(),
        check,
    }
}
/// Admit a record-driven plan with one output identity column.
pub(crate) fn record_plan(
    source: &Table,
    output: TableSchema,
    columns: Vec<Assignment>,
    mut checks: Vec<Verification>,
) -> DatasetPlan {
    for (index, check) in checks.iter_mut().enumerate() {
        check.path = format!("verifications[{index}]");
    }
    DatasetPlan::new(
        source.schema.clone(),
        output,
        vec![RowTemplate {
            filter: None,
            mode: RowMode::Records,
            assignments: vec![],
        }],
        columns,
        vec![0],
        checks,
    )
    .unwrap()
}

/// Compile authored arithmetic with a closed static name-to-read map.
pub(crate) fn computed(text: &str, bindings: &[(&str, Read)]) -> Expression {
    pub(crate) use yamaa_core::{bound_expression::Binding, dataset::BoundNumeric};
    Expression::Compute(
        BoundNumeric::new(
            yamaa_core::expression::compile(
                text,
                yamaa_core::expression::Context::Compute,
                "columns.N.derivation.compute",
            )
            .unwrap(),
            bindings
                .iter()
                .map(|(name, read)| Binding {
                    name: (*name).into(),
                    read: read.clone(),
                })
                .collect(),
        )
        .unwrap(),
    )
}

/// This reader accepts only the deliberately unquoted committed CSV fixture.
/// It is test setup, not a production ingestion or specification compiler path.
pub(crate) fn fixture(text: &str, float_column: usize) -> Table {
    assert!(!text.contains('"'));
    let mut lines = text.lines();
    let names: Vec<_> = lines.next().unwrap().split(',').collect();
    let columns: Vec<_> = names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            (
                *name,
                if index == float_column {
                    ColumnType::Float
                } else {
                    ColumnType::Str
                },
            )
        })
        .collect();
    let rows = lines
        .map(|line| {
            let fields: Vec<_> = line.split(',').collect();
            assert_eq!(fields.len(), names.len());
            fields
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    if value.is_empty() {
                        Value::Missing
                    } else if index == float_column {
                        Value::float(value.parse().unwrap())
                    } else {
                        Value::Str((*value).into())
                    }
                })
                .collect()
        })
        .collect();
    table(&columns, rows)
}

pub(crate) use yamaa_core::bound_expression::{
    Binding, BindingError, BoundPredicate as Filter, Read,
};
pub(crate) use yamaa_core::expression::{self, Comparison, CompiledExpression};

/// One authored predicate operand, written into the predicate's text.
#[derive(Clone, Debug)]
pub(crate) enum Scalar {
    Literal(Value),
    Identifier(String),
}

/// An authored predicate node; Boolean children index earlier entries.
#[derive(Clone, Debug)]
pub(crate) enum Node {
    Contains {
        value: Scalar,
        pattern: String,
    },
    Boolean(bool),
    Not(usize),
    And(usize, usize),
    Or(usize, usize),
    Compare {
        operator: Comparison,
        left: Scalar,
        right: Scalar,
    },
    IsNull {
        value: Scalar,
        negated: bool,
    },
    In {
        value: Scalar,
        items: Vec<Scalar>,
        negated: bool,
    },
    Between {
        value: Scalar,
        lower: Scalar,
        upper: Scalar,
        negated: bool,
    },
    Like {
        value: Scalar,
        pattern: Scalar,
        escape: Option<char>,
        negated: bool,
    },
}

fn quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

fn scalar_text(scalar: &Scalar) -> String {
    match scalar {
        Scalar::Identifier(name) => name.clone(),
        Scalar::Literal(Value::Missing) => "NULL".into(),
        Scalar::Literal(Value::Int(value)) => value.to_string(),
        Scalar::Literal(Value::Float(value)) => format!("{:?}", value.get()),
        Scalar::Literal(Value::Str(value)) => quoted(value),
        Scalar::Literal(Value::Date(value)) => format!("DATE {}", quoted(&value.to_string())),
        Scalar::Literal(Value::DateTime(value)) => {
            format!("DATETIME {}", quoted(&value.to_string()))
        }
        Scalar::Literal(Value::Bool(_)) => panic!("a truth value is not an operand"),
    }
}

/// Write an authored arena as the predicate text it stands for.
pub(crate) fn predicate_text(nodes: &[Node], root: usize) -> String {
    let negation = |negated: &bool| if *negated { "NOT " } else { "" };
    match &nodes[root] {
        Node::Contains { value, pattern } => {
            format!("str_contains({}, {})", scalar_text(value), quoted(pattern))
        }
        Node::Boolean(value) => if *value { "TRUE" } else { "FALSE" }.into(),
        Node::Not(child) => format!("NOT ({})", predicate_text(nodes, *child)),
        Node::And(left, right) => format!(
            "({}) AND ({})",
            predicate_text(nodes, *left),
            predicate_text(nodes, *right)
        ),
        Node::Or(left, right) => format!(
            "({}) OR ({})",
            predicate_text(nodes, *left),
            predicate_text(nodes, *right)
        ),
        Node::Compare {
            operator,
            left,
            right,
        } => format!(
            "{} {} {}",
            scalar_text(left),
            match operator {
                Comparison::Equal => "=",
                Comparison::NotEqual => "<>",
                Comparison::Less => "<",
                Comparison::LessEqual => "<=",
                Comparison::Greater => ">",
                Comparison::GreaterEqual => ">=",
            },
            scalar_text(right)
        ),
        Node::IsNull { value, negated } => {
            format!("{} IS {}NULL", scalar_text(value), negation(negated))
        }
        Node::In {
            value,
            items,
            negated,
        } => format!(
            "{} {}IN ({})",
            scalar_text(value),
            negation(negated),
            items.iter().map(scalar_text).collect::<Vec<_>>().join(", ")
        ),
        Node::Between {
            value,
            lower,
            upper,
            negated,
        } => format!(
            "{} {}BETWEEN {} AND {}",
            scalar_text(value),
            negation(negated),
            scalar_text(lower),
            scalar_text(upper)
        ),
        Node::Like {
            value,
            pattern,
            escape,
            negated,
        } => format!(
            "{} {}LIKE {}{}",
            scalar_text(value),
            negation(negated),
            scalar_text(pattern),
            escape
                .map(|escape| format!(" ESCAPE {}", quoted(&escape.to_string())))
                .unwrap_or_default()
        ),
    }
}

/// Compile an authored arena through the one predicate language.
pub(crate) fn predicate_plan(nodes: Vec<Node>, root: usize, path: &str) -> CompiledExpression {
    yamaa_core::expression::compile(
        &predicate_text(&nodes, root),
        yamaa_core::expression::Context::Predicate,
        path,
    )
    .unwrap()
}

/// Admit a single authored predicate and its explicit scope bindings.
pub(crate) fn filter(node: Node, bindings: Vec<Binding>) -> Filter {
    Filter::new(predicate_plan(vec![node], 0, "rows[0].filter"), bindings).unwrap()
}
/// Bind an occurrence without guessing whether it belongs to source or candidate scope.
pub(crate) fn binding(name: &str, read: Read) -> Binding {
    Binding {
        name: name.into(),
        read,
    }
}
/// Keep rows whose completed value exceeds zero; missing values yield unknown.
pub(crate) fn positive(read: Read) -> Filter {
    filter(
        Node::Compare {
            operator: Comparison::Greater,
            left: Scalar::Identifier("x".into()),
            right: Scalar::Literal(Value::Int(0)),
        },
        vec![binding("x", read)],
    )
}

/// Supply a verification predicate with its actual owning declaration path.
pub(crate) fn check_predicate(node: Node, bindings: Vec<Binding>, path: &str) -> Filter {
    Filter::new(predicate_plan(vec![node], 0, path), bindings).unwrap()
}

/// Bind no-template keys separately from later whole-column derivations.
pub(crate) fn key_plan(
    source: &Table,
    output: TableSchema,
    keys: Vec<usize>,
    key_assignments: Vec<Assignment>,
    columns: Vec<Assignment>,
) -> DatasetPlan {
    DatasetPlan::new(
        source.schema.clone(),
        output,
        vec![RowTemplate {
            mode: RowMode::Keys,
            assignments: key_assignments,
            filter: None,
        }],
        columns,
        keys,
        vec![],
    )
    .unwrap()
}
/// Read one distinct present raw source value across the complete feeding group.
pub(crate) fn collect(column: usize) -> Expression {
    Expression::Collect {
        column,
        identifier: format!("SRC.C{column}"),
        filter: None,
        selection: None,
    }
}

/// Admit three unfiltered numbering operations over the same completed output relation.
pub(crate) fn numbering_plan(source: &Table, descending: bool, nulls_first: bool) -> DatasetPlan {
    numbering_with_filter(source, descending, nulls_first, None, ColumnType::Int)
}
/// Admit optional output-column eligibility and ordinary completed-result conversion.
pub(crate) fn numbering_with_filter(
    source: &Table,
    descending: bool,
    nulls_first: bool,
    predicate: Option<Filter>,
    output_kind: ColumnType,
) -> DatasetPlan {
    pub(crate) use yamaa_core::dataset::{OrderTerm, Window, WindowKind};
    let mut fields = source.schema.columns().to_vec();
    for name in ["SEQ", "RANK", "DENSE"] {
        fields.push(Column {
            name: name.into(),
            kind: output_kind,
        });
    }
    let mut columns = vec![
        assign(
            1,
            Expression::Collect {
                column: 1,
                identifier: "SRC.G".into(),
                filter: None,
                selection: None,
            },
        ),
        assign(
            2,
            Expression::Collect {
                column: 2,
                identifier: "SRC.V".into(),
                filter: None,
                selection: None,
            },
        ),
    ];
    for (index, kind) in [
        WindowKind::RowNumber,
        WindowKind::Competition,
        WindowKind::Dense,
    ]
    .into_iter()
    .enumerate()
    {
        columns.push(assign(
            index + 3,
            Expression::Window(Window {
                filter: predicate.clone(),
                kind,
                group_by: vec![1],
                order_by: vec![OrderTerm {
                    column: 2,
                    descending,
                    nulls_first,
                }],
            }),
        ));
    }
    DatasetPlan::new(
        source.schema.clone(),
        TableSchema::new(fields).unwrap(),
        vec![RowTemplate {
            mode: RowMode::Keys,
            assignments: vec![assign(0, Expression::Source(0))],
            filter: None,
        }],
        columns,
        vec![0],
        vec![],
    )
    .unwrap()
}

/// Construct an eager predicate true on missing values but ill-typed on present integers.
pub(crate) fn missing_or_bad_window_predicate(and_false: bool) -> Filter {
    let nodes = vec![
        if and_false {
            Node::Boolean(false)
        } else {
            Node::IsNull {
                value: Scalar::Identifier("V".into()),
                negated: false,
            }
        },
        Node::Compare {
            operator: Comparison::Greater,
            left: Scalar::Identifier("V".into()),
            right: Scalar::Literal(Value::Str("x".into())),
        },
        if and_false {
            Node::And(0, 1)
        } else {
            Node::Or(0, 1)
        },
    ];
    Filter::new(
        predicate_plan(nodes, 2, "columns.SEQ.derivation.row_number"),
        vec![binding("V", Read::Column(2))],
    )
    .unwrap()
}

/// Build a value-window plan with a separate completed donor column and stable ordinal order.
pub(crate) fn value_window_plan(
    source: &Table,
    kind: yamaa_core::dataset::WindowKind,
    filter: Option<Filter>,
) -> Result<DatasetPlan, PlanError> {
    pub(crate) use yamaa_core::dataset::{OrderTerm, Window};
    let mut fields = source.schema.columns().to_vec();
    fields.push(Column {
        name: "RESULT".into(),
        kind: fields[2].kind,
    });
    DatasetPlan::new(
        source.schema.clone(),
        TableSchema::new(fields).unwrap(),
        vec![RowTemplate {
            mode: RowMode::Keys,
            assignments: vec![assign(0, Expression::Source(0))],
            filter: None,
        }],
        vec![
            assign(1, collect(1)),
            assign(2, collect(2)),
            assign(
                3,
                Expression::Window(Window {
                    kind,
                    group_by: vec![1],
                    order_by: vec![OrderTerm {
                        column: 0,
                        descending: false,
                        nulls_first: false,
                    }],
                    filter,
                }),
            ),
        ],
        vec![0],
        vec![],
    )
}

/// Admit temporal baseline selection over completed dates and partition keys.
/// Columns 2 and 3 are the date and reference; the key column is the value.
pub(crate) fn baseline_plan(
    source: &Table,
    groups: Vec<usize>,
    filter: Option<Filter>,
) -> DatasetPlan {
    baseline_options_plan(source, groups, filter, 0, true, vec![])
}

/// Admit a baseline over every source column with an explicit value, bound and
/// tie-break terms. Columns 2 and 3 are the date and reference, and the flag
/// is the last output column.
pub(crate) fn baseline_options_plan(
    source: &Table,
    groups: Vec<usize>,
    filter: Option<Filter>,
    value: usize,
    inclusive: bool,
    tie_break: Vec<yamaa_core::dataset::OrderTerm>,
) -> DatasetPlan {
    pub(crate) use yamaa_core::dataset::{Window, WindowKind};
    let mut fields = source.schema.columns().to_vec();
    let flag = fields.len();
    fields.push(Column {
        name: "BLFL".into(),
        kind: ColumnType::Str,
    });
    let mut assignments = (1..flag)
        .map(|column| assign(column, collect(column)))
        .collect::<Vec<_>>();
    assignments.push(assign(
        flag,
        Expression::Window(Window {
            kind: WindowKind::BaselineFlag {
                date: 2,
                reference_date: 3,
                value,
                inclusive,
            },
            group_by: groups,
            order_by: tie_break,
            filter,
        }),
    ));
    DatasetPlan::new(
        source.schema.clone(),
        TableSchema::new(fields).unwrap(),
        vec![RowTemplate {
            mode: RowMode::Keys,
            assignments: vec![assign(0, Expression::Source(0))],
            filter: None,
        }],
        assignments,
        vec![0],
        vec![],
    )
    .unwrap()
}

/// Bind a source-only root predicate before standalone key construction.
pub(crate) fn root_filtered_plan(
    source: &Table,
    predicate: Filter,
) -> Result<DatasetPlan, PlanError> {
    DatasetPlan::new(
        source.schema.clone(),
        schema(&[("ID", ColumnType::Int), ("V", ColumnType::Str)]),
        vec![RowTemplate {
            mode: RowMode::Keys,
            assignments: vec![assign(0, Expression::Source(0))],
            filter: Some(predicate),
        }],
        vec![assign(1, collect(1))],
        vec![0],
        vec![],
    )
}

/// Admit a source-only filter inside one collected non-key reading.
pub(crate) fn selected_source_plan(
    source: &Table,
    predicate: Filter,
) -> Result<DatasetPlan, PlanError> {
    DatasetPlan::new(
        source.schema.clone(),
        schema(&[("ID", ColumnType::Int), ("V", ColumnType::Str)]),
        vec![RowTemplate {
            mode: RowMode::Keys,
            assignments: vec![assign(0, Expression::Source(0))],
            filter: None,
        }],
        vec![assign(
            1,
            Expression::Collect {
                column: 1,
                identifier: "SRC.V".into(),
                filter: Some(predicate),
                selection: None,
            },
        )],
        vec![0],
        vec![],
    )
}

/// Bind one source choice without changing output identity or conversion ownership.
pub(crate) fn ordered_source_plan(
    source: &Table,
    keep: yamaa_core::dataset::Keep,
    descending: bool,
    nulls_first: bool,
    later_failure: bool,
) -> DatasetPlan {
    pub(crate) use yamaa_core::dataset::{OrderTerm, SourceSelection};
    let mut columns = vec![assign(
        1,
        Expression::Collect {
            column: 1,
            identifier: "SRC.V".into(),
            filter: None,
            selection: Some(SourceSelection {
                order_by: vec![OrderTerm {
                    column: 2,
                    descending,
                    nulls_first,
                }],
                keep,
            }),
        },
    )];
    let mut fields = vec![("ID", ColumnType::Int), ("V", ColumnType::Int)];
    if later_failure {
        fields.push(("LATER", ColumnType::Int));
        columns.push(assign(2, Expression::Literal(Value::Str("bad".into()))));
    }
    key_plan(
        source,
        schema(&fields),
        vec![0],
        vec![assign(0, Expression::Source(0))],
        columns,
    )
}

/// Bind an implicit many-to-one read against a separately owned immutable relation.
pub(crate) fn secondary_lookup_plan(
    source: &Table,
    secondary: &Table,
) -> Result<DatasetPlan, PlanError> {
    pub(crate) use yamaa_core::dataset::{Lookup, MatchKey, SecondarySource};
    DatasetPlan::new_with_sources(
        source.schema.clone(),
        vec![SecondarySource {
            name: "OTHER".into(),
            schema: secondary.schema.clone(),
        }],
        schema(&[("ID", ColumnType::Int), ("V", ColumnType::Int)]),
        vec![RowTemplate {
            mode: RowMode::Keys,
            assignments: vec![assign(0, Expression::Source(0))],
            filter: None,
        }],
        vec![assign(
            1,
            Expression::Lookup(Lookup {
                source: 0,
                column: 1,
                keys: vec![MatchKey {
                    source_column: 0,
                    output_column: 0,
                }],
            }),
        )],
        vec![0],
        vec![],
    )
}

/// Reuse one named record selection for two independently converted readings.
pub(crate) fn named_plan(
    source: &Table,
    secondary: &Table,
    item: yamaa_core::dataset::Intermediate,
) -> Result<DatasetPlan, PlanError> {
    pub(crate) use yamaa_core::dataset::{SecondarySource, SourceSchemas};
    DatasetPlan::new_with_intermediates(
        SourceSchemas {
            primary: source.schema.clone(),
            secondary: vec![SecondarySource {
                name: "OTHER".into(),
                schema: secondary.schema.clone(),
            }],
        },
        vec![item],
        schema(&[
            ("ID", ColumnType::Int),
            ("V", ColumnType::Int),
            ("W", ColumnType::Str),
        ]),
        vec![RowTemplate {
            mode: RowMode::Keys,
            assignments: vec![assign(0, Expression::Source(0))],
            filter: None,
        }],
        vec![
            assign(
                1,
                Expression::Intermediate {
                    index: 0,
                    column: 1,
                },
            ),
            assign(
                2,
                Expression::Intermediate {
                    index: 0,
                    column: 1,
                },
            ),
        ],
        vec![0],
        vec![],
    )
}

/// A declaration independent of source payloads, including explicit missing absence handling.
pub(crate) fn named_item() -> yamaa_core::dataset::Intermediate {
    pub(crate) use yamaa_core::dataset::{
        Intermediate, IntermediateKey, Keep, MatchRead, MatchValue, OrderTerm, SourceSelection,
    };
    Intermediate {
        identifier: "SELECTED".into(),
        path: "intermediates[0]".into(),
        source: yamaa_core::dataset::IntermediateSource::Input(0),
        keys: vec![IntermediateKey {
            source_column: 0,
            path: "intermediates[0].key".into(),
            value: MatchValue::Variable(MatchRead::Output {
                column: 0,
                identifier: "ID".into(),
            }),
        }],
        filter: None,
        selection: Some(SourceSelection {
            order_by: vec![OrderTerm {
                column: 2,
                descending: false,
                nulls_first: false,
            }],
            keep: Keep::Last,
        }),
        no_match: Some(Value::Missing),
        require_match: false,
        unique: Vec::new(),
        derivations: None,
        pending_schema: None,

        range: None,
        columns: None,
    }
}

/// Bind a replacement independently of assignment execution order.
pub(crate) fn conversion_handler(
    column: usize,
    value: Value,
) -> yamaa_core::dataset::ConversionHandler {
    yamaa_core::dataset::ConversionHandler {
        assignment_path: format!("columns.C{column}.derivation"),
        handler: yamaa_core::numeric_lifecycle::LiteralHandler {
            spec_path: format!("columns.C{column}.derivation.unconvertible"),
            value,
        },
    }
}
pub(crate) mod dataset_functions {

    pub(crate) use super::*;
    pub(crate) use yamaa_core::value::ValueType;
    pub(crate) use yamaa_core::{
        dataset::{BoundFunction, FunctionArgument, FunctionInput},
        function_invocation::{Argument, FailureKind, FunctionPort, HostError},
        function_signature::{FunctionIdentity, InvocationPlan, Parameter, Presence},
    };

    /// Fixed activated identity and independently declared host-argument ordering.
    pub(crate) fn signature(returns: ColumnType) -> InvocationPlan {
        InvocationPlan::new(
            FunctionIdentity {
                name: "sum".into(),
                contract_version: "1".into(),
                implementation_version: "2".into(),
                call: "project.sum".into(),
            },
            vec![
                Parameter {
                    name: "first".into(),
                    host_name: "lhs".into(),
                    kind: ValueType::Int,
                    accepts_missing: false,
                    presence: Presence::Required,
                },
                Parameter {
                    name: "second".into(),
                    host_name: "rhs".into(),
                    kind: ValueType::Int,
                    accepts_missing: false,
                    presence: Presence::Required,
                },
                Parameter {
                    name: "factor".into(),
                    host_name: "scale".into(),
                    kind: ValueType::Int,
                    accepts_missing: false,
                    presence: Presence::Optional(Value::Int(5)),
                },
            ],
            returns,
            false,
        )
        .unwrap()
    }

    pub(crate) struct Callbacks {
        pub(crate) signature: InvocationPlan,
        pub(crate) calls: Vec<Vec<(String, String)>>,
        pub(crate) result: Result<Value, &'static str>,
    }
    impl FunctionPort for Callbacks {
        type Error = &'static str;
        /// Slot lookup borrows metadata and never invokes the callable.
        fn signature(&self, slot: usize) -> Option<&InvocationPlan> {
            (slot == 0).then_some(&self.signature)
        }
        /// Capture the complete host call while preserving an opaque failure payload.
        fn call(
            &mut self,
            slot: usize,
            arguments: &[Argument<'_>],
        ) -> Result<Value, HostError<Self::Error>> {
            assert_eq!(slot, 0);
            self.calls.push(
                arguments
                    .iter()
                    .map(|a| (a.name.into(), format!("{:?}", a.value)))
                    .collect(),
            );
            self.result.clone().map_err(HostError::Raised)
        }
    }
    /// Construct an explicitly bound callback without activation or discovery effects.
    pub(crate) fn callbacks() -> Callbacks {
        Callbacks {
            signature: signature(ColumnType::Int),
            calls: vec![],
            result: Ok(Value::Int(11)),
        }
    }
    /// The authored order is deliberately the reverse of parameter declaration order.
    pub(crate) fn arguments() -> Vec<FunctionArgument> {
        vec![
            FunctionArgument {
                name: "second".into(),
                input: FunctionInput::Read(Read::Source(2)),
            },
            FunctionArgument {
                name: "first".into(),
                input: FunctionInput::Read(Read::Source(1)),
            },
        ]
    }
    /// Bind a call at one of the two assignment phases without changing its signature.
    pub(crate) fn plan(
        source: &Table,
        mode: RowMode,
        row_call: bool,
        predicate: Option<Filter>,
        signature: InvocationPlan,
        args: Vec<FunctionArgument>,
    ) -> Result<DatasetPlan, PlanError> {
        let call = assign(
            1,
            Expression::Function(BoundFunction::new(0, signature, args)?),
        );
        let mut row = vec![assign(0, Expression::Source(0))];
        let mut columns = vec![];
        if row_call {
            row.push(call);
        } else {
            columns.push(call);
        }
        DatasetPlan::new(
            source.schema.clone(),
            schema(&[("ID", ColumnType::Int), ("V", ColumnType::Int)]),
            vec![RowTemplate {
                mode,
                assignments: row,
                filter: predicate,
            }],
            columns,
            vec![0],
            vec![],
        )
    }
    /// Three independent integer source fields with deterministic row membership.
    pub(crate) fn source(rows: Vec<Vec<Value>>) -> Table {
        table(
            &[
                ("ID", ColumnType::Int),
                ("A", ColumnType::Int),
                ("B", ColumnType::Int),
            ],
            rows,
        )
    }
    /// A key-grain call explicitly collects each source leaf in authored order.
    pub(crate) fn collected_arguments() -> Vec<FunctionArgument> {
        [("second", 2, "SOURCE.B"), ("first", 1, "SOURCE.A")]
            .into_iter()
            .map(|(name, column, identifier)| FunctionArgument {
                name: name.into(),
                input: FunctionInput::Collect {
                    column,
                    identifier: identifier.into(),
                },
            })
            .collect()
    }
}
