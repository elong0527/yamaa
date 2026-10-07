//! Output-declaration findings retain their validation vocabulary at the cause.
use super::OutputFinding;
use crate::{
    diagnostic::{ConditionCode as C, ContextValue as V, Diagnostic},
    value::Value,
};
use alloc::{format, vec};

impl OutputFinding {
    /// This represents the existing finding. The caller still reaches output
    /// declaration checks only after derivation, key checks and verification.
    pub fn diagnostic(&self) -> Diagnostic {
        let (code, path, fields) = match self {
            Self::UnknownProfile { path } => (
                C::OutputUnknownProfile,
                "output.path".into(),
                vec![
                    ("path", V::Scalar(Value::Str(path.clone()))),
                    (
                        "permitted",
                        V::Sequence(
                            [".csv", ".parquet"]
                                .into_iter()
                                .map(|s| V::Scalar(Value::Str(s.into())))
                                .collect(),
                        ),
                    ),
                ],
            ),
            Self::DuplicateColumn { position, name } => (
                C::OutputDuplicateColumn,
                format!("output.columns[{position}]"),
                vec![("column", V::Scalar(Value::Str(name.clone())))],
            ),
            Self::UndeclaredColumn { position, name } => (
                C::OutputUndeclaredColumn,
                format!("output.columns[{position}]"),
                vec![("column", V::Scalar(Value::Str(name.clone())))],
            ),
            Self::InternalKey { position, name } => (
                C::OutputInternalKey,
                format!("keys[{position}]"),
                vec![("column", V::Scalar(Value::Str(name.clone())))],
            ),
        };
        Diagnostic {
            code,
            spec_paths: vec![path],
            context: fields.into_iter().map(|(k, v)| (k.into(), v)).collect(),
            source_span: None,
            operand_route: None,
        }
    }
}
