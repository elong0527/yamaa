//! Portable producer causes retain authored geometry without study data.
use super::{Cause, Contract, Error};
use crate::json::Value as V;
use crate::{
    diagnostic::{ConditionCode as C, Diagnostic},
    value::ColumnType,
};
use std::vec;

pub(crate) fn text(value: &str) -> V {
    V::String(value.into())
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
                context.push(("declarations".into(), V::from(*count)));
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
    /// Compare caller-held artifact metadata without reading values or sources.
    pub fn validate_header(
        &self,
        dataset: &str,
        actual: &[&str],
    ) -> Result<Option<Diagnostic>, Error> {
        self.validate_metadata(dataset, actual, None)
    }
    pub fn validate_typed_fields(
        &self,
        dataset: &str,
        actual: &[(&str, ColumnType)],
    ) -> Result<Option<Diagnostic>, Error> {
        crate::limits::Limits::columns(actual.len()).map_err(Error::Limit)?;
        let names = actual.iter().map(|(name, _)| *name).collect::<Vec<_>>();
        self.validate_metadata(dataset, &names, Some(actual))
    }
    fn validate_metadata(
        &self,
        dataset: &str,
        actual: &[&str],
        typed: Option<&[(&str, ColumnType)]>,
    ) -> Result<Option<Diagnostic>, Error> {
        crate::limits::Limits::columns(actual.len()).map_err(Error::Limit)?;
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
        let sequence = |names: &[&str]| V::Array(names.iter().map(|name| text(name)).collect());
        let mut context = vec![
            ("dataset".into(), text(dataset)),
            ("expected".into(), sequence(&expected)),
            ("actual".into(), sequence(actual)),
            (
                "reordered".into(),
                V::Bool(missing.is_empty() && extra.is_empty() && actual != expected),
            ),
            ("missing".into(), sequence(&missing)),
            ("extra".into(), sequence(&extra)),
        ];
        if let Some(typed) = typed {
            context.push((
                "expected_types".into(),
                V::Array(self.fields.iter().map(|f| text(kind(f.kind))).collect()),
            ));
            context.push((
                "actual_types".into(),
                V::Array(typed.iter().map(|(_, k)| text(kind(*k))).collect()),
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
