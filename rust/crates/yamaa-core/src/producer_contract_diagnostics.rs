//! Portable producer causes retain authored geometry without study data.
use super::{Cause, Contract, Error, Limits};
use crate::{
    diagnostic::{ConditionCode as C, ContextValue as V, Diagnostic},
    value::{ColumnType, Value},
};
use alloc::{format, string::ToString, vec, vec::Vec};

pub(crate) fn text(value: &str) -> V {
    V::Scalar(Value::Str(value.into()))
}
pub(crate) fn kind(value: ColumnType) -> &'static str {
    match value {
        ColumnType::Str => "str",
        ColumnType::Int => "int",
        ColumnType::Float => "float",
        ColumnType::Date => "date",
        ColumnType::DateTime => "datetime",
    }
}
impl Cause {
    pub fn diagnostic(&self, dataset: &str) -> Diagnostic {
        let prefix = format!("input.{dataset}.schema");
        let mut context = vec![("dataset".into(), text(dataset))];
        let path = match self {
            Self::Empty => {
                context.push(("reason".into(), text("empty")));
                format!("{prefix}.output.columns")
            }
            Self::Duplicate { position, field } => {
                context.push(("field".into(), text(field)));
                context.push(("reason".into(), text("duplicate")));
                format!("{prefix}.output.columns[{position}]")
            }
            Self::Declaration {
                position,
                field,
                count,
            } => {
                context.push(("field".into(), text(field)));
                context.push(("declarations".into(), V::Integer(count.to_string())));
                format!("{prefix}.output.columns[{position}]")
            }
            Self::Label { field } => {
                context.push(("field".into(), text(field)));
                context.push(("reason".into(), text("label")));
                format!("{prefix}.columns.{field}.label")
            }
        };
        Diagnostic {
            code: C::ProducerInvalidContract,
            spec_paths: vec![path],
            context: context.into_iter().collect(),
            source_span: None,
            operand_route: None,
        }
    }
}
impl Contract {
    /// Compare caller-held artifact metadata only. Quotas precede diagnostic
    /// allocation; values, source capture and type inference are unavailable.
    pub fn validate_header(
        &self,
        dataset: &str,
        actual: &[&str],
        limits: Limits,
    ) -> Result<Option<Diagnostic>, Error> {
        self.validate_metadata(dataset, actual, None, limits)
    }
    pub fn validate_typed_fields(
        &self,
        dataset: &str,
        actual: &[(&str, ColumnType)],
        limits: Limits,
    ) -> Result<Option<Diagnostic>, Error> {
        if actual.len() > limits.fields {
            return Err(Error::Limit("producer_actual_fields"));
        }
        let names = actual.iter().map(|(name, _)| *name).collect::<Vec<_>>();
        self.validate_metadata(dataset, &names, Some(actual), limits)
    }
    fn validate_metadata(
        &self,
        dataset: &str,
        actual: &[&str],
        typed: Option<&[(&str, ColumnType)]>,
        limits: Limits,
    ) -> Result<Option<Diagnostic>, Error> {
        if actual.len() > limits.fields || self.fields.len() > limits.fields {
            return Err(Error::Limit("producer_actual_fields"));
        }
        let mut bytes = dataset.len();
        for value in self
            .fields
            .iter()
            .map(|f| f.name.as_str())
            .chain(actual.iter().copied())
        {
            bytes = bytes
                .checked_add(value.len())
                .filter(|&n| n <= limits.text_bytes)
                .ok_or(Error::Limit("producer_actual_text_bytes"))?;
        }
        if bytes > limits.text_bytes {
            return Err(Error::Limit("producer_actual_text_bytes"));
        }
        if self.matches_header(actual) && typed.is_none_or(|typed| self.matches_typed_fields(typed))
        {
            return Ok(None);
        }
        let expected = self
            .fields
            .iter()
            .map(|f| f.name.as_str())
            .collect::<Vec<_>>();
        let missing = expected
            .iter()
            .copied()
            .filter(|name| !actual.contains(name))
            .collect::<Vec<_>>();
        let extra = actual
            .iter()
            .copied()
            .filter(|name| !expected.contains(name))
            .collect::<Vec<_>>();
        let sequence = |names: &[&str]| V::Sequence(names.iter().map(|name| text(name)).collect());
        let mut context = vec![
            ("dataset".into(), text(dataset)),
            ("expected".into(), sequence(&expected)),
            ("actual".into(), sequence(actual)),
            (
                "reordered".into(),
                V::Scalar(Value::Bool(
                    missing.is_empty() && extra.is_empty() && actual != expected,
                )),
            ),
            ("missing".into(), sequence(&missing)),
            ("extra".into(), sequence(&extra)),
        ];
        if let Some(typed) = typed {
            context.push((
                "expected_types".into(),
                V::Sequence(self.fields.iter().map(|f| text(kind(f.kind))).collect()),
            ));
            context.push((
                "actual_types".into(),
                V::Sequence(typed.iter().map(|(_, k)| text(kind(*k))).collect()),
            ));
        }
        Ok(Some(Diagnostic {
            code: C::ProducerContractMismatch,
            spec_paths: vec![
                format!("input.{dataset}.schema"),
                format!("input.{dataset}.path"),
            ],
            context: context.into_iter().collect(),
            source_span: None,
            operand_route: None,
        }))
    }
}
