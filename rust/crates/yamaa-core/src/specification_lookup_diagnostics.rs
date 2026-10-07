//! Core-owned vocabulary and context for named-selection compilation failures.
use super::*;
use crate::{diagnostic::Definition, predicate_parser::GrammarFailure, value::Value};

#[derive(Debug)]
pub struct LookupFinding {
    pub definition: Definition,
    pub path: String,
    pub context: BTreeMap<String, Value>,
}

impl LookupFinding {
    fn new(path: &str, condition: &'static str, requirement: &'static str) -> Self {
        Self {
            definition: Definition {
                phase: "validation",
                condition,
                requirement: Some(requirement),
            },
            path: path.into(),
            context: BTreeMap::new(),
        }
    }
    pub(super) fn reference(
        path: &str,
        name: &str,
        intermediate: Option<&str>,
        requirement: &'static str,
        suggestion: Option<String>,
    ) -> Self {
        let mut finding = Self::new(path, "unknown_field", requirement);
        finding
            .context
            .insert("identifier".into(), Value::Str(name.into()));
        if let Some(name) = intermediate {
            finding
                .context
                .insert("intermediate".into(), Value::Str(name.into()));
        }
        if let Some(name) = suggestion {
            finding
                .context
                .insert("suggestion".into(), Value::Str(name));
        }
        finding
    }
    pub(super) fn key_type(
        path: &str,
        intermediate: &str,
        source: &str,
        expected: ColumnType,
        actual: ColumnType,
    ) -> Self {
        let mut finding = Self::new(path, "incompatible_input_type", "REQ-0323");
        let name = |kind| match kind {
            ColumnType::Str => "str",
            ColumnType::Int => "int",
            ColumnType::Float => "float",
            ColumnType::Date => "date",
            ColumnType::DateTime => "datetime",
        };
        for (key, value) in [
            ("intermediate", intermediate),
            ("source", source),
            ("expected", name(expected)),
            ("actual", name(actual)),
        ] {
            finding.context.insert(key.into(), Value::Str(value.into()));
        }
        finding
    }
    pub(super) fn grammar(path: &str, text: &str, position: usize, error: &GrammarFailure) -> Self {
        let mut finding = Self::new(path, error.condition(), error.requirement());
        finding
            .context
            .insert("predicate".into(), Value::Str(text.into()));
        finding
            .context
            .insert("position".into(), Value::Int(position as i64));
        match error {
            GrammarFailure::InvalidExpression | GrammarFailure::InvalidEscape => {}
            GrammarFailure::InvalidRegex { byte, reason } => {
                finding
                    .context
                    .insert("pattern_byte".into(), Value::Int(*byte as i64));
                finding
                    .context
                    .insert("reason".into(), Value::Str((*reason).into()));
            }
            GrammarFailure::InvalidTemporal { kind, error } => {
                use crate::{predicate_parser::TemporalKind, temporal::TemporalError};
                finding.context.insert(
                    "literal_type".into(),
                    Value::Str(
                        match kind {
                            TemporalKind::Date => "date",
                            TemporalKind::DateTime => "datetime",
                        }
                        .into(),
                    ),
                );
                finding.context.insert(
                    "temporal_error".into(),
                    Value::Str(
                        match error {
                            TemporalError::InvalidForm => "invalid_form",
                            TemporalError::InvalidDate => "invalid_date",
                            TemporalError::InvalidTime => "invalid_time",
                        }
                        .into(),
                    ),
                );
            }
        }
        finding
    }
}
