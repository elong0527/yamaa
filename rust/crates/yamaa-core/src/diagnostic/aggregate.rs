//! Aggregate causes retain the authored expression and enclosing row grain.
use super::{ConditionCode as C, Diagnostic, ProjectionError};
use crate::json::Value as V;
use crate::{
    dataset::AggregatePhase,
    expression::Reducer,
    result_type::ResultType,
    value::{ColumnType, Value, ValueType},
};

#[derive(Clone, Copy, Debug)]
pub enum AggregateFinding<'a> {
    MissingKeys {
        path: &'a str,
        dataset: &'a str,
    },
    MultipleRecords {
        path: &'a str,
        expression: &'a str,
        phase: AggregatePhase,
        count: usize,
        row: Option<&'a str>,
        group: &'a [(String, Value)],
    },
    IdentifierNotGrouped {
        path: &'a str,
        expression: &'a str,
        dataset: Option<&'a str>,
        identifier: &'a str,
    },
    IncomparableSources {
        path: &'a str,
        expression: &'a str,
        source: &'a str,
        types: &'a [ValueType],
    },
    InvalidContext {
        path: &'a str,
        expression: &'a str,
        cause: AggregateContextCause,
    },
    MixedRelations {
        path: &'a str,
        expression: &'a str,
        relations: &'a [String],
    },
    /// A declared `key` column the relation does not have.
    DonorKeyReference {
        path: &'a str,
        identifier: &'a str,
    },
    /// A match value that reads no known current-row value.
    MatchValueReference {
        path: &'a str,
        identifier: &'a str,
    },
    KeyType {
        path: &'a str,
        source: &'a str,
        expected: ResultType,
        actual: ColumnType,
    },
    /// An input without `key`, which a keyless qualified aggregate over it
    /// would match on (LOOKUP-5).
    UndeclaredInputKey {
        path: &'a str,
        dataset: &'a str,
        /// The input is a pair relation, which declares no `key` (LOOKUP-27).
        pair: bool,
    },
}
#[derive(Clone, Copy, Debug)]
pub enum AggregateContextCause {
    RowOwnsGroups,
    RowNoRightSide,
    RowNoKeys,
    UngroupedRow,
    OutputNeedsGroups,
    OutputGroupsQualified,
    OutputBetween,
    QualifiedGroupNotOutputKey,
}
impl AggregateContextCause {
    fn reason(self) -> &'static str {
        match self {
            Self::RowOwnsGroups => "the enclosing row.group_by owns the keys",
            Self::RowNoRightSide => "a grouped row aggregate has no separate right side",
            Self::RowNoKeys => {
                "a grouped row aggregate reads its own group and declares no key pairs"
            }
            Self::UngroupedRow => "an ungrouped row template has no aggregate context",
            Self::OutputNeedsGroups => "an output-row reduction declares group_by",
            Self::OutputGroupsQualified => {
                "an output-row reduction groups on current-output columns"
            }
            Self::OutputBetween => "between narrows a qualified right side only",
            Self::QualifiedGroupNotOutputKey => {
                "a qualified aggregate group_by column must also be an output key"
            }
        }
    }
    fn code(self) -> C {
        match self {
            Self::RowNoKeys => C::AggregateOwnGroupKeys,
            Self::OutputNeedsGroups => C::AggregateOutputGroups,
            Self::QualifiedGroupNotOutputKey => C::AggregateGroupNotOutputKey,
            _ => C::AggregateDriverScope,
        }
    }
}
fn text(value: &str) -> V {
    V::String(value.into())
}
fn type_name(value: ValueType) -> &'static str {
    match value {
        ValueType::Str => "str",
        ValueType::Int => "int",
        ValueType::Float => "float",
        ValueType::Bool => "bool",
        ValueType::Date => "date",
        ValueType::DateTime => "datetime",
    }
}
impl AggregateFinding<'_> {
    pub fn diagnostic(self) -> Result<Diagnostic, ProjectionError> {
        // A qualified aggregate's match shares its causes with every other
        // lookup (LOOKUP-25).
        use super::lookup::{self, Owner};
        match self {
            Self::MissingKeys { path, dataset } => {
                return Ok(lookup::empty_key(path, Owner::Unnamed, dataset))
            }
            Self::DonorKeyReference { path, identifier } => {
                return Ok(lookup::donor_key_reference(
                    path,
                    Owner::Unnamed,
                    identifier,
                ))
            }
            Self::MatchValueReference { path, identifier } => {
                return Ok(lookup::match_value_reference(
                    path,
                    Owner::Unnamed,
                    identifier,
                ))
            }
            Self::KeyType {
                path,
                source,
                expected,
                actual,
            } => {
                return Ok(lookup::key_type(
                    path,
                    Owner::Unnamed,
                    source,
                    expected,
                    actual.into(),
                ))
            }
            Self::UndeclaredInputKey {
                path,
                dataset,
                pair,
            } => {
                return Ok(lookup::undeclared_input_key(path, dataset, true, pair));
            }
            _ => {}
        }
        let (path, expression) = match self {
            Self::MultipleRecords {
                path,
                expression,
                count,
                row,
                group,
                phase,
            } => {
                if count < 2
                    || (row.is_none() && !group.is_empty())
                    || (matches!(phase, AggregatePhase::RowConstruction) && row.is_none())
                {
                    return Err(ProjectionError::InvalidContext);
                }
                if row.is_some() {
                    for (index, (name, _)) in group.iter().enumerate() {
                        if group[..index].iter().any(|(prior, _)| prior == name) {
                            return Err(ProjectionError::InvalidContext);
                        }
                    }
                }
                (path, expression)
            }
            Self::IncomparableSources {
                path,
                expression,
                types,
                ..
            } => {
                if !types.iter().enumerate().any(|(index, left)| {
                    types[index + 1..].iter().any(|right| {
                        left != right
                            && !(matches!(left, ValueType::Int | ValueType::Float)
                                && matches!(right, ValueType::Int | ValueType::Float))
                    })
                }) {
                    return Err(ProjectionError::InvalidContext);
                }
                (path, expression)
            }
            Self::IdentifierNotGrouped {
                path, expression, ..
            }
            | Self::InvalidContext {
                path, expression, ..
            }
            | Self::MixedRelations {
                path, expression, ..
            } => (path, expression),
            Self::MissingKeys { .. }
            | Self::DonorKeyReference { .. }
            | Self::MatchValueReference { .. }
            | Self::KeyType { .. }
            | Self::UndeclaredInputKey { .. } => {
                unreachable!("key causes projected above")
            }
        };
        let mut context: super::Context = [("expr".into(), text(expression))].into();
        let code = match self {
            Self::MultipleRecords {
                phase,
                count,
                row,
                group,
                ..
            } => {
                context.insert("reducer".into(), text(Reducer::Only.name()));
                context.insert("record_count".into(), V::from(count));
                if let Some(row) = row {
                    context.insert("row".into(), text(row));
                    context.insert(
                        "group".into(),
                        V::Object(
                            group
                                .iter()
                                .map(|(name, value)| (name.clone(), V::from(value.clone())))
                                .collect(),
                        ),
                    );
                }
                match phase {
                    AggregatePhase::RowConstruction => C::AggregateRowMultipleRecords,
                    AggregatePhase::Derivation => C::AggregateMultipleRecords,
                }
            }
            Self::IdentifierNotGrouped {
                dataset,
                identifier,
                ..
            } => {
                context.insert("dataset".into(), dataset.map_or(V::Null, text));
                context.insert("identifier".into(), text(identifier));
                C::AggregateIdentifierNotGrouped
            }
            Self::IncomparableSources { source, types, .. } => {
                context.insert("sources".into(), V::Array(vec![text(source)]));
                let mut names = types
                    .iter()
                    .map(|kind| type_name(*kind))
                    .collect::<std::vec::Vec<_>>();
                names.sort_unstable();
                names.dedup();
                context.insert(
                    "types".into(),
                    V::Array(names.into_iter().map(text).collect()),
                );
                C::AggregateIncomparableSources
            }
            Self::InvalidContext { cause, .. } => {
                context.insert("reason".into(), text(cause.reason()));
                cause.code()
            }
            Self::MixedRelations { relations, .. } => {
                context.insert(
                    "relations".into(),
                    V::Array(relations.iter().map(|relation| text(relation)).collect()),
                );
                C::AggregateMixedRelations
            }
            Self::MissingKeys { .. }
            | Self::DonorKeyReference { .. }
            | Self::MatchValueReference { .. }
            | Self::KeyType { .. }
            | Self::UndeclaredInputKey { .. } => {
                unreachable!("key causes projected above")
            }
        };
        Ok(Diagnostic {
            code,
            spec_paths: vec![path.into()],
            context,
            source_span: None,
            operand_route: None,
        })
    }
}
