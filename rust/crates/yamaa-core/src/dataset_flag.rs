//! One owned three-valued flag plan; output conversion remains its assignment's owner.
use super::*;
use crate::{
    predicate::{self, Truth},
    table::ValueRef,
};
use alloc::format;

pub const MAX_FLAG_METADATA_BYTES: usize = 16_777_216;

/// A lazy operand is a current/completed scalar or a distinct key-grain collection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FlagRead {
    Read(Read),
    Collect { column: usize, identifier: String },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlagBinding {
    pub name: String,
    pub read: FlagRead,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlagError {
    MetadataLimit,
    Binding(BindingError),
    MissingValueRequired,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Flag {
    condition: predicate::Plan,
    bindings: Vec<FlagBinding>,
    true_value: Value,
    false_value: Option<Value>,
    missing: Option<Value>,
}
impl Flag {
    /// Inspect the entire supplied map and literals before allocating an index.
    /// `Some(Missing)` preserves an explicitly declared null branch.
    pub fn new(
        condition: predicate::Plan,
        mut bindings: Vec<FlagBinding>,
        true_value: Value,
        false_value: Option<Value>,
        missing: Option<Value>,
    ) -> Result<Self, FlagError> {
        let size = condition
            .expression()
            .len()
            .checked_add(condition.spec_path().len())
            .and_then(|n| {
                bindings.iter().try_fold(n, |n, b| {
                    n.checked_add(b.name.len())?
                        .checked_add(match &b.read {
                            FlagRead::Collect { identifier, .. } => identifier.len(),
                            _ => 0,
                        })?
                        .checked_add(128)
                })
            })
            .and_then(|n| {
                core::iter::once(&true_value)
                    .chain(false_value.iter())
                    .chain(missing.iter())
                    .try_fold(n, |n, v| {
                        n.checked_add(match v {
                            Value::Str(s) => s.len(),
                            _ => 0,
                        })
                    })
            })
            .and_then(|n| n.checked_mul(8));
        if bindings.len() > 65_536 || size.is_none_or(|n| n > MAX_FLAG_METADATA_BYTES) {
            return Err(FlagError::MetadataLimit);
        }
        if condition.spec_path().is_empty() {
            return Err(FlagError::Binding(BindingError::EmptyPath));
        }
        if false_value.is_some() && missing.is_none() {
            return Err(FlagError::MissingValueRequired);
        }
        let names = condition.identifiers();
        bindings.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        for (i, b) in bindings.iter().enumerate() {
            if i > 0 && bindings[i - 1].name == b.name {
                return Err(FlagError::Binding(BindingError::DuplicateName));
            }
            if !names.contains(&b.name.as_str()) {
                return Err(FlagError::Binding(BindingError::UnusedName));
            }
        }
        if names.iter().any(|name| {
            bindings
                .binary_search_by(|b| b.name.as_str().cmp(name))
                .is_err()
        }) {
            return Err(FlagError::Binding(BindingError::MissingName));
        }
        Ok(Self {
            condition,
            bindings,
            true_value,
            false_value,
            missing,
        })
    }
    pub fn condition(&self) -> &predicate::Plan {
        &self.condition
    }
    pub fn bindings(&self) -> &[FlagBinding] {
        &self.bindings
    }
    /// Borrow one raw literal; the engine admits text before owning this value.
    pub fn value(&self, truth: Truth) -> ValueRef<'_> {
        match truth {
            Truth::True => ValueRef::from(&self.true_value),
            Truth::False => self
                .false_value
                .as_ref()
                .map_or(ValueRef::Missing, ValueRef::from),
            Truth::Unknown => self
                .missing
                .as_ref()
                .map_or(ValueRef::Missing, ValueRef::from),
        }
    }
    pub(super) fn key_expression(&self) -> bool {
        self.bindings.iter().all(|b| {
            !matches!(
                b.read,
                FlagRead::Read(Read::Intermediate { .. }) | FlagRead::Collect { .. }
            )
        })
    }
    pub(super) fn reads_source(&self) -> bool {
        self.bindings
            .iter()
            .any(|b| matches!(b.read, FlagRead::Read(Read::Source(_))))
    }
    pub(super) fn validate(
        &self,
        source: &TableSchema,
        available: &[bool],
        secondary: &[SecondarySource],
        intermediates: &[Intermediate],
        output: &TableSchema,
        mode: &RowMode,
    ) -> Result<(), PlanError> {
        for binding in &self.bindings {
            match &binding.read {
                FlagRead::Read(Read::Source(column)) => {
                    if *column >= source.columns().len() {
                        return Err(PlanError::InvalidSource);
                    }
                    if matches!(mode, RowMode::Groups(groups) if !groups.contains(column)) {
                        return Err(PlanError::NonGroupSource);
                    }
                }
                FlagRead::Read(Read::Column(column)) => {
                    if !available.get(*column).copied().unwrap_or(false) {
                        return Err(PlanError::UnavailableColumn);
                    }
                }
                FlagRead::Read(Read::Intermediate { index, column }) => {
                    if !matches!(mode, RowMode::Keys) {
                        return Err(PlanError::InvalidIntermediate);
                    }
                    let item = intermediates
                        .get(*index)
                        .ok_or(PlanError::InvalidIntermediate)?;
                    super::intermediates::validate_read(
                        item, *column, source, secondary, available, output,
                    )?;
                }
                FlagRead::Collect { column, identifier } => {
                    if !matches!(mode, RowMode::Keys) || *column >= source.columns().len() {
                        return Err(PlanError::InvalidSource);
                    }
                    if identifier.is_empty() {
                        return Err(PlanError::EmptyPath);
                    }
                }
            }
        }
        Ok(())
    }
}

/// Flag predicates own their public validation vocabulary, including regex input
/// types (REQ-0190). Generic predicate consumers retain their current mapping.
pub fn flag_condition_diagnostic<E>(
    error: &predicate::EvaluationError<E>,
) -> Option<crate::diagnostic::Diagnostic> {
    use crate::diagnostic::{ConditionCode as C, Context, ContextValue as V, Diagnostic};
    use predicate::{Condition, ErrorKind};
    fn text(s: &str) -> V {
        V::Scalar(Value::Str(s.into()))
    }
    fn kind(k: crate::value::ValueType) -> &'static str {
        use crate::value::ValueType::*;
        match k {
            Str => "str",
            Int => "int",
            Float => "float",
            Bool => "bool",
            Date => "date",
            DateTime => "datetime",
        }
    }
    let ErrorKind::Condition(cause) = &error.kind else {
        return None;
    };
    let mut context = Context::new();
    let code = match cause {
        Condition::UnknownField { identifier } => {
            context.insert("identifier".into(), text(identifier));
            C::PredicateUnknownField
        }
        Condition::IncompatiblePair { left, right } => {
            context.insert("left_type".into(), text(kind(*left)));
            context.insert("right_type".into(), text(kind(*right)));
            C::PredicateInputType
        }
        Condition::ExpectedText { actual } | Condition::ContainsExpectedText { actual } => {
            context.insert("expected".into(), text("str"));
            context.insert("actual".into(), text(kind(*actual)));
            C::PredicateInputType
        }
        Condition::DanglingEscape => {
            context.insert("reason".into(), text("LIKE pattern has a dangling escape"));
            C::PredicateInvalidEscape
        }
    };
    Some(Diagnostic {
        code,
        spec_paths: vec![error.spec_path.clone()],
        context,
        source_span: None,
        operand_route: Some(error.route.iter().map(|r| format!("{r:?}")).collect()),
    })
}
