//! Pure presence checks over completed values; effect order belongs to the engine.
use crate::{
    diagnostic::{ConditionCode, ContextValue, Definition, Diagnostic},
    value::Value,
};
use alloc::{
    string::{String, ToString},
    vec,
    vec::Vec,
};

/// Full offending positions retain row order; hosts may bound their displayed keys.
pub struct NotMissingResult {
    pub offending_rows: Vec<usize>,
}

pub fn not_missing<'a>(values: impl IntoIterator<Item = &'a Value>) -> NotMissingResult {
    NotMissingResult {
        offending_rows: values
            .into_iter()
            .enumerate()
            .filter_map(|(row, value)| matches!(value, Value::Missing).then_some(row))
            .collect(),
    }
}

pub fn not_missing_definition() -> Definition {
    ConditionCode::VerificationNotMissingFailed.definition()
}

/// Preserve core-owned vocabulary and base context without inventing host keys.
pub fn not_missing_diagnostic(path: String, column: String, failures: usize) -> Option<Diagnostic> {
    (failures != 0).then(|| Diagnostic {
        code: ConditionCode::VerificationNotMissingFailed,
        spec_paths: vec![path],
        context: [
            ("column".into(), ContextValue::Scalar(Value::Str(column))),
            (
                "failure_count".into(),
                ContextValue::Integer(failures.to_string()),
            ),
        ]
        .into(),
        source_span: None,
        operand_route: None,
    })
}

impl NotMissingResult {
    pub fn diagnostic(&self, path: String, column: String) -> Option<Diagnostic> {
        not_missing_diagnostic(path, column, self.offending_rows.len())
    }
}
