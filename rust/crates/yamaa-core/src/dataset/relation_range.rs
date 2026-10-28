//! Donor narrowing retains endpoint policy, read effects and exact numeric comparison.
use super::*;
use crate::{
    expression_table::Closed,
    table::ValueRef,
    value::{compare_present_refs, ValueType},
};
use std::cmp::Ordering;

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
    /// The types of a range whose value has no common order with a typed
    /// bound (LOOKUP-9), if it has none.
    pub fn incomparable(
        value: ValueType,
        lower: Option<ValueType>,
        upper: Option<ValueType>,
    ) -> Option<Self> {
        [lower, upper]
            .into_iter()
            .flatten()
            .any(|kind| !comparable(value, kind))
            .then_some(Self {
                value,
                lower,
                upper,
            })
    }
    pub fn diagnostic(&self, path: &str, intermediate: &str) -> crate::diagnostic::Diagnostic {
        self.diagnostic_for(path, Some(intermediate))
    }
    /// Aggregate ranges omit the intermediate owner while retaining endpoint types.
    pub fn diagnostic_for(
        &self,
        path: &str,
        intermediate: Option<&str>,
    ) -> crate::diagnostic::Diagnostic {
        use crate::diagnostic::{ConditionCode, Diagnostic};
        use crate::json::Value as JsonValue;
        let name = |kind| match kind {
            ValueType::Str => "str",
            ValueType::Int => "int",
            ValueType::Float => "float",
            ValueType::Bool => "bool",
            ValueType::Date => "date",
            ValueType::DateTime => "datetime",
        };
        let text = |value: &str| JsonValue::String(value.into());
        let mut context = crate::json::Map::from([("value_type".into(), text(name(self.value)))]);
        if let Some(intermediate) = intermediate {
            context.insert("intermediate".into(), text(intermediate));
        }
        for (field, kind) in [("lower_type", self.lower), ("upper_type", self.upper)] {
            if let Some(kind) = kind {
                context.insert(field.into(), text(name(kind)));
            }
        }
        Diagnostic {
            code: ConditionCode::LookupRangeTypes,
            spec_paths: vec![path.into()],
            context,
            source_span: None,
            operand_route: None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RelationRangeError {
    #[error("dataset relation range invalid metadata")]
    InvalidMetadata,
    #[error("dataset relation range no bounds")]
    NoBounds,
    #[error("dataset relation range incomparable: {0:?}")]
    Incomparable(RelationRangeTypes),
}
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum RelationRangeFailure<E> {
    #[error("dataset relation range access: {0:?}")]
    Access(E),
    #[error("dataset relation range incomparable: {0:?}")]
    Incomparable(RelationRangeTypes),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationRange {
    path: String,
    current: CurrentField,
    value_kind: Option<ValueType>,
    lower: Option<RelationRangeBound>,
    upper: Option<RelationRangeBound>,
    closed: Closed,
    /// The variable of the source binding whose implicit join the range
    /// narrows, which findings about the range name; none for another kind of
    /// lookup.
    variable: Option<String>,
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
        closed: Closed,
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
        if let Some(types) = value_kind.and_then(|value| {
            RelationRangeTypes::incomparable(
                value,
                lower.as_ref().and_then(|bound| bound.kind),
                upper.as_ref().and_then(|bound| bound.kind),
            )
        }) {
            return Err(RelationRangeError::Incomparable(types));
        }
        Ok(Self {
            path,
            current,
            value_kind,
            lower,
            upper,
            closed,
            variable: None,
        })
    }
    /// Name the source binding whose implicit join the range narrows.
    pub fn for_variable(mut self, variable: &str) -> Self {
        self.variable = Some(variable.into());
        self
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    /// The variable of the source binding that declares the range.
    pub fn variable(&self) -> Option<&str> {
        self.variable.as_deref()
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
    pub fn closed(&self) -> Closed {
        self.closed
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
            let order = compare_present_refs(lower, value).map_err(|_| mismatch())?;
            if order == Ordering::Greater
                || (order == Ordering::Equal
                    && matches!(self.closed, Closed::Right | Closed::Neither))
            {
                return Ok(false);
            }
        }
        if let Some(upper) = upper {
            let order = compare_present_refs(value, upper).map_err(|_| mismatch())?;
            if order == Ordering::Greater
                || (order == Ordering::Equal
                    && matches!(self.closed, Closed::Left | Closed::Neither))
            {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
