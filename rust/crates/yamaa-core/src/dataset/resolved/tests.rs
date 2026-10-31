//! Resolution of hand-built plans: slots, schedule, registration and the
//! topology check, including its refusal.
use super::*;
use crate::bound_expression::{Binding as Bound, BoundPredicate, Read as BoundRead};
use crate::conversion::LiteralHandler;
use crate::dataset::{
    Assignment, CaseBranch, Check, ColumnVerifications, ConversionHandler, ExecutionError,
    ExecutionEvent, ExecutionPhase, Expression, FirstAvailable, Intermediate, IntermediateKey,
    IntermediateSource, MatchRead, MatchValue, OrderTerm, RowMode, RowTemplate, SecondarySource,
    SelectionRead, SelectionSource, SourceSchemas, Verification, Window, WindowKind,
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
    .with_column_verifications(vec![ColumnVerifications {
        column: 2,
        checks: vec![verification(
            "columns.RN.verifications[0]",
            Check::NOT_MISSING,
        )],
    }])
}

#[test]
fn a_record_template_resolves_cells_phases_and_the_schedule() {
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
    );
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
    let plan = DatasetPlan::new_with_intermediates(
        SourceSchemas {
            primary: ints(&["ID", "V"]),
            secondary: vec![SecondarySource {
                name: "SEC".into(),
                schema: ints(&["ID", "W"]),
            }],
        },
        vec![],
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
    );
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
    );
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

/// A key-grain template whose filter reads V, a column-phase collection and
/// a check on each column and on the dataset.
fn key_grain_plan() -> DatasetPlan {
    DatasetPlan::new(
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
    .with_column_verifications(vec![
        ColumnVerifications {
            column: 0,
            checks: vec![verification(
                "columns.ID.verifications[0]",
                Check::NOT_MISSING,
            )],
        },
        ColumnVerifications {
            column: 1,
            checks: vec![verification(
                "columns.N.verifications[0]",
                Check::NOT_MISSING,
            )],
        },
    ])
}

#[test]
fn key_grain_filters_every_record_before_its_key_bindings() {
    let resolved = key_grain_plan().resolve().unwrap();
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
        nearest: None,
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

/// Sites at every depth: a conversion handler, an operation, one nested in a
/// `first_available` operand, and a named read.
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
    .with_conversion_handlers(vec![ConversionHandler {
        assignment_path: "columns.C1.derivation".into(),
        handler: LiteralHandler {
            spec_path: "columns.UP.unconvertible".into(),
            value: Value::Missing,
        },
    }])
}

#[test]
fn registration_registers_every_site_up_front_in_expression_order() {
    let plan = handler_plan();
    let resolved = plan.resolve().unwrap();
    let table: Vec<_> = resolved
        .handlers()
        .iter()
        .map(|site| (&*site.path, site.kind))
        .collect();
    use HandlerKind as H;
    assert_eq!(
        table,
        [
            ("columns.UP.unconvertible", H::Unconvertible),
            ("columns.UP.derivation.missing", H::Missing),
            ("columns.ALT.derivation.missing", H::Missing),
            ("columns.ALT.derivation.sources[0].missing", H::Missing),
            ("columns.CAT.derivation.missing", H::Missing),
            ("columns.C4.derivation.absent", H::Absent),
        ]
    );
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
}

/// A named read counts its absence under `absent` whether or not the
/// intermediate declares an answer (D202). Only `require_match` removes the
/// site, and only `keep` adds `multiple_matches` (LOOKUP-24, LOOKUP-10).
#[test]
fn a_named_read_has_an_absence_site_unless_it_requires_a_match() {
    let sites = |no_match: Option<Value>, require_match: bool, keep: bool| {
        let mut intermediate = lookup();
        intermediate.no_match = no_match;
        intermediate.require_match = require_match;
        intermediate.selection = keep.then(|| crate::dataset::SourceSelection {
            order_by: vec![OrderTerm {
                column: 1,
                descending: false,
                nulls_first: true,
            }],
            keep: crate::dataset::Keep::First,
        });
        let plan = DatasetPlan::new_with_intermediates(
            SourceSchemas {
                primary: ints(&["ID"]),
                secondary: vec![SecondarySource {
                    name: "LKP".into(),
                    schema: schema(&[("ID", ColumnType::Int), ("VAL", ColumnType::Str)]),
                }],
            },
            vec![intermediate],
            schema(&[("ID", ColumnType::Int), ("LV", ColumnType::Str)]),
            vec![RowTemplate {
                mode: RowMode::Records,
                assignments: vec![
                    assign(0, Expression::Source(0)),
                    assign(
                        1,
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
        );
        let resolved = plan.resolve().unwrap();
        resolved
            .handlers()
            .iter()
            .map(|site| (site.path.to_string(), site.kind))
            .collect::<Vec<_>>()
    };
    let multiple = || {
        (
            "columns.C1.derivation.multiple_matches".to_string(),
            HandlerKind::MultipleMatches,
        )
    };
    let answer = || {
        (
            "columns.C1.derivation.absent".to_string(),
            HandlerKind::Absent,
        )
    };
    let declared = Some(Value::Missing);
    assert_eq!(sites(None, false, false), [answer()], "no answer");
    assert_eq!(
        sites(declared, false, false),
        [answer()],
        "a declared answer"
    );
    assert_eq!(sites(None, true, false), [], "require_match");
    let both = [multiple(), answer()];
    assert_eq!(sites(None, false, true), both, "keep");
    assert_eq!(
        sites(None, true, true),
        [multiple()],
        "keep and require_match"
    );
}

/// A value-level `mandatory` or `data_type` check selects its rows by a where
/// column. When the column phase completes that column after the checked one,
/// the check waits for it in the schedule, so the plan resolves.
#[test]
fn a_scoped_check_waits_for_a_where_column_the_column_phase_completes() {
    use crate::dataset::{RowScope, WhereClause};
    let scope = || RowScope {
        clauses: vec![WhereClause {
            terms: vec![crate::dataset::WhereTerm::listed(2, vec!["ONSET".into()])],
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
        .with_column_verifications(vec![ColumnVerifications {
            column: 1,
            checks: vec![verification(
                "columns.RES.submission.value_level.ONSET",
                check,
            )],
        }]);
        let resolved = plan.resolve().unwrap();
        assert!(
            position(&resolved, Step::Column(ColumnId(2)))
                < position(&resolved, Step::Checks(CheckId(0)))
        );
    }
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
    );
    let mut resolved = build::resolve(&plan).unwrap();
    let first = position(&resolved, Step::Row(BindingId(0)));
    resolved.schedule.swap(first, first + 1);
    let error = verify::verify(&resolved).unwrap_err();
    assert_eq!(
        error,
        ResolutionError::Topology {
            region: 0,
            position: first,
            step: Step::Row(BindingId(1)),
            read: "reads column ID of template 0 before any step completes it".into(),
        }
    );
    assert!(error.to_string().contains("reads before completion"));
}

/// Records the order in which the executor admits callback signatures.
struct Recording {
    plan: crate::function_signature::ProjectInvocationPlan,
    admitted: std::cell::RefCell<Vec<usize>>,
}
impl crate::function_invocation::FunctionPort for Recording {
    type Error = std::convert::Infallible;
    fn project_signature(
        &self,
        slot: usize,
    ) -> Option<&crate::function_signature::ProjectInvocationPlan> {
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
    use crate::dataset::{BoundProjectFunction, FunctionArgument, FunctionInput};
    use crate::function_signature::{
        Parameter, Presence, ProjectFunctionIdentity, ProjectInvocationPlan,
    };
    let signature = ProjectInvocationPlan::new(
        ProjectFunctionIdentity {
            name: "identity".into(),
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
        Expression::ProjectFunction(
            BoundProjectFunction::new_project(
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
    );
    let resolved = plan.resolve().unwrap();
    let mut port = Recording {
        plan: signature.clone(),
        admitted: Default::default(),
    };
    let attempt = resolved.execute(&Empty(ints(&["ID"])), &[], &mut port, None, &|| false);
    assert!(attempt.result.is_ok());
    let slots: Vec<_> = resolved.functions().iter().map(|call| call.slot).collect();
    assert_eq!(slots, [3, 1, 2]);
    assert_eq!(slots, *port.admitted.borrow());
    let info = resolved.schedule()[position(&resolved, Step::Row(BindingId(1)))];
    assert_eq!(info.effects, Effects::Callbacks);
}

#[test]
fn every_node_carries_its_operation_and_every_relation_check_its_declaration() {
    for plan in [records_plan(), handler_plan()] {
        let resolved = plan.resolve().unwrap();
        for node in resolved.exprs() {
            assert!(
                node.op.fits(node.kind),
                "{:?} holds {:?}",
                node.kind,
                node.op
            );
        }
        for relation in resolved.relations() {
            assert_eq!(
                relation.named.is_some(),
                matches!(relation.kind, RelationKind::Named { .. })
            );
        }
        for check in resolved.checks() {
            let keys = check.scope == CheckScope::OutputKeys;
            assert_eq!(matches!(check.payload, CheckPayload::Keys), keys);
        }
    }
    // The handler plan's named read carries the bound intermediate.
    let resolved = handler_plan().resolve().unwrap();
    let named = resolved.relations()[0].named.as_deref().unwrap();
    assert_eq!(named.identifier, "L");
    assert_eq!(resolved.region(RegionId::DATASET).secondary[0].name, "LKP");
}

/// Rows of int values, read through the executor's ports.
struct Rows(TableSchema, Vec<Vec<i64>>);
impl TableAccess for Rows {
    type Error = std::convert::Infallible;
    fn schema(&self) -> &TableSchema {
        &self.0
    }
    fn row_count(&self) -> usize {
        self.1.len()
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Self::Error>> {
        self.1
            .get(row)
            .and_then(|values| values.get(column))
            .map(|&value| ValueRef::Int(value))
            .ok_or(CellError::OutOfBounds { row, column })
    }
}

/// Three templates, told apart by T: the first over the primary input derives
/// V itself, the second over SEC and the third over the primary leave V to
/// `columns`.
fn driver_bindings_plan(columns: Vec<crate::dataset::ColumnBinding>) -> DatasetPlan {
    let template = |mode: RowMode, tag: i64| RowTemplate {
        mode,
        assignments: std::iter::once(assign(0, Expression::Source(0)))
            .chain((tag == 0).then(|| assign(1, Expression::Literal(Value::Int(0)))))
            .chain([assign(2, Expression::Literal(Value::Int(tag)))])
            .collect(),
        filter: None,
    };
    DatasetPlan::new_with_bindings(
        SourceSchemas {
            primary: ints(&["ID", "A"]),
            secondary: vec![SecondarySource {
                name: "SEC".into(),
                schema: ints(&["ID", "B", "C"]),
            }],
        },
        Vec::new(),
        ints(&["ID", "V", "T"]),
        vec![
            template(RowMode::Records, 0),
            template(
                RowMode::Driver {
                    source: 1,
                    mode: Box::new(RowMode::Records),
                },
                1,
            ),
            template(RowMode::Records, 2),
        ],
        columns,
        vec![0, 2],
        vec![],
    )
}

fn binding(
    expression: Expression,
    templates: Vec<usize>,
    scope: crate::dataset::ColumnScope,
) -> crate::dataset::ColumnBinding {
    crate::dataset::ColumnBinding {
        assignment: assign(1, expression),
        templates,
        scope,
    }
}

#[test]
fn a_column_bound_per_driver_reads_each_rows_own_record() {
    use crate::dataset::ColumnScope::Template;
    // V is bound once for SEC, reading its second field, and once for the
    // primary, reading its second field: the same position in two inputs.
    let plan = driver_bindings_plan(vec![
        binding(Expression::Source(1), vec![1], Template),
        binding(Expression::Source(1), vec![2], Template),
    ]);
    let resolved = plan.resolve().unwrap();
    let value = ColumnId(1);
    assert_eq!(resolved.column(value).phase, ColumnPhase::Mixed);
    let bindings: Vec<_> = resolved.column_bindings(value).collect();
    assert_eq!(bindings.len(), 2);
    let (secondary, primary) = (resolved.binding(bindings[0]), resolved.binding(bindings[1]));
    assert_eq!(secondary.templates, [TemplateId(1)]);
    assert_eq!(secondary.driver.source, SourceId(1));
    assert_eq!(primary.templates, [TemplateId(2)]);
    assert_eq!(primary.driver.source, SourceId(0));
    // Each binding of the bound plan is its own expression.
    assert_ne!(secondary.expr, primary.expr);
    assert_eq!(secondary.origin, BindingOrigin::Column { assignment: 0 });
    assert_eq!(primary.origin, BindingOrigin::Column { assignment: 1 });
    assert_eq!(resolved.cell(TemplateId(1), value), Some(bindings[0]));
    assert_eq!(resolved.cell(TemplateId(2), value), Some(bindings[1]));
    // One column step runs both bindings.
    let columns: Vec<_> = steps(&resolved, RegionId::DATASET)
        .into_iter()
        .filter(|step| matches!(step, Step::Column(_)))
        .collect();
    assert_eq!(columns, [Step::Column(value)]);
    // The primary rows build the first and third templates; SEC's build the
    // second. Each row reads its own driver's record.
    let primary_rows = Rows(ints(&["ID", "A"]), vec![vec![1, 10], vec![2, 20]]);
    let secondary_rows = Rows(ints(&["ID", "B", "C"]), vec![vec![3, 30, 300]]);
    let attempt = resolved.execute(
        &primary_rows,
        &[&secondary_rows],
        &mut crate::function_invocation::NoFunctions::new(),
        None,
        &|| false,
    );
    let rows = attempt.result.unwrap().dataset.rows().to_vec();
    let int = |values: [i64; 3]| values.map(Value::Int).to_vec();
    assert_eq!(
        rows,
        [
            int([1, 0, 0]),
            int([2, 0, 0]),
            int([3, 30, 1]),
            int([1, 10, 2]),
            int([2, 20, 2])
        ]
    );
}

#[test]
fn a_whole_dataset_binding_reads_through_the_primary_port() {
    use crate::dataset::ColumnScope::Dataset;
    let plan = driver_bindings_plan(vec![binding(
        Expression::Literal(Value::Int(9)),
        vec![1, 2],
        Dataset,
    )]);
    let resolved = plan.resolve().unwrap();
    let bindings: Vec<_> = resolved.column_bindings(ColumnId(1)).collect();
    // Both templates share the primary port, so they share one binding.
    assert_eq!(bindings.len(), 1);
    let bound = resolved.binding(bindings[0]);
    assert_eq!(bound.templates, [TemplateId(1), TemplateId(2)]);
    assert_eq!(bound.driver.source, SourceId(0));
    assert_eq!(bound.driver.named, None);
}

#[test]
fn column_bindings_register_their_sites_one_binding_after_another() {
    use crate::dataset::ColumnScope::Template;
    // The SEC binding answers an absent variable; the primary binding counts
    // a missing `first_available`. Each binding's sites register in turn,
    // whatever their kinds.
    let source = Expression::SourceBinding {
        source: Box::new(assign(1, Expression::Source(1))),
        absent: Some(Value::Int(0)),
        selection_declared: false,
        variable: "SEC.B".into(),
        path: "columns.V.derivation.source".into(),
    };
    let first_available = Expression::FirstAvailable(Box::new(FirstAvailable::new(
        "columns.V.derivation.first_available".into(),
        vec![SelectionSource {
            path: "columns.V.derivation.first_available.sources[0]".into(),
            read: SelectionRead::Source(1),
        }],
        Value::Missing,
    )));
    let plan = driver_bindings_plan(vec![
        binding(source, vec![1], Template),
        binding(first_available, vec![2], Template),
    ]);
    let resolved = plan.resolve().unwrap();
    let sites: Vec<_> = resolved
        .handlers()
        .iter()
        .map(|site| (&*site.path, site.kind))
        .collect();
    assert_eq!(
        sites,
        [
            ("columns.V.derivation.source.absent", HandlerKind::Absent),
            (
                "columns.V.derivation.first_available.missing",
                HandlerKind::Missing
            ),
        ]
    );
}

/// A cancellation probe that counts its polls and answers true from poll
/// `from` on, or never.
struct Probe {
    polls: std::cell::Cell<usize>,
    from: Option<usize>,
}
impl Probe {
    fn new(from: Option<usize>) -> Self {
        Self {
            polls: std::cell::Cell::new(0),
            from,
        }
    }
    fn cancelled(&self) -> bool {
        let poll = self.polls.get() + 1;
        self.polls.set(poll);
        self.from.is_some_and(|from| poll >= from)
    }
}

fn checkpoints(plan: &ResolvedPlan) -> usize {
    plan.region_schedule(RegionId::DATASET)
        .iter()
        .filter(|info| info.checkpoint)
        .count()
}

/// A run asks its probe once at each checkpoint of the dataset's schedule
/// and nowhere else when nothing calls the host, and an uncancelled run is
/// the run without a probe (#2822).
#[test]
fn a_run_polls_its_probe_once_at_every_checkpoint_of_its_schedule() {
    for (plan, rows) in [
        (
            records_plan(),
            Rows(ints(&["ID", "V"]), vec![vec![1, 20], vec![2, 10]]),
        ),
        (
            key_grain_plan(),
            Rows(ints(&["ID", "V"]), vec![vec![1, 5], vec![2, 6], vec![3, 0]]),
        ),
    ] {
        let resolved = plan.resolve().unwrap();
        assert_eq!(checkpoints(&resolved), 7);
        let probe = Probe::new(None);
        let polled = resolved.execute(
            &rows,
            &[],
            &mut crate::function_invocation::NoFunctions::new(),
            None,
            &|| probe.cancelled(),
        );
        assert_eq!(probe.polls.get(), 7);
        let plain = resolved.execute(
            &rows,
            &[],
            &mut crate::function_invocation::NoFunctions::new(),
            None,
            &|| false,
        );
        assert!(plain.result.is_ok(), "{:?}", plain.result);
        assert_eq!(polled, plain);
    }
}

/// A run cancelled at any checkpoint ends cancelled, asks its probe no more
/// and keeps nothing it built: no output, handler counts, verifications or
/// template rows.
#[test]
fn a_run_cancelled_at_any_checkpoint_ends_cancelled_and_keeps_nothing() {
    for (plan, rows) in [
        (
            records_plan(),
            Rows(ints(&["ID", "V"]), vec![vec![1, 20], vec![2, 10]]),
        ),
        (
            key_grain_plan(),
            Rows(ints(&["ID", "V"]), vec![vec![1, 5], vec![2, 6], vec![3, 0]]),
        ),
    ] {
        let resolved = plan.resolve().unwrap();
        for from in 1..=checkpoints(&resolved) {
            let probe = Probe::new(Some(from));
            let mut events = Vec::new();
            let mut observe = |event| events.push(event);
            let attempt = resolved.execute(
                &rows,
                &[],
                &mut crate::function_invocation::NoFunctions::new(),
                Some(&mut observe),
                &|| probe.cancelled(),
            );
            assert!(
                matches!(
                    attempt.result.as_ref().map_err(|error| &**error),
                    Err(ExecutionError::Cancelled)
                ),
                "cancelled at checkpoint {from}: {:?}",
                attempt.result
            );
            assert_eq!(probe.polls.get(), from);
            assert!(attempt.handler_counts.is_empty());
            assert!(attempt.retained_verifications.is_empty());
            assert!(attempt.template_rows.is_empty());
            // The phase it stopped in exits as failed, then the run finishes.
            assert!(matches!(
                events[events.len() - 2],
                ExecutionEvent::Exit {
                    outcome: crate::application_observer::Outcome::Failed,
                    ..
                }
            ));
            assert_eq!(
                events.last(),
                Some(&ExecutionEvent::Phase(ExecutionPhase::Finished))
            );
        }
    }
}

/// A function port that counts its calls and answers 0.
struct Counting {
    plan: crate::function_signature::ProjectInvocationPlan,
    calls: std::rc::Rc<std::cell::Cell<usize>>,
}
impl crate::function_invocation::FunctionPort for Counting {
    type Error = std::convert::Infallible;
    fn project_signature(
        &self,
        _slot: usize,
    ) -> Option<&crate::function_signature::ProjectInvocationPlan> {
        Some(&self.plan)
    }
    fn call(
        &mut self,
        _slot: usize,
        _arguments: &[crate::function_invocation::Argument<'_>],
    ) -> Result<Value, crate::function_invocation::HostError<Self::Error>> {
        self.calls.set(self.calls.get() + 1);
        Ok(Value::Int(0))
    }
}

/// A unit that calls the host polls before each row's calls, so a probe
/// that a call cancels, as a host's probe is once its interpreter has an
/// interrupt pending, stops the run before the next row's call.
#[test]
fn a_run_polls_before_each_batch_of_project_function_calls() {
    use crate::dataset::{BoundProjectFunction, FunctionArgument, FunctionInput};
    use crate::function_signature::{
        Parameter, Presence, ProjectFunctionIdentity, ProjectInvocationPlan,
    };
    let signature = ProjectInvocationPlan::new(
        ProjectFunctionIdentity {
            name: "identity".into(),
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
    let plan = DatasetPlan::new(
        ints(&["ID"]),
        ints(&["ID", "F"]),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![
                assign(0, Expression::Source(0)),
                assign(
                    1,
                    Expression::ProjectFunction(
                        BoundProjectFunction::new_project(
                            0,
                            signature.clone(),
                            vec![FunctionArgument {
                                name: "value".into(),
                                input: FunctionInput::Read(BoundRead::Source(0)),
                            }],
                        )
                        .unwrap(),
                    ),
                ),
            ],
            filter: None,
        }],
        vec![],
        vec![0],
        vec![],
    );
    let resolved = plan.resolve().unwrap();
    let rows = Rows(ints(&["ID"]), vec![vec![1], vec![2], vec![3]]);
    let run = |cancel_after: Option<usize>| {
        let calls = std::rc::Rc::new(std::cell::Cell::new(0));
        let mut port = Counting {
            plan: signature.clone(),
            calls: calls.clone(),
        };
        let probe = Probe::new(None);
        let attempt = resolved.execute(&rows, &[], &mut port, None, &|| {
            probe.cancelled();
            cancel_after.is_some_and(|after| calls.get() >= after)
        });
        (attempt, calls.get(), probe.polls.get())
    };
    // Uncancelled: the checkpoints, then one poll before each of the three
    // rows' calls.
    let (attempt, calls, polls) = run(None);
    assert!(attempt.result.is_ok(), "{:?}", attempt.result);
    assert_eq!(calls, 3);
    assert_eq!(polls, checkpoints(&resolved) + 3);
    // The first call cancels: the second row's poll stops the run.
    let (attempt, calls, _) = run(Some(1));
    assert!(matches!(
        attempt.result.as_ref().map_err(|error| &**error),
        Err(ExecutionError::Cancelled)
    ));
    assert_eq!(calls, 1);
}

/// A source of S, U and P (text) and V (integer), with SEC, a secondary
/// input of S, U and V, and an output of S, U, P, V and R (integer) keyed
/// [S, U, P]. The record template copies the four fields, except that
/// `literal_u` derives U from a literal, and R derives `shape`.
fn keyed_plan(templates: Vec<RowTemplate>, columns: Vec<Assignment>) -> DatasetPlan {
    let text = ColumnType::Str;
    let int = ColumnType::Int;
    DatasetPlan::new_with_intermediates(
        SourceSchemas {
            primary: schema(&[("S", text), ("U", text), ("P", text), ("V", int)]),
            secondary: vec![SecondarySource {
                name: "SEC".into(),
                schema: schema(&[("S", text), ("U", text), ("V", int)]),
            }],
        },
        vec![Intermediate {
            identifier: "L".into(),
            path: "intermediates[0]".into(),
            source: IntermediateSource::Input(0),
            keys: vec![pair(0, 0)],
            filter: None,
            selection: None,
            nearest: None,
            range: None,
            columns: None,
            no_match: Some(Value::Missing),
            require_match: false,
            unique: vec![],
            derivations: None,
            pending_schema: None,
        }],
        schema(&[
            ("S", text),
            ("U", text),
            ("P", text),
            ("V", int),
            ("R", int),
        ]),
        templates,
        columns,
        vec![0, 1, 2],
        vec![],
    )
}
fn copies(mode: RowMode, literal_u: bool) -> RowTemplate {
    let u = if literal_u {
        Expression::Literal(Value::Str("u".into()))
    } else {
        Expression::Source(1)
    };
    RowTemplate {
        mode,
        assignments: vec![
            assign(0, Expression::Source(0)),
            assign(1, u),
            assign(2, Expression::Source(2)),
            assign(3, Expression::Source(3)),
        ],
        filter: None,
    }
}
/// A template driven by SEC, whose rows belong to no partition: it copies
/// SEC's S, U and V, writes P as a literal, and derives R from `r`, if any.
fn from_sec(r: Option<Expression>) -> RowTemplate {
    let mut assignments = vec![
        assign(0, Expression::Source(0)),
        assign(1, Expression::Source(1)),
        assign(2, Expression::Literal(Value::Str("p".into()))),
        assign(3, Expression::Source(2)),
    ];
    assignments.extend(r.map(|r| assign(4, r)));
    RowTemplate {
        mode: RowMode::Driver {
            source: 1,
            mode: Box::new(RowMode::Records),
        },
        assignments,
        filter: None,
    }
}
/// R derives `shape` in the column phase.
fn column_shape(shape: Expression) -> DatasetPlan {
    keyed_plan(
        vec![copies(RowMode::Records, false)],
        vec![assign(4, shape)],
    )
}
/// R derives `shape` in the template's row stage.
fn row_shape(mode: RowMode, shape: Expression) -> DatasetPlan {
    let mut template = copies(mode, false);
    template.assignments.push(assign(4, shape));
    keyed_plan(vec![template], vec![])
}
/// A row number over V within the groups of the output columns `group_by`.
fn numbered_by(group_by: Vec<usize>) -> Expression {
    Expression::Window(Window {
        kind: WindowKind::RowNumber,
        group_by,
        order_by: vec![OrderTerm {
            column: 3,
            descending: false,
            nulls_first: false,
        }],
        filter: None,
    })
}
/// A count of the records of `relation`.
fn counted(relation: crate::dataset::AggregateRelation, row: bool) -> Expression {
    let path = "columns.R.derivation.aggregate";
    Expression::Aggregate(Box::new(
        crate::dataset::BoundAggregate::new(
            crate::expression::compile(
                "COUNT(D.*)",
                crate::expression::Context::Aggregate,
                &format!("{path}.expr"),
            )
            .unwrap(),
            relation,
            vec![],
            None,
            if row {
                crate::dataset::AggregatePhase::RowConstruction
            } else {
                crate::dataset::AggregatePhase::Derivation
            },
            None,
            path,
        )
        .unwrap(),
    ))
}
/// A key pairing the donor field `field` with the output column `column`.
fn pair(field: usize, column: usize) -> IntermediateKey {
    IntermediateKey {
        source_column: field,
        path: format!("key[{field}]"),
        value: MatchValue::Variable(MatchRead::Output {
            column,
            identifier: ["S", "U", "P", "V", "R"][column].into(),
        }),
    }
}
/// A key pairing the donor field `field` with the driver field `record`.
fn record_pair(field: usize, record: usize) -> IntermediateKey {
    IntermediateKey {
        source_column: field,
        path: format!("key[{field}]"),
        value: MatchValue::Variable(MatchRead::Record {
            column: record,
            identifier: format!("SRC.{}", ["S", "U", "P", "V"][record]),
        }),
    }
}
/// A qualified aggregate over `source` whose key pairs `keys`.
fn matched(source: crate::dataset::AggregateSource, keys: &[(usize, usize)]) -> Expression {
    counted(
        crate::dataset::AggregateRelation::Matched {
            source,
            keys: keys
                .iter()
                .map(|&(field, column)| pair(field, column))
                .collect(),
            group_by: vec![],
        },
        false,
    )
}

/// The class of the step that derives R, under consecutive partitions, then
/// under key-bounded partitions over one, two and three keys.
fn classes_of_r(plan: &DatasetPlan) -> [PartitionClass; 4] {
    let resolved = plan.resolve().unwrap();
    let (r, binding) = (
        ColumnId(4),
        resolved.cell(TemplateId(0), ColumnId(4)).unwrap(),
    );
    let step = match resolved.binding(binding).stage {
        Stage::Column => Step::Column(r),
        Stage::Window => Step::Window(binding),
        Stage::Prewindow => Step::Row(binding),
    };
    let info = resolved.schedule()[position(&resolved, step)];
    [0, 1, 2, 3].map(|keys| info.class(keys))
}

/// The schedule's classes under key-bounded partitions (#3763), a seam no
/// run's output shows. Under such partitions over the leading k declared
/// keys, a window or aggregate step is partition-safe exactly when its
/// grouping includes each of those k keys, each a key the driver input
/// holds as a field the record template copies; otherwise, and under
/// consecutive partitions, it is global. Each expectation reads the keys
/// [S, U, P] and the shape's grouping: the window's `group_by` (WINDOW-3),
/// read as an output column or as the driver field the key copies (WINDOW-11);
/// an unqualified aggregate's `group_by`, a grouped row template's group
/// fields and a qualified aggregate's key (LOOKUP-18, LOOKUP-19), which over
/// the driver input must pair each key with the field that holds it. A row
/// of a template driven by SEC belongs to no partition, so a column window
/// or qualified aggregate that reads one holds no key, while the record
/// template's own row window still does. A named read of SEC, held whole
/// beside every partition, is partition-safe under any partitions (#4191).
#[test]
fn a_window_or_aggregate_is_partition_safe_exactly_when_it_groups_by_every_partition_key() {
    use crate::dataset::{AggregateRelation as Relation, AggregateSource as Source};
    use PartitionClass::{Global as G, Safe as S};
    let groups = |fields: Vec<usize>| RowMode::Groups(fields);
    let cases: [(&str, DatasetPlan, [PartitionClass; 4]); 22] = [
        (
            "a column window over every row",
            column_shape(numbered_by(vec![])),
            [G, G, G, G],
        ),
        (
            "a column window by S",
            column_shape(numbered_by(vec![0])),
            [G, S, G, G],
        ),
        (
            "a column window by S and U",
            column_shape(numbered_by(vec![0, 1])),
            [G, S, S, G],
        ),
        (
            "a column window by U",
            column_shape(numbered_by(vec![1])),
            [G, G, G, G],
        ),
        (
            "a column window by S and the driver field U",
            column_shape(Expression::ContextWindow(crate::dataset::ContextWindow {
                window: Window {
                    kind: WindowKind::RowNumber,
                    group_by: vec![0, 5],
                    order_by: vec![OrderTerm {
                        column: 3,
                        descending: false,
                        nulls_first: false,
                    }],
                    filter: None,
                },
                sources: vec![("SRC.U".into(), 1)],
            })),
            [G, S, S, G],
        ),
        (
            "a column window by V, U and S",
            column_shape(numbered_by(vec![3, 1, 0])),
            [G, S, S, G],
        ),
        (
            "a column window by S, U and P",
            column_shape(numbered_by(vec![0, 1, 2])),
            [G, S, S, S],
        ),
        (
            "a row template's window by S and U",
            row_shape(RowMode::Records, numbered_by(vec![0, 1])),
            [G, S, S, G],
        ),
        (
            "an unqualified aggregate by S and U",
            column_shape(counted(
                Relation::Output {
                    group_by: vec![0, 1],
                },
                false,
            )),
            [G, S, S, G],
        ),
        (
            "an unqualified aggregate by U and P",
            column_shape(counted(
                Relation::Output {
                    group_by: vec![1, 2],
                },
                false,
            )),
            [G, G, G, G],
        ),
        (
            "an aggregate of rows grouped by S, U and P",
            row_shape(groups(vec![0, 1, 2]), counted(Relation::CurrentGroup, true)),
            [G, S, S, S],
        ),
        (
            "an aggregate of rows grouped by S",
            row_shape(groups(vec![0]), counted(Relation::CurrentGroup, true)),
            [G, S, G, G],
        ),
        (
            "a qualified aggregate of SEC keyed by S and U",
            column_shape(matched(Source::Secondary(0), &[(0, 0), (1, 1)])),
            [G, S, S, G],
        ),
        (
            "a qualified aggregate of SEC keyed by U",
            column_shape(matched(Source::Secondary(0), &[(1, 1)])),
            [G, G, G, G],
        ),
        (
            "a qualified aggregate of SEC keyed by the driver fields S and U",
            column_shape(counted(
                Relation::Matched {
                    source: Source::Secondary(0),
                    keys: vec![record_pair(0, 0), record_pair(1, 1)],
                    group_by: vec![],
                },
                false,
            )),
            [G, S, S, G],
        ),
        (
            "a qualified aggregate of the driver input pairing S and U with their fields",
            column_shape(matched(Source::Primary, &[(0, 0), (1, 1)])),
            [G, S, S, G],
        ),
        (
            "a qualified aggregate of the driver input pairing U with the field P",
            column_shape(matched(Source::Primary, &[(0, 0), (2, 1)])),
            [G, S, G, G],
        ),
        (
            "a column window by S and U where the template derives U from a literal",
            keyed_plan(
                vec![copies(RowMode::Records, true)],
                vec![assign(4, numbered_by(vec![0, 1]))],
            ),
            [G, S, G, G],
        ),
        (
            "a named intermediate's read of SEC",
            column_shape(Expression::Intermediate {
                index: 0,
                column: 2,
            }),
            [S, S, S, S],
        ),
        (
            "a column window by S and U over the rows of a template driven by SEC too",
            keyed_plan(
                vec![copies(RowMode::Records, false), from_sec(None)],
                vec![assign(4, numbered_by(vec![0, 1]))],
            ),
            [G, G, G, G],
        ),
        (
            "a qualified aggregate of SEC keyed by S and U, for a template driven by SEC too",
            keyed_plan(
                vec![copies(RowMode::Records, false), from_sec(None)],
                vec![assign(4, matched(Source::Secondary(0), &[(0, 0), (1, 1)]))],
            ),
            [G, G, G, G],
        ),
        (
            "a row template's window by S and U beside a template driven by SEC",
            keyed_plan(
                vec![
                    {
                        let mut template = copies(RowMode::Records, false);
                        template
                            .assignments
                            .push(assign(4, numbered_by(vec![0, 1])));
                        template
                    },
                    from_sec(Some(Expression::Literal(Value::Int(0)))),
                ],
                vec![],
            ),
            [G, S, S, G],
        ),
    ];
    for (label, plan, expected) in cases {
        assert_eq!(classes_of_r(&plan), expected, "{label}");
    }
}

/// `ResolvedPlan::partition_fields`, a seam no run's output shows:
/// key-bounded partitions cut the driver input by the field each of their
/// keys copies (#3763), as many keys as the specification declares, and a
/// key a template derives any other way leaves nothing to cut by.
#[test]
fn partition_fields_are_the_fields_every_driver_template_copies_into_the_keys() {
    let copied = column_shape(numbered_by(vec![0, 1])).resolve().unwrap();
    let literal = keyed_plan(
        vec![copies(RowMode::Records, true)],
        vec![assign(4, numbered_by(vec![0, 1]))],
    )
    .resolve()
    .unwrap();
    let cases = [
        ("one key", &copied, 1, Some(vec![0])),
        ("two keys", &copied, 2, Some(vec![0, 1])),
        (
            "more keys than the three declared",
            &copied,
            5,
            Some(vec![0, 1, 2]),
        ),
        ("S before the literal U", &literal, 1, Some(vec![0])),
        ("the literal U", &literal, 2, None),
    ];
    for (label, plan, keys, expected) in cases {
        assert_eq!(plan.partition_fields(keys), expected, "{label}");
    }
}

/// The confinement gate (#3830), a seam no run's output shows: a run that
/// holds one partition at a time reads together only what one partition
/// holds, so the records a grouped template, the key grain or an ODM read
/// reads together must sit in one partition. Under consecutive partitions
/// the cut keeps whole the runs of the fields every such read shares
/// (`ResolvedPlan::cut_fields`), none for a plan without one, and a plan
/// whose reads share no field needs the whole table at the read that
/// leaves none. Under key-bounded partitions
/// the cut is by the partition fields, and a read whose fields omit one of
/// them, or any read when the driver input does not hold every partition
/// key, needs the whole table. Each expectation reads the keys [S, U, P]
/// (an ODM plan's [STUDYID, USUBJID]), the group fields, the fields the
/// keys copy and the ODM fields a read's scope names (`odm_source::FIELDS`);
/// a grouped template names its `group_by` (`rows.<row>.group_by`), the key
/// grain its `keys` and an ODM read its column's derivation.
#[test]
fn a_cross_record_read_is_confined_to_its_partition_or_needs_the_whole_table() {
    let grouped = |fields: Vec<usize>| copies(RowMode::Groups(fields), false);
    let named = |plan: DatasetPlan, rows: &[&str]| {
        let contexts = rows
            .iter()
            .map(|&row| {
                Some(crate::dataset::AggregateRowContext {
                    row: row.into(),
                    groups: Vec::new(),
                })
            })
            .collect();
        plan.with_row_contexts(contexts)
    };
    let literal = |column: usize| assign(column, Expression::Literal(Value::Str("k".into())));
    let key_grain = |assignments: Vec<Assignment>| RowTemplate {
        mode: RowMode::Keys,
        assignments,
        filter: None,
    };
    let sv = |fields: &[usize]| Some(fields.to_vec());
    // R, which no template derives, completes in the column phase.
    let r = || vec![assign(4, Expression::Literal(Value::Int(0)))];
    // An ODM input's record template copying StudyOID and SubjectKey into
    // the keys STUDYID and USUBJID, and VAL an ODM read of the input scoped
    // by `scope`.
    let odm = |scope: crate::dataset::OdmScope| {
        use crate::dataset::{AggregatePhase, AggregateSource, BoundOdm, OdmRead};
        let read = BoundOdm::new(OdmRead {
            dataset: "ODM".into(),
            item: "ODM.IT.VAL".into(),
            item_oid: "IT.VAL".into(),
            path: "columns.VAL.derivation.odm".into(),
            row: None,
            phase: AggregatePhase::Derivation,
            scope,
            source: Some(AggregateSource::Primary),
            selectors: [None, None, None],
            filter: None,
        })
        .unwrap();
        let fields: Vec<_> = crate::odm_source::FIELDS
            .iter()
            .map(|&name| (name, ColumnType::Str))
            .collect();
        let text = ColumnType::Str;
        DatasetPlan::new(
            schema(&fields),
            schema(&[("STUDYID", text), ("USUBJID", text), ("VAL", text)]),
            vec![RowTemplate {
                mode: RowMode::Records,
                assignments: vec![
                    assign(0, Expression::Source(0)),
                    assign(1, Expression::Source(2)),
                ],
                filter: None,
            }],
            vec![assign(2, Expression::Odm(Box::new(read)))],
            vec![0, 1],
            vec![],
        )
    };
    let hierarchy = || crate::dataset::OdmScope::Records;
    let events = || crate::dataset::OdmScope::Groups(vec![3]);
    // A label, the plan, its partition keys, the path of the step that
    // refuses it and the fields its partitions are cut by.
    type Case = (
        &'static str,
        DatasetPlan,
        usize,
        Option<&'static str>,
        Option<Vec<usize>>,
    );
    let cases: [Case; 16] = [
        (
            "a record template, consecutive",
            keyed_plan(vec![copies(RowMode::Records, false)], r()),
            0,
            None,
            sv(&[]),
        ),
        (
            "a template grouped by S and U, consecutive",
            named(keyed_plan(vec![grouped(vec![0, 1])], r()), &["a"]),
            0,
            None,
            sv(&[0, 1]),
        ),
        (
            "a template grouped by S and U, by two keys",
            named(keyed_plan(vec![grouped(vec![0, 1])], r()), &["a"]),
            2,
            None,
            sv(&[0, 1]),
        ),
        (
            "a template grouped by S and U, by three keys",
            named(keyed_plan(vec![grouped(vec![0, 1])], r()), &["a"]),
            3,
            Some("rows.a.group_by"),
            None,
        ),
        (
            "templates grouped by S and U and by S, consecutive",
            named(
                keyed_plan(vec![grouped(vec![0, 1]), grouped(vec![0])], r()),
                &["a", "b"],
            ),
            0,
            None,
            sv(&[0]),
        ),
        (
            "templates grouped by S and U and by S, by one key",
            named(
                keyed_plan(vec![grouped(vec![0, 1]), grouped(vec![0])], r()),
                &["a", "b"],
            ),
            1,
            None,
            sv(&[0]),
        ),
        (
            "templates grouped by S and U and by S, by two keys",
            named(
                keyed_plan(vec![grouped(vec![0, 1]), grouped(vec![0])], r()),
                &["a", "b"],
            ),
            2,
            Some("rows.a.group_by"),
            None,
        ),
        (
            "templates grouped by S and by U, consecutive",
            named(
                keyed_plan(vec![grouped(vec![0]), grouped(vec![1])], r()),
                &["a", "b"],
            ),
            0,
            Some("rows.b.group_by"),
            None,
        ),
        (
            "the key grain of S, U and P",
            keyed_plan(
                vec![key_grain(copies(RowMode::Keys, false).assignments)],
                r(),
            ),
            0,
            None,
            sv(&[0, 1, 2]),
        ),
        (
            "the key grain of S, a literal U, and P",
            keyed_plan(
                vec![key_grain(copies(RowMode::Keys, true).assignments)],
                r(),
            ),
            0,
            None,
            sv(&[0, 2]),
        ),
        (
            "the key grain of S, a literal U, and P, by two keys",
            keyed_plan(
                vec![key_grain(copies(RowMode::Keys, true).assignments)],
                r(),
            ),
            2,
            Some("keys"),
            None,
        ),
        (
            "the key grain of literal keys",
            keyed_plan(
                vec![key_grain(vec![
                    literal(0),
                    literal(1),
                    literal(2),
                    assign(3, Expression::Source(3)),
                ])],
                r(),
            ),
            0,
            Some("keys"),
            None,
        ),
        (
            "an ODM read scoped by its record's hierarchy, consecutive",
            odm(hierarchy()),
            0,
            None,
            sv(&[0, 2, 3, 4, 5, 6, 7, 8]),
        ),
        (
            "an ODM read scoped by its record's hierarchy, by two keys",
            odm(hierarchy()),
            2,
            None,
            sv(&[0, 2]),
        ),
        (
            "an ODM read scoped by StudyEventOID, consecutive",
            odm(events()),
            0,
            None,
            sv(&[3]),
        ),
        (
            "an ODM read scoped by StudyEventOID, by two keys",
            odm(events()),
            2,
            Some("columns.C2.derivation"),
            sv(&[0, 2]),
        ),
    ];
    for (label, plan, keys, refused, fields) in cases {
        let plan = plan.resolve().unwrap();
        assert_eq!(
            (
                plan.whole_table_step(keys)
                    .map(|step| plan.step_path(step).into_owned()),
                plan.cut_fields(keys)
            ),
            (refused.map(str::to_owned), fields),
            "{label}"
        );
    }
}
