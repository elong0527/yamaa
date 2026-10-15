//! Inclusive donor narrowing retains endpoint effects and exact numeric comparison.
use super::*;
use crate::{
    table::ValueRef,
    value::{compare_present_refs, ValueType},
};
use core::cmp::Ordering;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationRangeBound {
    pub column: usize,
    pub identifier: String,
    pub kind: Option<ValueType>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RelationRangeTypes {
    pub value: ValueType,
    pub lower: Option<ValueType>,
    pub upper: Option<ValueType>,
}
impl RelationRangeTypes {
    pub fn diagnostic(&self, path: &str, intermediate: &str) -> crate::diagnostic::Diagnostic {
        self.diagnostic_for(path, Some(intermediate))
    }
    /// Aggregate ranges omit the intermediate owner while retaining endpoint types.
    pub fn diagnostic_for(
        &self,
        path: &str,
        intermediate: Option<&str>,
    ) -> crate::diagnostic::Diagnostic {
        use crate::diagnostic::{ConditionCode, ContextValue, Diagnostic};
        let name = |kind| match kind {
            ValueType::Str => "str",
            ValueType::Int => "int",
            ValueType::Float => "float",
            ValueType::Bool => "bool",
            ValueType::Date => "date",
            ValueType::DateTime => "datetime",
        };
        let text = |value: &str| ContextValue::Scalar(Value::Str(value.into()));
        let mut context = BTreeMap::from([("value_type".into(), text(name(self.value)))]);
        if let Some(intermediate) = intermediate {
            context.insert("intermediate".into(), text(intermediate));
        }
        for (field, kind) in [("lower_type", self.lower), ("upper_type", self.upper)] {
            if let Some(kind) = kind {
                context.insert(field.into(), text(name(kind)));
            }
        }
        Diagnostic {
            code: ConditionCode::IntermediateRangeTypes,
            spec_paths: vec![path.into()],
            context,
            source_span: None,
            operand_route: None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RelationRangeError {
    InvalidMetadata,
    Limit,
    NoBounds,
    Incomparable(RelationRangeTypes),
}
#[derive(Debug, PartialEq, Eq)]
pub enum RelationRangeFailure<E> {
    Access(E),
    Incomparable(RelationRangeTypes),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationRange {
    path: String,
    current: CurrentField,
    value_kind: Option<ValueType>,
    lower: Option<RelationRangeBound>,
    upper: Option<RelationRangeBound>,
}
fn comparable(a: ValueType, b: ValueType) -> bool {
    matches!(
        (a, b),
        (
            ValueType::Int | ValueType::Float,
            ValueType::Int | ValueType::Float
        )
    ) || (a == b && a != ValueType::Bool)
}
impl RelationRange {
    pub fn new(
        path: String,
        current: CurrentField,
        value_kind: Option<ValueType>,
        lower: Option<RelationRangeBound>,
        upper: Option<RelationRangeBound>,
        metadata_bytes: usize,
    ) -> Result<Self, RelationRangeError> {
        if lower.is_none() && upper.is_none() {
            return Err(RelationRangeError::NoBounds);
        }
        if path.is_empty()
            || current.identifier.is_empty()
            || [&lower, &upper].iter().any(|bound| {
                bound
                    .as_ref()
                    .is_some_and(|bound| bound.identifier.is_empty())
            })
        {
            return Err(RelationRangeError::InvalidMetadata);
        }
        let initial = path
            .capacity()
            .checked_add(current.identifier.capacity())
            .and_then(|n| n.checked_add(core::mem::size_of::<Self>()))
            .ok_or(RelationRangeError::Limit)?;
        let bytes = [&lower, &upper]
            .iter()
            .filter_map(|bound| bound.as_ref())
            .try_fold(initial, |n, bound| {
                n.checked_add(bound.identifier.capacity())
            })
            .filter(|&n| n <= metadata_bytes)
            .ok_or(RelationRangeError::Limit)?;
        let _ = bytes;
        if let Some(value) = value_kind {
            if [&lower, &upper]
                .iter()
                .filter_map(|bound| bound.as_ref().and_then(|bound| bound.kind))
                .any(|kind| !comparable(value, kind))
            {
                return Err(RelationRangeError::Incomparable(RelationRangeTypes {
                    value,
                    lower: lower.as_ref().and_then(|bound| bound.kind),
                    upper: upper.as_ref().and_then(|bound| bound.kind),
                }));
            }
        }
        Ok(Self {
            path,
            current,
            value_kind,
            lower,
            upper,
        })
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn current(&self) -> &CurrentField {
        &self.current
    }
    pub fn value_kind(&self) -> Option<ValueType> {
        self.value_kind
    }
    pub fn lower(&self) -> Option<&RelationRangeBound> {
        self.lower.as_ref()
    }
    pub fn upper(&self) -> Option<&RelationRangeBound> {
        self.upper.as_ref()
    }
    pub fn validate_shape(&self, shape: &RelationShape) -> Result<(), RelationRangeError> {
        for bound in [&self.lower, &self.upper]
            .iter()
            .filter_map(|bound| bound.as_ref())
        {
            if shape.fields().get(bound.column).is_none_or(|field| {
                field.identifier != bound.identifier || field.kind != bound.kind
            }) {
                return Err(RelationRangeError::InvalidMetadata);
            }
        }
        Ok(())
    }
    /// Stored-only plans retain their original schema rather than copying donor metadata.
    pub(super) fn validate_source(&self, schema: &TableSchema) -> Result<(), RelationRangeError> {
        for bound in [&self.lower, &self.upper]
            .iter()
            .filter_map(|bound| bound.as_ref())
        {
            if schema.columns().get(bound.column).is_none_or(|field| {
                field.name != bound.identifier || Some(field.kind.into()) != bound.kind
            }) {
                return Err(RelationRangeError::InvalidMetadata);
            }
        }
        Ok(())
    }
    /// Endpoint reads precede comparisons. Missing lower prevents an upper read;
    /// a lower comparison which excludes the record prevents an upper comparison.
    pub fn allows<'donor, E>(
        &self,
        value: ValueRef<'_>,
        mut read: impl FnMut(usize) -> Result<ValueRef<'donor>, E>,
    ) -> Result<bool, RelationRangeFailure<E>> {
        let Some(value_type) = value.value_type() else {
            return Ok(false);
        };
        let lower = if let Some(bound) = &self.lower {
            let value = read(bound.column).map_err(RelationRangeFailure::Access)?;
            if matches!(value, ValueRef::Missing) {
                return Ok(false);
            }
            Some(value)
        } else {
            None
        };
        let upper = if let Some(bound) = &self.upper {
            let value = read(bound.column).map_err(RelationRangeFailure::Access)?;
            if matches!(value, ValueRef::Missing) {
                return Ok(false);
            }
            Some(value)
        } else {
            None
        };
        let mismatch = || {
            RelationRangeFailure::Incomparable(RelationRangeTypes {
                value: value_type,
                lower: lower.and_then(ValueRef::value_type),
                upper: upper.and_then(ValueRef::value_type),
            })
        };
        if let Some(lower) = lower {
            if compare_present_refs(lower, value).map_err(|_| mismatch())? == Ordering::Greater {
                return Ok(false);
            }
        }
        if let Some(upper) = upper {
            if compare_present_refs(value, upper).map_err(|_| mismatch())? == Ordering::Greater {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
