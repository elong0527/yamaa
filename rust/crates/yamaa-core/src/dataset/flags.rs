//! One owned three-valued flag plan; output conversion remains its assignment's owner.
use super::*;
use crate::{
    expression::{CompiledExpression, Condition, ErrorKind, EvaluationError, Truth},
    table::ValueRef,
};

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
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum FlagError {
    #[error("dataset flags binding: {0:?}")]
    Binding(BindingError),
    #[error("dataset flags missing value required")]
    MissingValueRequired,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Flag {
    condition: CompiledExpression,
    bindings: Vec<FlagBinding>,
    true_value: Value,
    false_value: Option<Value>,
    missing: Option<Value>,
    /// The flag operation's path, owning its `missing` count.
    path: String,
}
impl Flag {
    /// Inspect the entire supplied map and literals before allocating an index.
    /// `Some(Missing)` preserves an explicitly declared null branch. The
    /// operation's path is its condition's path without `.condition`.
    pub fn new(
        condition: CompiledExpression,
        mut bindings: Vec<FlagBinding>,
        true_value: Value,
        false_value: Option<Value>,
        missing: Option<Value>,
    ) -> Result<Self, FlagError> {
        if condition.spec_path().is_empty() {
            return Err(FlagError::Binding(BindingError::EmptyPath));
        }
        if false_value.is_some() && missing.is_none() {
            return Err(FlagError::MissingValueRequired);
        }
        let names: Vec<&str> = condition.identifiers().collect();
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
        let path = condition
            .spec_path()
            .strip_suffix(".condition")
            .unwrap_or(condition.spec_path())
            .to_owned();
        Ok(Self {
            condition,
            bindings,
            true_value,
            false_value,
            missing,
            path,
        })
    }
    pub fn condition(&self) -> &CompiledExpression {
        &self.condition
    }
    /// An `UNKNOWN` condition is counted here, whether `missing` is declared
    /// or its default substitution gives missing (EXPR-8).
    pub fn missing_path(&self) -> String {
        std::format!("{}.missing", self.path)
    }
    pub fn bindings(&self) -> &[FlagBinding] {
        &self.bindings
    }
    /// Borrow one raw literal without changing its logical type.
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
}

/// Flag predicates own their public validation vocabulary, including regex input
/// types (FORMULA-5). A numeric operand's failure keeps the shared projection.
pub fn flag_condition_diagnostic<E>(
    error: &EvaluationError<E>,
) -> Option<crate::diagnostic::Diagnostic> {
    use crate::diagnostic::{ConditionCode as C, Context, Diagnostic};
    use crate::json::Value as V;
    fn text(s: &str) -> V {
        V::String(s.into())
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
            C::PredicateDanglingEscape
        }
        Condition::NumericUnknownField { .. }
        | Condition::NumericInput { .. }
        | Condition::Arithmetic(_) => return error.diagnostic().ok(),
    };
    Some(Diagnostic {
        code,
        spec_paths: vec![error.spec_path.clone()],
        context,
        source_span: None,
        operand_route: Some(error.route.iter().map(|step| step.label()).collect()),
    })
}
