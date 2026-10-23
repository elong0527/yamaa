//! Rules of the 1.0 surface that its schema cannot state, checked on the
//! lowered tree before any input is read. Their findings belong to
//! validation, as the schema's do, and name the 1.0 paths (P-CLR-2).

use crate::ast::{
    Direction, ExprId, ExprKind, Origin, Profile, ScalarOp, Severity, Spec, SubmissionMetadata,
    Window, WindowFunction,
};
use crate::diagnostic::ConditionCode;
use crate::schema::{SchemaContext as C, SchemaDiagnostic};

/// SUB-14 (D69): each origin of `metadata` that conflicts with an earlier one,
/// at its own position: one with the same type and source as an earlier
/// origin, and any origin beside a `Predecessor` or `Not Available` one,
/// which stands alone.
fn conflicting_origins(
    spec: &Spec,
    metadata: &SubmissionMetadata,
    column: &str,
    entry: Option<&str>,
    findings: &mut Vec<SchemaDiagnostic>,
) {
    fn exclusive(origin: &Origin) -> bool {
        matches!(origin.kind.text.as_str(), "Predecessor" | "Not Available")
    }
    fn source(origin: &Origin) -> Option<&str> {
        origin.source.as_ref().map(|source| source.text.as_str())
    }
    for (index, origin) in metadata.origins.iter().enumerate() {
        let earlier = &metadata.origins[..index];
        let reason = if earlier
            .iter()
            .any(|other| other.kind.text == origin.kind.text && source(other) == source(origin))
        {
            "same_type_and_source"
        } else if !earlier.is_empty() && (exclusive(origin) || earlier.iter().any(exclusive)) {
            "exclusive_type"
        } else {
            continue;
        };
        let mut context = vec![("column", C::Text(column.into()))];
        context.extend(entry.map(|entry| ("entry", C::Text(entry.into()))));
        context.push(("origin_type", C::Text(origin.kind.text.clone())));
        context.extend(source(origin).map(|source| ("origin_source", C::Text(source.into()))));
        context.push(("reason", C::Text(reason.into())));
        findings.push(SchemaDiagnostic {
            path: spec.cite(origin.meta),
            code: ConditionCode::SubmissionConflictingOrigins,
            context,
        });
    }
}

/// The findings of a lowered 1.0 specification, in written order.
pub fn findings(spec: &Spec) -> Vec<SchemaDiagnostic> {
    let mut findings = Vec::new();
    // A specification that declares root or column submission metadata
    // names the model its dataset follows (SUB-3, D25); one without it, such
    // as a language test, follows none. The record is the schema's own for a
    // missing required field.
    let declares_submission = spec.submission.is_some()
        || spec
            .columns
            .iter()
            .any(|column| column.submission.is_some());
    if declares_submission && spec.model.is_none() {
        findings.push(SchemaDiagnostic {
            path: "model".into(),
            code: ConditionCode::SubmissionModelRequired,
            context: vec![
                ("field", C::Text("model".into())),
                ("class", C::Text("root_class".into())),
            ],
        });
    }
    for column in &spec.columns {
        let Some(submission) = &column.submission else {
            continue;
        };
        let name = column.name.name.as_str();
        conflicting_origins(spec, &submission.metadata, name, None, &mut findings);
        for entry in &submission.value_level {
            conflicting_origins(
                spec,
                &entry.metadata,
                name,
                Some(&entry.id.name),
                &mut findings,
            );
        }
    }
    // Only the csv profile reads a byte-order mark (CSV-10). An input of
    // another profile declares none, whatever the value written; an input
    // whose extension selects no profile leaves its fault to
    // `source_profile_unknown` (INPUT-1).
    for input in &spec.inputs {
        let profile = input.source.profile;
        if input.source.written.contains("byte_order_mark")
            && profile.is_some_and(|profile| profile != Profile::Csv)
        {
            findings.push(SchemaDiagnostic {
                path: format!("{}.byte_order_mark", spec.cite(input.meta)),
                code: ConditionCode::CsvByteOrderMarkProfile,
                context: vec![("dataset", C::Text(input.name.name.clone()))],
            });
        }
    }
    // An intermediate states its own `key`, unless it drives a row template,
    // which declares none (LOOKUP-3, ROW-2; D23, D63). The record is the
    // schema's own for a missing required field.
    for item in &spec.intermediates {
        if spec.lacks_key(item) {
            findings.push(SchemaDiagnostic {
                path: format!("{}.key", spec.cite(item.meta)),
                code: ConditionCode::IntermediateKeyRequired,
                context: vec![
                    ("field", C::Text("key".into())),
                    ("class", C::Text("intermediate_class".into())),
                ],
            });
        }
        // A `unique` may warn only where a selection chooses the record
        // every read takes: `keep` and `order_by`, or `nearest` (LOOKUP-17,
        // D210, LOOKUP-26); an `order_by` with no term orders nothing.
        let chooses = item.keep.is_some() && !item.order_by.is_empty() || item.nearest.is_some();
        for check in &item.checks {
            if check.severity == Severity::Warning && !chooses {
                findings.push(SchemaDiagnostic {
                    path: format!("{}.severity", spec.cite(check.meta)),
                    code: ConditionCode::IntermediateUniqueSeverity,
                    context: vec![
                        ("intermediate", C::Text(item.id.name.clone())),
                        ("value", C::Text("warning".into())),
                        ("permitted", C::TextList(vec!["error".into()])),
                    ],
                });
            }
        }
    }
    let name = |id: ExprId| match &spec.expr(id).kind {
        ExprKind::Ref(name) => name.as_str().to_string(),
        _ => String::new(),
    };
    for expr in &spec.exprs {
        // `baseline_flag`'s declared order starts with its date ascending,
        // and its later terms break ties (WINDOW-5); the lowering keeps any
        // other declared order as written, for this refusal.
        if let ExprKind::Window(Window {
            function:
                WindowFunction::BaselineFlag {
                    window_order_by, ..
                },
            order_by,
            ..
        }) = &expr.kind
        {
            if let (Some(first), Some(date)) = (window_order_by.first(), order_by.first()) {
                let direction = match first.direction {
                    Direction::Asc => "asc",
                    Direction::Desc => "desc",
                };
                findings.push(SchemaDiagnostic {
                    path: spec.cite(first.meta),
                    code: ConditionCode::WindowBaselineOrder,
                    context: vec![
                        ("variable", C::Text(name(first.variable))),
                        ("direction", C::Text(direction.into())),
                        ("date", C::Text(name(date.variable))),
                    ],
                });
            }
        }
        // A declared `month` requires a declared `day`: a source missing its
        // month also misses its day, so the month alone completes no date
        // (TIME-10).
        if let ExprKind::Scalar {
            op:
                ScalarOp::Impute {
                    month: Some(_),
                    day: None,
                    ..
                },
            ..
        } = &expr.kind
        {
            findings.push(SchemaDiagnostic {
                path: spec.cite(expr.meta),
                code: ConditionCode::ImputeMonthWithoutDay,
                context: vec![("field", C::Text("day".into()))],
            });
        }
        let ExprKind::Call { function, args, .. } = &expr.kind else {
            continue;
        };
        // A call argument is one leaf of the grammar: a name, quoted text, a
        // number, a temporal literal or missing, never an operator, a call
        // or a formula (FUNC-6). Text that is no leaf lowers to the formula
        // it is.
        for (argument, value) in args {
            if !matches!(
                spec.expr(*value).kind,
                ExprKind::Ref(_) | ExprKind::Literal(_)
            ) {
                findings.push(SchemaDiagnostic {
                    path: spec.cite(argument.meta),
                    code: ConditionCode::FunctionArgumentLeaf,
                    context: vec![
                        ("function", C::Text(function.name.clone())),
                        ("argument", C::Text(argument.name.clone())),
                        ("cause", C::Text("expression".into())),
                    ],
                });
            }
        }
    }
    findings
}
