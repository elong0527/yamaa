//! Resolution of hand-built plans: slots, schedule, registration and the
//! three checks, including their refusals.
use super::*;
use crate::bound_expression::{Binding as Bound, BoundPredicate, Read as BoundRead};
use crate::conversion::LiteralHandler;
use crate::dataset::{
    Assignment, CaseBranch, Check, ColumnVerifications, ConversionHandler, Expression,
    FirstAvailable, Intermediate, IntermediateKey, IntermediateSource, MatchRead, MatchValue,
    OrderTerm, RowMode, RowTemplate, SecondarySource, SelectionRead, SelectionSource,
    SourceSchemas, Verification, Window, WindowKind,
};
use crate::table::{CellError, Column, TableAccess, ValueRef};
use crate::text_case::{Case, StringCase};

fn schema(columns: &[(&str, ColumnType)]) -> TableSchema {
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
}
fn ints(names: &[&str]) -> TableSchema {
    schema(
        &names
            .iter()
            .map(|&name| (name, ColumnType::Int))
            .collect::<Vec<_>>(),
    )
}
fn assign(column: usize, expression: Expression) -> Assignment {
    Assignment {
        column,
        expression,
        path: format!("columns.C{column}.derivation"),
    }
}
fn predicate(text: &str, path: &str, bindings: Vec<Bound>) -> BoundPredicate {
    BoundPredicate::new(
        crate::expression::compile(text, crate::expression::Context::Predicate, path).unwrap(),
        bindings,
    )
    .unwrap()
}
fn bound(name: &str, read: BoundRead) -> Bound {
    Bound {
        name: name.into(),
        read,
    }
}
fn verification(path: &str, check: Check) -> Verification {
    Verification {
        path: path.into(),
        check,
    }
}
fn steps(plan: &ResolvedPlan, region: RegionId) -> Vec<Step> {
    plan.region_schedule(region)
        .iter()
        .map(|info| info.step)
        .collect()
}
fn position(plan: &ResolvedPlan, step: Step) -> usize {
    plan.schedule
        .iter()
        .position(|info| info.step == step)
        .unwrap()
}

/// One record template, a column-phase window, a column check and a dataset check.
fn records_plan() -> DatasetPlan {
    DatasetPlan::new(
        ints(&["ID", "V"]),
        ints(&["ID", "V", "RN"]),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![
                assign(0, Expression::Source(0)),
                assign(1, Expression::Source(1)),
            ],
            filter: None,
        }],
        vec![assign(
            2,
            Expression::Window(Window {
                kind: WindowKind::RowNumber,
                group_by: vec![],
                order_by: vec![OrderTerm {
                    column: 1,
                    descending: false,
                    nulls_first: false,
                }],
                filter: None,
            }),
        )],
        vec![0],
        vec![verification(
            "verifications[0]",
            Check::RowCount {
                min: Some(1),
                max: None,
            },
        )],
    )
    .unwrap()
    .with_column_verifications(vec![ColumnVerifications {
        column: 2,
        checks: vec![verification(
            "columns.RN.verifications[0]",
            Check::NotMissing,
        )],
    }])
    .unwrap()
}

#[test]
fn a_record_template_resolves_cells_phases_and_the_executor_schedule() {
    let plan = records_plan();
    let resolved = plan.resolve().unwrap();
    let dataset = RegionId::DATASET;
    assert_eq!(resolved.regions().len(), 1);
    let template = TemplateId(0);
    let (id, value, number) = (ColumnId(0), ColumnId(1), ColumnId(2));
    assert_eq!(resolved.column(id).phase, ColumnPhase::RowOnly);
    assert_eq!(resolved.column(number).phase, ColumnPhase::ColumnOnly);
    let rows: Vec<_> = resolved.row_bindings(template).collect();
    assert_eq!(rows, [BindingId(0), BindingId(1)]);
    assert_eq!(resolved.cell(template, value), Some(BindingId(1)));
    let column = resolved.cell(template, number).unwrap();
    assert_eq!(resolved.binding(column).stage, Stage::Column);
    assert_eq!(resolved.binding(column).templates, [template]);
    // Checks: the column group, the two key checks, then the dataset check.
    let (group, missing_key, duplicate_keys, count) =
        (CheckId(0), CheckId(1), CheckId(2), CheckId(3));
    assert_eq!(resolved.check(missing_key).family, CheckFamily::KeyMissing);
    assert_eq!(
        resolved.check(duplicate_keys).family,
        CheckFamily::KeyDuplicates
    );
    assert_eq!(
        steps(&resolved, dataset),
        [
            Step::Admit,
            Step::VerifyIntermediates { self_rows: false },
            Step::Driver(template),
            Step::Row(BindingId(0)),
            Step::Row(BindingId(1)),
            Step::PublishSelf(template),
            Step::VerifyIntermediates { self_rows: true },
            Step::Column(number),
            Step::Checks(group),
            Step::UnobservedFilterTypes,
            Step::OutputKeys,
            Step::Checks(count),
            Step::Order,
        ]
    );
    // Checks are ready at the step that evaluates them.
    let at = |step| StepId(position(&resolved, step) as u32);
    assert_eq!(resolved.check(group).ready_at, at(Step::Checks(group)));
    assert_eq!(resolved.check(missing_key).ready_at, at(Step::OutputKeys));
    assert_eq!(resolved.check(count).ready_at, at(Step::Checks(count)));
    // Cancellation points, partition classes and effects.
    let info = |step| resolved.schedule()[position(&resolved, step)];
    assert!(info(Step::Driver(template)).checkpoint);
    assert!(!info(Step::Row(BindingId(0))).checkpoint);
    assert!(info(Step::Column(number)).checkpoint);
    assert!(info(Step::Checks(count)).checkpoint);
    assert!(!info(Step::Checks(group)).checkpoint);
    assert_eq!(
        info(Step::Row(BindingId(0))).partition,
        PartitionClass::Safe
    );
    assert_eq!(info(Step::Column(number)).partition, PartitionClass::Global);
    assert_eq!(
        info(Step::OutputKeys).partition,
        PartitionClass::CombineStep
    );
    assert_eq!(info(Step::Row(BindingId(0))).effects, Effects::Pure);
    assert_eq!(resolved.keys(), [id]);
}

#[test]
fn a_grouped_filter_that_reads_a_window_runs_after_the_window_stage() {
    let plan = DatasetPlan::new(
        ints(&["ID"]),
        ints(&["ID", "RN"]),
        vec![RowTemplate {
            mode: RowMode::Groups(vec![0]),
            assignments: vec![
                assign(0, Expression::Source(0)),
                assign(
                    1,
                    Expression::Window(Window {
                        kind: WindowKind::RowNumber,
                        group_by: vec![],
                        order_by: vec![OrderTerm {
                            column: 0,
                            descending: false,
                            nulls_first: false,
                        }],
                        filter: None,
                    }),
                ),
            ],
            filter: Some(predicate(
                "RN = 1",
                "rows[0].filter",
                vec![bound("RN", BoundRead::Column(1))],
            )),
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap();
    let resolved = plan.resolve().unwrap();
    let template = TemplateId(0);
    let node = resolved.template(template);
    assert_eq!(
        node.filter.map(|filter| filter.placement),
        Some(FilterPlacement::Postwindow)
    );
    assert_eq!(node.driver.grain, Grain::Groups(vec![0]));
    assert_eq!(resolved.binding(BindingId(0)).stage, Stage::Prewindow);
    assert_eq!(resolved.binding(BindingId(1)).stage, Stage::Window);
    assert_eq!(
        steps(&resolved, RegionId::DATASET)[2..6],
        [
            Step::Driver(template),
            Step::Row(BindingId(0)),
            Step::Window(BindingId(1)),
            Step::PostwindowFilter(template),
        ]
    );
}

#[test]
fn column_phase_cells_are_bound_per_driver_context() {
    // The first template is driven by the primary input, the second by the
    // secondary input. Both need Y in the column phase; only the second
    // still needs X there.
    let plan = DatasetPlan::new_with_sources(
        ints(&["ID", "V"]),
        vec![SecondarySource {
            name: "SEC".into(),
            schema: ints(&["ID", "W"]),
        }],
        ints(&["ID", "X", "Y"]),
        vec![
            RowTemplate {
                mode: RowMode::Records,
                assignments: vec![
                    assign(0, Expression::Source(0)),
                    assign(1, Expression::Literal(Value::Int(1))),
                ],
                filter: None,
            },
            RowTemplate {
                mode: RowMode::Driver {
                    source: 1,
                    mode: Box::new(RowMode::Records),
                },
                assignments: vec![assign(0, Expression::Source(0))],
                filter: None,
            },
        ],
        vec![
            assign(1, Expression::Literal(Value::Int(2))),
            assign(2, Expression::Literal(Value::Int(3))),
        ],
        vec![0],
        vec![],
    )
    .unwrap();
    let resolved = plan.resolve().unwrap();
    let (primary, secondary) = (TemplateId(0), TemplateId(1));
    let (x, y) = (ColumnId(1), ColumnId(2));
    assert_eq!(resolved.column(x).phase, ColumnPhase::Mixed);
    assert_eq!(resolved.column(y).phase, ColumnPhase::ColumnOnly);
    let x_bindings: Vec<_> = resolved.column_bindings(x).collect();
    assert_eq!(x_bindings.len(), 1);
    assert_eq!(resolved.binding(x_bindings[0]).templates, [secondary]);
    assert_eq!(
        resolved.cell(primary, x),
        resolved.row_bindings(primary).nth(1)
    );
    // One declaration, one expression, one binding per driver context.
    let y_bindings: Vec<_> = resolved.column_bindings(y).collect();
    assert_eq!(y_bindings.len(), 2);
    let (first, second) = (
        resolved.binding(y_bindings[0]),
        resolved.binding(y_bindings[1]),
    );
    assert_eq!(first.templates, [primary]);
    assert_eq!(second.templates, [secondary]);
    assert_eq!(first.driver.source, SourceId(0));
    assert_eq!(second.driver.source, SourceId(1));
    assert_eq!(first.expr, second.expr);
    assert_eq!(resolved.cell(primary, y), Some(y_bindings[0]));
    assert_eq!(resolved.cell(secondary, y), Some(y_bindings[1]));
}

#[test]
fn a_deferred_odm_read_is_bound_per_template_with_that_template_s_scope() {
    use crate::dataset::{AggregatePhase, AggregateSource, BoundOdm, OdmRead, OdmScope};
    let scope = |row: &str| OdmTemplateScope {
        row: row.into(),
        driver: 0,
        groups: None,
    };
    let read = BoundOdm::new(OdmRead {
        dataset: "ODM".into(),
        item: "ODM.IT.VAL".into(),
        item_oid: "IT.VAL".into(),
        path: "columns.VAL.derivation.odm".into(),
        row: None,
        phase: AggregatePhase::Derivation,
        scope: OdmScope::Templates(vec![scope("first"), scope("second")].into()),
        source: Some(AggregateSource::Primary),
        selectors: [None, None, None],
        filter: None,
    })
    .unwrap();
    let fields: Vec<_> = crate::odm_source::FIELDS
        .iter()
        .map(|&name| (name, ColumnType::Str))
        .collect();
    let template = |column| RowTemplate {
        mode: RowMode::Records,
        assignments: vec![assign(0, Expression::Source(column))],
        filter: None,
    };
    let plan = DatasetPlan::new(
        schema(&fields),
        schema(&[("ID", ColumnType::Str), ("VAL", ColumnType::Str)]),
        vec![template(0), template(2)],
        vec![assign(1, Expression::Odm(Box::new(read)))],
        vec![0],
        vec![],
    )
    .unwrap();
    let resolved = plan.resolve().unwrap();
    let value = ColumnId(1);
    let bindings: Vec<_> = resolved.column_bindings(value).collect();
    assert_eq!(bindings.len(), 2);
    for (index, row) in ["first", "second"].into_iter().enumerate() {
        let binding = resolved.binding(bindings[index]);
        assert_eq!(binding.templates, [TemplateId(index as u32)]);
        assert_eq!(binding.driver.source, SourceId(0));
        assert_eq!(binding.driver.odm, Some(scope(row)));
        assert_eq!(binding.expr, resolved.binding(bindings[0]).expr);
        let ExprKind::Odm(relation) = resolved.expr(binding.expr).kind else {
            panic!("an ODM item read")
        };
        assert_eq!(resolved.relation(relation).kind, RelationKind::OdmItems);
    }
}

#[test]
fn key_grain_filters_every_record_before_its_key_bindings() {
    let plan = DatasetPlan::new(
        ints(&["ID", "V"]),
        ints(&["ID", "N"]),
        vec![RowTemplate {
            mode: RowMode::Keys,
            assignments: vec![assign(0, Expression::Source(0))],
            filter: Some(predicate(
                "V > 0",
                "filter",
                vec![bound("V", BoundRead::Source(1))],
            )),
        }],
        vec![assign(
            1,
            Expression::Collect {
                column: 1,
                identifier: "V".into(),
                filter: None,
                selection: None,
            },
        )],
        vec![0],
        vec![verification(
            "verifications[0]",
            Check::RowCount {
                min: None,
                max: Some(10),
            },
        )],
    )
    .unwrap()
    .with_column_verifications(vec![
        ColumnVerifications {
            column: 0,
            checks: vec![verification(
                "columns.ID.verifications[0]",
                Check::NotMissing,
            )],
        },
        ColumnVerifications {
            column: 1,
            checks: vec![verification(
                "columns.N.verifications[0]",
                Check::NotMissing,
            )],
        },
    ])
    .unwrap();
    let resolved = plan.resolve().unwrap();
    let template = TemplateId(0);
    assert_eq!(resolved.template(template).driver.grain, Grain::Keys);
    assert_eq!(
        resolved
            .template(template)
            .filter
            .map(|filter| filter.placement),
        Some(FilterPlacement::BeforeKeys)
    );
    assert_eq!(
        steps(&resolved, RegionId::DATASET),
        [
            Step::Admit,
            Step::VerifyIntermediates { self_rows: false },
            Step::Filter(template),
            Step::Row(BindingId(0)),
            Step::KeyGrain,
            // The key column's group is ready before any column step.
            Step::Checks(CheckId(0)),
            Step::Column(ColumnId(1)),
            Step::Checks(CheckId(1)),
            Step::UnobservedFilterTypes,
            Step::OutputKeys,
            Step::Checks(CheckId(4)),
            Step::Order,
        ]
    );
    let column = resolved.cell(template, ColumnId(1)).unwrap();
    assert_eq!(resolved.binding(column).driver.grain, Grain::Keys);
    assert_eq!(
        resolved.reads(resolved.binding(column).expr),
        [Read::Field(1)]
    );
}

fn lookup() -> Intermediate {
    Intermediate {
        identifier: "L".into(),
        path: "intermediates[0]".into(),
        source: IntermediateSource::Input(0),
        keys: vec![IntermediateKey {
            source_column: 0,
            path: "intermediates[0].key[0]".into(),
            value: MatchValue::Variable(MatchRead::Output {
                column: 0,
                identifier: "ID".into(),
            }),
        }],
        filter: None,
        selection: None,
        range: None,
        columns: None,
        no_match: Some(Value::Missing),
        require_match: false,
        unique: vec![],
        derivations: None,
        pending_schema: None,
    }
}
fn upper(column: usize, path: &str, operand: Expression) -> Expression {
    Expression::StrCase {
        source: Box::new(Assignment {
            column,
            expression: operand,
            path: format!("{path}.source"),
        }),
        operation: StringCase::new(
            "NAME".into(),
            path.into(),
            Case::Upper,
            Some(Value::Str("?".into())),
        )
        .unwrap(),
    }
}

/// Registration reaches some sites up front; others register when they fire.
fn handler_plan() -> DatasetPlan {
    let text = ColumnType::Str;
    DatasetPlan::new_with_intermediates(
        SourceSchemas {
            primary: schema(&[("ID", ColumnType::Int), ("NAME", text)]),
            secondary: vec![SecondarySource {
                name: "LKP".into(),
                schema: schema(&[("ID", ColumnType::Int), ("VAL", text)]),
            }],
        },
        vec![lookup()],
        schema(&[
            ("ID", ColumnType::Int),
            ("UP", text),
            ("ALT", text),
            ("CAT", text),
            ("LV", text),
        ]),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![
                assign(0, Expression::Source(0)),
                assign(1, upper(1, "columns.UP.derivation", Expression::Source(1))),
                // The case walker does not descend into `first_available`.
                assign(
                    2,
                    Expression::FirstAvailable(Box::new(FirstAvailable::new(
                        "columns.ALT.derivation".into(),
                        vec![SelectionSource {
                            path: "columns.ALT.derivation.sources[0]".into(),
                            read: SelectionRead::Expression(Box::new(Assignment {
                                column: 2,
                                expression: upper(
                                    2,
                                    "columns.ALT.derivation.sources[0]",
                                    Expression::Source(1),
                                ),
                                path: "columns.ALT.derivation.sources[0]".into(),
                            })),
                        }],
                        Value::Missing,
                    ))),
                ),
                assign(3, upper(3, "columns.CAT.derivation", Expression::Source(1))),
                assign(
                    4,
                    Expression::Intermediate {
                        index: 0,
                        column: 1,
                    },
                ),
            ],
            filter: None,
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap()
    .with_conversion_handlers(vec![ConversionHandler {
        assignment_path: "columns.C1.derivation".into(),
        handler: LiteralHandler {
            spec_path: "columns.UP.unconvertible".into(),
            value: Value::Missing,
        },
    }])
    .unwrap()
}

#[test]
fn registration_reproduces_the_eager_walk_and_marks_first_firings() {
    let plan = handler_plan();
    let resolved = plan.resolve().unwrap();
    let table: Vec<_> = resolved
        .handlers()
        .iter()
        .map(|site| (&*site.path, site.kind, site.registration))
        .collect();
    use HandlerKind as H;
    use Registration::{Eager, OnFirstFire};
    assert_eq!(
        table,
        [
            ("columns.UP.unconvertible", H::Unconvertible, Eager),
            ("columns.UP.derivation.missing", H::Missing, Eager),
            ("columns.ALT.derivation.missing", H::Missing, Eager),
            ("columns.CAT.derivation.missing", H::Missing, Eager),
            (
                "columns.ALT.derivation.sources[0].missing",
                H::Missing,
                OnFirstFire
            ),
            ("columns.C4.derivation.no_match", H::NoMatch, OnFirstFire),
        ]
    );
    // The same walk the executor runs registers the eager prefix.
    let mut counter = crate::numeric_lifecycle::HandlerCounter::default();
    plan.register_handlers(&mut counter);
    let walked: Vec<_> = counter
        .snapshot()
        .iter()
        .map(|entry| (entry.spec_path.as_str(), entry.handler, Eager))
        .collect();
    assert_eq!(walked, table[..4]);
    let up = resolved.binding(BindingId(1));
    assert_eq!(up.unconvertible, Some(HandlerSiteId(0)));
    let read = resolved.binding(BindingId(4));
    assert_eq!(
        resolved.exprs()[read.expr.index()].kind,
        ExprKind::Intermediate(RelationId(0))
    );
    assert_eq!(
        resolved.relation(RelationId(0)).keys[0].value,
        KeyValue::Read(Read::Column(ColumnId(0)))
    );
    let info = resolved.schedule()[position(&resolved, Step::Row(BindingId(1)))];
    assert_eq!(info.effects, Effects::Handlers);
    let info = resolved.schedule()[position(&resolved, Step::Row(BindingId(4)))];
    assert_eq!(info.partition, PartitionClass::Global);
}

/// A value-level `mandatory` or `data_type` check selects its rows by a where
/// column. When the column phase completes that column after the checked one,
/// the check waits for it in the schedule and in the executor's reference
/// order alike, so the plan resolves.
#[test]
fn a_scoped_check_waits_for_a_where_column_the_column_phase_completes() {
    use crate::dataset::{RowScope, WhereClause};
    let scope = || RowScope {
        clauses: vec![WhereClause {
            terms: vec![(2, vec!["ONSET".into()])],
        }],
        include: true,
    };
    for check in [
        Check::Mandatory(Some(scope())),
        Check::DataType {
            form: crate::iso8601::Form::Date,
            scope: Some(scope()),
        },
    ] {
        let plan = DatasetPlan::new(
            schema(&[("ID", ColumnType::Int), ("RES", ColumnType::Str)]),
            schema(&[
                ("ID", ColumnType::Int),
                ("RES", ColumnType::Str),
                ("CODE", ColumnType::Str),
            ]),
            vec![RowTemplate {
                mode: RowMode::Records,
                assignments: vec![
                    assign(0, Expression::Source(0)),
                    assign(1, Expression::Source(1)),
                ],
                filter: None,
            }],
            vec![assign(2, Expression::Literal(Value::Str("ONSET".into())))],
            vec![0],
            vec![],
        )
        .unwrap()
        .with_column_verifications(vec![ColumnVerifications {
            column: 1,
            checks: vec![verification(
                "columns.RES.submission.value_level.ONSET",
                check,
            )],
        }])
        .unwrap();
        let resolved = plan.resolve().unwrap();
        assert!(
            position(&resolved, Step::Column(ColumnId(2)))
                < position(&resolved, Step::Checks(CheckId(0)))
        );
    }
}

#[test]
fn a_reordered_schedule_is_refused_at_the_first_diverging_step() {
    let plan = records_plan();
    let (mut resolved, plans) = build::resolve(&plan).unwrap();
    // The two row bindings are independent, so only the order check sees it.
    let first = position(&resolved, Step::Row(BindingId(0)));
    resolved.schedule.swap(first, first + 1);
    assert_eq!(
        verify::verify(&resolved, &plans),
        Err(ResolutionError::Schedule {
            region: 0,
            position: first,
            expected: Some(ReferenceStep::Row {
                template: 0,
                assignment: 0
            }),
            found: Some(ReferenceStep::Row {
                template: 0,
                assignment: 1
            }),
        })
    );
}

#[test]
fn a_read_before_its_cell_is_complete_is_refused() {
    let plan = DatasetPlan::new(
        ints(&["ID"]),
        ints(&["ID", "COPY"]),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![
                assign(0, Expression::Source(0)),
                assign(1, Expression::Column(0)),
            ],
            filter: None,
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap();
    let (mut resolved, plans) = build::resolve(&plan).unwrap();
    let first = position(&resolved, Step::Row(BindingId(0)));
    resolved.schedule.swap(first, first + 1);
    let error = verify::verify(&resolved, &plans).unwrap_err();
    assert_eq!(
        error,
        ResolutionError::Topology {
            region: 0,
            position: first,
            step: ReferenceStep::Row {
                template: 0,
                assignment: 1
            },
            read: "reads column ID of template 0 before any step completes it".into(),
        }
    );
    assert!(error.to_string().contains("reads before completion"));
}

#[test]
fn a_diverging_handler_table_is_refused() {
    let plan = handler_plan();
    let (mut resolved, plans) = build::resolve(&plan).unwrap();
    resolved.handlers.swap(1, 2);
    assert_eq!(
        verify::verify(&resolved, &plans),
        Err(ResolutionError::Handlers {
            position: 1,
            expected: Some(("columns.UP.derivation.missing".into(), HandlerKind::Missing)),
            found: Some((
                "columns.ALT.derivation.missing".into(),
                HandlerKind::Missing
            )),
        })
    );
}

/// Records the order in which the executor admits callback signatures.
struct Recording {
    plan: crate::function_signature::InvocationPlan,
    admitted: std::cell::RefCell<Vec<usize>>,
}
impl crate::function_invocation::FunctionPort for Recording {
    type Error = std::convert::Infallible;
    fn signature(&self, slot: usize) -> Option<&crate::function_signature::InvocationPlan> {
        self.admitted.borrow_mut().push(slot);
        Some(&self.plan)
    }
    fn call(
        &mut self,
        _slot: usize,
        _arguments: &[crate::function_invocation::Argument<'_>],
    ) -> Result<Value, crate::function_invocation::HostError<Self::Error>> {
        Ok(Value::Int(0))
    }
}
struct Empty(TableSchema);
impl TableAccess for Empty {
    type Error = std::convert::Infallible;
    fn schema(&self) -> &TableSchema {
        &self.0
    }
    fn row_count(&self) -> usize {
        0
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Self::Error>> {
        Err(CellError::OutOfBounds { row, column })
    }
}

#[test]
fn callback_sites_follow_the_admission_order() {
    use crate::dataset::{BoundFunction, FunctionArgument, FunctionInput};
    use crate::function_signature::{FunctionIdentity, InvocationPlan, Parameter, Presence};
    let signature = InvocationPlan::new(
        FunctionIdentity {
            name: "identity".into(),
            contract_version: "1".into(),
            implementation_version: "1".into(),
            call: "identity".into(),
        },
        vec![Parameter {
            name: "value".into(),
            host_name: "value".into(),
            kind: crate::value::ValueType::Int,
            accepts_missing: true,
            presence: Presence::Required,
        }],
        ColumnType::Int,
        true,
    )
    .unwrap();
    let call = |slot, read| {
        Expression::Function(
            BoundFunction::new(
                slot,
                signature.clone(),
                vec![FunctionArgument {
                    name: "value".into(),
                    input: FunctionInput::Read(read),
                }],
            )
            .unwrap(),
        )
    };
    let plan = DatasetPlan::new(
        ints(&["ID"]),
        ints(&["ID", "F", "G", "H"]),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![
                assign(0, Expression::Source(0)),
                assign(1, call(3, BoundRead::Source(0))),
                assign(
                    2,
                    Expression::Case(vec![CaseBranch {
                        when: Some(predicate(
                            "ID > 0",
                            "columns.G.derivation.case[0].when",
                            vec![bound("ID", BoundRead::Source(0))],
                        )),
                        assignment: Box::new(assign(2, call(1, BoundRead::Column(0)))),
                    }]),
                ),
            ],
            filter: None,
        }],
        vec![assign(3, call(2, BoundRead::Column(1)))],
        vec![0],
        vec![],
    )
    .unwrap();
    let resolved = plan.resolve().unwrap();
    let mut port = Recording {
        plan: signature.clone(),
        admitted: Default::default(),
    };
    let attempt = plan.execute(&Empty(ints(&["ID"])), &[], &mut port, None);
    assert!(attempt.result.is_ok());
    let slots: Vec<_> = resolved.functions().iter().map(|call| call.slot).collect();
    assert_eq!(slots, [3, 1, 2]);
    assert_eq!(slots, *port.admitted.borrow());
    let info = resolved.schedule()[position(&resolved, Step::Row(BindingId(1)))];
    assert_eq!(info.effects, Effects::Callbacks);
}
