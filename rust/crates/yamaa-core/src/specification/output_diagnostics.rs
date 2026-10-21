//! Output-declaration findings retain their validation vocabulary at the cause.
use super::{OutputFinding, MAX_OUTPUT_DECIMALS};
use crate::diagnostic::{ConditionCode as C, Diagnostic};
use crate::json::Value as V;

/// `unknown_artifact_profile` (OUTPUT-5) for `path`, whose extension the
/// closed mapping does not hold, at `spec_paths`: `output.path` or
/// `output.report` for a specification's file, none for a table saved to a
/// path its caller names. Its context names the path as written and the
/// extensions the mapping holds.
pub fn unknown_artifact_profile(path: &str, spec_paths: Vec<String>) -> Diagnostic {
    Diagnostic {
        code: C::OutputUnknownProfile,
        spec_paths,
        context: [
            ("path".into(), V::String(path.into())),
            (
                "permitted".into(),
                V::Array(
                    [".csv", ".parquet"]
                        .into_iter()
                        .map(|s| V::String(s.into()))
                        .collect(),
                ),
            ),
        ]
        .into(),
        source_span: None,
        operand_route: None,
    }
}

impl OutputFinding {
    /// This represents the existing finding. The caller still reaches output
    /// declaration checks only after derivation, key checks and verification.
    pub fn diagnostic(&self) -> Diagnostic {
        let (code, path, fields) = match self {
            Self::UnknownProfile { path } => {
                return unknown_artifact_profile(path, vec!["output.path".into()])
            }
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
