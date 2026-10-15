//! Aggregate causes retain the authored expression and enclosing row grain.
use super::{ConditionCode as C, Diagnostic, ProjectionError};
use crate::{
    dataset::AggregatePhase,
    key_relation::ComparableType,
    value::{ColumnType, Value, ValueType},
};
use serde_json::Value as V;

#[derive(Clone, Copy, Debug)]
pub enum AggregateFinding<'a> {
    MissingKeys {
        path: &'a str,
        dataset: &'a str,
        key: AggregateEmptyKey,
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
    KeyReference {
        path: &'a str,
        identifier: &'a str,
    },
    KeyType {
        path: &'a str,
        source: &'a str,
        expected: ComparableType,
        actual: ColumnType,
        inferred: bool,
    },
    NoApplicableKeys {
        path: &'a str,
        dataset: &'a str,
        keys: &'a [String],
    },
}
#[derive(Clone, Copy, Debug)]
pub enum AggregateEmptyKey {
    Mapping,
    List,
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
fn comparable_name(kind: ComparableType) -> &'static str {
    match kind {
        ComparableType::Boolean => "bool",
        ComparableType::Column(kind) => match kind {
            ColumnType::Str => "str",
            ColumnType::Int => "int",
            ColumnType::Float => "float",
            ColumnType::Date => "date",
            ColumnType::DateTime => "datetime",
        },
    }
}
impl AggregateFinding<'_> {
    pub fn diagnostic(self) -> Result<Diagnostic, ProjectionError> {
        let key = match self {
            Self::MissingKeys { path, dataset, key } => Some((
                path,
                C::AggregateMissingKeys,
                [
                    ("dataset".into(), text(dataset)),
                    (
                        "key".into(),
                        match key {
                            AggregateEmptyKey::Mapping => V::Object(Default::default()),
                            AggregateEmptyKey::List => V::Array(vec![]),
                        },
                    ),
                ]
                .into(),
            )),
            Self::KeyReference { path, identifier } => Some((
                path,
                C::AggregateKeyReference,
                [("identifier".into(), text(identifier))].into(),
            )),
            Self::KeyType {
                path,
                source,
                expected,
                actual,
                inferred,
            } => Some((
                path,
                if inferred {
                    C::ImplicitSourceKeyType
                } else {
                    C::AggregateKeyType
                },
                [
                    ("source".into(), text(source)),
                    ("expected".into(), text(comparable_name(expected))),
                    ("actual".into(), text(comparable_name(actual.into()))),
                ]
                .into(),
            )),
            Self::NoApplicableKeys {
                path,
                dataset,
                keys,
            } => {
                let hint = "declare an explicit `intermediate:` with `source`/`key` pairs";

                Some((
                    path,
                    C::ImplicitSourceNoApplicableKeys,
                    [
                        ("dataset".into(), text(dataset)),
                        ("hint".into(), text(hint)),
                        (
                            "keys".into(),
                            V::Array(keys.iter().map(|key| text(key)).collect()),
                        ),
                    ]
                    .into(),
                ))
            }
            _ => None,
        };
        if let Some((path, code, context)) = key {
            return Ok(Diagnostic {
                code,
                spec_paths: vec![path.into()],
                context,
                source_span: None,
                operand_route: None,
            });
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
            | Self::KeyReference { .. }
            | Self::KeyType { .. }
            | Self::NoApplicableKeys { .. } => {
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
                context.insert("reducer".into(), text("ONLY"));
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
            | Self::KeyReference { .. }
            | Self::KeyType { .. }
            | Self::NoApplicableKeys { .. } => {
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
