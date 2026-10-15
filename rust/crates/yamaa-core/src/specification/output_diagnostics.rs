//! Output-declaration findings retain their validation vocabulary at the cause.
use super::{OutputFinding, MAX_OUTPUT_DECIMALS};
use crate::diagnostic::{ConditionCode as C, Diagnostic};
use serde_json::Value as V;

impl OutputFinding {
    /// This represents the existing finding. The caller still reaches output
    /// declaration checks only after derivation, key checks and verification.
    pub fn diagnostic(&self) -> Diagnostic {
        let (code, path, fields) = match self {
            Self::UnknownProfile { path } => (
                C::OutputUnknownProfile,
                "output.path".into(),
                vec![
                    ("path", V::String(path.clone())),
                    (
                        "permitted",
                        V::Array(
                            [".csv", ".parquet"]
                                .into_iter()
                                .map(|s| V::String(s.into()))
                                .collect(),
                        ),
                    ),
                ],
            ),
            Self::InvalidDecimals { value } => (
                C::OutputInvalidDecimals,
                "output.decimals".into(),
                vec![
                    (
                        "expected",
                        V::String(format!("an integer from 0 through {MAX_OUTPUT_DECIMALS}")),
                    ),
                    (
                        "actual",
                        V::Number((value.clone()).parse().expect("canonical integer")),
                    ),
                ],
            ),
            Self::DecimalsNotApplicable { path } => (
                C::OutputDecimalsNotApplicable,
                "output.decimals".into(),
                vec![
                    ("path", V::String(path.clone())),
                    ("profile", V::String("parquet".into())),
                ],
            ),
            Self::DuplicateColumn { position, name } => (
                C::OutputDuplicateColumn,
                format!("output.columns[{position}]"),
                vec![("column", V::String(name.clone()))],
            ),
            Self::UndeclaredColumn { position, name } => (
                C::OutputUndeclaredColumn,
                format!("output.columns[{position}]"),
                vec![("column", V::String(name.clone()))],
            ),
            Self::InternalKey { position, name } => (
                C::OutputInternalKey,
                format!("keys[{position}]"),
                vec![("column", V::String(name.clone()))],
            ),
            Self::DuplicateOrderTerm { position, name } => (
                C::OutputDuplicateOrderTerm,
                format!("output.order_by[{position}]"),
                vec![("column", V::String(name.clone()))],
            ),
            Self::UndeclaredOrderColumn { position, name } => (
                C::OutputUndeclaredOrderColumn,
                format!("output.order_by[{position}]"),
                vec![("column", V::String(name.clone()))],
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
