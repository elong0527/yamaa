//! Rules of the 1.0 surface that its schema cannot state, checked on the
//! lowered tree before any input is read. Their findings belong to
//! validation, as the schema's do, and name the 1.0 paths (P-CLR-2).
//!
//! [`unbuilt`] lists what a 1.0 specification writes that the engine does not
//! build yet; each is refused as `unsupported_operation` (COLUMN-15) until its
//! pull request lands (#3030 D76).

use crate::ast::{
    Comparator, Direction, ExprId, ExprKind, Profile, ScalarOp, Spec, SubmissionMetadata, Window,
    WindowFunction,
};
use crate::diagnostic::ConditionCode;
use crate::schema::{SchemaContext as C, SchemaDiagnostic};
use crate::specification::UnsupportedFeature;

/// More than one origin on a column or a value-level entry (P-ORIGIN-1),
/// refused until P1-13 builds them.
pub const MULTIPLE_ORIGINS: &str = "multiple_origins";
/// A dataset's `submission.standard` (P-STD-1), refused until P1-13.
pub const SUBMISSION_STANDARD: &str = "submission_standard";
/// A value-level `where` comparator, NE or NOTIN (P-VLM-1), refused until
/// P1-14.
pub const VALUE_LEVEL_COMPARATOR: &str = "value_level_comparator";
/// An input whose path names an ODM XML file (P-ODMIN-1), refused until
/// P1-15.
pub const ODM_XML_INPUT: &str = "odm_xml_input";

fn unsupported(operation: &str, path: String) -> UnsupportedFeature {
    UnsupportedFeature {
        operation: operation.into(),
        path,
    }
}

/// What a lowered 1.0 specification writes that the engine does not build
/// yet, in written order: an ODM XML input; more than one origin; a
/// value-level comparator; and a dataset's `standard`.
pub fn unbuilt(spec: &Spec) -> Vec<UnsupportedFeature> {
    let mut found = Vec::new();
    for input in &spec.inputs {
        if input.source.profile == Some(Profile::OdmXml) {
            found.push(unsupported(
                ODM_XML_INPUT,
                spec.cite(input.source.path.meta),
            ));
        }
    }
    let origins = |metadata: &SubmissionMetadata, at: String, found: &mut Vec<_>| {
        if metadata.origins.len() > 1 {
            found.push(unsupported(MULTIPLE_ORIGINS, format!("{at}.origins")));
        }
    };
    for column in &spec.columns {
        let Some(submission) = &column.submission else {
            continue;
        };
        origins(&submission.metadata, spec.cite(submission.meta), &mut found);
        for entry in &submission.value_level {
            for clause in &entry.conditions {
                if clause.comparator == Comparator::NotIn {
                    found.push(unsupported(
                        VALUE_LEVEL_COMPARATOR,
                        format!("{}.comparator", spec.cite(clause.meta)),
                    ));
                }
            }
            origins(&entry.metadata, spec.cite(entry.meta), &mut found);
        }
    }
    if let Some(standard) = spec.submission.as_ref().and_then(|s| s.standard.as_ref()) {
        found.push(unsupported(SUBMISSION_STANDARD, spec.cite(standard.meta)));
    }
    found
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
