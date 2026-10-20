//! Rules of the 1.0 surface that its schema cannot state, checked on the
//! lowered tree before any input is read. Their findings belong to
//! validation, as the schema's do, and name the 1.0 paths (P-CLR-2).

use crate::ast::{Direction, ExprId, ExprKind, Spec, Window, WindowFunction};
use crate::diagnostic::ConditionCode;
use crate::schema::{SchemaContext as C, SchemaDiagnostic};

/// The findings of a lowered 1.0 specification, in written order.
pub fn findings(spec: &Spec) -> Vec<SchemaDiagnostic> {
    let mut findings = Vec::new();
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
